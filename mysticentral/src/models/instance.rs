//! MystiProxy Instance model
//!
//! Represents a registered MystiProxy instance.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Re-export SyncStatus from mysti-common
pub use mysti_common::SyncStatus;

/// MystiProxy instance registration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MystiProxyInstance {
    pub id: Uuid,
    pub name: String,
    pub endpoint_url: String,
    pub api_key_hash: Option<String>,
    pub sync_status: SyncStatus,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub config_checksum: Option<String>,
    pub registered_at: DateTime<Utc>,
    pub last_heartbeat: Option<DateTime<Utc>>,
}

impl MystiProxyInstance {
    /// Create a new instance registration
    pub fn new(name: String, endpoint_url: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            endpoint_url,
            api_key_hash: None,
            sync_status: SyncStatus::Disconnected,
            last_sync_at: None,
            config_checksum: None,
            registered_at: Utc::now(),
            last_heartbeat: None,
        }
    }

    /// Update heartbeat
    pub fn update_heartbeat(&mut self) {
        self.last_heartbeat = Some(Utc::now());
        self.sync_status = SyncStatus::Connected;
    }

    /// Check if instance is healthy (heartbeat within last 60 seconds)
    #[allow(dead_code)]
    pub fn is_healthy(&self) -> bool {
        match self.last_heartbeat {
            Some(heartbeat) => {
                let elapsed = Utc::now() - heartbeat;
                elapsed.num_seconds() < 60
            }
            None => false,
        }
    }
}

/// Request to register a new instance
#[derive(Debug, Clone, Deserialize)]
pub struct InstanceRegisterRequest {
    pub name: String,
    pub endpoint_url: String,
    pub api_key: Option<String>,
}

/// Heartbeat request
#[derive(Debug, Clone, Deserialize)]
pub struct HeartbeatRequest {
    pub config_checksum: Option<String>,
}

/// Filter for querying instances
#[derive(Debug, Clone, Default, Deserialize)]
pub struct InstanceFilter {
    pub sync_status: Option<SyncStatus>,
    #[allow(dead_code)]
    pub is_healthy: Option<bool>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

impl InstanceFilter {
    pub fn page(&self) -> u32 {
        self.page.unwrap_or(1).max(1)
    }

    pub fn limit(&self) -> u32 {
        self.limit.unwrap_or(20).min(100).max(1)
    }

    pub fn offset(&self) -> u32 {
        (self.page() - 1) * self.limit()
    }
}
