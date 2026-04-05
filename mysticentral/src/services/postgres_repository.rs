//! PostgreSQL implementation of MockRepository
//!
//! Provides persistent storage for mock configurations using PostgreSQL.

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::ApiError;
use crate::models::{HttpMethod, MockConfiguration, MockFilter, MockSource};
use crate::services::MockRepository;

/// PostgreSQL implementation of MockRepository
pub struct PostgresMockRepository {
    pool: PgPool,
}

impl PostgresMockRepository {
    /// Create a new PostgreSQL repository
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MockRepository for PostgresMockRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<MockConfiguration>, ApiError> {
        let row = sqlx::query_as::<_, MockConfigurationRow>(
            r#"
            SELECT id, name, path, method, team_id, environment_id,
                   matching_rules, response_config, state_config, source,
                   version_vector, content_hash, created_at, updated_at, created_by
            FROM mock_configurations
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some(r.into_config()?)),
            None => Ok(None),
        }
    }

    async fn find_all(&self, filter: MockFilter) -> Result<Vec<MockConfiguration>, ApiError> {
        let offset = filter.offset() as i64;
        let limit = filter.limit() as i64;

        let rows = sqlx::query_as::<_, MockConfigurationRow>(
            r#"
            SELECT id, name, path, method, team_id, environment_id,
                   matching_rules, response_config, state_config, source,
                   version_vector, content_hash, created_at, updated_at, created_by
            FROM mock_configurations
            WHERE ($1::uuid IS NULL OR environment_id = $1)
              AND ($2::uuid IS NULL OR team_id = $2)
              AND ($3::text IS NULL OR path LIKE '%' || $3 || '%')
              AND ($4::text IS NULL OR method = $4)
            ORDER BY created_at DESC
            LIMIT $5 OFFSET $6
            "#,
        )
        .bind(filter.environment)
        .bind(filter.team)
        .bind(filter.path.as_deref())
        .bind(filter.method.map(|m| m.to_string()))
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let configs: Result<Vec<MockConfiguration>, _> =
            rows.into_iter().map(|r| r.into_config()).collect();

        Ok(configs?)
    }

    async fn save(&self, config: &MockConfiguration) -> Result<(), ApiError> {
        let matching_rules = serde_json::to_value(&config.matching_rules)?;
        let response_config = serde_json::to_value(&config.response_config)?;
        let state_config = config
            .state_config
            .as_ref()
            .map(|s| serde_json::to_value(s))
            .transpose()?;
        let version_vector = serde_json::to_value(&config.version_vector)?;
        let source = match config.source {
            MockSource::Central => "central",
            MockSource::Local => "local",
        };

        sqlx::query(
            r#"
            INSERT INTO mock_configurations (
                id, name, path, method, team_id, environment_id,
                matching_rules, response_config, state_config, source,
                version_vector, content_hash, created_at, updated_at, created_by
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                path = EXCLUDED.path,
                method = EXCLUDED.method,
                team_id = EXCLUDED.team_id,
                environment_id = EXCLUDED.environment_id,
                matching_rules = EXCLUDED.matching_rules,
                response_config = EXCLUDED.response_config,
                state_config = EXCLUDED.state_config,
                source = EXCLUDED.source,
                version_vector = EXCLUDED.version_vector,
                content_hash = EXCLUDED.content_hash,
                updated_at = EXCLUDED.updated_at,
                created_by = EXCLUDED.created_by
            "#,
        )
        .bind(config.id)
        .bind(&config.name)
        .bind(&config.path)
        .bind(config.method.to_string())
        .bind(config.team_id)
        .bind(config.environment_id)
        .bind(&matching_rules)
        .bind(&response_config)
        .bind(&state_config)
        .bind(source)
        .bind(&version_vector)
        .bind(&config.content_hash)
        .bind(config.created_at)
        .bind(config.updated_at)
        .bind(config.created_by)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), ApiError> {
        let result = sqlx::query("DELETE FROM mock_configurations WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(ApiError::NotFound(format!(
                "Mock configuration with id {} not found",
                id
            )));
        }

        Ok(())
    }

    async fn count(&self, filter: &MockFilter) -> Result<u32, ApiError> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint
            FROM mock_configurations
            WHERE ($1::uuid IS NULL OR environment_id = $1)
              AND ($2::uuid IS NULL OR team_id = $2)
              AND ($3::text IS NULL OR path LIKE '%' || $3 || '%')
              AND ($4::text IS NULL OR method = $4)
            "#,
        )
        .bind(filter.environment)
        .bind(filter.team)
        .bind(filter.path.as_deref())
        .bind(filter.method.map(|m| m.to_string()))
        .fetch_one(&self.pool)
        .await?;

        Ok(count as u32)
    }
}

/// Database row representation
#[derive(sqlx::FromRow)]
struct MockConfigurationRow {
    id: Uuid,
    name: String,
    path: String,
    method: String,
    team_id: Option<Uuid>,
    environment_id: Option<Uuid>,
    matching_rules: serde_json::Value,
    response_config: serde_json::Value,
    state_config: Option<serde_json::Value>,
    source: String,
    version_vector: serde_json::Value,
    content_hash: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    created_by: Option<Uuid>,
}

impl MockConfigurationRow {
    fn into_config(self) -> Result<MockConfiguration, ApiError> {
        let source = match self.source.as_str() {
            "central" => MockSource::Central,
            "local" => MockSource::Local,
            _ => MockSource::Central,
        };

        let matching_rules: crate::models::MatchingRules =
            serde_json::from_value(self.matching_rules)?;
        let response_config: crate::models::ResponseConfig =
            serde_json::from_value(self.response_config)?;
        let state_config: Option<crate::models::StateConfig> = self
            .state_config
            .map(|v| serde_json::from_value(v))
            .transpose()?;
        let version_vector: crate::models::VersionVector =
            serde_json::from_value(self.version_vector)?;

        Ok(MockConfiguration {
            id: self.id,
            name: self.name,
            path: self.path,
            method: self.method.parse().unwrap_or(HttpMethod::Get),
            team_id: self.team_id,
            environment_id: self.environment_id,
            matching_rules,
            response_config,
            state_config,
            source,
            version_vector,
            content_hash: self.content_hash,
            created_at: self.created_at,
            updated_at: self.updated_at,
            created_by: self.created_by,
            is_active: true, // Default value for existing records
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repository_creation() {
        // This test would require a database connection
        // In real tests, use test containers or mock
    }
}
