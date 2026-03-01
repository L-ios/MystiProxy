//! Services module for MystiCentral
//!
//! Provides business logic and repository abstractions.

mod auth_service;
mod conflict_service;
mod environment_repository;
mod environment_service;
mod instance_repository;
mod instance_service;
mod mock_service;
mod postgres_repository;
mod repository;
mod sync_protocol;
mod sync_service;
mod websocket;

pub use auth_service::{AuthService, Claims, Permission, PermissionChecker};
pub use conflict_service::{ConflictService, ConflictStore, StoredConflict};
pub use environment_repository::{EnvironmentRepository, PostgresEnvironmentRepository};
pub use environment_service::EnvironmentService;
pub use instance_repository::{InstanceRepository, PostgresInstanceRepository};
pub use instance_service::InstanceService;
pub use mock_service::MockService;
pub use postgres_repository::PostgresMockRepository;
pub use repository::{MockRepository, InMemoryMockRepository};
pub use sync_protocol::{
    SyncMessage, SyncSource, SyncConflict, ConflictReason, ConflictResolution,
    ConflictResolveRequest, SyncPullResponse, SyncPushResponse,
};
pub use sync_service::SyncService;
pub use websocket::{WebSocketBroadcaster, WebSocketClient, ws_handler};
