//! E2E tests for JWT authentication and HttpClientPool connection reuse.
//!
//! These cover the last two gaps in feature coverage:
//! 1. JWT token generation, validation, and rejection
//! 2. Connection pool: client reuse for same target, separate clients for different targets

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use mystiproxy::http::{AuthConfig, AuthType, Authenticator, HttpClientPool};

// ===========================================================================
// JWT Authentication tests
// ===========================================================================

/// Generate a valid JWT token for testing
fn make_jwt_token(secret: &str, sub: &str) -> String {
    use jsonwebtoken::{encode, EncodingKey, Header};
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut claims = HashMap::new();
    claims.insert("sub".to_string(), serde_json::json!(sub));
    claims.insert("exp".to_string(), serde_json::json!(now + 3600));
    claims.insert("iat".to_string(), serde_json::json!(now));

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap()
}

/// Generate an expired JWT token
fn make_expired_jwt_token(secret: &str, sub: &str) -> String {
    use jsonwebtoken::{encode, EncodingKey, Header};
    let mut claims = HashMap::new();
    claims.insert("sub".to_string(), serde_json::json!(sub));
    claims.insert("exp".to_string(), serde_json::json!(100)); // expired in 1970
    claims.insert("iat".to_string(), serde_json::json!(50));

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap()
}

#[test]
fn test_e2e_jwt_authenticator_accepts_valid_token() {
    let secret = "test-secret-key";
    let token = make_jwt_token(secret, "user123");

    let config = AuthConfig {
        auth_type: AuthType::Jwt {
            secret: secret.to_string(),
            issuer: None,
            audience: None,
        },
        header_name: "Authorization".to_string(),
        expected_value: None,
        enabled: true,
    };

    let auth = Authenticator::new(config);

    let mut headers = hyper::HeaderMap::new();
    headers.insert("Authorization", format!("Bearer {token}").parse().unwrap());

    let result = auth.authenticate(&headers).unwrap();
    assert!(result.authenticated, "valid JWT should authenticate");
    assert_eq!(result.user.as_deref(), Some("user123"));
    assert!(result.claims.is_some(), "should contain JWT claims");
}

#[test]
fn test_e2e_jwt_authenticator_rejects_expired_token() {
    let secret = "test-secret-key";
    let token = make_expired_jwt_token(secret, "user123");

    let config = AuthConfig {
        auth_type: AuthType::Jwt {
            secret: secret.to_string(),
            issuer: None,
            audience: None,
        },
        header_name: "Authorization".to_string(),
        expected_value: None,
        enabled: true,
    };

    let auth = Authenticator::new(config);

    let mut headers = hyper::HeaderMap::new();
    headers.insert("Authorization", format!("Bearer {token}").parse().unwrap());

    let result = auth.authenticate(&headers).unwrap();
    assert!(!result.authenticated, "expired JWT should be rejected");
}

#[test]
fn test_e2e_jwt_authenticator_rejects_wrong_secret() {
    let token = make_jwt_token("correct-secret", "user123");

    let config = AuthConfig {
        auth_type: AuthType::Jwt {
            secret: "wrong-secret".to_string(),
            issuer: None,
            audience: None,
        },
        header_name: "Authorization".to_string(),
        expected_value: None,
        enabled: true,
    };

    let auth = Authenticator::new(config);

    let mut headers = hyper::HeaderMap::new();
    headers.insert("Authorization", format!("Bearer {token}").parse().unwrap());

    let result = auth.authenticate(&headers).unwrap();
    assert!(
        !result.authenticated,
        "JWT with wrong secret should be rejected"
    );
}

#[test]
fn test_e2e_jwt_authenticator_rejects_missing_header() {
    let config = AuthConfig {
        auth_type: AuthType::Jwt {
            secret: "secret".to_string(),
            issuer: None,
            audience: None,
        },
        header_name: "Authorization".to_string(),
        expected_value: None,
        enabled: true,
    };

    let auth = Authenticator::new(config);
    let headers = hyper::HeaderMap::new();

    let result = auth.authenticate(&headers).unwrap();
    assert!(
        !result.authenticated,
        "missing auth header should be rejected"
    );
}

#[test]
fn test_e2e_jwt_authenticator_rejects_garbage_token() {
    let config = AuthConfig {
        auth_type: AuthType::Jwt {
            secret: "secret".to_string(),
            issuer: None,
            audience: None,
        },
        header_name: "Authorization".to_string(),
        expected_value: None,
        enabled: true,
    };

    let auth = Authenticator::new(config);

    let mut headers = hyper::HeaderMap::new();
    headers.insert("Authorization", "Bearer not.a.valid.jwt".parse().unwrap());

    let result = auth.authenticate(&headers).unwrap();
    assert!(!result.authenticated, "garbage token should be rejected");
}

#[test]
fn test_e2e_jwt_authenticator_accepts_token_without_bearer_prefix() {
    let secret = "test-secret";
    let token = make_jwt_token(secret, "user456");

    let config = AuthConfig {
        auth_type: AuthType::Jwt {
            secret: secret.to_string(),
            issuer: None,
            audience: None,
        },
        header_name: "Authorization".to_string(),
        expected_value: None,
        enabled: true,
    };

    let auth = Authenticator::new(config);

    let mut headers = hyper::HeaderMap::new();
    // Token without "Bearer " prefix should also work
    headers.insert("Authorization", token.parse().unwrap());

    let result = auth.authenticate(&headers).unwrap();
    assert!(
        result.authenticated,
        "JWT without Bearer prefix should work"
    );
}

#[test]
fn test_e2e_jwt_authenticator_claims_extraction() {
    let secret = "claims-test-secret";
    use jsonwebtoken::{encode, EncodingKey, Header};
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut claims = HashMap::new();
    claims.insert("sub".to_string(), serde_json::json!("admin"));
    claims.insert("exp".to_string(), serde_json::json!(now + 3600));
    claims.insert("iat".to_string(), serde_json::json!(now));
    claims.insert("role".to_string(), serde_json::json!("superuser"));
    claims.insert("department".to_string(), serde_json::json!("engineering"));

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap();

    let config = AuthConfig {
        auth_type: AuthType::Jwt {
            secret: secret.to_string(),
            issuer: None,
            audience: None,
        },
        header_name: "Authorization".to_string(),
        expected_value: None,
        enabled: true,
    };

    let auth = Authenticator::new(config);

    let mut headers = hyper::HeaderMap::new();
    headers.insert("Authorization", format!("Bearer {token}").parse().unwrap());

    let result = auth.authenticate(&headers).unwrap();
    assert!(result.authenticated);

    let claims = result.claims.unwrap();
    assert_eq!(claims.get("sub").unwrap(), &serde_json::json!("admin"));
    assert_eq!(claims.get("role").unwrap(), &serde_json::json!("superuser"));
    assert_eq!(
        claims.get("department").unwrap(),
        &serde_json::json!("engineering")
    );
}

// ===========================================================================
// HttpClientPool connection reuse tests
// ===========================================================================

#[tokio::test]
async fn test_e2e_connection_pool_reuses_same_target() {
    let pool = HttpClientPool::new();

    let client1 = pool
        .get_or_create("tcp://127.0.0.1:1".to_string(), None)
        .await;
    let client2 = pool
        .get_or_create("tcp://127.0.0.1:1".to_string(), None)
        .await;

    // Same target should return the same client (Arc pointer equality)
    assert!(
        Arc::ptr_eq(&client1, &client2),
        "pool should reuse client for same target"
    );
}

#[tokio::test]
async fn test_e2e_connection_pool_creates_separate_for_different_targets() {
    let pool = HttpClientPool::new();

    let client1 = pool
        .get_or_create("tcp://127.0.0.1:1".to_string(), None)
        .await;
    let client2 = pool
        .get_or_create("tcp://127.0.0.1:2".to_string(), None)
        .await;

    // Different targets should get different clients
    assert!(
        !Arc::ptr_eq(&client1, &client2),
        "pool should create separate client for different target"
    );
    assert_ne!(client1.target(), client2.target());
}

#[tokio::test]
async fn test_e2e_connection_pool_clear() {
    let pool = HttpClientPool::new();

    let _client1 = pool
        .get_or_create("tcp://127.0.0.1:1".to_string(), None)
        .await;
    let _client2 = pool
        .get_or_create("tcp://127.0.0.1:2".to_string(), None)
        .await;

    pool.clear().await;

    // After clear, new client should be created
    let client3 = pool
        .get_or_create("tcp://127.0.0.1:1".to_string(), None)
        .await;
    assert_eq!(client3.target(), "tcp://127.0.0.1:1");
}

#[tokio::test]
async fn test_e2e_connection_pool_multiple_targets() {
    let pool = HttpClientPool::new();

    let targets = vec![
        "tcp://10.0.0.1:80",
        "tcp://10.0.0.2:80",
        "tcp://10.0.0.3:80",
        "tcp://10.0.0.1:80", // duplicate
    ];

    let mut clients = Vec::new();
    for target in &targets {
        let client = pool.get_or_create(target.to_string(), None).await;
        clients.push(client);
    }

    // client[0] and client[3] should be the same (same target)
    assert!(
        Arc::ptr_eq(&clients[0], &clients[3]),
        "duplicate target should reuse client"
    );
    // All others should be different
    assert!(!Arc::ptr_eq(&clients[0], &clients[1]));
    assert!(!Arc::ptr_eq(&clients[1], &clients[2]));
}

#[tokio::test]
async fn test_e2e_connection_pool_default() {
    let pool = HttpClientPool::default();
    let client = pool
        .get_or_create(
            "tcp://127.0.0.1:8080".to_string(),
            Some(Duration::from_secs(30)),
        )
        .await;
    assert_eq!(client.target(), "tcp://127.0.0.1:8080");
}
