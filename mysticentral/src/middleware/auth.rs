//! Authentication middleware for Axum
//!
//! Validates JWT from `Authorization: Bearer <token>` and injects the
//! authenticated `User` into request extensions.

use crate::error::ApiError;
use crate::models::user::User;
use crate::services::auth_service::AuthService;
use crate::services::user_repository::{PostgresUserRepository, UserRepository};
use axum::{
    extract::{Extension, FromRequestParts, Request},
    http::{request::Parts, HeaderMap},
    middleware::Next,
    response::Response,
};

/// Auth context injected into handlers after middleware runs
#[derive(Clone)]
pub struct AuthContext {
    pub user: User,
}

impl AuthContext {
    /// Convenience accessor for the authenticated user id
    #[allow(dead_code)]
    pub fn user_id(&self) -> uuid::Uuid {
        self.user.id
    }
}

/// Axum extractor: pull the injected AuthContext from request extensions.
#[axum::async_trait]
impl<S: Send + Sync> FromRequestParts<S> for AuthContext {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthContext>()
            .cloned()
            .ok_or_else(|| ApiError::Unauthorized("Not authenticated".to_string()))
    }
}

/// Extract the bearer token from headers
pub fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let prefix = "Bearer ";
    if value.len() > prefix.len() && value[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(value[prefix.len()..].to_string())
    } else {
        None
    }
}

/// Authentication middleware
pub async fn auth_middleware(
    Extension(state): Extension<AuthMiddlewareState>,
    headers: HeaderMap,
    mut request: Request,
    next: Next,
) -> Response {
    match authenticate(&state, &headers).await {
        Ok(user) => {
            request.extensions_mut().insert(AuthContext { user });
            next.run(request).await
        }
        Err(err) => err.into_response(),
    }
}

/// State required by the auth middleware
#[derive(Clone)]
pub struct AuthMiddlewareState {
    pub auth_service: AuthService,
    pub pool: sqlx::PgPool,
}

/// Verify the request and load the current user
pub async fn authenticate(
    state: &AuthMiddlewareState,
    headers: &HeaderMap,
) -> Result<User, ApiError> {
    let token = extract_bearer_token(headers)
        .ok_or_else(|| ApiError::Unauthorized("Missing bearer token".to_string()))?;

    let claims = state.auth_service.validate_token(&token)?;

    let repo = PostgresUserRepository::new(state.pool.clone());
    let user = repo
        .find_by_id(claims.sub)
        .await?
        .ok_or_else(|| ApiError::Unauthorized("User no longer exists".to_string()))?;
    Ok(user)
}

use axum::response::IntoResponse;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_str(value).unwrap(),
        );
        h
    }

    #[test]
    fn test_extract_bearer_valid() {
        let token = extract_bearer_token(&headers_with("Bearer abc123"));
        assert_eq!(token.as_deref(), Some("abc123"));
    }

    #[test]
    fn test_extract_bearer_case_insensitive_prefix() {
        let token = extract_bearer_token(&headers_with("bearer abc"));
        assert_eq!(token.as_deref(), Some("abc"));
    }

    #[test]
    fn test_extract_bearer_missing_header() {
        assert!(extract_bearer_token(&HeaderMap::new()).is_none());
    }

    #[test]
    fn test_extract_bearer_wrong_scheme() {
        assert!(extract_bearer_token(&headers_with("Basic abc")).is_none());
    }

    #[test]
    fn test_extract_bearer_empty_token() {
        assert!(extract_bearer_token(&headers_with("Bearer ")).is_none());
    }
}
