//! Auth handlers: login and logout.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Deserialize;

use crate::error::ApiError;
use crate::handlers::AppState;
use crate::services::auth_service::AuthService;
use crate::services::user_repository::{PostgresUserRepository, UserRepository};

/// Login request body
#[derive(Debug, Deserialize)]
pub struct LoginBody {
    pub username: String,
    pub password: String,
}

/// POST /api/v1/auth/login
pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginBody>,
) -> Result<impl IntoResponse, ApiError> {
    let repo = PostgresUserRepository::new(state.pool.clone());

    // Anti-enumeration: identical error for unknown user and wrong password.
    const INVALID: &str = "Invalid username or password";

    let user = repo
        .find_by_username(&body.username)
        .await?
        .ok_or_else(|| ApiError::Unauthorized(INVALID.to_string()))?;

    if !AuthService::verify_password(&body.password, &user.password_hash) {
        return Err(ApiError::Unauthorized(INVALID.to_string()));
    }

    repo.update_last_login(user.id).await?;

    let response = state.auth_service.generate_token(&user)?;
    Ok(Json(serde_json::to_value(&response).unwrap_or_default()))
}

/// POST /api/v1/auth/logout
///
/// JWTs are stateless; the client discards the token. Endpoint kept for
/// frontend compatibility and future blacklist support.
pub async fn logout() -> StatusCode {
    StatusCode::NO_CONTENT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_login_body_deserializes() {
        let body: LoginBody = serde_json::from_str(r#"{"username":"a","password":"b"}"#).unwrap();
        assert_eq!(body.username, "a");
        assert_eq!(body.password, "b");
    }

    #[test]
    fn test_login_body_missing_field_fails() {
        assert!(serde_json::from_str::<LoginBody>(r#"{"username":"a"}"#).is_err());
    }

    #[test]
    fn test_login_body_extra_fields_ignored() {
        let body: LoginBody =
            serde_json::from_str(r#"{"username":"a","password":"b","extra":1}"#).unwrap();
        assert_eq!(body.username, "a");
    }
}
