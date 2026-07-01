//! E2E tests for the Gateway URI mapping module.
//!
//! These tests exercise the URI matching and target URI building logic
//! for all match types: exact, prefix, variable, and variable-prefix.

use mystiproxy::gateway::UriMapping;

// ---------------------------------------------------------------------------
// Tests: exact / prefix matching
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_gateway_exact_match_root() {
    let mut mapping = UriMapping::default();
    mapping.uri = "/".to_string();
    assert_eq!(
        mapping.match_uri("/"),
        Some(mystiproxy::gateway::UriMatch::Exact)
    );
}

#[test]
fn test_e2e_gateway_exact_match_specific_path() {
    let mut mapping = UriMapping::default();
    mapping.uri = "/api/v1/users".to_string();
    assert_eq!(
        mapping.match_uri("/api/v1/users"),
        Some(mystiproxy::gateway::UriMatch::Exact)
    );
    assert!(mapping.match_uri("/api/v1/users/123").is_some()); // prefix
    assert!(mapping.match_uri("/api/v2/users").is_none());
}

#[test]
fn test_e2e_gateway_prefix_match() {
    let mut mapping = UriMapping::default();
    mapping.uri = "/api".to_string();

    assert_eq!(
        mapping.match_uri("/api/users"),
        Some(mystiproxy::gateway::UriMatch::Prefix)
    );
    assert_eq!(
        mapping.match_uri("/api/v1/items/123"),
        Some(mystiproxy::gateway::UriMatch::Prefix)
    );
    assert!(mapping.match_uri("/other").is_none());
}

// ---------------------------------------------------------------------------
// Tests: variable matching
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_gateway_variable_match_simple() {
    let mut mapping = UriMapping::default();
    mapping.uri = "/api/users/{id}".to_string();

    assert_eq!(
        mapping.match_uri("/api/users/123"),
        Some(mystiproxy::gateway::UriMatch::Variable)
    );
    assert_eq!(
        mapping.match_uri("/api/users/abc"),
        Some(mystiproxy::gateway::UriMatch::Variable)
    );
}

#[test]
fn test_e2e_gateway_variable_match_with_regex_constraint() {
    let mut mapping = UriMapping::default();
    mapping.uri = "/api/users/{id:[0-9]+}".to_string();

    assert_eq!(
        mapping.match_uri("/api/users/123"),
        Some(mystiproxy::gateway::UriMatch::Variable)
    );
    // Non-numeric id should not match
    assert!(mapping.match_uri("/api/users/abc").is_none());
}

#[test]
fn test_e2e_gateway_variable_prefix_match() {
    let mut mapping = UriMapping::default();
    mapping.uri = "/api/users/{id:[0-9]+}".to_string();

    assert_eq!(
        mapping.match_uri("/api/users/123/details"),
        Some(mystiproxy::gateway::UriMatch::VariablePrefix)
    );
}

#[test]
fn test_e2e_gateway_multiple_variables() {
    let mut mapping = UriMapping::default();
    mapping.uri = "/api/users/{id:[0-9]+}/records/{rid:[0-9a-z]+}".to_string();

    assert_eq!(
        mapping.match_uri("/api/users/123/records/abc456"),
        Some(mystiproxy::gateway::UriMatch::Variable)
    );
    assert_eq!(
        mapping.match_uri("/api/users/999/records/xyz"),
        Some(mystiproxy::gateway::UriMatch::Variable)
    );
}

// ---------------------------------------------------------------------------
// Tests: target URI building
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_gateway_build_target_uri_simple_variable() {
    let mut mapping = UriMapping::default();
    mapping.uri = "/api/users/{id:[0-9]+}/records/{rid:[0-9]+}".to_string();
    mapping.target_uri = "/user/{id}/record/{rid}".to_string();

    let result = mapping.build_target_uri("/api/users/123/records/456");
    assert_eq!(result, Some("/user/123/record/456".to_string()));
}

#[test]
fn test_e2e_gateway_build_target_uri_switched_order() {
    let mut mapping = UriMapping::default();
    mapping.uri = "/api/users/{rid}/records/{id}".to_string();
    mapping.target_uri = "/record/{id}/user/{rid}".to_string();

    let result = mapping.build_target_uri("/api/users/123/records/456");
    assert_eq!(result, Some("/record/456/user/123".to_string()));
}

#[test]
fn test_e2e_gateway_build_target_uri_with_extra_path() {
    let mut mapping = UriMapping::default();
    mapping.uri = "/api/users/{rid}/records/{id}".to_string();
    mapping.target_uri = "/record/{id}/user/{rid}".to_string();

    let result = mapping.build_target_uri("/api/users/123/records/456/extra");
    assert_eq!(result, Some("/record/456/user/123/extra".to_string()));
}

#[test]
fn test_e2e_gateway_build_target_uri_exact() {
    let mut mapping = UriMapping::default();
    mapping.uri = "/old/path".to_string();
    mapping.target_uri = "/new/path".to_string();

    let result = mapping.build_target_uri("/old/path");
    assert_eq!(result, Some("/new/path".to_string()));
}

#[test]
fn test_e2e_gateway_build_target_uri_prefix() {
    let mut mapping = UriMapping::default();
    mapping.uri = "/old".to_string();
    mapping.target_uri = "/new".to_string();

    let result = mapping.build_target_uri("/old/sub/resource");
    assert_eq!(result, Some("/new/sub/resource".to_string()));
}

// ---------------------------------------------------------------------------
// Tests: serialization
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_gateway_uri_mapping_deserialize() {
    let json = r#"{
  "method": "GET,POST|put,*",
  "uri": "/test",
  "service": "test",
  "target_uri": "http://127.0.0.1:8080",
  "target_service": "target"
}"#;

    let mapping: UriMapping = serde_json::from_str(json).expect("deserialize failed");
    assert_eq!(mapping.methods, vec!["*", "GET", "POST", "PUT"]);
    assert_eq!(mapping.uri, "/test");
    assert_eq!(mapping.target_uri, "http://127.0.0.1:8080");
    assert_eq!(mapping.service, Some("test".to_string()));
    assert_eq!(mapping.target_service, Some("target".to_string()));
}

// ---------------------------------------------------------------------------
// Tests: method support
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_gateway_supports_method() {
    let mut mapping = UriMapping::default();
    mapping.methods = vec!["GET".to_string(), "POST".to_string()];

    assert!(mapping.supports_method("GET"));
    assert!(mapping.supports_method("POST"));
    assert!(!mapping.supports_method("DELETE"));
    assert!(mapping.supports_method("get")); // case insensitive
}

#[test]
fn test_e2e_gateway_supports_method_wildcard() {
    let mut mapping = UriMapping::default();
    mapping.methods = vec!["*".to_string()];

    assert!(mapping.supports_method("GET"));
    assert!(mapping.supports_method("POST"));
    assert!(mapping.supports_method("PUT"));
    assert!(mapping.supports_method("DELETE"));
}
