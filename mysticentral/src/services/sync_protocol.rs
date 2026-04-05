//! Sync Protocol
//!
//! Defines the synchronization protocol between MystiCentral and MystiProxy instances.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::models::MockConfiguration;

/// Response to a pull request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPullResponse {
    /// Updated configurations
    pub configs: Vec<MockConfiguration>,
    /// IDs of deleted configurations
    pub deleted_ids: Vec<Uuid>,
    /// Server timestamp for next sync
    pub server_time: DateTime<Utc>,
    /// Whether full sync is required
    #[serde(default)]
    pub full_sync_required: bool,
}

/// Response to a push request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPushResponse {
    /// IDs of accepted configurations
    pub accepted: Vec<Uuid>,
    /// Conflicts detected
    pub conflicts: Vec<SyncConflict>,
    /// Server timestamp
    pub server_time: DateTime<Utc>,
}

/// Sync message types for communication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SyncMessage {
    /// Request to pull configurations from central
    SyncPullRequest {
        /// Instance ID making the request
        instance_id: Uuid,
        /// Pull configurations modified since this timestamp
        since: Option<DateTime<Utc>>,
        /// Content hashes of known configurations for conflict detection
        known_checksums: HashMap<Uuid, String>,
    },

    /// Response to a pull request
    SyncPullResponse(SyncPullResponse),

    /// Request to push local changes to central
    SyncPushRequest {
        /// Instance ID making the request
        instance_id: Uuid,
        /// Configurations to push
        configs: Vec<MockConfiguration>,
        /// IDs of deleted configurations
        deleted_ids: Vec<Uuid>,
    },

    /// Response to a push request
    SyncPushResponse(SyncPushResponse),

    /// Notification of configuration update
    ConfigUpdate {
        /// Updated configuration
        config: MockConfiguration,
        /// Source of the update
        source: SyncSource,
    },

    /// Notification of configuration deletion
    ConfigDelete {
        /// Deleted configuration ID
        id: Uuid,
    },

    /// Conflict detected notification
    ConflictDetected {
        /// Configuration ID with conflict
        config_id: Uuid,
        /// Local version
        local: MockConfiguration,
        /// Central version
        central: MockConfiguration,
    },

    /// Heartbeat from instance
    Heartbeat {
        /// Instance ID
        instance_id: Uuid,
        /// Current config checksum
        config_checksum: Option<String>,
    },

    /// Heartbeat acknowledgment
    HeartbeatAck {
        /// Whether sync is needed
        sync_needed: bool,
        /// Server timestamp
        server_time: DateTime<Utc>,
    },
}

/// Source of a sync operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncSource {
    Central,
    Instance,
}

/// Conflict information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConflict {
    /// Configuration ID
    pub config_id: Uuid,
    /// Reason for conflict
    pub reason: ConflictReason,
    /// Local version
    pub local: MockConfiguration,
    /// Central version
    pub central: MockConfiguration,
    /// Detected at
    pub detected_at: DateTime<Utc>,
}

/// Reason for conflict
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictReason {
    /// Concurrent modification detected
    ConcurrentModification,
    /// Version mismatch
    VersionMismatch,
    /// Content hash mismatch
    ContentMismatch,
}

/// Resolution strategy for conflicts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    /// Keep the central version
    KeepCentral,
    /// Keep the local version
    KeepLocal,
    /// Merge the changes
    Merge,
}

/// Request to resolve a conflict
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ConflictResolveRequest {
    /// Resolution strategy
    pub strategy: ConflictResolution,
    /// Merged configuration (for Merge strategy)
    pub merged_config: Option<MockConfiguration>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_message_serialization() {
        let msg = SyncMessage::Heartbeat {
            instance_id: Uuid::nil(),
            config_checksum: Some("abc123".to_string()),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("heartbeat"));

        let decoded: SyncMessage = serde_json::from_str(&json).unwrap();
        match decoded {
            SyncMessage::Heartbeat {
                config_checksum, ..
            } => {
                assert_eq!(config_checksum, Some("abc123".to_string()));
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_sync_pull_response() {
        let response = SyncPullResponse {
            configs: vec![],
            deleted_ids: vec![],
            server_time: Utc::now(),
            full_sync_required: false,
        };

        let json = serde_json::to_string(&response).unwrap();
        let decoded: SyncPullResponse = serde_json::from_str(&json).unwrap();
        assert!(decoded.configs.is_empty());
    }
}
