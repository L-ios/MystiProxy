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
pub use models::{Environment, MockConfiguration, MockFilter, MystiProxyInstance, VersionVector};
pub use services::{
    AuthService, EnvironmentService, InstanceService, MockRepository, MockService,
    PostgresMockRepository,
};
