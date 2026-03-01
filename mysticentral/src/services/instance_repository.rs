//! Instance Repository
//!
//! Provides data access for MystiProxy instances.

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::ApiError;
use crate::models::{InstanceFilter, MystiProxyInstance, SyncStatus};

/// Repository trait for instance persistence
#[async_trait]
pub trait InstanceRepository: Send + Sync {
    /// Find an instance by ID
    async fn find_by_id(&self, id: Uuid) -> Result<Option<MystiProxyInstance>, ApiError>;

    /// Find all instances matching the filter
    async fn find_all(&self, filter: InstanceFilter) -> Result<Vec<MystiProxyInstance>, ApiError>;

    /// Save an instance (create or update)
    async fn save(&self, instance: &MystiProxyInstance) -> Result<(), ApiError>;

    /// Delete an instance by ID
    async fn delete(&self, id: Uuid) -> Result<(), ApiError>;

    /// Count total instances
    async fn count(&self, filter: &InstanceFilter) -> Result<u32, ApiError>;

    /// Find instance by name
    async fn find_by_name(&self, name: &str) -> Result<Option<MystiProxyInstance>, ApiError>;
}

/// PostgreSQL implementation of InstanceRepository
pub struct PostgresInstanceRepository {
    pool: PgPool,
}

impl PostgresInstanceRepository {
    /// Create a new PostgreSQL repository
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl InstanceRepository for PostgresInstanceRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<MystiProxyInstance>, ApiError> {
        let row = sqlx::query_as::<_, InstanceRow>(
            r#"
            SELECT id, name, endpoint_url, api_key_hash, sync_status, last_sync_at,
                   config_checksum, registered_at, last_heartbeat
            FROM mystiproxy_instances
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some(r.into_instance()?)),
            None => Ok(None),
        }
    }

    async fn find_all(&self, filter: InstanceFilter) -> Result<Vec<MystiProxyInstance>, ApiError> {
        let offset = filter.offset() as i64;
        let limit = filter.limit() as i64;
        let status = filter.sync_status.map(|s| s.to_string());

        let rows = sqlx::query_as::<_, InstanceRow>(
            r#"
            SELECT id, name, endpoint_url, api_key_hash, sync_status, last_sync_at,
                   config_checksum, registered_at, last_heartbeat
            FROM mystiproxy_instances
            WHERE ($1::text IS NULL OR sync_status = $1)
            ORDER BY registered_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(status)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let instances: Result<Vec<MystiProxyInstance>, _> = rows
            .into_iter()
            .map(|r| r.into_instance())
            .collect();

        Ok(instances?)
    }

    async fn save(&self, instance: &MystiProxyInstance) -> Result<(), ApiError> {
        let status = instance.sync_status.to_string();

        sqlx::query(
            r#"
            INSERT INTO mystiproxy_instances (
                id, name, endpoint_url, api_key_hash, sync_status, last_sync_at,
                config_checksum, registered_at, last_heartbeat
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                endpoint_url = EXCLUDED.endpoint_url,
                api_key_hash = EXCLUDED.api_key_hash,
                sync_status = EXCLUDED.sync_status,
                last_sync_at = EXCLUDED.last_sync_at,
                config_checksum = EXCLUDED.config_checksum,
                last_heartbeat = EXCLUDED.last_heartbeat
            "#,
        )
        .bind(instance.id)
        .bind(&instance.name)
        .bind(&instance.endpoint_url)
        .bind(&instance.api_key_hash)
        .bind(&status)
        .bind(instance.last_sync_at)
        .bind(&instance.config_checksum)
        .bind(instance.registered_at)
        .bind(instance.last_heartbeat)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), ApiError> {
        let result = sqlx::query("DELETE FROM mystiproxy_instances WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(ApiError::NotFound(format!(
                "Instance with id {} not found",
                id
            )));
        }

        Ok(())
    }

    async fn count(&self, filter: &InstanceFilter) -> Result<u32, ApiError> {
        let status = filter.sync_status.as_ref().map(|s| s.to_string());

        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)::bigint
            FROM mystiproxy_instances
            WHERE ($1::text IS NULL OR sync_status = $1)
            "#,
        )
        .bind(status)
        .fetch_one(&self.pool)
        .await?;

        Ok(count as u32)
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<MystiProxyInstance>, ApiError> {
        let row = sqlx::query_as::<_, InstanceRow>(
            r#"
            SELECT id, name, endpoint_url, api_key_hash, sync_status, last_sync_at,
                   config_checksum, registered_at, last_heartbeat
            FROM mystiproxy_instances
            WHERE name = $1
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some(r.into_instance()?)),
            None => Ok(None),
        }
    }
}

/// Database row representation
#[derive(sqlx::FromRow)]
struct InstanceRow {
    id: Uuid,
    name: String,
    endpoint_url: String,
    api_key_hash: Option<String>,
    sync_status: String,
    last_sync_at: Option<chrono::DateTime<chrono::Utc>>,
    config_checksum: Option<String>,
    registered_at: chrono::DateTime<chrono::Utc>,
    last_heartbeat: Option<chrono::DateTime<chrono::Utc>>,
}

impl InstanceRow {
    fn into_instance(self) -> Result<MystiProxyInstance, ApiError> {
        let sync_status = self.sync_status.parse::<SyncStatus>()
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("Invalid sync status: {}", e)))?;

        Ok(MystiProxyInstance {
            id: self.id,
            name: self.name,
            endpoint_url: self.endpoint_url,
            api_key_hash: self.api_key_hash,
            sync_status,
            last_sync_at: self.last_sync_at,
            config_checksum: self.config_checksum,
            registered_at: self.registered_at,
            last_heartbeat: self.last_heartbeat,
        })
    }
}
