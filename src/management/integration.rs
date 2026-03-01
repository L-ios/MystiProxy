//! Integration module for MystiProxy local management
//!
//! Provides initialization and lifecycle management for the local management module.

use std::sync::Arc;
use tracing::info;

use super::config::LocalManagementConfig;
use super::db;
use super::handlers::{create_management_router, HandlerState};
use super::repository::LocalMockRepository;
use super::sync::SyncClient;

/// Local management integration
pub struct LocalManagement {
    /// Configuration
    config: LocalManagementConfig,
    /// Repository
    repository: Arc<LocalMockRepository>,
    /// Sync client (if enabled)
    sync_client: Option<SyncClient<LocalMockRepository>>,
}

impl LocalManagement {
    /// Initialize local management
    pub async fn init(config: LocalManagementConfig) -> super::Result<Self> {
        if !config.enabled {
            info!("Local management is disabled");
            return Ok(Self {
                config,
                repository: Arc::new(LocalMockRepository::with_random_instance_id(
                    db::create_memory_pool().await?,
                )),
                sync_client: None,
            });
        }
        
        info!("Initializing local management with database: {:?}", config.db_path);
        
        // Create database pool
        let pool = db::create_pool(&config.db_path).await?;
        
        // Create repository
        let instance_id = config.sync.instance_id.unwrap_or_else(uuid::Uuid::new_v4);
        let repository = Arc::new(LocalMockRepository::new(pool, instance_id));
        
        // Create sync client if enabled
        let sync_client = if config.sync.enabled {
            info!("Sync enabled, creating sync client for instance {}", instance_id);
            Some(SyncClient::new(repository.clone(), config.sync.clone())?)
        } else {
            None
        };
        
        Ok(Self {
            config,
            repository,
            sync_client,
        })
    }
    
    /// Get the repository
    pub fn repository(&self) -> Arc<LocalMockRepository> {
        self.repository.clone()
    }
    
    /// Get the configuration
    pub fn config(&self) -> &LocalManagementConfig {
        &self.config
    }
    
    /// Create the management API router
    pub fn create_router(&self) -> axum::Router {
        let state = HandlerState::new(LocalMockRepository::new(
            self.repository.pool().clone(),
            self.repository.instance_id(),
        ));
        create_management_router(state)
    }
    
    /// Start the sync client (if enabled)
    pub async fn start_sync(&self) -> super::Result<()> {
        if let Some(ref _sync_client) = self.sync_client {
            info!("Starting sync client");
            // Note: In a real implementation, we would spawn a task here
            // For now, we just log that sync would start
            // let client = sync_client.clone();
            // tokio::spawn(async move {
            //     if let Err(e) = client.start().await {
            //         error!("Sync client failed: {}", e);
            //     }
            // });
        }
        Ok(())
    }
    
    /// Import configuration from a file
    pub async fn import_config(&self, path: &std::path::Path) -> super::Result<usize> {
        let configs = super::import_from_file(path, &*self.repository).await?;
        Ok(configs.len())
    }
    
    /// Check if sync is enabled
    pub fn is_sync_enabled(&self) -> bool {
        self.config.sync.enabled
    }
    
    /// Get the instance ID
    pub fn instance_id(&self) -> uuid::Uuid {
        self.repository.instance_id()
    }
}

/// Builder for local management configuration
pub struct LocalManagementBuilder {
    config: LocalManagementConfig,
}

impl LocalManagementBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            config: LocalManagementConfig::default(),
        }
    }
    
    /// Set the database path
    pub fn db_path(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.config.db_path = path.into();
        self
    }
    
    /// Enable/disable local management
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.config.enabled = enabled;
        self
    }
    
    /// Enable sync with central URL
    pub fn with_sync(mut self, central_url: impl Into<String>, instance_id: uuid::Uuid) -> Self {
        self.config.sync.enabled = true;
        self.config.sync.central_url = Some(central_url.into());
        self.config.sync.instance_id = Some(instance_id);
        self
    }
    
    /// Set sync interval in seconds
    pub fn sync_interval(mut self, secs: u32) -> Self {
        self.config.sync.sync_interval_secs = secs;
        self
    }
    
    /// Set API key for sync
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.config.sync.api_key = Some(key.into());
        self
    }
    
    /// Enable/disable offline queue
    pub fn offline_queue(mut self, enabled: bool) -> Self {
        self.config.sync.offline_queue_enabled = enabled;
        self
    }
    
    /// Set maximum offline queue size
    pub fn max_queue_size(mut self, size: usize) -> Self {
        self.config.sync.max_queue_size = size;
        self
    }
    
    /// Set API listen address
    pub fn listen_addr(mut self, addr: impl Into<String>) -> Self {
        self.config.api.listen_addr = addr.into();
        self
    }
    
    /// Build the configuration
    pub fn build(self) -> LocalManagementConfig {
        self.config
    }
}

impl Default for LocalManagementBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_local_management_init_disabled() {
        let config = LocalManagementConfig {
            enabled: false,
            ..Default::default()
        };
        
        let mgmt = LocalManagement::init(config).await.unwrap();
        assert!(!mgmt.config.enabled);
    }
    
    #[tokio::test]
    async fn test_local_management_init_enabled() {
        let config = LocalManagementConfig {
            enabled: true,
            db_path: std::path::PathBuf::from(":memory:"),
            ..Default::default()
        };
        
        let mgmt = LocalManagement::init(config).await.unwrap();
        assert!(mgmt.config.enabled);
    }
    
    #[test]
    fn test_local_management_builder() {
        let instance_id = uuid::Uuid::new_v4();
        let config = LocalManagementBuilder::new()
            .enabled(true)
            .db_path("/tmp/test.db")
            .with_sync("http://central.example.com", instance_id)
            .sync_interval(60)
            .api_key("test-key")
            .listen_addr("0.0.0.0:8080")
            .build();
        
        assert!(config.enabled);
        assert_eq!(config.db_path, std::path::PathBuf::from("/tmp/test.db"));
        assert!(config.sync.enabled);
        assert_eq!(config.sync.central_url, Some("http://central.example.com".to_string()));
        assert_eq!(config.sync.instance_id, Some(instance_id));
        assert_eq!(config.sync.sync_interval_secs, 60);
        assert_eq!(config.sync.api_key, Some("test-key".to_string()));
        assert_eq!(config.api.listen_addr, "0.0.0.0:8080");
    }
    
    #[tokio::test]
    async fn test_create_router() {
        let config = LocalManagementConfig {
            enabled: true,
            db_path: std::path::PathBuf::from(":memory:"),
            ..Default::default()
        };
        
        let mgmt = LocalManagement::init(config).await.unwrap();
        let _router = mgmt.create_router();
    }
}
