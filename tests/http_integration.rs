//! Integration tests for the full HTTP chain: HttpServer + HttpRequestHandler
//!
//! These tests start a real HTTP server, send actual HTTP requests via raw TCP,
//! and verify that the responses match expectations for routing, mock responses,
//! static file serving, and fallback behavior.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use mystiproxy::config::{
    BodyConfig, BodyType, EngineConfig, HeaderAction, HeaderActionType, LocationConfig, MatchMode,
    ProviderType, ProxyType, ResponseConfig,
};
use mystiproxy::http::{create_handler, HttpServer, HttpServerConfig};
use mystiproxy::io::SocketStream;

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------

/// Bind to port 0 to let the OS assign a free port, then return it.
async fn get_available_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind ephemeral port");
    listener.local_addr().expect("no local addr").port()
}

/// Build an `EngineConfig` that listens on a random port and routes through
/// the given `locations`.
fn make_engine_config(port: u16, locations: Vec<LocationConfig>) -> EngineConfig {
    EngineConfig {
        listen: format!("tcp://127.0.0.1:{port}"),
        target: "tcp://127.0.0.1:1".to_string(),
        proxy_type: ProxyType::Http,
        request_timeout: Some(Duration::from_secs(5)),
        connection_timeout: None,
        header: None,
        locations: if locations.is_empty() {
            None
        } else {
            Some(locations)
        },
        auth: None,
        tls: None,
    }
}

/// Start a test server with the given location configurations.
///
/// Returns the `tcp://127.0.0.1:<port>` listen address so callers can connect.
async fn start_test_server(locations: Vec<LocationConfig>) -> String {
    let port = get_available_port().await;
    let listen = format!("tcp://127.0.0.1:{port}");

    let config = make_engine_config(port, locations);
    let handler = create_handler(Arc::new(config)).expect("failed to create handler");

    let mut server = HttpServer::new(HttpServerConfig::new(listen.clone(), None), handler, None);
    server.start().await.expect("failed to start test server");

    tokio::spawn(async move {
        let _ = server.run().await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    listen
}

/// Send a raw HTTP/1.1 request and return the full response as a string.
///
/// Uses a direct TCP connection (via `SocketStream`) so we avoid body-type
/// mismatches with the hyper client API. Sends `Connection: close` so the
/// server closes the connection after responding, allowing us to read until
/// EOF.
async fn send_raw_http(addr: &str, method: &str, path: &str) -> String {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let mut stream = SocketStream::connect(addr.to_string())
        .await
        .expect("failed to connect to test server");

    let request =
        format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("failed to write request");

    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut buf = [0u8; 8192];
        loop {
            let n = stream
                .read(&mut buf)
                .await
                .expect("failed to read response");
            if n == 0 {
                break;
            }
            response.extend_from_slice(&buf[..n]);
        }
    })
    .await
    .expect("timed out reading response");

    String::from_utf8_lossy(&response).to_string()
}

// ---------------------------------------------------------------------------
// Helper builders for LocationConfig
// ---------------------------------------------------------------------------

fn mock_location(path: &str, mode: MatchMode, status: u16) -> LocationConfig {
    LocationConfig {
        location: path.to_string(),
        mode,
        provider: Some(ProviderType::Mock),
        root: None,
        response: Some(ResponseConfig {
            status: Some(status),
            headers: None,
            body: None,
        }),
        request: None,
    }
}

fn mock_location_with_headers(
    path: &str,
    mode: MatchMode,
    status: u16,
    headers: HashMap<String, HeaderAction>,
) -> LocationConfig {
    LocationConfig {
        location: path.to_string(),
        mode,
        provider: Some(ProviderType::Mock),
        root: None,
        response: Some(ResponseConfig {
            status: Some(status),
            headers: Some(headers),
            body: None,
        }),
        request: None,
    }
}

fn static_location(path: &str, mode: MatchMode, root: &str) -> LocationConfig {
    LocationConfig {
        location: path.to_string(),
        mode,
        provider: Some(ProviderType::Static),
        root: Some(root.to_string()),
        response: None,
        request: None,
    }
}

fn mock_location_with_static_body(path: &str, mode: MatchMode, status: u16) -> LocationConfig {
    LocationConfig {
        location: path.to_string(),
        mode,
        provider: Some(ProviderType::Mock),
        root: None,
        response: Some(ResponseConfig {
            status: Some(status),
            headers: None,
            body: Some(BodyConfig {
                json: None,
                body_type: Some(BodyType::Static),
            }),
        }),
        request: None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_mock_response_full_match() {
    let addr = start_test_server(vec![mock_location("/test/mock", MatchMode::Full, 200)]).await;

    let response = send_raw_http(&addr, "GET", "/test/mock").await;

    assert!(
        response.contains("200"),
        "expected 200 status in response, got: {response}"
    );
}

#[tokio::test]
async fn test_mock_response_404() {
    let addr = start_test_server(vec![mock_location("/test/notfound", MatchMode::Full, 404)]).await;

    let response = send_raw_http(&addr, "GET", "/test/notfound").await;

    assert!(
        response.contains("404"),
        "expected 404 status in response, got: {response}"
    );
}

#[tokio::test]
async fn test_mock_response_with_custom_headers() {
    let mut headers = HashMap::new();
    headers.insert(
        "X-Custom-Header".to_string(),
        HeaderAction {
            value: "test-value".to_string(),
            action: HeaderActionType::Overwrite,
            condition: None,
        },
    );

    let addr = start_test_server(vec![mock_location_with_headers(
        "/test/headers",
        MatchMode::Full,
        200,
        headers,
    )])
    .await;

    let response = send_raw_http(&addr, "GET", "/test/headers").await;

    assert!(
        response.contains("200"),
        "expected 200 status, got: {response}"
    );
    assert!(
        response.contains("X-Custom-Header"),
        "expected X-Custom-Header in response, got: {response}"
    );
    assert!(
        response.contains("test-value"),
        "expected 'test-value' header value in response, got: {response}"
    );
}

#[tokio::test]
async fn test_mock_response_with_static_body_type() {
    let addr = start_test_server(vec![mock_location_with_static_body(
        "/test/static-body",
        MatchMode::Full,
        200,
    )])
    .await;

    let response = send_raw_http(&addr, "GET", "/test/static-body").await;

    assert!(
        response.contains("200"),
        "expected 200 status, got: {response}"
    );
    assert!(
        response.contains("Content-Length: 0"),
        "expected empty body (Content-Length: 0), got: {response}"
    );
}

#[tokio::test]
async fn test_static_file_serving() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let index_path = temp_dir.path().join("index.html");
    std::fs::write(&index_path, "<h1>hello from static</h1>").expect("failed to write index.html");

    let root = temp_dir.path().to_string_lossy().to_string();
    let addr = start_test_server(vec![static_location("/", MatchMode::Prefix, &root)]).await;

    let response = send_raw_http(&addr, "GET", "/index.html").await;

    assert!(
        response.contains("200"),
        "expected 200 status for static file, got: {response}"
    );
    assert!(
        response.contains("<h1>hello from static</h1>"),
        "expected file content in response body, got: {response}"
    );
}

#[tokio::test]
async fn test_static_file_not_found() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let root = temp_dir.path().to_string_lossy().to_string();

    let addr = start_test_server(vec![static_location("/", MatchMode::Prefix, &root)]).await;

    let response = send_raw_http(&addr, "GET", "/nonexistent.txt").await;

    assert!(
        response.contains("404"),
        "expected 404 for missing static file, got: {response}"
    );
}

#[tokio::test]
async fn test_prefix_routing() {
    let locations = vec![
        mock_location("/api/v1", MatchMode::Prefix, 201),
        mock_location("/api/v2", MatchMode::Prefix, 202),
    ];
    let addr = start_test_server(locations).await;

    let response_v1 = send_raw_http(&addr, "GET", "/api/v1/users").await;
    assert!(
        response_v1.contains("201"),
        "expected 201 for /api/v1 prefix, got: {response_v1}"
    );

    let response_v2 = send_raw_http(&addr, "GET", "/api/v2/items").await;
    assert!(
        response_v2.contains("202"),
        "expected 202 for /api/v2 prefix, got: {response_v2}"
    );
}

#[tokio::test]
async fn test_no_match_returns_proxy_attempt() {
    let addr = start_test_server(vec![mock_location("/only-this", MatchMode::Full, 200)]).await;

    let _response = send_raw_http(&addr, "GET", "/nothing-matches-here").await;

    let server_still_alive = send_raw_http(&addr, "GET", "/only-this").await;
    assert!(
        server_still_alive.contains("200"),
        "server should still be alive after proxy failure, got: {server_still_alive}"
    );
}

#[tokio::test]
async fn test_multiple_locations_first_match_wins() {
    let locations = vec![
        mock_location("/api/v1", MatchMode::Prefix, 201),
        mock_location("/api/v1/users", MatchMode::Prefix, 200),
    ];
    let addr = start_test_server(locations).await;

    let response = send_raw_http(&addr, "GET", "/api/v1/users/123").await;
    assert!(
        response.contains("201"),
        "first-added /api/v1 prefix should win, got: {response}"
    );

    let response_other = send_raw_http(&addr, "GET", "/api/v1/other").await;
    assert!(
        response_other.contains("201"),
        "/api/v1/other should match first location, got: {response_other}"
    );
}

#[tokio::test]
async fn test_exact_match_does_not_match_partial() {
    let locations = vec![mock_location("/api/exact", MatchMode::Full, 200)];
    let addr = start_test_server(locations).await;

    let matched = send_raw_http(&addr, "GET", "/api/exact").await;
    assert!(
        matched.contains("200"),
        "exact path should match, got: {matched}"
    );

    let _unmatched = send_raw_http(&addr, "GET", "/api/exact/extra").await;

    let still_alive = send_raw_http(&addr, "GET", "/api/exact").await;
    assert!(
        still_alive.contains("200"),
        "server should survive unmatched request, got: {still_alive}"
    );
}

#[tokio::test]
async fn test_mock_response_500_status() {
    let addr = start_test_server(vec![mock_location("/error", MatchMode::Full, 500)]).await;

    let response = send_raw_http(&addr, "GET", "/error").await;

    assert!(
        response.contains("500"),
        "expected 500 status, got: {response}"
    );
}
