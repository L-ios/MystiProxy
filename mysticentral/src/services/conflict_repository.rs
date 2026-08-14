//! Conflict Repository
//!
//! Persistent queue of version-vector conflicts detected during sync push.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use mysti_common::MockConfiguration;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::ApiError;
use crate::services::user_repository::map_db_err;

/// Domain record for a persisted conflict
#[derive(Debug, Clone)]
pub struct ConflictRecord {
    pub config_id: Uuid,
    pub local_version: MockConfiguration,
    pub central_version: MockConfiguration,
    pub detected_at: DateTime<Utc>,
}

/// Row representation for sqlx
#[derive(sqlx::FromRow)]
struct ConflictRow {
    config_id: Uuid,
    local_version: Value,
    central_version: Value,
    detected_at: DateTime<Utc>,
}

/// Deserialize a JSONB column into a MockConfiguration.
/// Corrupt rows degrade to a structured error instead of panicking.
fn parse_version(column: &str, v: &Value) -> Result<MockConfiguration, ApiError> {
    serde_json::from_value(v.clone())
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("corrupt {column} in sync_conflicts: {e}")))
}

impl TryFrom<ConflictRow> for ConflictRecord {
    type Error = ApiError;

    fn try_from(r: ConflictRow) -> Result<Self, Self::Error> {
        Ok(ConflictRecord {
            config_id: r.config_id,
            local_version: parse_version("local_version", &r.local_version)?,
            central_version: parse_version("central_version", &r.central_version)?,
            detected_at: r.detected_at,
        })
    }
}

/// Serialize a record to the frontend response shape
pub fn conflict_json(c: &ConflictRecord) -> Value {
    serde_json::json!({
        "config_id": c.config_id,
        "local_version": c.local_version,
        "central_version": c.central_version,
        "detected_at": c.detected_at.to_rfc3339(),
    })
}

/// Trait for conflict persistence
#[async_trait]
pub trait ConflictRepository: Send + Sync {
    /// Insert or replace the conflict row for a config
    async fn upsert(&self, r: ConflictRecord) -> Result<(), ApiError>;
    async fn find_by_config(&self, config_id: Uuid) -> Result<Option<ConflictRecord>, ApiError>;
    async fn find_all(&self) -> Result<Vec<ConflictRecord>, ApiError>;
    /// Returns true when a row was removed
    async fn delete(&self, config_id: Uuid) -> Result<bool, ApiError>;
}

/// PostgreSQL implementation
pub struct PostgresConflictRepository {
    pool: PgPool,
}

impl PostgresConflictRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ConflictRepository for PostgresConflictRepository {
    async fn upsert(&self, r: ConflictRecord) -> Result<(), ApiError> {
        let local = serde_json::to_value(&r.local_version)
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("serialize local: {e}")))?;
        let central = serde_json::to_value(&r.central_version)
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("serialize central: {e}")))?;

        sqlx::query(
            r#"
            INSERT INTO sync_conflicts (config_id, local_version, central_version, detected_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (config_id) DO UPDATE SET
                local_version = EXCLUDED.local_version,
                central_version = EXCLUDED.central_version,
                detected_at = EXCLUDED.detected_at
            "#,
        )
        .bind(r.config_id)
        .bind(local)
        .bind(central)
        .bind(r.detected_at)
        .execute(&self.pool)
        .await
        .map_err(map_db_err)?;
        Ok(())
    }

    async fn find_by_config(&self, config_id: Uuid) -> Result<Option<ConflictRecord>, ApiError> {
        let row = sqlx::query_as::<_, ConflictRow>(
            "SELECT config_id, local_version, central_version, detected_at FROM sync_conflicts WHERE config_id = $1",
        )
        .bind(config_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_db_err)?;

        row.map(TryInto::try_into).transpose()
    }

    async fn find_all(&self) -> Result<Vec<ConflictRecord>, ApiError> {
        let rows = sqlx::query_as::<_, ConflictRow>(
            "SELECT config_id, local_version, central_version, detected_at FROM sync_conflicts ORDER BY detected_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_db_err)?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn delete(&self, config_id: Uuid) -> Result<bool, ApiError> {
        let result = sqlx::query("DELETE FROM sync_conflicts WHERE config_id = $1")
            .bind(config_id)
            .execute(&self.pool)
            .await
            .map_err(map_db_err)?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config(id: Uuid, name: &str) -> MockConfiguration {
        let mut c = MockConfiguration::new(
            name.to_string(),
            format!("/{name}"),
            mysti_common::HttpMethod::Get,
            mysti_common::MatchingRules::default(),
            mysti_common::ResponseConfig::default(),
        );
        c.id = id;
        c
    }

    #[test]
    fn test_row_to_record_roundtrip() {
        let id = Uuid::new_v4();
        let local = sample_config(id, "local");
        let central = sample_config(id, "central");
        let lv = serde_json::to_value(&local).unwrap();
        let cv = serde_json::to_value(&central).unwrap();

        let row = ConflictRow {
            config_id: id,
            local_version: lv,
            central_version: cv,
            detected_at: Utc::now(),
        };
        let rec = ConflictRecord::try_from(row).unwrap();
        assert_eq!(rec.config_id, id);
        assert_eq!(rec.local_version.name, "local");
        assert_eq!(rec.central_version.name, "central");
    }

    #[test]
    fn test_corrupt_json_yields_structured_error() {
        let row = ConflictRow {
            config_id: Uuid::new_v4(),
            local_version: serde_json::json!({"unexpected": true}),
            central_version: serde_json::json!({}),
            detected_at: Utc::now(),
        };
        assert!(ConflictRecord::try_from(row).is_err());
    }

    #[test]
    fn test_conflict_json_shape() {
        let id = Uuid::new_v4();
        let rec = ConflictRecord {
            config_id: id,
            local_version: sample_config(id, "L"),
            central_version: sample_config(id, "C"),
            detected_at: Utc::now(),
        };
        let j = conflict_json(&rec);
        assert_eq!(j["config_id"], id.to_string());
        assert!(j["local_version"].is_object());
        assert!(j["central_version"].is_object());
        assert!(j["detected_at"].as_str().is_some());
        assert!(j.get("password_hash").is_none());
    }
}
