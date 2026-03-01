//! API routes and handlers
//!
//! Defines all HTTP routes for the MystiCentral API.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json, Router,
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::ApiError;
use crate::models::{
    EnvironmentCreateRequest, EnvironmentFilter, EnvironmentUpdateRequest,
    HeartbeatRequest, InstanceFilter, InstanceRegisterRequest,
    MockCreateRequest, MockFilter, MockUpdateRequest,
};
use crate::services::{
    AuthService, EnvironmentService, InstanceService, MockService,
    PostgresEnvironmentRepository, PostgresInstanceRepository, PostgresMockRepository,
};

/// Application state
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    #[allow(dead_code)]
    pub auth_service: AuthService,
}

impl AppState {
    pub fn new(pool: PgPool, auth_service: AuthService) -> Self {
        Self { pool, auth_service }
    }
}

/// Create the application router with all routes
pub fn create_routes() -> Router<AppState> {
    Router::new()
        .route("/health", axum::routing::get(health_check))
        .route(
            "/api/v1/mocks",
            axum::routing::get(list_mocks).post(create_mock),
        )
        .route(
            "/api/v1/mocks/:id",
            axum::routing::get(get_mock)
                .put(update_mock)
                .delete(delete_mock),
        )
        .route(
            "/api/v1/environments",
            axum::routing::get(list_environments).post(create_environment),
        )
        .route(
            "/api/v1/environments/:id",
            axum::routing::get(get_environment)
                .put(update_environment)
                .delete(delete_environment),
        )
        .route("/api/v1/instances", axum::routing::get(list_instances).post(register_instance))
        .route(
            "/api/v1/instances/:id",
            axum::routing::get(get_instance).delete(delete_instance),
        )
        .route("/api/v1/instances/:id/heartbeat", axum::routing::post(heartbeat))
        .route("/api/v1/sync/pull", axum::routing::post(sync_pull))
        .route("/api/v1/sync/push", axum::routing::post(sync_push))
        .route("/api/v1/sync/conflicts", axum::routing::get(list_conflicts))
        .route(
            "/api/v1/sync/conflicts/:id/resolve",
            axum::routing::post(resolve_conflict),
        )
        .route("/api/v1/mocks/import", axum::routing::post(import_mocks))
        .route("/api/v1/mocks/export", axum::routing::get(export_mocks))
        .route("/api/v1/analytics/stats", axum::routing::get(get_analytics_stats))
        .route("/api/v1/analytics/mock/:id", axum::routing::get(get_mock_analytics))
}

// ============================================================================
// Health Check
// ============================================================================

pub async fn health_check() -> impl IntoResponse {
    Json(json!({
        "status": "healthy",
        "service": "mysticentral"
    }))
}

// ============================================================================
// Mock Configuration Handlers
// ============================================================================

pub async fn list_mocks(
    State(state): State<AppState>,
    Query(filter): Query<MockFilter>,
) -> Response {
    let repo = PostgresMockRepository::new(state.pool);
    let service = MockService::new(repo);

    let page = filter.page();
    let limit = filter.limit();

    match service.list(filter).await {
        Ok((configs, total)) => {
            let total_pages = (total + 19) / 20;
            Json(json!({
                "data": configs,
                "pagination": {
                    "page": page,
                    "limit": limit,
                    "total": total,
                    "total_pages": total_pages
                }
            })).into_response()
        }
        Err(e) => e.into_response(),
    }
}

pub async fn get_mock(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let repo = PostgresMockRepository::new(state.pool);
    let service = MockService::new(repo);

    let config = service.get(id).await?;
    Ok(Json(json!(config)))
}

pub async fn create_mock(
    State(state): State<AppState>,
    Json(request): Json<MockCreateRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let repo = PostgresMockRepository::new(state.pool);
    let service = MockService::new(repo);

    let config = service.create(request, None).await?;
    Ok((StatusCode::CREATED, Json(json!(config))))
}

pub async fn update_mock(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<MockUpdateRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let repo = PostgresMockRepository::new(state.pool);
    let service = MockService::new(repo);

    let instance_id = Uuid::new_v4();
    let config = service.update(id, request, instance_id).await?;
    Ok(Json(json!(config)))
}

pub async fn delete_mock(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let repo = PostgresMockRepository::new(state.pool);
    let service = MockService::new(repo);

    service.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Environment Handlers
// ============================================================================

pub async fn list_environments(
    State(state): State<AppState>,
    Query(filter): Query<EnvironmentFilter>,
) -> Response {
    let repo = PostgresEnvironmentRepository::new(state.pool);
    let service = EnvironmentService::new(repo);

    let page = filter.page();
    let limit = filter.limit();

    match service.list(filter).await {
        Ok((envs, total)) => {
            let total_pages = (total + 19) / 20;
            Json(json!({
                "data": envs,
                "pagination": {
                    "page": page,
                    "limit": limit,
                    "total": total,
                    "total_pages": total_pages
                }
            })).into_response()
        }
        Err(e) => e.into_response(),
    }
}

pub async fn get_environment(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let repo = PostgresEnvironmentRepository::new(state.pool);
    let service = EnvironmentService::new(repo);

    let env = service.get(id).await?;
    Ok(Json(json!(env)))
}

pub async fn create_environment(
    State(state): State<AppState>,
    Json(request): Json<EnvironmentCreateRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let repo = PostgresEnvironmentRepository::new(state.pool);
    let service = EnvironmentService::new(repo);

    let env = service.create(request).await?;
    Ok((StatusCode::CREATED, Json(json!(env))))
}

pub async fn update_environment(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<EnvironmentUpdateRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let repo = PostgresEnvironmentRepository::new(state.pool);
    let service = EnvironmentService::new(repo);

    let env = service.update(id, request).await?;
    Ok(Json(json!(env)))
}

pub async fn delete_environment(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let repo = PostgresEnvironmentRepository::new(state.pool);
    let service = EnvironmentService::new(repo);

    service.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Instance Handlers
// ============================================================================

pub async fn list_instances(
    State(state): State<AppState>,
    Query(filter): Query<InstanceFilter>,
) -> Response {
    let repo = PostgresInstanceRepository::new(state.pool);
    let service = InstanceService::new(repo);

    let page = filter.page();
    let limit = filter.limit();

    match service.list(filter).await {
        Ok((instances, total)) => {
            let total_pages = (total + 19) / 20;
            Json(json!({
                "data": instances,
                "pagination": {
                    "page": page,
                    "limit": limit,
                    "total": total,
                    "total_pages": total_pages
                }
            })).into_response()
        }
        Err(e) => e.into_response(),
    }
}

pub async fn get_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let repo = PostgresInstanceRepository::new(state.pool);
    let service = InstanceService::new(repo);

    let instance = service.get(id).await?;
    Ok(Json(json!(instance)))
}

pub async fn register_instance(
    State(state): State<AppState>,
    Json(request): Json<InstanceRegisterRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let repo = PostgresInstanceRepository::new(state.pool);
    let service = InstanceService::new(repo);

    let instance = service.register(request).await?;
    Ok((StatusCode::CREATED, Json(json!(instance))))
}

pub async fn heartbeat(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<HeartbeatRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let repo = PostgresInstanceRepository::new(state.pool);
    let service = InstanceService::new(repo);

    let instance = service.heartbeat(id, request).await?;
    Ok(Json(json!(instance)))
}

pub async fn delete_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let repo = PostgresInstanceRepository::new(state.pool);
    let service = InstanceService::new(repo);

    service.unregister(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// Sync Handlers
// ============================================================================

pub async fn sync_pull(
    State(state): State<AppState>,
    Json(request): Json<serde_json::Value>,
) -> Response {
    let repo = PostgresMockRepository::new(state.pool);
    let service = MockService::new(repo);

    let since = request.get("since")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let filter = MockFilter::default();

    match service.find_modified_since(since.unwrap_or(chrono::DateTime::UNIX_EPOCH), filter).await {
        Ok(configs) => {
            Json(json!({
                "configs": configs,
                "deleted_ids": [],
                "server_time": chrono::Utc::now().to_rfc3339()
            })).into_response()
        }
        Err(e) => e.into_response(),
    }
}

pub async fn sync_push(
    State(state): State<AppState>,
    Json(request): Json<serde_json::Value>,
) -> Response {
    let repo = PostgresMockRepository::new(state.pool);
    let service = MockService::new(repo);

    let mut accepted = Vec::new();
    let mut conflicts = Vec::new();

    if let Some(configs) = request.get("configs").and_then(|v| v.as_array()) {
        for config_value in configs {
            if let Ok(config) = serde_json::from_value::<crate::models::MockConfiguration>(config_value.clone()) {
                match service.get(config.id).await {
                    Ok(existing) => {
                        if existing.version_vector.is_concurrent_with(&config.version_vector) {
                            conflicts.push(json!({
                                "id": config.id,
                                "reason": "concurrent_modification",
                                "local": config,
                                "central": existing
                            }));
                        } else {
                            match service.save(&config).await {
                                Ok(()) => accepted.push(config.id),
                                Err(e) => tracing::error!("Failed to save config: {}", e),
                            }
                        }
                    }
                    Err(_) => {
                        match service.save(&config).await {
                            Ok(()) => accepted.push(config.id),
                            Err(e) => tracing::error!("Failed to save new config: {}", e),
                        }
                    }
                }
            }
        }
    }

    Json(json!({
        "accepted": accepted,
        "conflicts": conflicts
    })).into_response()
}

pub async fn list_conflicts(
    State(_state): State<AppState>,
) -> impl IntoResponse {
    Json(json!({
        "data": []
    }))
}

pub async fn resolve_conflict(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let repo = PostgresMockRepository::new(state.pool);
    let service = MockService::new(repo);

    let strategy = request.get("strategy")
        .and_then(|v| v.as_str())
        .unwrap_or("keep_central");

    let resolution = request.get("resolution")
        .cloned()
        .unwrap_or(json!({}));

    let config = match strategy {
        "keep_local" => {
            let local = request.get("local")
                .ok_or_else(|| ApiError::BadRequest("Missing local version".to_string()))?;
            let mut config: crate::models::MockConfiguration = serde_json::from_value(local.clone())?;
            config.version_vector.increment(Uuid::new_v4());
            service.save(&config).await?;
            config
        }
        "keep_central" => {
            service.get(id).await?
        }
        "merge" => {
            let mut config = service.get(id).await?;
            if let Some(name) = resolution.get("name").and_then(|v| v.as_str()) {
                config.name = name.to_string();
            }
            if let Some(matching_rules) = resolution.get("matching_rules") {
                config.matching_rules = serde_json::from_value(matching_rules.clone())?;
            }
            if let Some(response_config) = resolution.get("response_config") {
                config.response_config = serde_json::from_value(response_config.clone())?;
            }
            config.version_vector.increment(Uuid::new_v4());
            service.save(&config).await?;
            config
        }
        _ => return Err(ApiError::BadRequest(format!("Unknown resolution strategy: {}", strategy))),
    };

    Ok(Json(json!(config)))
}

// ============================================================================
// Import/Export Handlers
// ============================================================================

pub async fn import_mocks(
    State(state): State<AppState>,
    Json(request): Json<serde_json::Value>,
) -> impl IntoResponse {
    let repo = PostgresMockRepository::new(state.pool);
    let service = MockService::new(repo);

    let mut imported = 0;
    let mut skipped = 0;
    let mut errors = Vec::new();

    let configs = request.get("configs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    for config_value in configs {
        match serde_json::from_value::<crate::models::MockConfiguration>(config_value) {
            Ok(config) => {
                match service.save(&config).await {
                    Ok(()) => imported += 1,
                    Err(_) => skipped += 1,
                }
            }
            Err(e) => {
                errors.push(e.to_string());
                skipped += 1;
            }
        }
    }

    Json(json!({
        "imported": imported,
        "skipped": skipped,
        "errors": errors
    }))
}

pub async fn export_mocks(
    State(state): State<AppState>,
    Query(params): Query<serde_json::Value>,
) -> Response {
    let repo = PostgresMockRepository::new(state.pool);
    let service = MockService::new(repo);

    let filter = MockFilter {
        environment: params.get("environment").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()),
        team: params.get("team").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()),
        ..Default::default()
    };

    match service.list(filter).await {
        Ok((configs, _)) => {
            Json(json!({
                "version": "1.0",
                "exported_at": chrono::Utc::now().to_rfc3339(),
                "configs": configs
            })).into_response()
        }
        Err(e) => e.into_response(),
    }
}

// ============================================================================
// Analytics Handlers
// ============================================================================

pub async fn get_analytics_stats(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let mock_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mock_configurations")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);

    let instance_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mystiproxy_instances")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);

    let env_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM environments")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);

    Json(json!({
        "total_mocks": mock_count,
        "total_instances": instance_count,
        "total_environments": env_count,
        "active_instances": instance_count,
    }))
}

pub async fn get_mock_analytics(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Response {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM mock_configurations WHERE id = $1)"
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(false);

    if !exists {
        return ApiError::NotFound(format!("Mock with id {} not found", id)).into_response();
    }

    let total_requests: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM analytics_records WHERE mock_id = $1"
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    let avg_response_time: Option<f64> = sqlx::query_scalar(
        "SELECT AVG(response_time_ms) FROM analytics_records WHERE mock_id = $1"
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(None);

    Json(json!({
        "mock_id": id,
        "total_requests": total_requests,
        "average_response_time_ms": avg_response_time,
        "requests_by_status": {},
        "requests_over_time": []
    })).into_response()
}
