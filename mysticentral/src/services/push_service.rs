//! Push service: central → instance config delivery.
//!
//! POSTs to each registered instance's local-management API
//! (`{endpoint_url}/api/v1/sync/trigger`) and records the outcome.

use chrono::Utc;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::ApiError;
use crate::services::instance_repository::{InstanceRepository, PostgresInstanceRepository};

/// Per-instance push outcome
#[derive(Debug, Clone, Serialize)]
pub struct PushOutcome {
    pub instance_id: Uuid,
    pub ok: bool,
    pub detail: String,
}

/// Trigger config push for one instance by id.
pub async fn push_to_instance(pool: &PgPool, instance_id: Uuid) -> Result<PushOutcome, ApiError> {
    let repo = PostgresInstanceRepository::new(pool.clone());
    let instance = repo
        .find_by_id(instance_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("instance {instance_id} not found")))?;

    let endpoint = instance.endpoint_url.trim_end_matches('/').to_string();
    let url = format!("{endpoint}/api/v1/sync/trigger");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("http client: {e}")))?;

    let outcome = match client.post(&url).json(&serde_json::json!({})).send().await {
        Ok(resp) if resp.status().is_success() => PushOutcome {
            instance_id,
            ok: true,
            detail: format!(
                "instance accepted push ({}), endpoint {endpoint}",
                resp.status()
            ),
        },
        Ok(resp) => PushOutcome {
            instance_id,
            ok: false,
            detail: format!("instance returned {}: {endpoint}", resp.status()),
        },
        Err(e) => PushOutcome {
            instance_id,
            ok: false,
            detail: format!("cannot reach {endpoint}: {e}"),
        },
    };

    // Record outcome on the instance row.
    let status = if outcome.ok { "connected" } else { "error" };
    let _ = sqlx::query(
        "UPDATE mystiproxy_instances SET sync_status = $2, last_sync_at = CASE WHEN $3 THEN NOW() ELSE last_sync_at END WHERE id = $1",
    )
    .bind(instance_id)
    .bind(status)
    .bind(outcome.ok)
    .execute(pool)
    .await;

    Ok(outcome)
}

/// Push to every registered instance (concurrently).
pub async fn push_to_all(pool: &PgPool) -> Result<Vec<PushOutcome>, ApiError> {
    let repo = PostgresInstanceRepository::new(pool.clone());
    let instances = repo
        .find_all(crate::models::InstanceFilter::default())
        .await?;

    let mut outcomes = Vec::with_capacity(instances.len());
    for instance in instances {
        // Sequential keeps the DB pool happy under the 3s timeout each.
        outcomes.push(push_to_instance(pool, instance.id).await?);
    }
    Ok(outcomes)
}

/// Convenience: summary counts for push-all / POST /sync responses.
pub fn summarize(outcomes: &[PushOutcome]) -> (usize, usize) {
    let pushed = outcomes.iter().filter(|o| o.ok).count();
    let failed = outcomes.len() - pushed;
    (pushed, failed)
}

/// RFC3339 "now" for sync responses.
pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_outcome_shape() {
        let o = PushOutcome {
            instance_id: Uuid::new_v4(),
            ok: true,
            detail: "ok".into(),
        };
        let j = serde_json::to_value(&o).unwrap();
        assert!(j["instance_id"].is_string());
        assert_eq!(j["ok"], true);
        assert_eq!(j["detail"], "ok");
    }

    #[test]
    fn test_summarize_counts() {
        let a = PushOutcome {
            instance_id: Uuid::new_v4(),
            ok: true,
            detail: String::new(),
        };
        let b = PushOutcome {
            instance_id: Uuid::new_v4(),
            ok: false,
            detail: String::new(),
        };
        let c = PushOutcome {
            instance_id: Uuid::new_v4(),
            ok: true,
            detail: String::new(),
        };
        assert_eq!(summarize(&[a, b, c]), (2, 1));
        assert_eq!(summarize(&[]), (0, 0));
    }
}
