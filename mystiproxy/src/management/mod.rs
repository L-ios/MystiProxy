//! Local Management Module for MystiProxy
//!
//! This module provides embedded SQLite-based mock configuration management
//! with offline support and synchronization capabilities.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Management Module                         │
//! ├─────────────────────────────────────────────────────────────┤
//! │  handlers.rs    - HTTP API handlers (Axum compatible)       │
//! │  repository.rs  - MockRepository trait & SQLite impl        │
//! │  db.rs          - SQLite connection & migrations            │
//! │  config.rs      - Configuration management                  │
//! │  import.rs      - YAML/JSON config file import              │
//! │  models.rs      - Core data structures                      │
//! │  sync.rs        - Synchronization client                    │
//! │  integration.rs - MystiProxy integration                   │
//! │  error.rs       - Error types                               │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Feature Flag
//!
//! This module is gated behind the `local-management` feature flag.

mod config;
mod db;
mod error;
mod handlers;
mod import;
mod integration;
mod models;
mod repository;
mod sync;

pub use config::{LocalManagementConfig, SyncConfig};
pub use error::{ManagementError, Result};
pub use handlers::create_management_router;
pub use import::import_from_file;
pub use integration::{LocalManagement, LocalManagementBuilder};
pub use models::*;
pub use repository::{LocalMockRepository, MockRepository};
pub use sync::{OfflineQueueEntry, OfflineQueueManager, RetryPolicy, SyncClient, SyncOperation};

use sqlx::SqlitePool;

/// Local management state shared across handlers
#[derive(Clone)]
pub struct ManagementState {
    pub pool: SqlitePool,
    pub config: LocalManagementConfig,
}

impl ManagementState {
    /// Create a new management state with the given pool and config
    pub fn new(pool: SqlitePool, config: LocalManagementConfig) -> Self {
        Self { pool, config }
    }
}
