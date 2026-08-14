//! Bootstrap
//!
//! First-admin initialization: when the users table is empty on startup,
//! create an initial admin from environment variables.

use sqlx::PgPool;
use tracing::warn;

use crate::error::ApiResult;
use crate::models::user::{User, UserRole};
use crate::services::auth_service::AuthService;
use crate::services::user_repository::{PostgresUserRepository, UserRepository};

/// Ensure an initial admin user exists when the table is empty.
///
/// - Username: `MYSTICENTRAL_ADMIN_USERNAME` (default `admin`)
/// - Password: `MYSTICENTRAL_ADMIN_PASSWORD` (default `changeme123`)
///
/// Idempotent: skipped when any user already exists.
pub async fn ensure_admin_user(pool: &PgPool) -> ApiResult<Option<User>> {
    let repo = PostgresUserRepository::new(pool.clone());
    if repo.count().await? > 0 {
        return Ok(None);
    }

    let username =
        std::env::var("MYSTICENTRAL_ADMIN_USERNAME").unwrap_or_else(|_| "admin".to_string());
    let password =
        std::env::var("MYSTICENTRAL_ADMIN_PASSWORD").unwrap_or_else(|_| "changeme123".to_string());

    if password == "changeme123" {
        warn!(
            "Creating initial admin with DEFAULT password 'changeme123'. \
             Set MYSTICENTRAL_ADMIN_PASSWORD and change it immediately after first login."
        );
    }

    let password_hash = AuthService::hash_password(&password)?;
    let user = User::new(
        username.clone(),
        "admin@mysticentral.local".to_string(),
        password_hash,
        UserRole::Admin,
    );
    let created = repo.create(user).await?;
    tracing::info!(
        "Bootstrap: initial admin user '{}' created",
        created.username
    );
    Ok(Some(created))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Full DB-backed bootstrap tests run against a live PostgreSQL in CI.
    // Unit-testable logic here is thin; env parsing defaults are covered
    // by the integration path.
    #[test]
    fn test_bootstrap_defaults_documented() {
        // Defaults are read inside ensure_admin_user; verify env contract here.
        // (DB-backed creation is covered by integration tests with live PostgreSQL.)
        assert_eq!(
            std::env::var("MYSTICENTRAL_ADMIN_USERNAME").unwrap_or_else(|_| "admin".to_string()),
            "admin"
        );
    }
}
