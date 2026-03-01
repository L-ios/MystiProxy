//! Conflict Service
//!
//! Handles detection, storage, and resolution of synchronization conflicts.

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::models::MockConfiguration;
use crate::services::{ConflictReason, ConflictResolution, SyncConflict};

/// In-memory conflict store (in production, use database)
pub type ConflictStore = Arc<RwLock<HashMap<Uuid, StoredConflict>>>;

/// Stored conflict with metadata
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StoredConflict {
    pub id: Uuid,
    pub config_id: Uuid,
    pub reason: ConflictReason,
    pub local: MockConfiguration,
    pub central: MockConfiguration,
    pub detected_at: DateTime<Utc>,
    pub resolved: bool,
    pub resolution: Option<ConflictResolution>,
    pub resolved_at: Option<DateTime<Utc>>,
}

impl From<SyncConflict> for StoredConflict {
    fn from(conflict: SyncConflict) -> Self {
        Self {
            id: Uuid::new_v4(),
            config_id: conflict.config_id,
            reason: conflict.reason,
            local: conflict.local,
            central: conflict.central,
            detected_at: conflict.detected_at,
            resolved: false,
            resolution: None,
            resolved_at: None,
        }
    }
}

/// Service for managing conflicts
#[allow(dead_code)]
pub struct ConflictService {
    store: ConflictStore,
}

impl ConflictService {
    /// Create a new ConflictService
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create with a shared store
    #[allow(dead_code)]
    pub fn with_store(store: ConflictStore) -> Self {
        Self { store }
    }

    /// Store a new conflict
    #[allow(dead_code)]
    pub async fn store_conflict(&self, conflict: SyncConflict) -> Uuid {
        let stored = StoredConflict::from(conflict);
        let id = stored.id;

        let mut store = self.store.write().await;
        store.insert(id, stored);

        tracing::warn!("Conflict stored: {}", id);
        id
    }

    /// Get a conflict by ID
    #[allow(dead_code)]
    pub async fn get_conflict(&self, id: Uuid) -> Option<StoredConflict> {
        let store = self.store.read().await;
        store.get(&id).cloned()
    }

    /// List all unresolved conflicts
    #[allow(dead_code)]
    pub async fn list_unresolved(&self) -> Vec<StoredConflict> {
        let store = self.store.read().await;
        store
            .values()
            .filter(|c| !c.resolved)
            .cloned()
            .collect()
    }

    /// List all conflicts
    #[allow(dead_code)]
    pub async fn list_all(&self) -> Vec<StoredConflict> {
        let store = self.store.read().await;
        store.values().cloned().collect()
    }

    /// Resolve a conflict
    #[allow(dead_code)]
    pub async fn resolve(
        &self,
        id: Uuid,
        resolution: ConflictResolution,
        merged_config: Option<MockConfiguration>,
    ) -> ApiResult<MockConfiguration> {
        let mut store = self.store.write().await;

        let conflict = store
            .get_mut(&id)
            .ok_or_else(|| ApiError::NotFound(format!("Conflict {} not found", id)))?;

        if conflict.resolved {
            return Err(ApiError::BadRequest("Conflict already resolved".to_string()));
        }

        let resolved_config = match resolution {
            ConflictResolution::KeepCentral => conflict.central.clone(),
            ConflictResolution::KeepLocal => {
                let mut config = conflict.local.clone();
                config.version_vector.increment(Uuid::new_v4());
                config
            }
            ConflictResolution::Merge => {
                merged_config.ok_or_else(|| {
                    ApiError::BadRequest("Merged configuration required for merge resolution".to_string())
                })?
            }
        };

        conflict.resolved = true;
        conflict.resolution = Some(resolution);
        conflict.resolved_at = Some(Utc::now());

        tracing::info!("Conflict {} resolved with {:?}", id, resolution);

        Ok(resolved_config)
    }

    /// Get conflict count
    #[allow(dead_code)]
    pub async fn count(&self) -> usize {
        self.store.read().await.len()
    }

    /// Get unresolved conflict count
    #[allow(dead_code)]
    pub async fn unresolved_count(&self) -> usize {
        self.store
            .read()
            .await
            .values()
            .filter(|c| !c.resolved)
            .count()
    }

    /// Clear resolved conflicts older than a given time
    #[allow(dead_code)]
    pub async fn cleanup_resolved(&self, older_than: DateTime<Utc>) -> usize {
        let mut store = self.store.write().await;
        let initial_len = store.len();

        store.retain(|_, c| {
            !c.resolved || c.resolved_at.map_or(true, |t| t > older_than)
        });

        initial_len - store.len()
    }
}

impl Default for ConflictService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{HttpMethod, MatchingRules, ResponseConfig};

    fn create_test_config(name: &str) -> MockConfiguration {
        MockConfiguration::new(
            name.to_string(),
            "/test".to_string(),
            HttpMethod::Get,
            MatchingRules::default(),
            ResponseConfig::default(),
        )
    }

    #[tokio::test]
    async fn test_store_and_get_conflict() {
        let service = ConflictService::new();

        let config1 = create_test_config("local");
        let config2 = create_test_config("central");

        let conflict = SyncConflict {
            config_id: config1.id,
            reason: ConflictReason::ConcurrentModification,
            local: config1,
            central: config2,
            detected_at: Utc::now(),
        };

        let id = service.store_conflict(conflict).await;
        let stored = service.get_conflict(id).await;

        assert!(stored.is_some());
        assert!(!stored.unwrap().resolved);
    }

    #[tokio::test]
    async fn test_resolve_conflict() {
        let service = ConflictService::new();

        let config1 = create_test_config("local");
        let config2 = create_test_config("central");

        let conflict = SyncConflict {
            config_id: config1.id,
            reason: ConflictReason::ConcurrentModification,
            local: config1,
            central: config2,
            detected_at: Utc::now(),
        };

        let id = service.store_conflict(conflict).await;

        let resolved = service
            .resolve(id, ConflictResolution::KeepCentral, None)
            .await
            .unwrap();

        assert_eq!(resolved.name, "central");

        let stored = service.get_conflict(id).await.unwrap();
        assert!(stored.resolved);
    }

    #[tokio::test]
    async fn test_list_unresolved() {
        let service = ConflictService::new();

        // Add two conflicts
        let config1 = create_test_config("local1");
        let config2 = create_test_config("central1");
        let conflict1 = SyncConflict {
            config_id: config1.id,
            reason: ConflictReason::ConcurrentModification,
            local: config1,
            central: config2,
            detected_at: Utc::now(),
        };

        let config3 = create_test_config("local2");
        let config4 = create_test_config("central2");
        let conflict2 = SyncConflict {
            config_id: config3.id,
            reason: ConflictReason::VersionMismatch,
            local: config3,
            central: config4,
            detected_at: Utc::now(),
        };

        service.store_conflict(conflict1).await;
        let id2 = service.store_conflict(conflict2).await;

        assert_eq!(service.unresolved_count().await, 2);

        // Resolve one
        service
            .resolve(id2, ConflictResolution::KeepLocal, None)
            .await
            .unwrap();

        assert_eq!(service.unresolved_count().await, 1);
    }
}
