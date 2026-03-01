//! Authentication middleware for Axum
//!
//! Provides JWT-based authentication middleware for protecting API endpoints.

use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
};

/// Authentication middleware
/// 
/// Validates JWT token from Authorization header.
/// Currently a placeholder - authentication is handled at the API gateway level.
#[allow(dead_code)]
pub async fn auth_middleware(
    request: Request,
    next: Next,
) -> Response {
    next.run(request).await
}
