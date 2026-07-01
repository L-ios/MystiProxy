//! Synchronization client for MystiProxy local management
//!
//! Provides bidirectional sync between local MystiProxy instances and the central
//! management system with offline support and conflict detection.

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::config::SyncConfig;
use super::error::{ManagementError, Result};
use super::models::{MockConfiguration, SyncMessage, SyncStatus};
use super::repository::MockRepository;

/// Sync operation type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncOperation {
    /// Create a new configuration
    Create,
    /// Update an existing configuration
    Update,
    /// Delete a configuration
    Delete,
}

/// Offline queue entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflineQueueEntry {
    /// Unique entry ID
    pub id: i64,
    /// Operation type
    pub operation_type: SyncOperation,
    /// Configuration ID (if applicable)
    pub config_id: Option<Uuid>,
    /// Payload (JSON)
    pub payload: String,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Retry count
    pub retry_count: u32,
    /// Last error message
    pub last_error: Option<String>,
}

/// Sync client for communication with central management
pub struct SyncClient<R: MockRepository + 'static> {
    /// HTTP client
    client: Client,
    /// Repository
    repository: Arc<R>,
    /// Sync configuration
    config: SyncConfig,
    /// Instance ID
    instance_id: Uuid,
    /// Current sync status
    status: Arc<RwLock<SyncStatus>>,
    /// Offline queue sender
    offline_queue_tx: mpsc::Sender<OfflineQueueEntry>,
    /// Offline queue receiver (for background task)
    offline_queue_rx: Option<mpsc::Receiver<OfflineQueueEntry>>,
    /// Last successful sync timestamp
    last_sync: Arc<RwLock<Option<DateTime<Utc>>>>,
}

impl<R: MockRepository + 'static> SyncClient<R> {
    /// Create a new sync client
    pub fn new(repository: Arc<R>, config: SyncConfig) -> Result<Self> {
        let instance_id = config.instance_id.unwrap_or_else(Uuid::new_v4);
        
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| ManagementError::Internal(format!("Failed to create HTTP client: {}", e)))?;
        
        let (offline_queue_tx, offline_queue_rx) = mpsc::channel(config.max_queue_size);
        
        Ok(Self {
            client,
            repository,
            config,
            instance_id,
            status: Arc::new(RwLock::new(SyncStatus::Disconnected)),
            offline_queue_tx,
            offline_queue_rx: Some(offline_queue_rx),
            last_sync: Arc::new(RwLock::new(None)),
        })
    }
    
    /// Get the instance ID
    pub fn instance_id(&self) -> Uuid {
        self.instance_id
    }
    
    /// Get current sync status
    pub async fn status(&self) -> SyncStatus {
        *self.status.read().await
    }
    
    /// Get last sync timestamp
    pub async fn last_sync(&self) -> Option<DateTime<Utc>> {
        *self.last_sync.read().await
    }
    
    /// Start the sync client background task
    pub async fn start(mut self) -> Result<()> {
        if !self.config.enabled {
            info!("Sync is disabled, not starting sync client");
            return Ok(());
        }
        
        let central_url = self.config.central_url.clone()
            .ok_or_else(|| ManagementError::sync("Central URL not configured"))?;
        
        info!("Starting sync client for instance {}", self.instance_id);
        
        // Register with central
        self.register_with_central(&central_url).await?;
        
        // Start periodic sync if configured
        if self.config.sync_interval_secs > 0 {
            let sync_interval = Duration::from_secs(self.config.sync_interval_secs as u64);
            let status = self.status.clone();
            let last_sync = self.last_sync.clone();
            let repository = self.repository.clone();
            let client = self.client.clone();
            let central_url_clone = central_url.clone();
            let api_key = self.config.api_key.clone();
            let instance_id = self.instance_id;
            
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(sync_interval);
                loop {
                    interval.tick().await;
                    
                    if let Err(e) = perform_periodic_sync(
                        &client,
                        &central_url_clone,
                        &api_key,
                        instance_id,
                        &repository,
                        &status,
                        &last_sync,
                    ).await {
                        error!("Periodic sync failed: {}", e);
                    }
                }
            });
        }
        
        // Process offline queue
        if self.config.offline_queue_enabled {
            if let Some(mut rx) = self.offline_queue_rx.take() {
                let client = self.client.clone();
                let central_url_clone = central_url.clone();
                let api_key = self.config.api_key.clone();
                let status = self.status.clone();
                
                tokio::spawn(async move {
                    while let Some(entry) = rx.recv().await {
                        if let Err(e) = process_offline_entry(
                            &client,
                            &central_url_clone,
                            &api_key,
                            entry,
                            &status,
                        ).await {
                            error!("Failed to process offline entry: {}", e);
                        }
                    }
                });
            }
        }
        
        Ok(())
    }
    
    /// Register this instance with central management
    async fn register_with_central(&self, central_url: &str) -> Result<()> {
        let url = format!("{}/api/v1/instances/register", central_url);
        
        let mut status = self.status.write().await;
        *status = SyncStatus::Syncing;
        
        let response = self.client
            .post(&url)
            .header("X-Instance-ID", self.instance_id.to_string())
            .header("X-API-Key", self.config.api_key.as_deref().unwrap_or(""))
            .json(&serde_json::json!({
                "instance_id": self.instance_id,
                "timestamp": Utc::now(),
            }))
            .send()
            .await;
        
        match response {
            Ok(resp) if resp.status().is_success() => {
                info!("Successfully registered with central management");
                *status = SyncStatus::Connected;
                Ok(())
            }
            Ok(resp) => {
                let error_msg = format!("Registration failed: {}", resp.status());
                error!("{}", error_msg);
                *status = SyncStatus::Disconnected;
                Err(ManagementError::sync(error_msg))
            }
            Err(e) => {
                error!("Failed to connect to central: {}", e);
                *status = SyncStatus::Disconnected;
                Err(ManagementError::sync(format!("Connection failed: {}", e)))
            }
        }
    }
    
    /// Pull changes from central management
    pub async fn pull(&self) -> Result<Vec<MockConfiguration>> {
        let central_url = self.config.central_url.as_ref()
            .ok_or_else(|| ManagementError::sync("Central URL not configured"))?;
        
        let url = format!("{}/api/v1/sync/pull", central_url);
        
        let checksums = self.repository.get_all_hashes().await?;
        let checksums_map: HashMap<Uuid, String> = checksums.into_iter().collect();
        
        let last_sync = self.last_sync().await;
        
        let response = self.client
            .post(&url)
            .header("X-Instance-ID", self.instance_id.to_string())
            .header("X-API-Key", self.config.api_key.as_deref().unwrap_or(""))
            .json(&SyncMessage::SyncRequest {
                since: last_sync.unwrap_or_else(|| DateTime::UNIX_EPOCH),
                checksums: checksums_map,
            })
            .send()
            .await?;
        
        if !response.status().is_success() {
            return Err(ManagementError::sync(format!(
                "Pull failed: {}",
                response.status()
            )));
        }
        
        let sync_response: SyncMessage = response.json().await?;
        
        match sync_response {
            SyncMessage::SyncResponse { configs, deleted } => {
                // Apply received configurations
                for config in configs {
                    self.repository.save(&config).await?;
                }
                
                // Delete removed configurations
                for id in deleted {
                    self.repository.delete(id).await?;
                }
                
                // Update last sync timestamp
                let mut last_sync = self.last_sync.write().await;
                *last_sync = Some(Utc::now());
                
                info!("Pull completed successfully");
                Ok(vec![])
            }
            _ => Err(ManagementError::sync("Unexpected response from central")),
        }
    }
    
    /// Push a configuration change to central management
    pub async fn push(&self, operation: SyncOperation, config: &MockConfiguration) -> Result<()> {
        let central_url = self.config.central_url.as_ref()
            .ok_or_else(|| ManagementError::sync("Central URL not configured"))?;
        
        let status = self.status.read().await.clone();
        
        // If offline, queue the operation
        if status == SyncStatus::Disconnected && self.config.offline_queue_enabled {
            self.queue_offline_operation(operation, config).await?;
            return Ok(());
        }
        
        let url = format!("{}/api/v1/sync/push", central_url);
        
        let response = self.client
            .post(&url)
            .header("X-Instance-ID", self.instance_id.to_string())
            .header("X-API-Key", self.config.api_key.as_deref().unwrap_or(""))
            .json(&serde_json::json!({
                "operation": operation,
                "config": config,
            }))
            .send()
            .await?;
        
        if response.status().is_success() {
            info!("Push completed successfully for config {}", config.id);
            Ok(())
        } else if response.status().as_u16() == 409 {
            // Conflict detected
            let mut status = self.status.write().await;
            *status = SyncStatus::Conflict;
            Err(ManagementError::conflict(config.id, "Version conflict detected"))
        } else {
            Err(ManagementError::sync(format!(
                "Push failed: {}",
                response.status()
            )))
        }
    }
    
    /// Queue an operation for offline processing
    async fn queue_offline_operation(
        &self,
        operation: SyncOperation,
        config: &MockConfiguration,
    ) -> Result<()> {
        let entry = OfflineQueueEntry {
            id: 0, // Will be assigned by database
            operation_type: operation,
            config_id: Some(config.id),
            payload: serde_json::to_string(config)?,
            created_at: Utc::now(),
            retry_count: 0,
            last_error: None,
        };
        
        self.offline_queue_tx.send(entry).await
            .map_err(|e| ManagementError::sync(format!("Failed to queue offline operation: {}", e)))?;
        
        info!("Queued offline operation for config {}", config.id);
        Ok(())
    }
    
    /// Force a full sync with central
    pub async fn force_sync(&self) -> Result<()> {
        let mut status = self.status.write().await;
        *status = SyncStatus::Syncing;
        drop(status);
        
        self.pull().await?;
        
        let current_status = self.status.read().await.clone();
        let mut status = self.status.write().await;
        *status = if current_status == SyncStatus::Conflict {
            SyncStatus::Conflict
        } else {
            SyncStatus::Connected
        };
        
        Ok(())
    }
}

/// Perform periodic sync
async fn perform_periodic_sync<R: MockRepository>(
    client: &Client,
    central_url: &str,
    api_key: &Option<String>,
    instance_id: Uuid,
    repository: &Arc<R>,
    status: &Arc<RwLock<SyncStatus>>,
    last_sync: &Arc<RwLock<Option<DateTime<Utc>>>>,
) -> Result<()> {
    debug!("Performing periodic sync");
    
    let mut status_guard = status.write().await;
    *status_guard = SyncStatus::Syncing;
    drop(status_guard);
    
    // Pull changes
    let url = format!("{}/api/v1/sync/pull", central_url);
    
    let checksums = repository.get_all_hashes().await?;
    let checksums_map: HashMap<Uuid, String> = checksums.into_iter().collect();
    
    let last_sync_val = *last_sync.read().await;
    
    let response = client
        .post(&url)
        .header("X-Instance-ID", instance_id.to_string())
        .header("X-API-Key", api_key.as_deref().unwrap_or(""))
        .json(&SyncMessage::SyncRequest {
            since: last_sync_val.unwrap_or_else(|| DateTime::UNIX_EPOCH),
            checksums: checksums_map,
        })
        .send()
        .await?;
    
    if !response.status().is_success() {
        let mut status_guard = status.write().await;
        *status_guard = SyncStatus::Disconnected;
        return Err(ManagementError::sync(format!(
            "Periodic sync failed: {}",
            response.status()
        )));
    }
    
    let sync_response: SyncMessage = response.json().await?;
    
    if let SyncMessage::SyncResponse { configs, deleted } = sync_response {
        // Apply received configurations
        for config in configs {
            repository.save(&config).await?;
        }
        
        // Delete removed configurations
        for id in deleted {
            repository.delete(id).await?;
        }
        
        // Update last sync timestamp
        let mut last_sync_guard = last_sync.write().await;
        *last_sync_guard = Some(Utc::now());
        
        let mut status_guard = status.write().await;
        *status_guard = SyncStatus::Connected;
        
        info!("Periodic sync completed successfully");
    }
    
    Ok(())
}

/// Process an offline queue entry
async fn process_offline_entry(
    client: &Client,
    central_url: &str,
    api_key: &Option<String>,
    entry: OfflineQueueEntry,
    status: &Arc<RwLock<SyncStatus>>,
) -> Result<()> {
    debug!("Processing offline entry: {:?}", entry.operation_type);
    
    // Check if we're online
    let current_status = *status.read().await;
    if current_status == SyncStatus::Disconnected {
        // Re-queue with exponential backoff
        warn!("Still offline, skipping offline entry processing");
        return Err(ManagementError::sync("Still offline"));
    }
    
    let url = format!("{}/api/v1/sync/push", central_url);
    
    let config: MockConfiguration = serde_json::from_str(&entry.payload)?;
    
    let response = client
        .post(&url)
        .header("X-Instance-ID", config.id.to_string())
        .header("X-API-Key", api_key.as_deref().unwrap_or(""))
        .json(&serde_json::json!({
            "operation": entry.operation_type,
            "config": config,
        }))
        .send()
        .await?;
    
    if response.status().is_success() {
        info!("Offline entry processed successfully");
        Ok(())
    } else {
        let error_msg = format!("Failed to process offline entry: {}", response.status());
        error!("{}", error_msg);
        Err(ManagementError::sync(error_msg))
    }
}

/// Offline queue manager
pub struct OfflineQueueManager {
    /// Queue entries
    entries: Arc<RwLock<Vec<OfflineQueueEntry>>>,
    /// Maximum queue size
    max_size: usize,
}

impl OfflineQueueManager {
    /// Create a new offline queue manager
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
            max_size,
        }
    }
    
    /// Add an entry to the queue
    pub async fn push(&self, entry: OfflineQueueEntry) -> Result<()> {
        let mut entries = self.entries.write().await;
        
        if entries.len() >= self.max_size {
            // Remove oldest entry
            entries.remove(0);
            warn!("Offline queue full, removed oldest entry");
        }
        
        entries.push(entry);
        Ok(())
    }
    
    /// Get all entries
    pub async fn get_all(&self) -> Vec<OfflineQueueEntry> {
        self.entries.read().await.clone()
    }
    
    /// Remove an entry by ID
    pub async fn remove(&self, id: i64) -> Result<bool> {
        let mut entries = self.entries.write().await;
        let initial_len = entries.len();
        entries.retain(|e| e.id != id);
        Ok(entries.len() < initial_len)
    }
    
    /// Get queue size
    pub async fn size(&self) -> usize {
        self.entries.read().await.len()
    }
    
    /// Clear the queue
    pub async fn clear(&self) {
        self.entries.write().await.clear();
    }
}

/// Retry policy with exponential backoff
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retries
    pub max_retries: u32,
    /// Initial delay in milliseconds
    pub initial_delay_ms: u64,
    /// Maximum delay in milliseconds
    pub max_delay_ms: u64,
    /// Backoff multiplier
    pub multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_delay_ms: 1000,
            max_delay_ms: 60000,
            multiplier: 2.0,
        }
    }
}

impl RetryPolicy {
    /// Create a new retry policy
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Calculate delay for a given retry attempt
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let delay_ms = (self.initial_delay_ms as f64 * self.multiplier.powi(attempt as i32))
            .min(self.max_delay_ms as f64) as u64;
        Duration::from_millis(delay_ms)
    }
    
    /// Execute an operation with retry
    pub async fn execute<F, Fut, T, E>(&self, mut operation: F) -> std::result::Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = std::result::Result<T, E>>,
        E: std::fmt::Debug,
    {
        let mut attempt = 0;
        
        loop {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if attempt >= self.max_retries {
                        return Err(e);
                    }
                    
                    let delay = self.delay_for_attempt(attempt);
                    warn!(
                        "Operation failed (attempt {}/{}): {:?}, retrying in {:?}",
                        attempt + 1,
                        self.max_retries,
                        e,
                        delay
                    );
                    
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::management::db::create_memory_pool;
    use crate::management::repository::LocalMockRepository;
    
    #[test]
    fn test_retry_policy_delay() {
        let policy = RetryPolicy::default();
        
        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(1000));
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(2000));
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(4000));
        assert_eq!(policy.delay_for_attempt(10), Duration::from_millis(60000)); // Capped at max
    }
    
    #[tokio::test]
    async fn test_offline_queue_manager() {
        let manager = OfflineQueueManager::new(10);
        
        let entry = OfflineQueueEntry {
            id: 1,
            operation_type: SyncOperation::Create,
            config_id: Some(Uuid::new_v4()),
            payload: "{}".to_string(),
            created_at: Utc::now(),
            retry_count: 0,
            last_error: None,
        };
        
        manager.push(entry.clone()).await.unwrap();
        assert_eq!(manager.size().await, 1);
        
        manager.remove(1).await.unwrap();
        assert_eq!(manager.size().await, 0);
    }
    
    #[tokio::test]
    async fn test_offline_queue_max_size() {
        let manager = OfflineQueueManager::new(2);
        
        for i in 0..3 {
            let entry = OfflineQueueEntry {
                id: i,
                operation_type: SyncOperation::Create,
                config_id: Some(Uuid::new_v4()),
                payload: "{}".to_string(),
                created_at: Utc::now(),
                retry_count: 0,
                last_error: None,
            };
            manager.push(entry).await.unwrap();
        }
        
        assert_eq!(manager.size().await, 2);
    }
    
    #[tokio::test]
    async fn test_sync_client_creation() {
        let pool = create_memory_pool().await.unwrap();
        let repo = Arc::new(LocalMockRepository::with_random_instance_id(pool));
        
        let config = SyncConfig {
            enabled: false,
            central_url: None,
            instance_id: Some(Uuid::new_v4()),
            sync_interval_secs: 0,
            api_key: None,
            offline_queue_enabled: true,
            max_queue_size: 100,
        };
        
        let client = SyncClient::new(repo, config).expect("Failed to create sync client");
        assert!(!client.config.enabled);
    }
    
    #[tokio::test]
    async fn test_retry_policy_execute() {
        let policy = RetryPolicy {
            max_retries: 3,
            initial_delay_ms: 10,
            max_delay_ms: 100,
            multiplier: 2.0,
        };
        
        let mut attempts = 0;
        let result = policy.execute(|| {
            attempts += 1;
            async move {
                if attempts < 3 {
                    Err("temporary error")
                } else {
                    Ok("success")
                }
            }
        }).await;
        
        assert_eq!(result, Ok("success"));
        assert_eq!(attempts, 3);
    }
}
