//! Instance Service
//!
//! Business logic for MystiProxy instance management.

use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::models::{
    HeartbeatRequest, InstanceFilter, InstanceRegisterRequest, MystiProxyInstance,
};
use crate::services::InstanceRepository;

/// Service for managing MystiProxy instances
pub struct InstanceService<R: InstanceRepository> {
    repository: R,
}

impl<R: InstanceRepository> InstanceService<R> {
    /// Create a new InstanceService
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    /// Register a new instance
    pub async fn register(
        &self,
        request: InstanceRegisterRequest,
    ) -> ApiResult<MystiProxyInstance> {
        // Validate name
        if request.name.trim().is_empty() {
            return Err(ApiError::Validation(
                "Instance name cannot be empty".to_string(),
            ));
        }

        // Validate endpoint URL
        if request.endpoint_url.trim().is_empty() {
            return Err(ApiError::Validation(
                "Endpoint URL cannot be empty".to_string(),
            ));
        }

        // Check for duplicate name
        if self.repository.find_by_name(&request.name).await?.is_some() {
            return Err(ApiError::Conflict(format!(
                "Instance with name '{}' already exists",
                request.name
            )));
        }

        // Create instance
        let mut instance = MystiProxyInstance::new(request.name, request.endpoint_url);

        // Hash API key if provided
        if let Some(api_key) = request.api_key {
            instance.api_key_hash = Some(hash_api_key(&api_key));
        }

        instance.update_heartbeat();

        // Save to repository
        self.repository.save(&instance).await?;

        Ok(instance)
    }

    /// Get an instance by ID
    pub async fn get(&self, id: Uuid) -> ApiResult<MystiProxyInstance> {
        self.repository
            .find_by_id(id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Instance with id {} not found", id)))
    }

    /// List instances with filtering
    pub async fn list(&self, filter: InstanceFilter) -> ApiResult<(Vec<MystiProxyInstance>, u32)> {
        let instances = self.repository.find_all(filter.clone()).await?;
        let total = self.repository.count(&filter).await?;
        Ok((instances, total))
    }

    /// Process heartbeat from an instance
    pub async fn heartbeat(
        &self,
        id: Uuid,
        request: HeartbeatRequest,
    ) -> ApiResult<MystiProxyInstance> {
        let mut instance = self.get(id).await?;

        instance.update_heartbeat();
        instance.config_checksum = request.config_checksum;

        self.repository.save(&instance).await?;

        Ok(instance)
    }

    /// Unregister an instance
    pub async fn unregister(&self, id: Uuid) -> ApiResult<()> {
        self.repository.delete(id).await
    }

    /// Get all healthy instances
    #[allow(dead_code)]
    pub async fn get_healthy_instances(&self) -> ApiResult<Vec<MystiProxyInstance>> {
        let filter = InstanceFilter::default();
        let instances = self.repository.find_all(filter).await?;
        Ok(instances.into_iter().filter(|i| i.is_healthy()).collect())
    }

    /// Update sync status
    #[allow(dead_code)]
    pub async fn update_sync_status(
        &self,
        id: Uuid,
        status: crate::models::SyncStatus,
    ) -> ApiResult<()> {
        let mut instance = self.get(id).await?;
        instance.sync_status = status;
        instance.last_sync_at = Some(Utc::now());
        self.repository.save(&instance).await?;
        Ok(())
    }
}

/// Hash an API key using SHA-256
fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    // In-memory implementation for testing
    struct InMemoryInstanceRepository {
        instances: Arc<RwLock<HashMap<Uuid, MystiProxyInstance>>>,
    }

    impl InMemoryInstanceRepository {
        fn new() -> Self {
            Self {
                instances: Arc::new(RwLock::new(HashMap::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl InstanceRepository for InMemoryInstanceRepository {
        async fn find_by_id(&self, id: Uuid) -> Result<Option<MystiProxyInstance>, ApiError> {
            let instances = self.instances.read().await;
            Ok(instances.get(&id).cloned())
        }

        async fn find_all(
            &self,
            filter: InstanceFilter,
        ) -> Result<Vec<MystiProxyInstance>, ApiError> {
            let instances = self.instances.read().await;
            let result: Vec<MystiProxyInstance> = instances
                .values()
                .filter(|i| filter.sync_status.map_or(true, |s| i.sync_status == s))
                .cloned()
                .collect();
            Ok(result)
        }

        async fn save(&self, instance: &MystiProxyInstance) -> Result<(), ApiError> {
            let mut instances = self.instances.write().await;
            instances.insert(instance.id, instance.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), ApiError> {
            let mut instances = self.instances.write().await;
            if instances.remove(&id).is_none() {
                return Err(ApiError::NotFound(format!(
                    "Instance with id {} not found",
                    id
                )));
            }
            Ok(())
        }

        async fn count(&self, filter: &InstanceFilter) -> Result<u32, ApiError> {
            let instances = self.instances.read().await;
            let count = instances
                .values()
                .filter(|i| filter.sync_status.map_or(true, |s| i.sync_status == s))
                .count();
            Ok(count as u32)
        }

        async fn find_by_name(&self, name: &str) -> Result<Option<MystiProxyInstance>, ApiError> {
            let instances = self.instances.read().await;
            Ok(instances.values().find(|i| i.name == name).cloned())
        }
    }

    #[tokio::test]
    async fn test_register_instance() {
        let repo = InMemoryInstanceRepository::new();
        let service = InstanceService::new(repo);

        let request = InstanceRegisterRequest {
            name: "test-instance".to_string(),
            endpoint_url: "http://localhost:8081".to_string(),
            api_key: Some("secret-key".to_string()),
        };

        let instance = service.register(request).await.unwrap();
        assert_eq!(instance.name, "test-instance");
        assert!(instance.api_key_hash.is_some());
    }

    #[tokio::test]
    async fn test_duplicate_name() {
        let repo = InMemoryInstanceRepository::new();
        let service = InstanceService::new(repo);

        let request1 = InstanceRegisterRequest {
            name: "test-instance".to_string(),
            endpoint_url: "http://localhost:8081".to_string(),
            api_key: None,
        };

        service.register(request1).await.unwrap();

        let request2 = InstanceRegisterRequest {
            name: "test-instance".to_string(),
            endpoint_url: "http://localhost:8082".to_string(),
            api_key: None,
        };

        let result = service.register(request2).await;
        assert!(result.is_err());
    }
}
