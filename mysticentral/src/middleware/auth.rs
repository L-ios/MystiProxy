//! Authentication middleware for Axum
//!
//! Provides JWT-based authentication middleware for protecting API endpoints.

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;

/// Authentication middleware
/// 
/// Validates JWT token from Authorization header.
/// Currently a placeholder - authentication is handled at the API gateway level.
pub async fn auth_middleware(
    request: Request,
    next: Next,
) -> Response {
    next.run(request).await
}

/// Create an unauthorized response
#[allow(dead_code)]
fn unauthorized_response(message: &str) -> (StatusCode, axum::Json<serde_json::Value>) {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(json!({
            "code": 401,
            "message": message
        })),
    )
}
