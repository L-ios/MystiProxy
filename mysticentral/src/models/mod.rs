//! Data models for MystiCentral
//!
//! Re-exports shared models from mysti-common and defines central-specific models.

// Re-export all shared models from mysti-common
pub use mysti_common::{
    // Core types
    HttpMethod, MockSource, VersionVector,
    // Matching rules
    MatchingRules, PathPatternType, MatchType, HeaderMatch, QueryParamMatch, BodyMatch,
    BodyMatchType,
    // Response configuration
    ResponseConfig, ResponseBody, ResponseBodyType, TemplateVar, TemplateVarSource,
    // State machine
    StateConfig, StateTransition, StateTrigger, StateTriggerType,
    // Mock configuration
    MockConfiguration, MockFilter, MockCreateRequest, MockUpdateRequest,
    // Sync types
    SyncStatus, SyncMessage, ConflictResolution,
};

// Central-specific models
pub mod environment;
pub mod instance;
pub mod user;

pub use environment::{
    Environment, EnvironmentCreateRequest, EnvironmentUpdateRequest, EnvironmentFilter,
};
pub use instance::{
    MystiProxyInstance, InstanceRegisterRequest, HeartbeatRequest, InstanceFilter,
};
pub use user::{
    User, Team, TeamMember, UserRole, LoginRequest, LoginResponse, UserInfo,
    CreateUserRequest, UpdateUserRequest,
};
