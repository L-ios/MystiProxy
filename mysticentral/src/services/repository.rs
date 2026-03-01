//! Repository trait and implementations
//!
//! Provides data access abstraction for mock configurations.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::ApiError;
use crate::models::{MockConfiguration, MockFilter};

/// Repository trait for mock configuration persistence
#[async_trait]
pub trait MockRepository: Send + Sync {
    /// Find a mock configuration by ID
    async fn find_by_id(&self, id: Uuid) -> Result<Option<MockConfiguration>, ApiError>;

    /// Find all mock configurations matching the filter
    async fn find_all(&self, filter: MockFilter) -> Result<Vec<MockConfiguration>, ApiError>;

    /// Save a mock configuration (create or update)
    async fn save(&self, config: &MockConfiguration) -> Result<(), ApiError>;

    /// Delete a mock configuration by ID
    async fn delete(&self, id: Uuid) -> Result<(), ApiError>;

    /// Count total mock configurations
    async fn count(&self, filter: &MockFilter) -> Result<u32, ApiError>;
}

/// In-memory mock repository for testing
pub struct InMemoryMockRepository {
    configs: Arc<RwLock<HashMap<Uuid, MockConfiguration>>>,
}

impl InMemoryMockRepository {
    /// Create a new in-memory repository
    pub fn new() -> Self {
        Self {
            configs: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryMockRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MockRepository for InMemoryMockRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<MockConfiguration>, ApiError> {
        let configs = self.configs.read().await;
        Ok(configs.get(&id).cloned())
    }

    async fn find_all(&self, filter: MockFilter) -> Result<Vec<MockConfiguration>, ApiError> {
        let configs = self.configs.read().await;
        let mut result: Vec<MockConfiguration> = configs
            .values()
            .filter(|config| {
                if let Some(env_id) = filter.environment {
                    if config.environment_id != Some(env_id) {
                        return false;
                    }
                }
                if let Some(team_id) = filter.team {
                    if config.team_id != Some(team_id) {
                        return false;
                    }
                }
                if let Some(ref path) = filter.path {
                    if !config.path.contains(path) {
                        return false;
                    }
                }
                if let Some(ref method) = filter.method {
                    if &config.method != method {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        // Sort by created_at descending
        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // Apply pagination
        let offset = filter.offset() as usize;
        let limit = filter.limit() as usize;
        
        Ok(result.into_iter().skip(offset).take(limit).collect())
    }

    async fn save(&self, config: &MockConfiguration) -> Result<(), ApiError> {
        let mut configs = self.configs.write().await;
        configs.insert(config.id, config.clone());
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), ApiError> {
        let mut configs = self.configs.write().await;
        if configs.remove(&id).is_none() {
            return Err(ApiError::NotFound(format!("Mock configuration with id {} not found", id)));
        }
        Ok(())
    }

    async fn count(&self, filter: &MockFilter) -> Result<u32, ApiError> {
        let configs = self.configs.read().await;
        let count = configs
            .values()
            .filter(|config| {
                if let Some(env_id) = filter.environment {
                    if config.environment_id != Some(env_id) {
                        return false;
                    }
                }
                if let Some(team_id) = filter.team {
                    if config.team_id != Some(team_id) {
                        return false;
                    }
                }
                if let Some(ref path) = filter.path {
                    if !config.path.contains(path) {
                        return false;
                    }
                }
                if let Some(ref method) = filter.method {
                    if &config.method != method {
                        return false;
                    }
                }
                true
            })
            .count();
        Ok(count as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MatchingRules, ResponseConfig, HttpMethod};

    #[tokio::test]
    async fn test_in_memory_repository_crud() {
        let repo = InMemoryMockRepository::new();
        
        let config = MockConfiguration::new(
            "Test Mock".to_string(),
            "/api/test".to_string(),
            HttpMethod::Get,
            MatchingRules::default(),
            ResponseConfig::default(),
        );
        
        let id = config.id;
        
        // Create
        repo.save(&config).await.unwrap();
        
        // Read
        let found = repo.find_by_id(id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Test Mock");
        
        // Update
        let mut updated = config.clone();
        updated.name = "Updated Mock".to_string();
        repo.save(&updated).await.unwrap();
        
        let found = repo.find_by_id(id).await.unwrap().unwrap();
        assert_eq!(found.name, "Updated Mock");
        
        // Delete
        repo.delete(id).await.unwrap();
        let found = repo.find_by_id(id).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_in_memory_repository_filter() {
        let repo = InMemoryMockRepository::new();
        
        let config1 = MockConfiguration::new(
            "Mock 1".to_string(),
            "/api/users".to_string(),
            HttpMethod::Get,
            MatchingRules::default(),
            ResponseConfig::default(),
        );
        
        let config2 = MockConfiguration::new(
            "Mock 2".to_string(),
            "/api/posts".to_string(),
            HttpMethod::Post,
            MatchingRules::default(),
            ResponseConfig::default(),
        );
        
        repo.save(&config1).await.unwrap();
        repo.save(&config2).await.unwrap();
        
        let filter = MockFilter {
            method: Some(HttpMethod::Get),
            ..Default::default()
        };
        
        let results = repo.find_all(filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Mock 1");
    }
}
