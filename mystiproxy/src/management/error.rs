//! Error types for local management module

use thiserror::Error;

/// Result type alias for management operations
pub type Result<T> = std::result::Result<T, ManagementError>;

/// Management error types
#[derive(Debug, Error)]
pub enum ManagementError {
    /// Database operation failed
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Mock not found
    #[error("Mock configuration not found: {0}")]
    NotFound(uuid::Uuid),

    /// Invalid input
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Import error
    #[error("Import error: {0}")]
    Import(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// YAML parsing error
    #[error("YAML parsing error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// Sync error
    #[error("Sync error: {0}")]
    Sync(String),

    /// HTTP request error
    #[error("HTTP request error: {0}")]
    Http(String),

    /// Conflict detected
    #[error("Conflict detected for config {id}: {message}")]
    Conflict {
        id: uuid::Uuid,
        message: String,
    },

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<reqwest::Error> for ManagementError {
    fn from(err: reqwest::Error) -> Self {
        Self::Http(err.to_string())
    }
}

impl ManagementError {
    /// Create a new not found error
    pub fn not_found(id: uuid::Uuid) -> Self {
        Self::NotFound(id)
    }

    /// Create a new invalid input error
    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::InvalidInput(msg.into())
    }

    /// Create a new config error
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// Create a new import error
    pub fn import(msg: impl Into<String>) -> Self {
        Self::Import(msg.into())
    }

    /// Create a new sync error
    pub fn sync(msg: impl Into<String>) -> Self {
        Self::Sync(msg.into())
    }

    /// Create a new conflict error
    pub fn conflict(id: uuid::Uuid, msg: impl Into<String>) -> Self {
        Self::Conflict {
            id,
            message: msg.into(),
        }
    }

    /// Check if this is a not found error
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound(_))
    }

    /// Check if this is a conflict error
    pub fn is_conflict(&self) -> bool {
        matches!(self, Self::Conflict { .. })
    }
}
