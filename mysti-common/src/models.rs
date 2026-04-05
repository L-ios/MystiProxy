//! Core data models for MystiProxy ecosystem
//!
//! These models are shared between mysticentral and http_proxy to ensure
//! consistency and avoid duplication.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// HTTP Method
// ============================================================================

/// HTTP method for mock matching
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
    /// Match any method
    #[serde(rename = "*")]
    Any,
}

impl Default for HttpMethod {
    fn default() -> Self {
        Self::Get
    }
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpMethod::Get => write!(f, "GET"),
            HttpMethod::Post => write!(f, "POST"),
            HttpMethod::Put => write!(f, "PUT"),
            HttpMethod::Delete => write!(f, "DELETE"),
            HttpMethod::Patch => write!(f, "PATCH"),
            HttpMethod::Head => write!(f, "HEAD"),
            HttpMethod::Options => write!(f, "OPTIONS"),
            HttpMethod::Any => write!(f, "*"),
        }
    }
}

impl std::str::FromStr for HttpMethod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "GET" => Ok(HttpMethod::Get),
            "POST" => Ok(HttpMethod::Post),
            "PUT" => Ok(HttpMethod::Put),
            "DELETE" => Ok(HttpMethod::Delete),
            "PATCH" => Ok(HttpMethod::Patch),
            "HEAD" => Ok(HttpMethod::Head),
            "OPTIONS" => Ok(HttpMethod::Options),
            "*" => Ok(HttpMethod::Any),
            _ => Err(format!("Invalid HTTP method: {}", s)),
        }
    }
}

impl From<String> for HttpMethod {
    fn from(s: String) -> Self {
        s.parse().unwrap_or(HttpMethod::Get)
    }
}

impl From<&str> for HttpMethod {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(HttpMethod::Get)
    }
}

// ============================================================================
// Mock Source
// ============================================================================

/// Source of the mock configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MockSource {
    /// Created/modified in central system
    #[default]
    Central,
    /// Created/modified locally
    Local,
}

// ============================================================================
// Version Vector (Vector Clock)
// ============================================================================

/// Version vector for conflict detection using vector clocks
/// Maps instance_id -> counter
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct VersionVector(pub HashMap<Uuid, u64>);

impl VersionVector {
    /// Create a new empty version vector
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Create a new version vector with an initial instance
    pub fn with_instance(instance_id: Uuid) -> Self {
        let mut map = HashMap::new();
        map.insert(instance_id, 1);
        Self(map)
    }

    /// Increment the counter for an instance
    pub fn increment(&mut self, instance_id: Uuid) {
        *self.0.entry(instance_id).or_insert(0) += 1;
    }

    /// Get the counter for an instance
    pub fn get(&self, instance_id: &Uuid) -> u64 {
        self.0.get(instance_id).copied().unwrap_or(0)
    }

    /// Merge with another version vector (taking the maximum of each component)
    pub fn merge(&mut self, other: &VersionVector) {
        for (id, counter) in &other.0 {
            let entry = self.0.entry(*id).or_insert(0);
            *entry = (*entry).max(*counter);
        }
    }

    /// Check if this version vector dominates another
    /// Returns true if all components are >= other's components and at least one is >
    pub fn dominates(&self, other: &VersionVector) -> bool {
        let mut has_greater = false;

        for (id, counter) in &self.0 {
            let other_counter = other.get(id);
            if *counter < other_counter {
                return false;
            }
            if *counter > other_counter {
                has_greater = true;
            }
        }

        // Check for keys in other but not in self
        for id in other.0.keys() {
            if !self.0.contains_key(id) {
                return false;
            }
        }

        has_greater
    }

    /// Check if two version vectors are concurrent (potential conflict)
    /// Returns true if neither dominates the other
    pub fn is_concurrent_with(&self, other: &VersionVector) -> bool {
        !self.dominates(other) && !other.dominates(self) && self != other
    }

    /// Get the inner map
    pub fn as_map(&self) -> &HashMap<Uuid, u64> {
        &self.0
    }

    /// Check if the version vector is empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<HashMap<Uuid, u64>> for VersionVector {
    fn from(map: HashMap<Uuid, u64>) -> Self {
        Self(map)
    }
}

impl From<VersionVector> for HashMap<Uuid, u64> {
    fn from(vv: VersionVector) -> Self {
        vv.0
    }
}

// ============================================================================
// Matching Rules
// ============================================================================

/// Path pattern matching type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PathPatternType {
    /// Exact path match
    #[default]
    Exact,
    /// Prefix match
    Prefix,
    /// Regex pattern match
    Regex,
}

/// Match type for header/query/body values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MatchType {
    /// Exact string match
    #[default]
    Exact,
    /// Regex pattern match
    Regex,
    /// Check if exists (non-empty)
    Exists,
    /// JSONPath match
    JsonPath,
}

/// Header matching rule
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderMatch {
    /// Header name (case-insensitive)
    pub name: String,
    /// Header value pattern
    pub value: String,
    /// Match type
    #[serde(default)]
    pub match_type: MatchType,
}

/// Query parameter matching rule
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryParamMatch {
    /// Parameter name
    pub name: String,
    /// Parameter value pattern
    pub value: String,
    /// Match type
    #[serde(default)]
    pub match_type: MatchType,
}

/// Body matching rule
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BodyMatch {
    /// JSONPath expression for body matching
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_path: Option<String>,
    /// Expected value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Match type
    #[serde(default)]
    pub match_type: BodyMatchType,
}

/// Body match type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BodyMatchType {
    #[default]
    Exact,
    Regex,
    JsonPath,
}

/// Matching rules for incoming requests
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MatchingRules {
    /// Path pattern (e.g., "/api/users/:id")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_pattern: Option<String>,
    /// Path pattern type
    #[serde(default)]
    pub path_pattern_type: PathPatternType,
    /// Header matching rules
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<HeaderMatch>,
    /// Query parameter matching rules
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query_params: Vec<QueryParamMatch>,
    /// Body matching rule
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<BodyMatch>,
}

// ============================================================================
// Response Configuration
// ============================================================================

/// Response body type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ResponseBodyType {
    /// Static response body
    #[default]
    Static,
    /// Template with variable substitution
    Template,
    /// Load from file
    File,
    /// Script-generated response
    Script,
}

/// Template variable source
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemplateVarSource {
    /// Extract from path parameter
    Path,
    /// Extract from query parameter
    Query,
    /// Extract from header
    Header,
    /// Extract from request body
    Body,
}

/// Template variable definition
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateVar {
    /// Variable name
    pub name: String,
    /// Source of the variable
    pub source: TemplateVarSource,
    /// Path/expression to extract value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Response body configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ResponseBody {
    /// Body type
    #[serde(default, rename = "type")]
    pub body_type: ResponseBodyType,
    /// Body content
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Template variables (for template type)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub template_vars: Vec<TemplateVar>,
}

/// Response configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseConfig {
    /// HTTP status code
    #[serde(default = "default_status")]
    pub status: u16,
    /// Response headers
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
    /// Response body
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<ResponseBody>,
    /// Response delay in milliseconds
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<u32>,
}

fn default_status() -> u16 {
    200
}

impl Default for ResponseConfig {
    fn default() -> Self {
        Self {
            status: 200,
            headers: HashMap::new(),
            body: None,
            delay_ms: None,
        }
    }
}

// ============================================================================
// State Machine Configuration
// ============================================================================

/// State trigger type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StateTriggerType {
    Request,
    Body,
    Header,
}

/// State trigger definition
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateTrigger {
    /// Trigger type
    #[serde(rename = "type")]
    pub trigger_type: StateTriggerType,
    /// Trigger condition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

/// State transition definition
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateTransition {
    /// Source state
    pub from_state: String,
    /// Target state
    pub to_state: String,
    /// Transition trigger
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<StateTrigger>,
    /// Response for this transition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<ResponseConfig>,
}

/// State machine configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateConfig {
    /// Initial state
    pub initial_state: String,
    /// State transitions
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transitions: Vec<StateTransition>,
}

// ============================================================================
// Mock Configuration
// ============================================================================

/// Mock configuration - core entity
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MockConfiguration {
    /// Unique identifier
    pub id: Uuid,
    /// Human-readable name
    pub name: String,
    /// URL path pattern
    pub path: String,
    /// HTTP method
    pub method: HttpMethod,
    /// Team ID (for multi-tenancy)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<Uuid>,
    /// Environment ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_id: Option<Uuid>,
    /// Matching rules
    pub matching_rules: MatchingRules,
    /// Response configuration
    pub response_config: ResponseConfig,
    /// State machine configuration (for stateful mocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_config: Option<StateConfig>,
    /// Configuration source
    #[serde(default)]
    pub source: MockSource,
    /// Version vector for conflict detection
    #[serde(default)]
    pub version_vector: VersionVector,
    /// SHA-256 content hash
    #[serde(default)]
    pub content_hash: String,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    /// Creator user ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<Uuid>,
    /// Whether this configuration is active
    #[serde(default = "default_active")]
    pub is_active: bool,
}

fn default_active() -> bool {
    true
}

impl MockConfiguration {
    /// Create a new mock configuration with generated ID and timestamps
    pub fn new(
        name: String,
        path: String,
        method: HttpMethod,
        matching_rules: MatchingRules,
        response_config: ResponseConfig,
    ) -> Self {
        let now = Utc::now();
        let id = Uuid::new_v4();
        let version_vector = VersionVector::with_instance(id);

        let content_hash =
            Self::compute_content_hash(&name, &path, &method, &matching_rules, &response_config);

        Self {
            id,
            name,
            path,
            method,
            team_id: None,
            environment_id: None,
            matching_rules,
            response_config,
            state_config: None,
            source: MockSource::default(),
            version_vector,
            content_hash,
            created_at: now,
            updated_at: now,
            created_by: None,
            is_active: true,
        }
    }

    /// Compute SHA-256 hash of the content
    pub fn compute_content_hash(
        name: &str,
        path: &str,
        method: &HttpMethod,
        matching_rules: &MatchingRules,
        response_config: &ResponseConfig,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(name.as_bytes());
        hasher.update(path.as_bytes());
        hasher.update(method.to_string().as_bytes());
        hasher.update(
            serde_json::to_string(matching_rules)
                .unwrap_or_default()
                .as_bytes(),
        );
        hasher.update(
            serde_json::to_string(response_config)
                .unwrap_or_default()
                .as_bytes(),
        );
        hex::encode(hasher.finalize())
    }

    /// Update the content hash based on current values
    pub fn update_content_hash(&mut self) {
        self.content_hash = Self::compute_content_hash(
            &self.name,
            &self.path,
            &self.method,
            &self.matching_rules,
            &self.response_config,
        );
    }

    /// Update the version vector for a given instance
    pub fn touch(&mut self, instance_id: Uuid) {
        self.updated_at = Utc::now();
        self.version_vector.increment(instance_id);
        self.update_content_hash();
    }
}

// ============================================================================
// Request/Response DTOs
// ============================================================================

/// Filter parameters for listing mock configurations
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MockFilter {
    /// Filter by environment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<Uuid>,
    /// Filter by team
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team: Option<Uuid>,
    /// Filter by path pattern (substring match)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Filter by HTTP method
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<HttpMethod>,
    /// Filter by active status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_active: Option<bool>,
    /// Filter by source
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<MockSource>,
    /// Page number (1-indexed)
    pub page: Option<u32>,
    /// Maximum number of results per page
    pub limit: Option<u32>,
    /// Offset for pagination
    pub offset: Option<u32>,
}

impl MockFilter {
    /// Get page number (default: 1)
    pub fn page(&self) -> u32 {
        self.page.unwrap_or(1).max(1)
    }

    /// Get limit (default: 20, max: 100)
    pub fn limit(&self) -> u32 {
        self.limit.unwrap_or(20).min(100).max(1)
    }

    /// Calculate offset from page
    pub fn offset(&self) -> u32 {
        self.offset
            .unwrap_or_else(|| (self.page() - 1) * self.limit())
    }
}

/// Request to create a new mock configuration
#[derive(Debug, Clone, Deserialize)]
pub struct MockCreateRequest {
    /// Human-readable name
    pub name: String,
    /// URL path pattern
    pub path: String,
    /// HTTP method
    pub method: HttpMethod,
    /// Team ID (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<Uuid>,
    /// Environment ID (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_id: Option<Uuid>,
    /// Matching rules (optional, uses defaults if not provided)
    #[serde(default)]
    pub matching_rules: MatchingRules,
    /// Response configuration (optional, uses defaults if not provided)
    #[serde(default)]
    pub response_config: ResponseConfig,
    /// State machine configuration (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_config: Option<StateConfig>,
    /// Whether this configuration is active
    #[serde(default = "default_active")]
    pub is_active: bool,
}

/// Request to update an existing mock configuration
#[derive(Debug, Clone, Deserialize, Default)]
pub struct MockUpdateRequest {
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
    /// State machine configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_config: Option<StateConfig>,
    /// Version vector for optimistic concurrency
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_vector: Option<VersionVector>,
    /// Whether this configuration is active
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_active: Option<bool>,
}

// ============================================================================
// Sync Types
// ============================================================================

/// Sync status of a MystiProxy instance
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SyncStatus {
    /// Connected and synced
    Connected,
    /// Disconnected from central
    #[default]
    Disconnected,
    /// Currently syncing
    Syncing,
    /// Conflict detected
    Conflict,
    /// Error during sync
    Error,
}

impl std::fmt::Display for SyncStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncStatus::Connected => write!(f, "connected"),
            SyncStatus::Disconnected => write!(f, "disconnected"),
            SyncStatus::Syncing => write!(f, "syncing"),
            SyncStatus::Conflict => write!(f, "conflict"),
            SyncStatus::Error => write!(f, "error"),
        }
    }
}

impl std::str::FromStr for SyncStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "connected" => Ok(Self::Connected),
            "disconnected" => Ok(Self::Disconnected),
            "syncing" => Ok(Self::Syncing),
            "conflict" => Ok(Self::Conflict),
            "error" => Ok(Self::Error),
            _ => Err(format!("Invalid sync status: {}", s)),
        }
    }
}

/// Sync message types for communication with central
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SyncMessage {
    /// Configuration update from central
    ConfigUpdate { config: MockConfiguration },
    /// Configuration deleted from central
    ConfigDelete { id: Uuid },
    /// Request sync from central
    SyncRequest {
        since: DateTime<Utc>,
        checksums: HashMap<Uuid, String>,
    },
    /// Sync response from central
    SyncResponse {
        configs: Vec<MockConfiguration>,
        deleted: Vec<Uuid>,
    },
    /// Conflict detected
    ConflictDetected {
        config_id: Uuid,
        local: MockConfiguration,
        central: MockConfiguration,
    },
}

/// Conflict resolution strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    /// Use local version
    KeepLocal,
    /// Use central version
    KeepCentral,
    /// Merge both versions (requires manual intervention)
    Merge,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_method_display() {
        assert_eq!(HttpMethod::Get.to_string(), "GET");
        assert_eq!(HttpMethod::Post.to_string(), "POST");
        assert_eq!(HttpMethod::Any.to_string(), "*");
    }

    #[test]
    fn test_http_method_from_str() {
        assert_eq!("GET".parse::<HttpMethod>(), Ok(HttpMethod::Get));
        assert_eq!("post".parse::<HttpMethod>(), Ok(HttpMethod::Post));
        assert_eq!("*".parse::<HttpMethod>(), Ok(HttpMethod::Any));
        assert!("INVALID".parse::<HttpMethod>().is_err());
    }

    #[test]
    fn test_version_vector_creation() {
        let id = Uuid::new_v4();
        let vv = VersionVector::with_instance(id);
        assert_eq!(vv.get(&id), 1);

        let vv_empty = VersionVector::new();
        assert!(vv_empty.is_empty());
    }

    #[test]
    fn test_version_vector_increment() {
        let id = Uuid::new_v4();
        let mut vv = VersionVector::with_instance(id);
        vv.increment(id);
        assert_eq!(vv.get(&id), 2);
    }

    #[test]
    fn test_version_vector_merge() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let mut vv1 = VersionVector::with_instance(id1);
        vv1.increment(id1);
        // vv1 = {id1: 2}

        let mut vv2 = VersionVector::with_instance(id2);
        vv2.increment(id2);
        // vv2 = {id2: 2}

        vv1.merge(&vv2);
        // vv1 = {id1: 2, id2: 2}

        assert_eq!(vv1.get(&id1), 2);
        assert_eq!(vv1.get(&id2), 2);
    }

    #[test]
    fn test_version_vector_dominates() {
        let id = Uuid::new_v4();

        let mut vv1 = VersionVector::with_instance(id);
        vv1.increment(id);

        let vv2 = VersionVector::with_instance(id);

        assert!(vv1.dominates(&vv2));
        assert!(!vv2.dominates(&vv1));
    }

    #[test]
    fn test_version_vector_concurrent() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let mut vv1 = VersionVector::with_instance(id1);
        vv1.increment(id1);

        let mut vv2 = VersionVector::with_instance(id2);
        vv2.increment(id2);

        // These are concurrent - neither dominates the other
        assert!(vv1.is_concurrent_with(&vv2));
        assert!(vv2.is_concurrent_with(&vv1));
    }

    #[test]
    fn test_mock_configuration_creation() {
        let matching_rules = MatchingRules::default();
        let response_config = ResponseConfig::default();

        let config = MockConfiguration::new(
            "Test Mock".to_string(),
            "/api/test".to_string(),
            HttpMethod::Get,
            matching_rules,
            response_config,
        );

        assert!(config.id != Uuid::nil());
        assert_eq!(config.name, "Test Mock");
        assert_eq!(config.path, "/api/test");
        assert_eq!(config.method, HttpMethod::Get);
        assert_eq!(config.source, MockSource::Central);
        assert!(config.is_active);
    }

    #[test]
    fn test_content_hash_computation() {
        let matching_rules = MatchingRules::default();
        let response_config = ResponseConfig::default();

        let hash1 = MockConfiguration::compute_content_hash(
            "Test",
            "/api/test",
            &HttpMethod::Get,
            &matching_rules,
            &response_config,
        );

        let hash2 = MockConfiguration::compute_content_hash(
            "Test",
            "/api/test",
            &HttpMethod::Get,
            &matching_rules,
            &response_config,
        );

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // SHA-256 produces 64 hex characters
    }

    #[test]
    fn test_mock_filter_defaults() {
        let filter = MockFilter::default();
        assert_eq!(filter.page(), 1);
        assert_eq!(filter.limit(), 20);
        assert_eq!(filter.offset(), 0);
    }

    #[test]
    fn test_mock_filter_pagination() {
        let filter = MockFilter {
            page: Some(3),
            limit: Some(10),
            ..Default::default()
        };
        assert_eq!(filter.page(), 3);
        assert_eq!(filter.limit(), 10);
        assert_eq!(filter.offset(), 20);
    }
}
