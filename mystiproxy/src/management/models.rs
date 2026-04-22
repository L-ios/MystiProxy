//! Core data models for local mock management
//!
//! Re-exports shared models from mysti-common for use in mystiproxy.

// Re-export all models from mysti-common
pub use mysti_common::{
    // Core types
    HttpMethod, MockSource, VersionVector,
    // Matching rules
    MatchingRules,
    // Response configuration
    ResponseConfig,
    // Mock configuration
    MockConfiguration, MockFilter, MockCreateRequest, MockUpdateRequest,
    // Sync types
    SyncStatus, ConflictResolution,
    // Additional types needed
    BodyMatchType, ResponseBodyType as BodyType, ResponseBody,
    HeaderMatch, QueryParamMatch, BodyMatch, MatchType, PathPatternType,
    TemplateVarSource, TemplateVar,
};

// Additional request types specific to mystiproxy
use serde::{Deserialize, Serialize};

/// Request to create a new mock configuration (local-specific alias)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMockRequest {
    /// Human-readable name
    pub name: String,
    /// URL path pattern
    pub path: String,
    /// HTTP method
    pub method: HttpMethod,
    /// Matching rules (optional, uses defaults if not provided)
    #[serde(default)]
    pub matching_rules: MatchingRules,
    /// Response configuration (optional, uses defaults if not provided)
    #[serde(default)]
    pub response_config: ResponseConfig,
    /// Whether this configuration is active
    #[serde(default = "default_active")]
    pub is_active: bool,
}

fn default_active() -> bool {
    true
}

impl From<CreateMockRequest> for MockCreateRequest {
    fn from(req: CreateMockRequest) -> Self {
        Self {
            name: req.name,
            path: req.path,
            method: req.method,
            team_id: None,
            environment_id: None,
            matching_rules: req.matching_rules,
            response_config: req.response_config,
            state_config: None,
            is_active: req.is_active,
        }
    }
}

/// Request to update an existing mock configuration (local-specific alias)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateMockRequest {
    /// Human-readable name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// URL path pattern
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// HTTP method
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<HttpMethod>,
    /// Matching rules
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matching_rules: Option<MatchingRules>,
    /// Response configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_config: Option<ResponseConfig>,
    /// Whether this configuration is active
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_active: Option<bool>,
}

impl From<UpdateMockRequest> for MockUpdateRequest {
    fn from(req: UpdateMockRequest) -> Self {
        Self {
            name: req.name,
            path: req.path,
            method: req.method,
            matching_rules: req.matching_rules,
            response_config: req.response_config,
            state_config: None,
            version_vector: None,
            is_active: req.is_active,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_vector_domination() {
        let instance1 = uuid::Uuid::new_v4();
        let instance2 = uuid::Uuid::new_v4();

        let mut v1 = VersionVector::new();
        v1.increment(instance1);

        let mut v2 = VersionVector::new();
        v2.increment(instance1);
        v2.increment(instance1);

        assert!(v2.dominates(&v1));
        assert!(!v1.dominates(&v2));
    }

    #[test]
    fn test_version_vector_concurrent() {
        let instance1 = uuid::Uuid::new_v4();
        let instance2 = uuid::Uuid::new_v4();

        let mut v1 = VersionVector::new();
        v1.increment(instance1);

        let mut v2 = VersionVector::new();
        v2.increment(instance2);

        assert!(v1.is_concurrent_with(&v2));
    }

    #[test]
    fn test_version_vector_merge() {
        let instance1 = uuid::Uuid::new_v4();
        let instance2 = uuid::Uuid::new_v4();

        let mut v1 = VersionVector::new();
        v1.increment(instance1);
        v1.increment(instance1);

        let mut v2 = VersionVector::new();
        v2.increment(instance2);

        v1.merge(&v2);

        assert_eq!(v1.get(&instance1), 2);
        assert_eq!(v1.get(&instance2), 1);
    }

    #[test]
    fn test_mock_configuration_hash() {
        let mut config = MockConfiguration::new(
            "Test Mock".to_string(),
            "/api/test".to_string(),
            HttpMethod::Get,
            MatchingRules::default(),
            ResponseConfig::default(),
        );
        config.update_content_hash();

        assert!(!config.content_hash.is_empty());
        assert_eq!(config.content_hash.len(), 64); // SHA-256 hex length
    }
}
