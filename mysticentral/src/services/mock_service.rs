//! Mock Service - Business Logic Layer
//!
//! Provides business logic for mock configuration management.

use chrono::Utc;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::models::{
    HttpMethod, MockConfiguration, MockCreateRequest, MockFilter, MockUpdateRequest, VersionVector,
};
use crate::services::MockRepository;

/// Service for managing mock configurations
pub struct MockService<R: MockRepository> {
    repository: R,
}

impl<R: MockRepository> MockService<R> {
    /// Create a new MockService
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// Create a new mock configuration
    pub async fn create(&self, request: MockCreateRequest, user_id: Option<Uuid>) -> ApiResult<MockConfiguration> {
        // Validate the request
        self.validate_create_request(&request)?;

        // Create the configuration
        let mut config = MockConfiguration::new(
            request.name,
            request.path,
            request.method,
            request.matching_rules,
            request.response_config,
        );

        config.team_id = request.team_id;
        config.environment_id = request.environment_id;
        config.state_config = request.state_config;
        config.created_by = user_id;
        config.is_active = request.is_active;

        // Save to repository
        self.repository.save(&config).await?;

        Ok(config)
    }

    /// Get a mock configuration by ID
    pub async fn get(&self, id: Uuid) -> ApiResult<MockConfiguration> {
        self.repository
            .find_by_id(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Mock configuration with id {} not found", id)))
    }

    /// List mock configurations with filtering
    pub async fn list(&self, filter: MockFilter) -> ApiResult<(Vec<MockConfiguration>, u32)> {
        let configs = self.repository.find_all(filter.clone()).await?;
        let total = self.repository.count(&filter).await?;
        Ok((configs, total))
    }

    /// Update a mock configuration
    pub async fn update(
        &self,
        id: Uuid,
        request: MockUpdateRequest,
        instance_id: Uuid,
    ) -> ApiResult<MockConfiguration> {
        // Get existing configuration
        let mut config = self.get(id).await?;

        // Check for conflicts using version vector
        if let Some(ref client_version) = request.version_vector {
            if config.version_vector.is_concurrent_with(client_version) {
                return Err(ApiError::Conflict(
                    "Concurrent modification detected. Please resolve the conflict.".to_string(),
                ));
            }
        }

        // Apply updates
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
        if let Some(state_config) = request.state_config {
            config.state_config = Some(state_config);
        }
        if let Some(is_active) = request.is_active {
            config.is_active = is_active;
        }

        // Update version vector
        config.version_vector.increment(instance_id);
        config.updated_at = Utc::now();
        config.update_content_hash();

        // Save changes
        self.repository.save(&config).await?;

        Ok(config)
    }

    /// Delete a mock configuration
    pub async fn delete(&self, id: Uuid) -> ApiResult<()> {
        self.repository.delete(id).await
    }

    /// Save a mock configuration directly (for sync/import)
    pub async fn save(&self, config: &MockConfiguration) -> ApiResult<()> {
        self.repository.save(config).await
    }

    /// Validate create request
    fn validate_create_request(&self, request: &MockCreateRequest) -> ApiResult<()> {
        // Validate name
        if request.name.trim().is_empty() {
            return Err(ApiError::Validation("Name cannot be empty".to_string()));
        }

        // Validate path
        if request.path.trim().is_empty() {
            return Err(ApiError::Validation("Path cannot be empty".to_string()));
        }

        if !request.path.starts_with('/') {
            return Err(ApiError::Validation("Path must start with '/'".to_string()));
        }

        // Validate method (already validated by HttpMethod enum)
        // Method is guaranteed to be valid by the type system

        // Validate response status
        if request.response_config.status < 100 || request.response_config.status > 599 {
            return Err(ApiError::Validation(
                "Response status must be between 100 and 599".to_string(),
            ));
        }

        Ok(())
    }

    /// Find configurations modified since a given time
    pub async fn find_modified_since(
        &self,
        since: chrono::DateTime<Utc>,
        filter: MockFilter,
    ) -> ApiResult<Vec<MockConfiguration>> {
        let all_configs = self.repository.find_all(filter).await?;
        let modified: Vec<MockConfiguration> = all_configs
            .into_iter()
            .filter(|c| c.updated_at > since)
            .collect();
        Ok(modified)
    }

    /// Find configurations by content hash
    pub async fn find_by_content_hash(&self, hash: &str) -> ApiResult<Option<MockConfiguration>> {
        let filter = MockFilter::default();
        let configs = self.repository.find_all(filter).await?;
        Ok(configs.into_iter().find(|c| c.content_hash == hash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MatchingRules, ResponseConfig};
    use crate::services::InMemoryMockRepository;

    #[tokio::test]
    async fn test_create_mock() {
        let repo = InMemoryMockRepository::new();
        let service = MockService::new(repo);

        let request = MockCreateRequest {
            name: "Test Mock".to_string(),
            path: "/api/test".to_string(),
            method: HttpMethod::Get,
            team_id: None,
            environment_id: None,
            matching_rules: MatchingRules::default(),
            response_config: ResponseConfig::default(),
            state_config: None,
            is_active: true,
        };

        let config = service.create(request, None).await.unwrap();
        assert_eq!(config.name, "Test Mock");
        assert_eq!(config.path, "/api/test");
        assert_eq!(config.method, HttpMethod::Get);
    }

    #[tokio::test]
    async fn test_create_mock_invalid_path() {
        let repo = InMemoryMockRepository::new();
        let service = MockService::new(repo);

        let request = MockCreateRequest {
            name: "Test Mock".to_string(),
            path: "api/test".to_string(), // Missing leading slash
            method: HttpMethod::Get,
            team_id: None,
            environment_id: None,
            matching_rules: MatchingRules::default(),
            response_config: ResponseConfig::default(),
            state_config: None,
            is_active: true,
        };

        let result = service.create(request, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_mock() {
        let repo = InMemoryMockRepository::new();
        let service = MockService::new(repo);

        // Create a mock
        let create_request = MockCreateRequest {
            name: "Test Mock".to_string(),
            path: "/api/test".to_string(),
            method: HttpMethod::Get,
            team_id: None,
            environment_id: None,
            matching_rules: MatchingRules::default(),
            response_config: ResponseConfig::default(),
            state_config: None,
            is_active: true,
        };

        let config = service.create(create_request, None).await.unwrap();
        let id = config.id;
        let instance_id = Uuid::new_v4();

        // Update the mock
        let update_request = MockUpdateRequest {
            name: Some("Updated Mock".to_string()),
            path: None,
            method: None,
            matching_rules: None,
            response_config: None,
            state_config: None,
            version_vector: None,
            is_active: None,
        };

        let updated = service.update(id, update_request, instance_id).await.unwrap();
        assert_eq!(updated.name, "Updated Mock");
    }

    #[tokio::test]
    async fn test_delete_mock() {
        let repo = InMemoryMockRepository::new();
        let service = MockService::new(repo);

        // Create a mock
        let create_request = MockCreateRequest {
            name: "Test Mock".to_string(),
            path: "/api/test".to_string(),
            method: HttpMethod::Get,
            team_id: None,
            environment_id: None,
            matching_rules: MatchingRules::default(),
            response_config: ResponseConfig::default(),
            state_config: None,
            is_active: true,
        };

        let config = service.create(create_request, None).await.unwrap();
        let id = config.id;

        // Delete the mock
        service.delete(id).await.unwrap();

        // Verify it's deleted
        let result = service.get(id).await;
        assert!(result.is_err());
    }
}
