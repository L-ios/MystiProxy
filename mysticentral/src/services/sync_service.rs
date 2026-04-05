//! Sync Service
//!
//! Core synchronization logic between MystiCentral and MystiProxy instances.

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::models::{MockConfiguration, MockFilter};
use crate::services::{
    ConflictReason, InstanceRepository, MockRepository, SyncConflict, SyncPullResponse,
    SyncPushResponse,
};

/// Service for handling synchronization
#[allow(dead_code)]
pub struct SyncService<MR: MockRepository, IR: InstanceRepository> {
    mock_repo: MR,
    instance_repo: IR,
}

impl<MR: MockRepository, IR: InstanceRepository> SyncService<MR, IR> {
    /// Create a new SyncService
    #[allow(dead_code)]
    pub fn new(mock_repo: MR, instance_repo: IR) -> Self {
        Self {
            mock_repo,
            instance_repo,
        }
    }

    /// Handle a pull request from an instance
    #[allow(dead_code)]
    pub async fn handle_pull(
        &self,
        instance_id: Uuid,
        since: Option<DateTime<Utc>>,
        known_checksums: HashMap<Uuid, String>,
    ) -> ApiResult<SyncPullResponse> {
        // Verify instance exists
        let _instance = self
            .instance_repo
            .find_by_id(instance_id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Instance {} not found", instance_id)))?;

        // Get configurations modified since the given time
        let since_time = since.unwrap_or(DateTime::UNIX_EPOCH);
        let filter = MockFilter::default();
        let all_configs = self.mock_repo.find_all(filter).await?;

        // Filter by modification time and check for conflicts
        let mut configs = Vec::new();
        let deleted_ids = Vec::new();
        let full_sync_required = false;

        for config in all_configs {
            if config.updated_at > since_time {
                // Check if the instance's version conflicts
                if let Some(known_hash) = known_checksums.get(&config.id) {
                    if known_hash != &config.content_hash {
                        // Content hash mismatch - need to send update
                        configs.push(config);
                    }
                } else {
                    // New configuration for this instance
                    configs.push(config);
                }
            }
        }

        Ok(SyncPullResponse {
            configs,
            deleted_ids,
            server_time: Utc::now(),
            full_sync_required,
        })
    }

    /// Handle a push request from an instance
    #[allow(dead_code)]
    pub async fn handle_push(
        &self,
        instance_id: Uuid,
        configs: Vec<MockConfiguration>,
        deleted_ids: Vec<Uuid>,
    ) -> ApiResult<SyncPushResponse> {
        // Verify instance exists
        let _instance = self
            .instance_repo
            .find_by_id(instance_id)
            .await?
            .ok_or_else(|| ApiError::NotFound(format!("Instance {} not found", instance_id)))?;

        let mut accepted = Vec::new();
        let mut conflicts = Vec::new();

        // Process each configuration
        for config in configs {
            match self.process_pushed_config(&config).await {
                Ok(()) => accepted.push(config.id),
                Err(conflict) => conflicts.push(conflict),
            }
        }

        // Process deletions
        for id in deleted_ids {
            match self.mock_repo.delete(id).await {
                Ok(()) => accepted.push(id),
                Err(_) => {
                    // Ignore errors for deletions of non-existent configs
                }
            }
        }

        Ok(SyncPushResponse {
            accepted,
            conflicts,
            server_time: Utc::now(),
        })
    }

    /// Process a pushed configuration
    #[allow(dead_code)]
    async fn process_pushed_config(&self, config: &MockConfiguration) -> Result<(), SyncConflict> {
        // Check if configuration exists
        match self.mock_repo.find_by_id(config.id).await {
            Ok(Some(existing)) => {
                // Check for conflicts using version vectors
                if existing
                    .version_vector
                    .is_concurrent_with(&config.version_vector)
                {
                    // Conflict detected
                    return Err(SyncConflict {
                        config_id: config.id,
                        reason: ConflictReason::ConcurrentModification,
                        local: config.clone(),
                        central: existing,
                        detected_at: Utc::now(),
                    });
                }

                // Check content hash
                if existing.content_hash != config.content_hash {
                    // Check if central version dominates
                    if existing.version_vector.dominates(&config.version_vector) {
                        // Central version is newer, no update needed
                        return Ok(());
                    }
                }

                // Save the configuration
                self.mock_repo
                    .save(config)
                    .await
                    .map_err(|_e| SyncConflict {
                        config_id: config.id,
                        reason: ConflictReason::VersionMismatch,
                        local: config.clone(),
                        central: existing,
                        detected_at: Utc::now(),
                    })?;
            }
            Ok(None) => {
                // New configuration, save it
                self.mock_repo
                    .save(config)
                    .await
                    .map_err(|_e| SyncConflict {
                        config_id: config.id,
                        reason: ConflictReason::VersionMismatch,
                        local: config.clone(),
                        central: MockConfiguration::new(
                            "placeholder".to_string(),
                            "/placeholder".to_string(),
                            crate::models::HttpMethod::Get,
                            Default::default(),
                            Default::default(),
                        ),
                        detected_at: Utc::now(),
                    })?;
            }
            Err(_e) => {
                return Err(SyncConflict {
                    config_id: config.id,
                    reason: ConflictReason::VersionMismatch,
                    local: config.clone(),
                    central: MockConfiguration::new(
                        "error".to_string(),
                        "/error".to_string(),
                        crate::models::HttpMethod::Get,
                        Default::default(),
                        Default::default(),
                    ),
                    detected_at: Utc::now(),
                });
            }
        }

        Ok(())
    }

    /// Check if an instance needs to sync
    #[allow(dead_code)]
    pub async fn needs_sync(&self, _instance_id: Uuid, checksum: &str) -> ApiResult<bool> {
        // Get all configurations and compute checksum
        let filter = MockFilter::default();
        let configs = self.mock_repo.find_all(filter).await?;

        let combined_hash = Self::compute_combined_checksum(&configs);

        Ok(combined_hash != checksum)
    }

    /// Compute a combined checksum for all configurations
    #[allow(dead_code)]
    fn compute_combined_checksum(configs: &[MockConfiguration]) -> String {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        for config in configs {
            hasher.update(config.id.as_bytes());
            hasher.update(config.content_hash.as_bytes());
        }
        hex::encode(hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MatchingRules, ResponseConfig};
    use crate::services::repository::InMemoryMockRepository;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    // In-memory instance repository for testing
    struct InMemoryInstanceRepository {
        instances: Arc<RwLock<HashMap<Uuid, crate::models::MystiProxyInstance>>>,
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
        async fn find_by_id(
            &self,
            id: Uuid,
        ) -> Result<Option<crate::models::MystiProxyInstance>, ApiError> {
            let instances = self.instances.read().await;
            Ok(instances.get(&id).cloned())
        }

        async fn find_all(
            &self,
            filter: crate::models::InstanceFilter,
        ) -> Result<Vec<crate::models::MystiProxyInstance>, ApiError> {
            let instances = self.instances.read().await;
            Ok(instances.values().cloned().collect())
        }

        async fn save(&self, instance: &crate::models::MystiProxyInstance) -> Result<(), ApiError> {
            let mut instances = self.instances.write().await;
            instances.insert(instance.id, instance.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), ApiError> {
            let mut instances = self.instances.write().await;
            instances.remove(&id);
            Ok(())
        }

        async fn count(&self, _filter: &crate::models::InstanceFilter) -> Result<u32, ApiError> {
            let instances = self.instances.read().await;
            Ok(instances.len() as u32)
        }

        async fn find_by_name(
            &self,
            name: &str,
        ) -> Result<Option<crate::models::MystiProxyInstance>, ApiError> {
            let instances = self.instances.read().await;
            Ok(instances.values().find(|i| i.name == name).cloned())
        }
    }

    #[tokio::test]
    async fn test_pull_empty() {
        let mock_repo = InMemoryMockRepository::new();
        let instance_repo = InMemoryInstanceRepository::new();

        // Register instance
        let mut instance = crate::models::MystiProxyInstance::new(
            "test".to_string(),
            "http://localhost:8081".to_string(),
        );
        let instance_id = instance.id;
        instance_repo.save(&instance).await.unwrap();

        let service = SyncService::new(mock_repo, instance_repo);
        let response = service
            .handle_pull(instance_id, None, HashMap::new())
            .await
            .unwrap();

        assert!(response.configs.is_empty());
        assert!(response.deleted_ids.is_empty());
    }

    #[tokio::test]
    async fn test_push_new_config() {
        let mock_repo = InMemoryMockRepository::new();
        let instance_repo = InMemoryInstanceRepository::new();

        // Register instance
        let mut instance = crate::models::MystiProxyInstance::new(
            "test".to_string(),
            "http://localhost:8081".to_string(),
        );
        let instance_id = instance.id;
        instance_repo.save(&instance).await.unwrap();

        let service = SyncService::new(mock_repo, instance_repo);

        // Push a new configuration
        let config = MockConfiguration::new(
            "Test".to_string(),
            "/test".to_string(),
            crate::models::HttpMethod::Get,
            MatchingRules::default(),
            ResponseConfig::default(),
        );

        let response = service
            .handle_push(instance_id, vec![config.clone()], vec![])
            .await
            .unwrap();

        assert_eq!(response.accepted.len(), 1);
        assert!(response.conflicts.is_empty());
    }
}
