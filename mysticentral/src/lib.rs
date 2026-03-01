//! MystiCentral Library
//!
//! Public API for the central management system.

pub mod config;
pub mod db;
pub mod error;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod services;

// Re-exports for convenience
pub use config::Config;
pub use error::ApiError;
pub use models::{
    MockConfiguration, MockFilter, VersionVector,
    Environment, MystiProxyInstance, User, UserRole,
};
pub use services::{
    MockRepository, InMemoryMockRepository, PostgresMockRepository,
    MockService, EnvironmentService, InstanceService,
    SyncService, ConflictService, AuthService,
};
