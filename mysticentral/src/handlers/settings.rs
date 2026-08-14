//! Settings + sync-status + instance-push handlers (JWT-protected).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::ApiError;
use crate::handlers::AppState;
use crate::middleware::auth::AuthContext;
use crate::services::{
    conflict_json, now_rfc3339, push_to_all, push_to_instance, summarize, ConflictRepository as _,
    PostgresConflictRepository, PostgresSettingsRepository,
};
use crate::services::{SettingsPatch, SettingsRepository as _};

// ============================================================================
// Settings
// ============================================================================

/// GET /api/v1/settings
pub async fn get_settings(
    _auth: AuthContext,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let repo = PostgresSettingsRepository::new(state.pool.clone());
    let settings = repo.get().await?;
    Ok(Json(serde_json::to_value(&settings).unwrap_or_default()))
}

/// PUT /api/v1/settings
pub async fn update_settings(
    _auth: AuthContext,
    State(state): State<AppState>,
    Json(patch): Json<SettingsPatch>,
) -> Result<impl IntoResponse, ApiError> {
    let repo = PostgresSettingsRepository::new(state.pool.clone());
    let updated = repo.update(&patch).await?;
    Ok(Json(serde_json::to_value(&updated).unwrap_or_default()))
}

// ============================================================================
// Sync status & manual trigger
// ============================================================================

/// GET /api/v1/sync/status
pub async fn sync_status(
    _auth: AuthContext,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let pool = &state.pool;

    // Fresh heartbeat within 90s means "connected"
    let fresh: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT MAX(last_heartbeat) FROM mystiproxy_instances WHERE last_heartbeat > NOW() - INTERVAL '90 seconds'",
    )
    .fetch_one(pool)
    .await
    .map_err(crate::services::user_repository::map_db_err)?;
    let connected = fresh.is_some();

    let last_sync: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT MAX(last_sync_at) FROM mystiproxy_instances")
            .fetch_one(pool)
            .await
            .map_err(crate::services::user_repository::map_db_err)?;

    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sync_conflicts")
        .fetch_one(pool)
        .await
        .map_err(crate::services::user_repository::map_db_err)?;

    let settings = PostgresSettingsRepository::new(pool.clone()).get().await?;

    Ok(Json(json!({
        "connected": connected,
        "last_sync_at": last_sync.map(|t| t.to_rfc3339()),
        "sync_in_progress": false,
        "pending_changes": pending,
        "central_url": settings.central_url,
    })))
}

/// POST /api/v1/sync — push configs to all instances.
pub async fn trigger_sync(
    _auth: AuthContext,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let outcomes = push_to_all(&state.pool).await?;
    let (pushed, failed) = summarize(&outcomes);

    let conflict_repo = PostgresConflictRepository::new(state.pool.clone());
    let conflicts = conflict_repo.find_all().await?;
    let conflict_list: Vec<Value> = conflicts.iter().map(conflict_json).collect();

    Ok(Json(json!({
        "success": pushed > 0 && failed == 0,
        "synced_count": pushed,
        "conflicts": conflict_list,
        "synced_at": now_rfc3339(),
    })))
}

// ============================================================================
// Instance push
// ============================================================================

/// POST /api/v1/instances/:id/push
pub async fn push_instance(
    _auth: AuthContext,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let outcome = push_to_instance(&state.pool, id).await?;
    let body = json!({ "ok": outcome.ok, "detail": outcome.detail });
    if outcome.ok {
        Ok((StatusCode::OK, Json(body)))
    } else {
        Ok((StatusCode::BAD_GATEWAY, Json(body)))
    }
}

/// POST /api/v1/instances/push-all
pub async fn push_all_instances(
    _auth: AuthContext,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let outcomes = push_to_all(&state.pool).await?;
    let (pushed, failed) = summarize(&outcomes);
    let results: Vec<Value> = outcomes
        .iter()
        .map(|o| serde_json::to_value(o).unwrap_or_default())
        .collect();
    Ok(Json(json!({
        "results": results,
        "pushed": pushed,
        "failed": failed,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_patch_parses_partial() {
        let p: SettingsPatch = serde_json::from_str(r#"{"sync_interval": 60}"#).unwrap();
        assert_eq!(p.sync_interval, Some(60));
        assert!(p.log_level.is_none());
    }
}
