//! User management handlers (JWT-protected).

use axum::{extract::Path, extract::State, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;
use uuid::Uuid;

use crate::error::ApiError;
use crate::handlers::AppState;
use crate::middleware::auth::AuthContext;
use crate::models::user::{
    ChangePasswordRequest, User, UserCreateRequest, UserRole, UserUpdateRequest,
};
use crate::services::auth_service::AuthService;
use crate::services::user_repository::{user_public_json, PostgresUserRepository, UserRepository};

/// GET /api/v1/users (admin/editor)
pub async fn list_users(
    auth: AuthContext,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&auth, |r| matches!(r, UserRole::Admin | UserRole::Editor))?;

    let repo = PostgresUserRepository::new(state.pool.clone());
    let users = repo.find_all().await?;
    let total = users.len();
    let data: Vec<_> = users.iter().map(user_public_json).collect();
    Ok(Json(json!({
        "data": data,
        "pagination": {"page": 1, "limit": 20, "total": total, "total_pages": 1}
    })))
}

/// POST /api/v1/users (admin)
pub async fn create_user(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<UserCreateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&auth, |r| matches!(r, UserRole::Admin))?;

    if body.password.len() < 8 {
        return Err(ApiError::BadRequest(
            "Password must be at least 8 characters".to_string(),
        ));
    }

    let repo = PostgresUserRepository::new(state.pool.clone());
    let hash = AuthService::hash_password(&body.password)?;
    let user = User::new(body.username, body.email, hash, body.role);
    let created = repo.create(user).await?;
    Ok((StatusCode::CREATED, Json(user_public_json(&created))))
}

/// GET /api/v1/users/me (any authenticated user)
pub async fn get_current_user(auth: AuthContext) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(user_public_json(&auth.user)))
}

/// PUT /api/v1/users/me/password (any authenticated user)
pub async fn change_own_password(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if body.new_password.len() < 8 {
        return Err(ApiError::BadRequest(
            "New password must be at least 8 characters".to_string(),
        ));
    }
    if !AuthService::verify_password(&body.old_password, &auth.user.password_hash) {
        return Err(ApiError::BadRequest(
            "Old password is incorrect".to_string(),
        ));
    }

    let repo = PostgresUserRepository::new(state.pool.clone());
    let hash = AuthService::hash_password(&body.new_password)?;
    repo.update_password(auth.user.id, &hash).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/users/:id (admin/editor)
pub async fn get_user(
    auth: AuthContext,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&auth, |r| matches!(r, UserRole::Admin | UserRole::Editor))?;

    let repo = PostgresUserRepository::new(state.pool.clone());
    let user = repo
        .find_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("user {id} not found")))?;
    Ok(Json(user_public_json(&user)))
}

/// PUT /api/v1/users/:id (admin)
pub async fn update_user(
    auth: AuthContext,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UserUpdateRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&auth, |r| matches!(r, UserRole::Admin))?;

    // Anti-lockout: cannot change own role.
    if id == auth.user.id && body.role.is_some() && body.role != Some(auth.user.role) {
        return Err(ApiError::BadRequest(
            "Cannot change your own role".to_string(),
        ));
    }

    let repo = PostgresUserRepository::new(state.pool.clone());
    let mut user = repo
        .find_by_id(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("user {id} not found")))?;

    if let Some(email) = body.email {
        user.email = email;
    }
    if let Some(role) = body.role {
        user.role = role;
    }
    let updated = repo.update(user).await?;
    Ok(Json(user_public_json(&updated)))
}

/// DELETE /api/v1/users/:id (admin)
pub async fn delete_user(
    auth: AuthContext,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&auth, |r| matches!(r, UserRole::Admin))?;

    // Anti-lockout: cannot delete self.
    if id == auth.user.id {
        return Err(ApiError::BadRequest("Cannot delete yourself".to_string()));
    }

    let repo = PostgresUserRepository::new(state.pool.clone());
    repo.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Role gate helper
fn require_role(auth: &AuthContext, allowed: fn(&UserRole) -> bool) -> Result<(), ApiError> {
    if allowed(&auth.user.role) {
        Ok(())
    } else {
        Err(ApiError::Forbidden("Insufficient permissions".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn admin() -> AuthContext {
        AuthContext {
            user: User::new(
                "admin".into(),
                "a@t.local".into(),
                "h".into(),
                UserRole::Admin,
            ),
        }
    }

    fn viewer() -> AuthContext {
        AuthContext {
            user: User::new("v".into(), "v@t.local".into(), "h".into(), UserRole::Viewer),
        }
    }

    #[test]
    fn test_viewer_cannot_list() {
        assert!(matches!(
            require_role(&viewer(), |r| matches!(
                r,
                UserRole::Admin | UserRole::Editor
            )),
            Err(ApiError::Forbidden(_))
        ));
    }

    #[test]
    fn test_admin_can_list() {
        assert!(require_role(&admin(), |r| matches!(
            r,
            UserRole::Admin | UserRole::Editor
        ))
        .is_ok());
    }

    #[test]
    fn test_editor_cannot_create() {
        let mut e = viewer();
        e.user.role = UserRole::Editor;
        assert!(matches!(
            require_role(&e, |r| matches!(r, UserRole::Admin)),
            Err(ApiError::Forbidden(_))
        ));
    }
}
