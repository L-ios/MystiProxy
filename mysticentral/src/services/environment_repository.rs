//! Environment Repository
//!
//! Provides data access for environments.

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::ApiError;
use crate::models::{Environment, EnvironmentFilter};

/// Repository trait for environment persistence
#[async_trait]
pub trait EnvironmentRepository: Send + Sync {
    /// Find an environment by ID
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Environment>, ApiError>;

    /// Find all environments matching the filter
    async fn find_all(&self, filter: EnvironmentFilter) -> Result<Vec<Environment>, ApiError>;

    /// Save an environment (create or update)
    async fn save(&self, env: &Environment) -> Result<(), ApiError>;

    /// Delete an environment by ID
    async fn delete(&self, id: Uuid) -> Result<(), ApiError>;

    /// Count total environments
    async fn count(&self, filter: &EnvironmentFilter) -> Result<u32, ApiError>;
}

/// PostgreSQL implementation of EnvironmentRepository
pub struct PostgresEnvironmentRepository {
    pool: PgPool,
}

impl PostgresEnvironmentRepository {
    /// Create a new PostgreSQL repository
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EnvironmentRepository for PostgresEnvironmentRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Environment>, ApiError> {
        let row = sqlx::query_as::<_, EnvironmentRow>(
            r#"
            SELECT id, name, description, endpoints, is_template, template_id, created_at, updated_at
            FROM environments
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some(r.into_env())),
            None => Ok(None),
        }
    }

    async fn find_all(&self, filter: EnvironmentFilter) -> Result<Vec<Environment>, ApiError> {
        let offset = filter.offset() as i64;
        let limit = filter.limit() as i64;

        let rows = sqlx::query_as::<_, EnvironmentRow>(
            r#"
            SELECT id, name, description, endpoints, is_template, template_id, created_at, updated_at
            FROM environments
            WHERE ($1::bool IS NULL OR is_template = $1)
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(filter.is_template)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into_env()).collect())
    }

    async fn save(&self, env: &Environment) -> Result<(), ApiError> {
        let endpoints = serde_json::to_value(&env.endpoints)?;

        sqlx::query(
            r#"
            INSERT INTO environments (
                id, name, description, endpoints, is_template, template_id, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                endpoints = EXCLUDED.endpoints,
                is_template = EXCLUDED.is_template,
                template_id = EXCLUDED.template_id,
                updated_at = EXCLUDED.updated_at
            "#,
        )
        .bind(env.id)
        .bind(&env.name)
        .bind(&env.description)
        .bind(&endpoints)
        .bind(env.is_template)
        .bind(env.template_id)
        .bind(env.created_at)
        .bind(env.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), ApiError> {
        let result = sqlx::query("DELETE FROM environments WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(ApiError::NotFound(format!(
                "Environment with id {} not found",
                id
            )));
        }

        Ok(())
    }

    async fn count(&self, filter: &EnvironmentFilter) -> Result<u32, ApiError> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint
            FROM environments
            WHERE ($1::bool IS NULL OR is_template = $1)
            "#,
        )
        .bind(filter.is_template)
        .fetch_one(&self.pool)
        .await?;

        Ok(count as u32)
    }
}

/// Database row representation
#[derive(sqlx::FromRow)]
struct EnvironmentRow {
    id: Uuid,
    name: String,
    description: Option<String>,
    endpoints: serde_json::Value,
    is_template: bool,
    template_id: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl EnvironmentRow {
    fn into_env(self) -> Environment {
        let endpoints: std::collections::HashMap<String, String> =
            serde_json::from_value(self.endpoints).unwrap_or_default();

        Environment {
            id: self.id,
            name: self.name,
            description: self.description,
            endpoints,
            is_template: self.is_template,
            template_id: self.template_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
