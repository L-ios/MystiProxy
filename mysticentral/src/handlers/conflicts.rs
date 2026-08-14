//! Conflict management handlers (JWT-protected).
//!
//! Frontend-aligned endpoints: GET /conflicts, GET /conflicts/:config_id,
//! PUT /conflicts/:config_id/resolve, DELETE /conflicts/:config_id.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::ApiError;
use crate::handlers::AppState;
use crate::middleware::auth::AuthContext;
use crate::services::conflict_json;
use crate::services::{
    ConflictRepository, MockService, PostgresConflictRepository, PostgresMockRepository,
};
use mysti_common::MockConfiguration;

/// Resolve request body (frontend contract)
#[derive(Debug, Deserialize)]
pub struct ResolveRequest {
    pub resolution: String,
    pub merged_config: Option<MockConfiguration>,
}

/// GET /api/v1/conflicts
pub async fn list_conflicts(
    _auth: AuthContext,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let repo = PostgresConflictRepository::new(state.pool.clone());
    let conflicts = repo.find_all().await?;
    let total = conflicts.len();
    let data: Vec<Value> = conflicts.iter().map(conflict_json).collect();
    Ok(Json(json!({ "data": data, "total": total })))
}

/// GET /api/v1/conflicts/:config_id
pub async fn get_conflict(
    _auth: AuthContext,
    State(state): State<AppState>,
    Path(config_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let repo = PostgresConflictRepository::new(state.pool.clone());
    let conflict = repo
        .find_by_config(config_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("conflict for config {config_id} not found")))?;
    Ok(Json(conflict_json(&conflict)))
}

/// PUT /api/v1/conflicts/:config_id/resolve
pub async fn resolve_conflict(
    _auth: AuthContext,
    State(state): State<AppState>,
    Path(config_id): Path<Uuid>,
    Json(body): Json<ResolveRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let conflict_repo = PostgresConflictRepository::new(state.pool.clone());
    let conflict = conflict_repo
        .find_by_config(config_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("conflict for config {config_id} not found")))?;

    let mock_repo = PostgresMockRepository::new(state.pool.clone());
    let service = MockService::new(mock_repo);

    let mut resolved = match body.resolution.as_str() {
        "keep_local" => {
            let mut config = conflict.local_version.clone();
            config.version_vector.increment(Uuid::new_v4());
            config
        }
        "keep_central" => conflict.central_version.clone(),
        "merge" => {
            let mut merged = body.merged_config.ok_or_else(|| {
                ApiError::BadRequest("merged_config is required for merge resolution".to_string())
            })?;
            if merged.id != config_id {
                return Err(ApiError::BadRequest(
                    "merged_config.id must match the conflict config_id".to_string(),
                ));
            }
            // Merge both sides of the causal history, then bump.
            merged
                .version_vector
                .merge(&conflict.local_version.version_vector);
            merged
                .version_vector
                .merge(&conflict.central_version.version_vector);
            merged.version_vector.increment(Uuid::new_v4());
            merged
        }
        other => {
            return Err(ApiError::BadRequest(format!(
                "invalid resolution '{other}', expected keep_local | keep_central | merge"
            )))
        }
    };

    resolved.updated_at = chrono::Utc::now();
    service.save(&resolved).await?;
    conflict_repo.delete(config_id).await?;

    Ok(Json(serde_json::to_value(&resolved).unwrap_or_default()))
}

/// DELETE /api/v1/conflicts/:config_id — dismiss, keep central version as-is.
pub async fn dismiss_conflict(
    _auth: AuthContext,
    State(state): State<AppState>,
    Path(config_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let repo = PostgresConflictRepository::new(state.pool.clone());
    let removed = repo.delete(config_id).await?;
    if !removed {
        return Err(ApiError::NotFound(format!(
            "conflict for config {config_id} not found"
        )));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_request_parses() {
        let r: ResolveRequest = serde_json::from_str(r#"{"resolution":"keep_central"}"#).unwrap();
        assert_eq!(r.resolution, "keep_central");
        assert!(r.merged_config.is_none());
    }

    #[test]
    fn test_resolve_request_with_merge_config() {
        let cfg = mysti_common::MockConfiguration::new(
            "m".to_string(),
            "/m".to_string(),
            mysti_common::HttpMethod::Get,
            mysti_common::MatchingRules::default(),
            mysti_common::ResponseConfig::default(),
        );
        let v = serde_json::to_value(&cfg).unwrap();
        let body = json!({"resolution":"merge","merged_config": v});
        let r: ResolveRequest = serde_json::from_value(body).unwrap();
        assert_eq!(r.resolution, "merge");
        assert_eq!(r.merged_config.unwrap().name, "m");
    }

    #[test]
    fn test_missing_resolution_fails() {
        assert!(serde_json::from_str::<ResolveRequest>(r#"{}"#).is_err());
    }
}
