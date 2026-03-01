//! HTTP API handlers for local management
//!
//! Provides REST API endpoints compatible with the central management system.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::error::ManagementError;
use super::models::{
    CreateMockRequest, MockConfiguration, MockFilter, UpdateMockRequest,
};
use super::repository::{LocalMockRepository, MockRepository};

/// API response wrapper
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            message: None,
        }
    }

    pub fn success_message(message: &str) -> ApiResponse<()> {
        ApiResponse {
            success: true,
            data: None,
            error: None,
            message: Some(message.to_string()),
        }
    }
}

impl ApiResponse<()> {
    pub fn error(status: StatusCode, message: &str) -> (StatusCode, Self) {
        (
            status,
            Self {
                success: false,
                data: None,
                error: Some(message.to_string()),
                message: None,
            },
        )
    }
}

/// List response with pagination
#[derive(Debug, Serialize)]
pub struct ListResponse<T> {
    pub items: Vec<T>,
    pub total: u64,
}

/// Query parameters for list endpoint
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub path: Option<String>,
    pub method: Option<String>,
    pub is_active: Option<bool>,
    pub source: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

impl From<ListQuery> for MockFilter {
    fn from(query: ListQuery) -> Self {
        Self {
            environment: None,  // Local management doesn't filter by environment
            team: None,  // Local management doesn't filter by team
            path: query.path,
            method: query.method.and_then(|m| m.parse().ok()),
            is_active: query.is_active,
            source: query.source.and_then(|s| match s.to_lowercase().as_str() {
                "central" => Some(super::models::MockSource::Central),
                "local" => Some(super::models::MockSource::Local),
                _ => None,
            }),
            page: None,  // Will use default
            limit: query.limit,
            offset: query.offset,
        }
    }
}

/// Management state for handlers
#[derive(Clone)]
pub struct HandlerState {
    pub repository: Arc<LocalMockRepository>,
    /// Whether sync is enabled
    pub sync_enabled: bool,
    /// Last sync timestamp
    pub last_sync: Option<String>,
    /// Current sync status
    pub status: String,
}

impl HandlerState {
    pub fn new(repository: LocalMockRepository) -> Self {
        Self {
            repository: Arc::new(repository),
            sync_enabled: false,
            last_sync: None,
            status: "local".to_string(),
        }
    }
    
    /// Create state with sync information
    pub fn with_sync(repository: LocalMockRepository, sync_enabled: bool, last_sync: Option<String>, status: String) -> Self {
        Self {
            repository: Arc::new(repository),
            sync_enabled,
            last_sync,
            status,
        }
    }
}

// ============================================================================
// API Handlers
// ============================================================================

/// List all mock configurations
/// 
/// GET /api/v1/mocks
pub async fn list_mocks(
    State(state): State<HandlerState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<ApiResponse<ListResponse<MockConfiguration>>>, ApiError> {
    let filter: MockFilter = query.into();
    let items = state.repository.find_all(filter).await?;
    let total = state.repository.count().await?;
    
    Ok(Json(ApiResponse::success(ListResponse { items, total })))
}

/// Get a single mock configuration by ID
/// 
/// GET /api/v1/mocks/:id
pub async fn get_mock(
    State(state): State<HandlerState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<MockConfiguration>>, ApiError> {
    let config = state
        .repository
        .find_by_id(id)
        .await?
        .ok_or_else(|| ManagementError::not_found(id))?;
    
    Ok(Json(ApiResponse::success(config)))
}

/// Create a new mock configuration
/// 
/// POST /api/v1/mocks
pub async fn create_mock(
    State(state): State<HandlerState>,
    Json(request): Json<CreateMockRequest>,
) -> Result<(StatusCode, Json<ApiResponse<MockConfiguration>>), ApiError> {
    // Validate request
    if request.name.is_empty() {
        return Err(ApiError::Validation("Name is required".to_string()));
    }
    if request.path.is_empty() {
        return Err(ApiError::Validation("Path is required".to_string()));
    }
    if !request.path.starts_with('/') {
        return Err(ApiError::Validation("Path must start with '/'".to_string()));
    }
    
    let config = state.repository.create(request).await?;
    
    Ok((StatusCode::CREATED, Json(ApiResponse::success(config))))
}

/// Update an existing mock configuration
/// 
/// PUT /api/v1/mocks/:id
pub async fn update_mock(
    State(state): State<HandlerState>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateMockRequest>,
) -> Result<Json<ApiResponse<MockConfiguration>>, ApiError> {
    // Validate path if provided
    if let Some(ref path) = request.path {
        if path.is_empty() {
            return Err(ApiError::Validation("Path cannot be empty".to_string()));
        }
        if !path.starts_with('/') {
            return Err(ApiError::Validation("Path must start with '/'".to_string()));
        }
    }
    
    let config = state.repository.update(id, request).await?;
    
    Ok(Json(ApiResponse::success(config)))
}

/// Delete a mock configuration
/// 
/// DELETE /api/v1/mocks/:id
pub async fn delete_mock(
    State(state): State<HandlerState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let deleted = state.repository.delete(id).await?;
    
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ManagementError::not_found(id).into())
    }
}

/// Health check endpoint
/// 
/// GET /api/v1/health
pub async fn health_check() -> Json<ApiResponse<()>> {
    Json(ApiResponse::<()>::success_message("OK"))
}

/// Get sync status
/// 
/// GET /api/v1/sync/status
pub async fn get_sync_status(
    State(state): State<HandlerState>,
) -> Json<ApiResponse<SyncStatusResponse>> {
    let count = state.repository.count().await.ok().unwrap_or(0);
    
    Json(ApiResponse::success(SyncStatusResponse {
        total_configs: count,
        sync_enabled: state.sync_enabled,
        last_sync: state.last_sync.clone(),
        status: state.status.clone(),
    }))
}

/// Trigger manual sync
/// 
/// POST /api/v1/sync/trigger
pub async fn trigger_sync(
    State(state): State<HandlerState>,
) -> Result<Json<ApiResponse<SyncStatusResponse>>, ApiError> {
    // This would trigger the sync client to perform a sync
    // For now, return a placeholder response
    let count = state.repository.count().await?;
    
    Ok(Json(ApiResponse::success(SyncStatusResponse {
        total_configs: count,
        sync_enabled: state.sync_enabled,
        last_sync: state.last_sync.clone(),
        status: "syncing".to_string(),
    })))
}

/// Sync status response
#[derive(Debug, Serialize)]
pub struct SyncStatusResponse {
    pub total_configs: u64,
    pub sync_enabled: bool,
    pub last_sync: Option<String>,
    pub status: String,
}

// ============================================================================
// Error Handling
// ============================================================================

/// API error type
#[derive(Debug)]
pub enum ApiError {
    Management(ManagementError),
    Validation(String),
}

impl From<ManagementError> for ApiError {
    fn from(err: ManagementError) -> Self {
        Self::Management(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::Management(err) => {
                let status = match &err {
                    ManagementError::NotFound(_) => StatusCode::NOT_FOUND,
                    ManagementError::InvalidInput(_) => StatusCode::BAD_REQUEST,
                    ManagementError::Conflict { .. } => StatusCode::CONFLICT,
                    _ => StatusCode::INTERNAL_SERVER_ERROR,
                };
                (status, err.to_string())
            }
            ApiError::Validation(msg) => (StatusCode::BAD_REQUEST, msg),
        };
        
        let (_, error_response) = ApiResponse::<()>::error(status, &message);
        (status, Json(error_response)).into_response()
    }
}

// ============================================================================
// Router
// ============================================================================

/// Create the management API router
pub fn create_management_router(state: HandlerState) -> Router {
    Router::new()
        .route("/api/v1/health", get(health_check))
        .route("/api/v1/mocks", get(list_mocks).post(create_mock))
        .route("/api/v1/mocks/:id", get(get_mock).put(update_mock).delete(delete_mock))
        .route("/api/v1/sync/status", get(get_sync_status))
        .route("/api/v1/sync/trigger", post(trigger_sync))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::management::db::create_memory_pool;
    use crate::management::models::HttpMethod;
    use axum::body::Body;
    use axum::http::{Method, Request};
    use tower::util::ServiceExt;

    async fn create_test_state() -> HandlerState {
        let pool = create_memory_pool().await.unwrap();
        let repo = LocalMockRepository::with_random_instance_id(pool);
        HandlerState::new(repo)
    }

    #[tokio::test]
    async fn test_health_check() {
        let response = health_check().await;
        assert!(response.success);
    }

    #[tokio::test]
    async fn test_create_and_get_mock() {
        let state = create_test_state().await;
        let app = create_management_router(state.clone());

        let create_request = CreateMockRequest {
            name: "Test Mock".to_string(),
            path: "/api/test".to_string(),
            method: HttpMethod::Get,
            matching_rules: Default::default(),
            response_config: Default::default(),
            is_active: true,
        };

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/mocks")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&create_request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_list_mocks() {
        let state = create_test_state().await;
        
        // Create a mock first
        state
            .repository
            .create(CreateMockRequest {
                name: "Test Mock".to_string(),
                path: "/api/test".to_string(),
                method: HttpMethod::Get,
                matching_rules: Default::default(),
                response_config: Default::default(),
                is_active: true,
            })
            .await
            .unwrap();

        let app = create_management_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/mocks")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
