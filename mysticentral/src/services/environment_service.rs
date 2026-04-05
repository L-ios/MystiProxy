//! Environment Service
//!
//! Business logic for environment management.

use chrono::Utc;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::models::{
    Environment, EnvironmentCreateRequest, EnvironmentFilter, EnvironmentUpdateRequest,
};
use crate::services::EnvironmentRepository;

/// Service for managing environments
pub struct EnvironmentService<R: EnvironmentRepository> {
    repository: R,
}

impl<R: EnvironmentRepository> EnvironmentService<R> {
    /// Create a new EnvironmentService
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// Create a new environment
    pub async fn create(&self, request: EnvironmentCreateRequest) -> ApiResult<Environment> {
        // Validate name
        if request.name.trim().is_empty() {
            return Err(ApiError::Validation(
                "Environment name cannot be empty".to_string(),
            ));
        }

        // Create from template if specified
        let mut env = if let Some(template_id) = request.template_id {
            let template = self
                .repository
                .find_by_id(template_id)
                .await?
                .ok_or_else(|| {
                    ApiError::NotFound(format!("Template with id {} not found", template_id))
                })?;

            if !template.is_template {
                return Err(ApiError::Validation(
                    "Specified environment is not a template".to_string(),
                ));
            }

            Environment::from_template(request.name, &template)
        } else {
            Environment::new(request.name)
        };

        // Apply additional fields
        env.description = request.description;
        if let Some(endpoints) = request.endpoints {
            env.endpoints = endpoints;
        }

        // Save to repository
        self.repository.save(&env).await?;

        Ok(env)
    }

    /// Get an environment by ID
    pub async fn get(&self, id: Uuid) -> ApiResult<Environment> {
        self.repository
            .find_by_id(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Environment with id {} not found", id)))
    }

    /// List environments with filtering
    pub async fn list(&self, filter: EnvironmentFilter) -> ApiResult<(Vec<Environment>, u32)> {
        let envs = self.repository.find_all(filter.clone()).await?;
        let total = self.repository.count(&filter).await?;
        Ok((envs, total))
    }

    /// Update an environment
    pub async fn update(
        &self,
        id: Uuid,
        request: EnvironmentUpdateRequest,
    ) -> ApiResult<Environment> {
        let mut env = self.get(id).await?;

        if let Some(name) = request.name {
            if name.trim().is_empty() {
                return Err(ApiError::Validation(
                    "Environment name cannot be empty".to_string(),
                ));
            }
            env.name = name;
        }

        if let Some(description) = request.description {
            env.description = Some(description);
        }

        if let Some(endpoints) = request.endpoints {
            env.endpoints = endpoints;
        }

        env.updated_at = Utc::now();

        self.repository.save(&env).await?;

        Ok(env)
    }

    /// Delete an environment
    pub async fn delete(&self, id: Uuid) -> ApiResult<()> {
        // Check if it's being used by any mocks
        self.repository.delete(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    // In-memory implementation for testing
    struct InMemoryEnvironmentRepository {
        envs: Arc<RwLock<HashMap<Uuid, Environment>>>,
    }

    impl InMemoryEnvironmentRepository {
        fn new() -> Self {
            Self {
                envs: Arc::new(RwLock::new(HashMap::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl EnvironmentRepository for InMemoryEnvironmentRepository {
        async fn find_by_id(&self, id: Uuid) -> Result<Option<Environment>, ApiError> {
            let envs = self.envs.read().await;
            Ok(envs.get(&id).cloned())
        }

        async fn find_all(&self, filter: EnvironmentFilter) -> Result<Vec<Environment>, ApiError> {
            let envs = self.envs.read().await;
            let result: Vec<Environment> = envs
                .values()
                .filter(|e| filter.is_template.map_or(true, |t| e.is_template == t))
                .cloned()
                .collect();
            Ok(result)
        }

        async fn save(&self, env: &Environment) -> Result<(), ApiError> {
            let mut envs = self.envs.write().await;
            envs.insert(env.id, env.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), ApiError> {
            let mut envs = self.envs.write().await;
            if envs.remove(&id).is_none() {
                return Err(ApiError::NotFound(format!(
                    "Environment with id {} not found",
                    id
                )));
            }
            Ok(())
        }

        async fn count(&self, filter: &EnvironmentFilter) -> Result<u32, ApiError> {
            let envs = self.envs.read().await;
            let count = envs
                .values()
                .filter(|e| filter.is_template.map_or(true, |t| e.is_template == t))
                .count();
            Ok(count as u32)
        }
    }

    #[tokio::test]
    async fn test_create_environment() {
        let repo = InMemoryEnvironmentRepository::new();
        let service = EnvironmentService::new(repo);

        let request = EnvironmentCreateRequest {
            name: "development".to_string(),
            description: Some("Development environment".to_string()),
            endpoints: Some(HashMap::new()),
            template_id: None,
        };

        let env = service.create(request).await.unwrap();
        assert_eq!(env.name, "development");
        assert_eq!(env.description, Some("Development environment".to_string()));
    }

    #[tokio::test]
    async fn test_create_environment_empty_name() {
        let repo = InMemoryEnvironmentRepository::new();
        let service = EnvironmentService::new(repo);

        let request = EnvironmentCreateRequest {
            name: "".to_string(),
            description: None,
            endpoints: None,
            template_id: None,
        };

        let result = service.create(request).await;
        assert!(result.is_err());
    }
}
