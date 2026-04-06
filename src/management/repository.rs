//! Mock repository trait and SQLite implementation
//!
//! Provides the core data access layer for mock configurations.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use super::error::{ManagementError, Result};
use super::models::{
    CreateMockRequest, HttpMethod, MatchingRules, MockConfiguration, MockFilter, MockSource,
    ResponseConfig, UpdateMockRequest, VersionVector,
};

/// Repository trait for mock configuration storage
///
/// This trait defines the contract for mock configuration persistence,
/// allowing for different storage backends (SQLite, in-memory, etc.)
#[async_trait]
pub trait MockRepository: Send + Sync {
    /// Find a mock configuration by ID
    async fn find_by_id(&self, id: Uuid) -> Result<Option<MockConfiguration>>;

    /// Find all mock configurations matching the filter
    async fn find_all(&self, filter: MockFilter) -> Result<Vec<MockConfiguration>>;

    /// Find mock configurations by path and method
    async fn find_by_path_method(
        &self,
        path: &str,
        method: HttpMethod,
    ) -> Result<Vec<MockConfiguration>>;

    /// Save a mock configuration (insert or update)
    async fn save(&self, config: &MockConfiguration) -> Result<()>;

    /// Delete a mock configuration by ID
    async fn delete(&self, id: Uuid) -> Result<bool>;

    /// Get the count of mock configurations
    async fn count(&self) -> Result<u64>;

    /// Get all content hashes for sync
    async fn get_all_hashes(&self) -> Result<Vec<(Uuid, String)>>;

    /// Get configurations modified since a given timestamp
    async fn find_modified_since(&self, since: DateTime<Utc>) -> Result<Vec<MockConfiguration>>;

    /// Batch create mock configurations
    async fn batch_create(&self, requests: Vec<CreateMockRequest>) -> Result<Vec<MockConfiguration>>;

    /// Batch update mock configurations
    async fn batch_update(&self, updates: Vec<(Uuid, UpdateMockRequest)>) -> Result<Vec<MockConfiguration>>;

    /// Batch delete mock configurations
    async fn batch_delete(&self, ids: Vec<Uuid>) -> Result<u64>;
}

/// SQLite implementation of MockRepository
pub struct LocalMockRepository {
    pool: SqlitePool,
    instance_id: Uuid,
}

impl LocalMockRepository {
    /// Create a new SQLite repository
    pub fn new(pool: SqlitePool, instance_id: Uuid) -> Self {
        Self { pool, instance_id }
    }
    
    /// Get the pool
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Create a new SQLite repository with a random instance ID
    pub fn with_random_instance_id(pool: SqlitePool) -> Self {
        Self {
            pool,
            instance_id: Uuid::new_v4(),
        }
    }

    /// Get the instance ID
    pub fn instance_id(&self) -> Uuid {
        self.instance_id
    }

    /// Convert a database row to MockConfiguration
    fn row_to_config(row: &sqlx::sqlite::SqliteRow) -> Result<MockConfiguration> {
        let id: String = row.try_get("id")?;
        let id = Uuid::parse_str(&id).map_err(|e| ManagementError::Internal(e.to_string()))?;

        let method_str: String = row.try_get("method")?;
        let method = method_str
            .parse()
            .map_err(|e: String| ManagementError::Internal(e))?;

        let source_str: String = row.try_get("source")?;
        let source = match source_str.as_str() {
            "central" => MockSource::Central,
            _ => MockSource::Local,
        };

        let matching_rules_json: String = row.try_get("matching_rules")?;
        let matching_rules: MatchingRules = serde_json::from_str(&matching_rules_json)?;

        let response_config_json: String = row.try_get("response_config")?;
        let response_config: ResponseConfig = serde_json::from_str(&response_config_json)?;

        let version_vector_json: String = row.try_get("version_vector")?;
        let version_vector: VersionVector = serde_json::from_str(&version_vector_json)?;

        let created_at_str: String = row.try_get("created_at")?;
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map_err(|e| ManagementError::Internal(e.to_string()))?
            .with_timezone(&Utc);

        let updated_at_str: String = row.try_get("updated_at")?;
        let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
            .map_err(|e| ManagementError::Internal(e.to_string()))?
            .with_timezone(&Utc);

        let is_active: i32 = row.try_get("is_active")?;

        Ok(MockConfiguration {
            id,
            name: row.try_get("name")?,
            path: row.try_get("path")?,
            method,
            team_id: None,  // Local storage doesn't track team
            environment_id: None,  // Local storage doesn't track environment
            matching_rules,
            response_config,
            state_config: None,  // Local storage doesn't use state config
            source,
            version_vector,
            content_hash: row.try_get("content_hash")?,
            created_at,
            updated_at,
            created_by: None,  // Local storage doesn't track creator
            is_active: is_active != 0,
        })
    }
}

#[async_trait]
impl MockRepository for LocalMockRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<MockConfiguration>> {
        let id_str = id.to_string();
        
        let row = sqlx::query(
            "SELECT * FROM mock_configurations WHERE id = ?"
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => Ok(Some(Self::row_to_config(&row)?)),
            None => Ok(None),
        }
    }

    async fn find_all(&self, filter: MockFilter) -> Result<Vec<MockConfiguration>> {
        let mut query = String::from("SELECT * FROM mock_configurations WHERE 1=1");
        let mut bindings: Vec<String> = Vec::new();

        if let Some(path) = &filter.path {
            query.push_str(" AND path LIKE ?");
            bindings.push(format!("%{}%", path));
        }

        if let Some(method) = &filter.method {
            query.push_str(" AND method = ?");
            bindings.push(method.to_string());
        }

        if let Some(is_active) = filter.is_active {
            query.push_str(" AND is_active = ?");
            bindings.push(if is_active { "1" } else { "0" }.to_string());
        }

        if let Some(source) = &filter.source {
            query.push_str(" AND source = ?");
            bindings.push(match source {
                MockSource::Central => "central",
                MockSource::Local => "local",
            }.to_string());
        }

        query.push_str(" ORDER BY updated_at DESC");

        if let Some(limit) = filter.limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = filter.offset {
            query.push_str(&format!(" OFFSET {}", offset));
        }

        let mut sql_query = sqlx::query(&query);
        for binding in bindings {
            sql_query = sql_query.bind(binding);
        }

        let rows = sql_query.fetch_all(&self.pool).await?;

        rows.iter()
            .map(|row| Self::row_to_config(row))
            .collect()
    }

    async fn find_by_path_method(
        &self,
        path: &str,
        method: HttpMethod,
    ) -> Result<Vec<MockConfiguration>> {
        let method_str = method.to_string();
        
        let rows = sqlx::query(
            "SELECT * FROM mock_configurations 
             WHERE path = ? AND method = ? AND is_active = 1
             ORDER BY updated_at DESC"
        )
        .bind(path)
        .bind(&method_str)
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| Self::row_to_config(row))
            .collect()
    }

    async fn save(&self, config: &MockConfiguration) -> Result<()> {
        let id_str = config.id.to_string();
        let method_str = config.method.to_string();
        let source_str = match config.source {
            MockSource::Central => "central",
            MockSource::Local => "local",
        };
        let matching_rules_json = serde_json::to_string(&config.matching_rules)?;
        let response_config_json = serde_json::to_string(&config.response_config)?;
        let version_vector_json = serde_json::to_string(&config.version_vector)?;
        let is_active = if config.is_active { 1i32 } else { 0i32 };
        let created_at_str = config.created_at.to_rfc3339();
        let updated_at_str = config.updated_at.to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO mock_configurations (
                id, name, path, method, matching_rules, response_config,
                source, version_vector, content_hash, is_active, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                path = excluded.path,
                method = excluded.method,
                matching_rules = excluded.matching_rules,
                response_config = excluded.response_config,
                source = excluded.source,
                version_vector = excluded.version_vector,
                content_hash = excluded.content_hash,
                is_active = excluded.is_active,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&id_str)
        .bind(&config.name)
        .bind(&config.path)
        .bind(&method_str)
        .bind(&matching_rules_json)
        .bind(&response_config_json)
        .bind(source_str)
        .bind(&version_vector_json)
        .bind(&config.content_hash)
        .bind(is_active)
        .bind(&created_at_str)
        .bind(&updated_at_str)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<bool> {
        let id_str = id.to_string();
        
        let result = sqlx::query("DELETE FROM mock_configurations WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn count(&self) -> Result<u64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mock_configurations")
            .fetch_one(&self.pool)
            .await?;

        Ok(count as u64)
    }

    async fn get_all_hashes(&self) -> Result<Vec<(Uuid, String)>> {
        let rows = sqlx::query(
            "SELECT id, content_hash FROM mock_configurations WHERE is_active = 1"
        )
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| {
                let id_str: String = row.try_get("id")?;
                let id = Uuid::parse_str(&id_str)
                    .map_err(|e| ManagementError::Internal(e.to_string()))?;
                let content_hash: String = row.try_get("content_hash")?;
                Ok((id, content_hash))
            })
            .collect()
    }

    async fn find_modified_since(&self, since: DateTime<Utc>) -> Result<Vec<MockConfiguration>> {
        let since_str = since.to_rfc3339();
        
        let rows = sqlx::query(
            "SELECT * FROM mock_configurations WHERE updated_at > ? ORDER BY updated_at ASC"
        )
        .bind(&since_str)
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| Self::row_to_config(row))
            .collect()
    }

    async fn batch_create(&self, requests: Vec<CreateMockRequest>) -> Result<Vec<MockConfiguration>> {
        let mut configs = Vec::with_capacity(requests.len());
        
        for request in requests {
            let config = self.create(request).await?;
            configs.push(config);
        }
        
        Ok(configs)
    }

    async fn batch_update(&self, updates: Vec<(Uuid, UpdateMockRequest)>) -> Result<Vec<MockConfiguration>> {
        let mut configs = Vec::with_capacity(updates.len());
        
        for (id, request) in updates {
            let config = self.update(id, request).await?;
            configs.push(config);
        }
        
        Ok(configs)
    }

    async fn batch_delete(&self, ids: Vec<Uuid>) -> Result<u64> {
        let mut count = 0;
        
        for id in ids {
            let deleted = self.delete(id).await?;
            if deleted {
                count += 1;
            }
        }
        
        Ok(count)
    }
}

impl LocalMockRepository {
    /// Create a new mock configuration from a request
    pub async fn create(&self, request: CreateMockRequest) -> Result<MockConfiguration> {
        let mut config = MockConfiguration::new(
            request.name,
            request.path,
            request.method,
            request.matching_rules,
            request.response_config,
        );
        config.is_active = request.is_active;
        config.touch(self.instance_id);

        self.save(&config).await?;
        Ok(config)
    }

    /// Update an existing mock configuration
    pub async fn update(&self, id: Uuid, request: UpdateMockRequest) -> Result<MockConfiguration> {
        let mut config = self
            .find_by_id(id)
            .await?
            .ok_or_else(|| ManagementError::not_found(id))?;

        if let Some(name) = request.name {
            config.name = name;
        }
        if let Some(path) = request.path {
            config.path = path;
        }
        if let Some(method) = request.method {
            config.method = method;
        }
        if let Some(matching_rules) = request.matching_rules {
            config.matching_rules = matching_rules;
        }
        if let Some(response_config) = request.response_config {
            config.response_config = response_config;
        }
        if let Some(is_active) = request.is_active {
            config.is_active = is_active;
        }

        config.touch(self.instance_id);
        self.save(&config).await?;
        Ok(config)
    }
}

/// In-memory mock repository for testing
#[cfg(test)]
pub struct InMemoryMockRepository {
    configs: std::sync::RwLock<std::collections::HashMap<Uuid, MockConfiguration>>,
    instance_id: Uuid,
}

#[cfg(test)]
impl InMemoryMockRepository {
    pub fn new() -> Self {
        Self {
            configs: std::sync::RwLock::new(std::collections::HashMap::new()),
            instance_id: Uuid::new_v4(),
        }
    }
}

#[cfg(test)]
#[async_trait]
impl MockRepository for InMemoryMockRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<MockConfiguration>> {
        let configs = self.configs.read().unwrap();
        Ok(configs.get(&id).cloned())
    }

    async fn find_all(&self, filter: MockFilter) -> Result<Vec<MockConfiguration>> {
        let configs = self.configs.read().unwrap();
        let mut result: Vec<MockConfiguration> = configs
            .values()
            .filter(|c| {
                if let Some(path) = &filter.path {
                    if !c.path.contains(path) {
                        return false;
                    }
                }
                if let Some(method) = &filter.method {
                    if c.method != *method {
                        return false;
                    }
                }
                if let Some(is_active) = filter.is_active {
                    if c.is_active != is_active {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        result.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        if let Some(limit) = filter.limit {
            result.truncate(limit as usize);
        }

        Ok(result)
    }

    async fn find_by_path_method(
        &self,
        path: &str,
        method: HttpMethod,
    ) -> Result<Vec<MockConfiguration>> {
        let configs = self.configs.read().unwrap();
        Ok(configs
            .values()
            .filter(|c| c.path == path && c.method == method && c.is_active)
            .cloned()
            .collect())
    }

    async fn save(&self, config: &MockConfiguration) -> Result<()> {
        let mut configs = self.configs.write().unwrap();
        configs.insert(config.id, config.clone());
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<bool> {
        let mut configs = self.configs.write().unwrap();
        Ok(configs.remove(&id).is_some())
    }

    async fn count(&self) -> Result<u64> {
        let configs = self.configs.read().unwrap();
        Ok(configs.len() as u64)
    }

    async fn get_all_hashes(&self) -> Result<Vec<(Uuid, String)>> {
        let configs = self.configs.read().unwrap();
        Ok(configs
            .values()
            .filter(|c| c.is_active)
            .map(|c| (c.id, c.content_hash.clone()))
            .collect())
    }

    async fn find_modified_since(&self, since: DateTime<Utc>) -> Result<Vec<MockConfiguration>> {
        let configs = self.configs.read().unwrap();
        let mut result: Vec<MockConfiguration> = configs
            .values()
            .filter(|c| c.updated_at > since)
            .cloned()
            .collect();
        result.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
        Ok(result)
    }

    async fn batch_create(&self, requests: Vec<CreateMockRequest>) -> Result<Vec<MockConfiguration>> {
        let mut configs = Vec::with_capacity(requests.len());
        let mut write_guard = self.configs.write().unwrap();
        
        for request in requests {
            let mut config = MockConfiguration::new(
                request.name,
                request.path,
                request.method,
                request.matching_rules,
                request.response_config,
            );
            config.is_active = request.is_active;
            config.touch(self.instance_id);
            
            write_guard.insert(config.id, config.clone());
            configs.push(config);
        }
        
        Ok(configs)
    }

    async fn batch_update(&self, updates: Vec<(Uuid, UpdateMockRequest)>) -> Result<Vec<MockConfiguration>> {
        let mut configs = Vec::with_capacity(updates.len());
        let mut write_guard = self.configs.write().unwrap();
        
        for (id, request) in updates {
            if let Some(mut config) = write_guard.get_mut(&id) {
                if let Some(name) = request.name {
                    config.name = name;
                }
                if let Some(path) = request.path {
                    config.path = path;
                }
                if let Some(method) = request.method {
                    config.method = method;
                }
                if let Some(matching_rules) = request.matching_rules {
                    config.matching_rules = matching_rules;
                }
                if let Some(response_config) = request.response_config {
                    config.response_config = response_config;
                }
                if let Some(is_active) = request.is_active {
                    config.is_active = is_active;
                }
                
                config.touch(self.instance_id);
                configs.push(config.clone());
            }
        }
        
        Ok(configs)
    }

    async fn batch_delete(&self, ids: Vec<Uuid>) -> Result<u64> {
        let mut count = 0;
        let mut write_guard = self.configs.write().unwrap();
        
        for id in ids {
            if write_guard.remove(&id).is_some() {
                count += 1;
            }
        }
        
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::management::db::create_memory_pool;

    #[tokio::test]
    async fn test_create_and_find() {
        let pool = create_memory_pool().await.unwrap();
        let repo = LocalMockRepository::with_random_instance_id(pool);

        let request = CreateMockRequest {
            name: "Test Mock".to_string(),
            path: "/api/test".to_string(),
            method: HttpMethod::Get,
            matching_rules: MatchingRules::default(),
            response_config: ResponseConfig::default(),
            is_active: true,
        };

        let config = repo.create(request).await.unwrap();
        assert!(!config.content_hash.is_empty());

        let found = repo.find_by_id(config.id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Test Mock");
    }

    #[tokio::test]
    async fn test_update() {
        let pool = create_memory_pool().await.unwrap();
        let repo = LocalMockRepository::with_random_instance_id(pool);

        let request = CreateMockRequest {
            name: "Test Mock".to_string(),
            path: "/api/test".to_string(),
            method: HttpMethod::Get,
            matching_rules: MatchingRules::default(),
            response_config: ResponseConfig::default(),
            is_active: true,
        };

        let config = repo.create(request).await.unwrap();

        let update = UpdateMockRequest {
            name: Some("Updated Mock".to_string()),
            ..Default::default()
        };

        let updated = repo.update(config.id, update).await.unwrap();
        assert_eq!(updated.name, "Updated Mock");
    }

    #[tokio::test]
    async fn test_delete() {
        let pool = create_memory_pool().await.unwrap();
        let repo = LocalMockRepository::with_random_instance_id(pool);

        let request = CreateMockRequest {
            name: "Test Mock".to_string(),
            path: "/api/test".to_string(),
            method: HttpMethod::Get,
            matching_rules: MatchingRules::default(),
            response_config: ResponseConfig::default(),
            is_active: true,
        };

        let config = repo.create(request).await.unwrap();

        let deleted = repo.delete(config.id).await.unwrap();
        assert!(deleted);

        let found = repo.find_by_id(config.id).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_find_by_path_method() {
        let pool = create_memory_pool().await.unwrap();
        let repo = LocalMockRepository::with_random_instance_id(pool);

        let request = CreateMockRequest {
            name: "Test Mock".to_string(),
            path: "/api/test".to_string(),
            method: HttpMethod::Get,
            matching_rules: MatchingRules::default(),
            response_config: ResponseConfig::default(),
            is_active: true,
        };

        repo.create(request).await.unwrap();

        let found = repo.find_by_path_method("/api/test", HttpMethod::Get).await.unwrap();
        assert_eq!(found.len(), 1);

        let not_found = repo.find_by_path_method("/api/test", HttpMethod::Post).await.unwrap();
        assert!(not_found.is_empty());
    }
}
