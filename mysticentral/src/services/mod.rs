//! Services module for MystiCentral
//!
//! Provides business logic and repository abstractions.

pub(crate) mod auth_service;
mod bootstrap;
pub(crate) mod conflict_repository;
mod conflict_service;
mod environment_repository;
mod environment_service;
mod instance_repository;
mod instance_service;
mod mock_service;
mod postgres_repository;
mod push_service;
mod repository;
mod settings_repository;
mod sync_protocol;
mod sync_service;
pub(crate) mod user_repository;
mod websocket;

pub use auth_service::AuthService;
pub use bootstrap::ensure_admin_user;
pub use conflict_repository::{
    conflict_json, ConflictRecord, ConflictRepository, PostgresConflictRepository,
};
pub use environment_repository::{EnvironmentRepository, PostgresEnvironmentRepository};
pub use environment_service::EnvironmentService;
pub use instance_repository::{InstanceRepository, PostgresInstanceRepository};
pub use instance_service::InstanceService;
pub use mock_service::MockService;
pub use postgres_repository::PostgresMockRepository;
pub use push_service::{now_rfc3339, push_to_all, push_to_instance, summarize};
pub use repository::MockRepository;
#[allow(unused_imports)]
pub use settings_repository::{
    PostgresSettingsRepository, SettingsPatch, SettingsRepository, SystemSettings,
};
#[allow(unused_imports)]
pub use sync_protocol::{
    ConflictReason, ConflictResolution, SyncConflict, SyncPullResponse, SyncPushResponse,
};
#[allow(unused_imports)]
pub use user_repository::{PostgresUserRepository, UserRepository};
