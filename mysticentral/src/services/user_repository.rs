//! User Repository
//!
//! PostgreSQL-backed persistence for users.

use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::ApiError;
use crate::models::user::User;

/// Map unique-constraint violations to Conflict; others pass through as Database
pub(crate) fn map_db_err(err: sqlx::Error) -> ApiError {
    match &err {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            ApiError::Conflict("username or email already exists".to_string())
        }
        sqlx::Error::RowNotFound => ApiError::NotFound("not found".to_string()),
        _ => ApiError::Database(err),
    }
}

/// Trait for user persistence
#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn create(&self, user: User) -> Result<User, ApiError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, ApiError>;
    async fn find_by_username(&self, username: &str) -> Result<Option<User>, ApiError>;
    async fn find_all(&self) -> Result<Vec<User>, ApiError>;
    async fn update(&self, user: User) -> Result<User, ApiError>;
    async fn delete(&self, id: Uuid) -> Result<(), ApiError>;
    async fn update_password(&self, id: Uuid, password_hash: &str) -> Result<(), ApiError>;
    async fn update_last_login(&self, id: Uuid) -> Result<(), ApiError>;
    async fn count(&self) -> Result<i64, ApiError>;
}

/// Row representation for sqlx
#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    username: String,
    email: String,
    password_hash: String,
    role: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    last_login_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<UserRow> for User {
    fn from(r: UserRow) -> Self {
        User {
            id: r.id,
            username: r.username,
            email: r.email,
            password_hash: r.password_hash,
            role: r.role.parse().unwrap_or_default(),
            created_at: r.created_at,
            updated_at: r.updated_at,
            last_login_at: r.last_login_at,
        }
    }
}

const COLS: &str =
    "id, username, email, password_hash, role, created_at, updated_at, last_login_at";

/// PostgreSQL implementation
pub struct PostgresUserRepository {
    pool: PgPool,
}

impl PostgresUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepository for PostgresUserRepository {
    async fn create(&self, user: User) -> Result<User, ApiError> {
        let row = sqlx::query_as::<_, UserRow>(
            r#"
            INSERT INTO users (id, username, email, password_hash, role, created_at, updated_at, last_login_at)
            VALUES ($1, $2, $3, $4, $5, $6, $6, NULL)
            RETURNING id, username, email, password_hash, role, created_at, updated_at, last_login_at
            "#,
        )
        .bind(user.id)
        .bind(&user.username)
        .bind(&user.email)
        .bind(&user.password_hash)
        .bind(user.role.to_string())
        .bind(user.created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(map_db_err)?;
        Ok(row.into())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, ApiError> {
        let sql = format!("SELECT {COLS} FROM users WHERE id = $1");
        let row = sqlx::query_as::<_, UserRow>(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_db_err)?;
        Ok(row.map(Into::into))
    }

    async fn find_by_username(&self, username: &str) -> Result<Option<User>, ApiError> {
        let sql = format!("SELECT {COLS} FROM users WHERE username = $1");
        let row = sqlx::query_as::<_, UserRow>(&sql)
            .bind(username)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_db_err)?;
        Ok(row.map(Into::into))
    }

    async fn find_all(&self) -> Result<Vec<User>, ApiError> {
        let sql = format!("SELECT {COLS} FROM users ORDER BY created_at");
        let rows = sqlx::query_as::<_, UserRow>(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(map_db_err)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn update(&self, user: User) -> Result<User, ApiError> {
        let sql = format!(
            r#"
            UPDATE users
            SET email = $2, role = $3, updated_at = $4
            WHERE id = $1
            RETURNING {COLS}
            "#
        );
        let row = sqlx::query_as::<_, UserRow>(&sql)
            .bind(user.id)
            .bind(&user.email)
            .bind(user.role.to_string())
            .bind(Utc::now())
            .fetch_optional(&self.pool)
            .await
            .map_err(map_db_err)?;
        row.map(Into::into)
            .ok_or_else(|| ApiError::NotFound(format!("user {} not found", user.id)))
    }

    async fn delete(&self, id: Uuid) -> Result<(), ApiError> {
        let result = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(map_db_err)?;
        if result.rows_affected() == 0 {
            return Err(ApiError::NotFound(format!("user {id} not found")));
        }
        Ok(())
    }

    async fn update_password(&self, id: Uuid, password_hash: &str) -> Result<(), ApiError> {
        sqlx::query("UPDATE users SET password_hash = $2, updated_at = $3 WHERE id = $1")
            .bind(id)
            .bind(password_hash)
            .bind(Utc::now())
            .execute(&self.pool)
            .await
            .map_err(map_db_err)?;
        Ok(())
    }

    async fn update_last_login(&self, id: Uuid) -> Result<(), ApiError> {
        sqlx::query("UPDATE users SET last_login_at = $2 WHERE id = $1")
            .bind(id)
            .bind(Utc::now())
            .execute(&self.pool)
            .await
            .map_err(map_db_err)?;
        Ok(())
    }

    async fn count(&self) -> Result<i64, ApiError> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await
            .map_err(map_db_err)?;
        Ok(count)
    }
}

/// Serialize user for API responses (no password hash)
pub fn user_public_json(u: &User) -> serde_json::Value {
    serde_json::json!({
        "id": u.id,
        "username": u.username,
        "email": u.email,
        "role": u.role.to_string(),
        "created_at": u.created_at,
        "updated_at": u.updated_at,
        "last_login_at": u.last_login_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::user::UserRole;

    fn sample_user(username: &str) -> User {
        User::new(
            username.to_string(),
            format!("{username}@test.local"),
            "hash".to_string(),
            UserRole::Viewer,
        )
    }

    #[test]
    fn test_user_public_json_hides_password() {
        let u = sample_user("alice");
        let j = user_public_json(&u);
        assert!(j.get("password_hash").is_none());
        assert_eq!(j["username"], "alice");
        assert!(j["last_login_at"].is_null());
        assert_eq!(j["role"], "viewer");
    }

    #[test]
    fn test_non_unique_error_maps_to_database_variant() {
        let err = sqlx::Error::from(std::io::Error::new(std::io::ErrorKind::Other, "io error"));
        assert!(matches!(map_db_err(err), ApiError::Database(_)));
    }

    #[test]
    fn test_row_not_found_maps_to_not_found() {
        let err = sqlx::Error::RowNotFound;
        assert!(matches!(map_db_err(err), ApiError::NotFound(_)));
    }

    #[test]
    fn test_user_row_conversion() {
        let row = UserRow {
            id: Uuid::new_v4(),
            username: "bob".to_string(),
            email: "bob@t.local".to_string(),
            password_hash: "h".to_string(),
            role: "admin".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_login_at: None,
        };
        let u: User = row.into();
        assert_eq!(u.role, UserRole::Admin);
        assert_eq!(u.username, "bob");
    }

    #[test]
    fn test_invalid_role_falls_back_to_viewer() {
        let row = UserRow {
            id: Uuid::new_v4(),
            username: "eve".to_string(),
            email: "eve@t.local".to_string(),
            password_hash: "h".to_string(),
            role: "garbage".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_login_at: None,
        };
        let u: User = row.into();
        assert_eq!(u.role, UserRole::Viewer);
    }

    #[test]
    fn test_user_new_defaults() {
        let u = sample_user("bob");
        assert_eq!(u.role, UserRole::Viewer);
        assert!(u.last_login_at.is_none());
    }
}
