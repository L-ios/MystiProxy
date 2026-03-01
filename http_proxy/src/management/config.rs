//! Configuration for local management module

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Local management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalManagementConfig {
    /// SQLite database path
    #[serde(default = "default_db_path")]
    pub db_path: PathBuf,

    /// Enable/disable local management
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Sync configuration
    #[serde(default)]
    pub sync: SyncConfig,

    /// API server configuration
    #[serde(default)]
    pub api: ApiConfig,
}

fn default_db_path() -> PathBuf {
    PathBuf::from("mystiproxy.db")
}

fn default_enabled() -> bool {
    true
}

impl Default for LocalManagementConfig {
    fn default() -> Self {
        Self {
            db_path: default_db_path(),
            enabled: default_enabled(),
            sync: SyncConfig::default(),
            api: ApiConfig::default(),
        }
    }
}

/// Synchronization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Enable synchronization with central
    #[serde(default)]
    pub enabled: bool,

    /// Central server URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub central_url: Option<String>,

    /// Instance ID for this MystiProxy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<uuid::Uuid>,

    /// Sync interval in seconds (0 = manual sync only)
    #[serde(default)]
    pub sync_interval_secs: u32,

    /// API key for authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Enable offline queue
    #[serde(default = "default_offline_queue")]
    pub offline_queue_enabled: bool,

    /// Maximum offline queue size
    #[serde(default = "default_max_queue_size")]
    pub max_queue_size: usize,
}

fn default_offline_queue() -> bool {
    true
}

fn default_max_queue_size() -> usize {
    1000
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            central_url: None,
            instance_id: None,
            sync_interval_secs: 0,
            api_key: None,
            offline_queue_enabled: default_offline_queue(),
            max_queue_size: default_max_queue_size(),
        }
    }
}

/// API server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// API listen address
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,

    /// Enable API authentication
    #[serde(default)]
    pub auth_enabled: bool,

    /// API token (if auth is enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_token: Option<String>,
}

fn default_listen_addr() -> String {
    "127.0.0.1:9090".to_string()
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            auth_enabled: false,
            api_token: None,
        }
    }
}

impl LocalManagementConfig {
    /// Load configuration from a file
    pub fn from_file(path: &std::path::Path) -> crate::management::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = if path.extension().map_or(false, |ext| ext == "yaml" || ext == "yml") {
            serde_yaml::from_str(&content)?
        } else {
            serde_json::from_str(&content)?
        };
        Ok(config)
    }

    /// Save configuration to a file
    pub fn to_file(&self, path: &std::path::Path) -> crate::management::Result<()> {
        let content = if path.extension().map_or(false, |ext| ext == "yaml" || ext == "yml") {
            serde_yaml::to_string(self)?
        } else {
            serde_json::to_string_pretty(self)?
        };
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Create a new configuration with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the database path
    pub fn with_db_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.db_path = path.into();
        self
    }

    /// Enable synchronization
    pub fn with_sync(mut self, central_url: impl Into<String>, instance_id: uuid::Uuid) -> Self {
        self.sync.enabled = true;
        self.sync.central_url = Some(central_url.into());
        self.sync.instance_id = Some(instance_id);
        self
    }

    /// Set the API listen address
    pub fn with_listen_addr(mut self, addr: impl Into<String>) -> Self {
        self.api.listen_addr = addr.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = LocalManagementConfig::default();
        assert!(config.enabled);
        assert!(!config.sync.enabled);
        assert_eq!(config.api.listen_addr, "127.0.0.1:9090");
    }

    #[test]
    fn test_config_builder() {
        let config = LocalManagementConfig::new()
            .with_db_path("/tmp/test.db")
            .with_listen_addr("0.0.0.0:8080");

        assert_eq!(config.db_path, PathBuf::from("/tmp/test.db"));
        assert_eq!(config.api.listen_addr, "0.0.0.0:8080");
    }
}
