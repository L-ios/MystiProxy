//! E2E tests for WebSocket upgrade handshake, Forward Proxy auth,
//! Static File Range requests, and Mock conditional matching.
//!
//! These tests cover the remaining feature modules that lacked e2e coverage.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use mystiproxy::config::{
    BodyConfig, BodyType, EngineConfig, HeaderAction, HeaderActionType, LocationConfig, MatchMode,
    ProviderType, ProxyType, ResponseConfig,
};
use mystiproxy::http::{
    create_handler, HttpProxyConfig, HttpServer, HttpServerConfig, ProxyAuthConfig,
};
use mystiproxy::io::SocketStream;

// ---------------------------------------------------------------------------
// Shared infrastructure
// ---------------------------------------------------------------------------

async fn get_available_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind failed");
    listener.local_addr().expect("no addr").port()
}

async fn send_raw_http(addr: &str, request: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = SocketStream::connect(addr.to_string())
        .await
        .expect("connect failed");

    stream.write_all(request.as_bytes()).await.expect("write");

    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut buf = [0u8; 8192];
        loop {
            let n = stream.read(&mut buf).await.expect("read");
            if n == 0 {
                break;
            }
            response.extend_from_slice(&buf[..n]);
        }
    })
    .await
    .expect("timeout");

    String::from_utf8_lossy(&response).to_string()
}

/// Send raw HTTP and read until headers are complete (for WebSocket upgrade).
async fn send_raw_http_short(addr: &str, request: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = SocketStream::connect(addr.to_string())
        .await
        .expect("connect failed");

    stream.write_all(request.as_bytes()).await.expect("write");

    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), async {
        let mut buf = [0u8; 8192];
        loop {
            let n = stream.read(&mut buf).await.expect("read");
            if n == 0 {
                break;
            }
            response.extend_from_slice(&buf[..n]);
            // Stop once we have complete headers
            if response.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
    })
    .await
    .expect("timeout");

    String::from_utf8_lossy(&response).to_string()
}

fn extract_status(response: &str) -> u16 {
    response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1).and_then(|s| s.parse().ok()))
        .unwrap_or(0)
}

// ===========================================================================
// WebSocket upgrade tests
// ===========================================================================

async fn start_test_server(locations: Vec<LocationConfig>) -> String {
    let port = get_available_port().await;
    let listen = format!("tcp://127.0.0.1:{port}");

    let config = EngineConfig {
        listen: listen.clone(),
        target: "tcp://127.0.0.1:1".to_string(),
        proxy_type: ProxyType::Http,
        request_timeout: Some(Duration::from_secs(5)),
        connection_timeout: None,
        header: None,
        locations: Some(locations),
        auth: None,
        upstream: None,
        allow: None,
        deny: None,
        tls: None,
    };

    let handler = create_handler(Arc::new(config)).expect("handler failed");
    let mut server = HttpServer::new(HttpServerConfig::new(listen.clone(), None), handler, None);
    server.start().await.expect("start failed");
    tokio::spawn(async move {
        let _ = server.run().await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    listen
}

#[tokio::test]
async fn test_e2e_websocket_upgrade_handshake() {
    let addr = start_test_server(vec![]).await;

    // Send a WebSocket upgrade request
    let request = "GET /ws HTTP/1.1\r\n\
Host: localhost\r\n\
Upgrade: websocket\r\n\
Connection: Upgrade\r\n\
Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
Sec-WebSocket-Version: 13\r\n\
\r\n";

    let response = send_raw_http_short(&addr, request).await;

    // F8 真代理语义：target(127.0.0.1:1) 不可达 -> 502（旧假握手曾返回 101）
    assert_eq!(
        extract_status(&response),
        502,
        "unreachable upstream must yield 502, got: {}",
        &response[..response.len().min(200)]
    );
    // 502 关闭语义：不再携带 Upgrade / Sec-WebSocket-Accept 头（升级未发生）
}

#[tokio::test]
async fn test_e2e_websocket_upgrade_missing_key() {
    let addr = start_test_server(vec![]).await;

    // WebSocket upgrade without Sec-WebSocket-Key
    let request = "GET /ws HTTP/1.1\r\n\
Host: localhost\r\n\
Upgrade: websocket\r\n\
Connection: Upgrade\r\n\
Connection: close\r\n\
\r\n";

    let response = send_raw_http(&addr, request).await;

    // Should NOT be 101
    assert_ne!(
        extract_status(&response),
        101,
        "should not upgrade without Sec-WebSocket-Key"
    );
}

// ===========================================================================
// Forward Proxy Basic Auth tests (unit-level, via ProxyAuthConfig API)
// ===========================================================================

#[tokio::test]
async fn test_e2e_proxy_auth_config_creates_user() {
    let auth = ProxyAuthConfig::new()
        .add_user("admin".to_string(), "secret".to_string())
        .enable();

    assert!(auth.enabled);
    assert!(auth.verify_password("admin", "secret"));
    assert!(!auth.verify_password("admin", "wrong"));
}

#[tokio::test]
async fn test_e2e_proxy_auth_authenticate_header() {
    let auth = ProxyAuthConfig::new()
        .add_user("testuser".to_string(), "testpass".to_string())
        .enable();

    let mut headers = hyper::HeaderMap::new();
    let credentials = base64_encode("testuser:testpass");
    headers.insert(
        "Proxy-Authorization",
        format!("Basic {credentials}").parse().unwrap(),
    );

    let result = auth.authenticate(&headers);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), "testuser");
}

#[tokio::test]
async fn test_e2e_proxy_auth_rejects_wrong_password() {
    let auth = ProxyAuthConfig::new()
        .add_user("admin".to_string(), "correct".to_string())
        .enable();

    let mut headers = hyper::HeaderMap::new();
    let credentials = base64_encode("admin:wrong");
    headers.insert(
        "Proxy-Authorization",
        format!("Basic {credentials}").parse().unwrap(),
    );

    let result = auth.authenticate(&headers);
    assert!(result.is_none(), "wrong password should be rejected");
}

#[tokio::test]
async fn test_e2e_proxy_auth_disabled_allows_all() {
    let auth = ProxyAuthConfig::new(); // disabled by default

    let headers = hyper::HeaderMap::new();
    let result = auth.authenticate(&headers);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), "anonymous");
}

#[tokio::test]
async fn test_e2e_proxy_config_host_filtering() {
    let config = HttpProxyConfig::new()
        .allow_host("allowed.com")
        .block_host("blocked.com");

    assert!(config.is_host_allowed("allowed.com"));
    assert!(config.is_host_allowed("api.allowed.com"));
    assert!(!config.is_host_allowed("blocked.com"));
    assert!(!config.is_host_allowed("sub.blocked.com"));
    assert!(!config.is_host_allowed("other.com")); // not in allowlist when allowlist is non-empty
}

fn base64_encode(input: &str) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine};
    STANDARD.encode(input)
}

// ===========================================================================
// Static File Range request tests
// ===========================================================================

#[tokio::test]
async fn test_e2e_static_file_range_request() {
    let temp_dir = tempfile::tempdir().expect("temp dir failed");
    let file_path = temp_dir.path().join("data.bin");

    // Write 100 bytes of known content
    let content: Vec<u8> = (0..100u8).collect();
    std::fs::write(&file_path, &content).expect("write failed");

    let root = temp_dir.path().to_string_lossy().to_string();

    let loc = LocationConfig {
        location: "/".to_string(),
        mode: MatchMode::Prefix,
        provider: Some(ProviderType::Static),
        root: Some(root),
        response: None,
        request: None,
        index_files: None,
        enable_directory_listing: None,
    };

    let addr = start_test_server(vec![loc]).await;

    // Request bytes 10-19 (10 bytes)
    let request = "GET /data.bin HTTP/1.1\r\n\
Host: localhost\r\n\
Range: bytes=10-19\r\n\
Connection: close\r\n\
\r\n";

    let response = send_raw_http(&addr, request).await;

    assert_eq!(
        extract_status(&response),
        206,
        "should return 206 Partial Content, got: {}",
        &response[..response.len().min(200)]
    );
    assert!(
        response.contains("Content-Range"),
        "should contain Content-Range header"
    );
    assert!(
        response.contains("Accept-Ranges"),
        "should contain Accept-Ranges header"
    );
}

#[tokio::test]
async fn test_e2e_static_file_full_request_no_range() {
    let temp_dir = tempfile::tempdir().expect("temp dir failed");
    let file_path = temp_dir.path().join("test.txt");
    std::fs::write(&file_path, "hello world").expect("write failed");

    let root = temp_dir.path().to_string_lossy().to_string();

    let loc = LocationConfig {
        location: "/".to_string(),
        mode: MatchMode::Prefix,
        provider: Some(ProviderType::Static),
        root: Some(root),
        response: None,
        request: None,
        index_files: None,
        enable_directory_listing: None,
    };

    let addr = start_test_server(vec![loc]).await;

    let request = "GET /test.txt HTTP/1.1\r\n\
Host: localhost\r\n\
Connection: close\r\n\
\r\n";

    let response = send_raw_http(&addr, request).await;

    assert_eq!(extract_status(&response), 200);
    assert!(
        response.contains("hello world"),
        "should contain file content"
    );
    assert!(
        response.contains("Accept-Ranges: bytes"),
        "should advertise range support"
    );
}

// ===========================================================================
// Mock conditional matching tests (via MockService API)
// ===========================================================================

#[tokio::test]
async fn test_e2e_mock_with_custom_status_and_body() {
    let mut headers = HashMap::new();
    headers.insert(
        "Content-Type".to_string(),
        HeaderAction {
            value: "application/json".to_string(),
            action: HeaderActionType::Overwrite,
            condition: None,
        },
    );

    let loc = LocationConfig {
        location: "/api/mock".to_string(),
        mode: MatchMode::Full,
        provider: Some(ProviderType::Mock),
        root: None,
        response: Some(ResponseConfig {
            status: Some(418),
            headers: Some(headers),
            body: Some(BodyConfig {
                json: None,
                content: None,
                template: None,
                body_type: Some(BodyType::Static),
            }),
            conditions: None,
        }),
        request: None,
        index_files: None,
        enable_directory_listing: None,
    };

    let addr = start_test_server(vec![loc]).await;

    let request = "GET /api/mock HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let response = send_raw_http(&addr, request).await;

    assert_eq!(extract_status(&response), 418);
    assert!(
        response.contains("Content-Type: application/json"),
        "should have custom Content-Type header"
    );
}

#[tokio::test]
async fn test_e2e_mock_prefix_matches_subpaths() {
    let loc = LocationConfig {
        location: "/api/v1".to_string(),
        mode: MatchMode::Prefix,
        provider: Some(ProviderType::Mock),
        root: None,
        response: Some(ResponseConfig {
            status: Some(201),
            headers: None,
            body: None,
            conditions: None,
        }),
        request: None,
        index_files: None,
        enable_directory_listing: None,
    };

    let addr = start_test_server(vec![loc]).await;

    // All subpaths should match
    for path in &["/api/v1", "/api/v1/users", "/api/v1/items/123"] {
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        let response = send_raw_http(&addr, &request).await;
        assert_eq!(
            extract_status(&response),
            201,
            "prefix match for {path} should return 201"
        );
    }

    // Non-matching path should NOT return 201
    let request = "GET /api/v2 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let response = send_raw_http(&addr, request).await;
    assert_ne!(
        extract_status(&response),
        201,
        "non-matching path should not return 201"
    );
}

#[tokio::test]
async fn test_e2e_multiple_mock_locations_priority() {
    let locations = vec![
        LocationConfig {
            location: "/api".to_string(),
            mode: MatchMode::Prefix,
            provider: Some(ProviderType::Mock),
            root: None,
            response: Some(ResponseConfig {
                status: Some(200),
                headers: None,
                body: None,
                conditions: None,
            }),
            request: None,
            index_files: None,
            enable_directory_listing: None,
        },
        LocationConfig {
            location: "/api/special".to_string(),
            mode: MatchMode::Prefix,
            provider: Some(ProviderType::Mock),
            root: None,
            response: Some(ResponseConfig {
                status: Some(301),
                headers: None,
                body: None,
                conditions: None,
            }),
            request: None,
            index_files: None,
            enable_directory_listing: None,
        },
    ];

    let addr = start_test_server(locations).await;

    // First-added route wins for /api/special
    let request = "GET /api/special HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let response = send_raw_http(&addr, request).await;
    assert_eq!(
        extract_status(&response),
        200,
        "first-added /api prefix should win over /api/special"
    );
}
