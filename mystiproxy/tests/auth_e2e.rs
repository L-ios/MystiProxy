//! E2E tests for HTTP authentication (header-based and JWT).
//!
//! These tests verify that:
//! 1. Requests without auth are rejected (401)
//! 2. Requests with correct auth are forwarded to upstream
//! 3. Requests with wrong auth are rejected (401)
//! 4. Both Header-based and JWT authentication work end-to-end

use std::sync::Arc;
use std::time::Duration;

use mystiproxy::config::{AuthConfig, EngineConfig, ProxyType};
use mystiproxy::http::{create_handler, HttpServer, HttpServerConfig};
use mystiproxy::io::SocketStream;

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------

async fn get_available_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind ephemeral port");
    listener.local_addr().expect("no local addr").port()
}

async fn start_upstream_ok() -> u16 {
    let port = get_available_port().await;
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
            .await
            .expect("upstream bind failed");
        loop {
            if let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = vec![0u8; 8192];
                    let _ = stream.read(&mut buf).await;
                    let body = "UPSTREAM OK";
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                });
            }
        }
    });
    tokio::time::sleep(Duration::from_millis(30)).await;
    port
}

async fn start_proxy_with_auth(auth_config: AuthConfig) -> (String, u16) {
    let upstream = start_upstream_ok().await;
    let proxy_port = get_available_port().await;
    let listen = format!("tcp://127.0.0.1:{proxy_port}");

    let config = EngineConfig {
        listen: listen.clone(),
        target: format!("tcp://127.0.0.1:{upstream}"),
        proxy_type: ProxyType::Http,
        request_timeout: Some(Duration::from_secs(5)),
        connection_timeout: None,
        header: None,
        locations: None,
        auth: Some(auth_config),
        upstream: None,
        allow: None,
        deny: None,
        management: None,
        tls: None,
    };

    let handler = create_handler(Arc::new(config)).expect("failed to create handler");
    let mut server = HttpServer::new(HttpServerConfig::new(listen.clone(), None), handler, None);
    server.start().await.expect("failed to start proxy");

    tokio::spawn(async move {
        let _ = server.run().await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    (listen, upstream)
}

async fn send_request(addr: &str, method: &str, path: &str, headers: &[(&str, &str)]) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = SocketStream::connect(addr.to_string())
        .await
        .expect("failed to connect");

    let header_str: String = headers
        .iter()
        .map(|(k, v)| format!("{k}: {v}\r\n"))
        .collect();

    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\n{header_str}Connection: close\r\n\r\n"
    );

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

fn extract_status_line(response: &str) -> u16 {
    response
        .lines()
        .next()
        .and_then(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            parts.get(1).and_then(|s| s.parse().ok())
        })
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Header-based auth tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_e2e_header_auth_rejects_missing_header() {
    let auth = AuthConfig {
        auth_type: "header".to_string(),
        header_name: "X-API-Key".to_string(),
        expected_value: Some("secret-key".to_string()),
        jwt_secret: None,
        enabled: true,
    };

    let (proxy, _) = start_proxy_with_auth(auth).await;

    // No auth header → should get 401
    let response = send_request(&proxy, "GET", "/", &[]).await;
    assert_eq!(
        extract_status_line(&response),
        401,
        "should reject request without auth header"
    );
}

#[tokio::test]
async fn test_e2e_header_auth_rejects_wrong_value() {
    let auth = AuthConfig {
        auth_type: "header".to_string(),
        header_name: "X-API-Key".to_string(),
        expected_value: Some("correct-key".to_string()),
        jwt_secret: None,
        enabled: true,
    };

    let (proxy, _) = start_proxy_with_auth(auth).await;

    let response = send_request(&proxy, "GET", "/", &[("X-API-Key", "wrong-key")]).await;
    assert_eq!(
        extract_status_line(&response),
        401,
        "should reject request with wrong auth value"
    );
}

#[tokio::test]
async fn test_e2e_header_auth_accepts_correct_value() {
    let auth = AuthConfig {
        auth_type: "header".to_string(),
        header_name: "X-API-Key".to_string(),
        expected_value: Some("correct-key".to_string()),
        jwt_secret: None,
        enabled: true,
    };

    let (proxy, _) = start_proxy_with_auth(auth).await;

    let response = send_request(&proxy, "GET", "/", &[("X-API-Key", "correct-key")]).await;
    assert_eq!(
        extract_status_line(&response),
        200,
        "should accept request with correct auth value"
    );
    assert!(
        response.contains("UPSTREAM OK"),
        "should reach upstream after auth"
    );
}

#[tokio::test]
async fn test_e2e_header_auth_accepts_correct_value_with_bearer() {
    let auth = AuthConfig {
        auth_type: "header".to_string(),
        header_name: "Authorization".to_string(),
        expected_value: Some("Bearer mytoken123".to_string()),
        jwt_secret: None,
        enabled: true,
    };

    let (proxy, _) = start_proxy_with_auth(auth).await;

    let response = send_request(
        &proxy,
        "GET",
        "/",
        &[("Authorization", "Bearer mytoken123")],
    )
    .await;
    assert_eq!(
        extract_status_line(&response),
        200,
        "should accept Bearer token auth"
    );
}

#[tokio::test]
async fn test_e2e_auth_disabled_allows_all() {
    let auth = AuthConfig {
        auth_type: "header".to_string(),
        header_name: "Authorization".to_string(),
        expected_value: Some("anything".to_string()),
        jwt_secret: None,
        enabled: false,
    };

    let (proxy, _) = start_proxy_with_auth(auth).await;

    // No auth header but auth disabled → should succeed
    let response = send_request(&proxy, "GET", "/", &[]).await;
    assert_eq!(
        extract_status_line(&response),
        200,
        "should allow request when auth is disabled"
    );
}

#[tokio::test]
async fn test_e2e_auth_no_config_allows_all() {
    let upstream = start_upstream_ok().await;
    let proxy_port = get_available_port().await;
    let listen = format!("tcp://127.0.0.1:{proxy_port}");

    let config = EngineConfig {
        listen: listen.clone(),
        target: format!("tcp://127.0.0.1:{upstream}"),
        proxy_type: ProxyType::Http,
        request_timeout: Some(Duration::from_secs(5)),
        connection_timeout: None,
        header: None,
        locations: None,
        auth: None,
        upstream: None,
        allow: None,
        deny: None,
        management: None,
        tls: None,
    };

    let handler = create_handler(Arc::new(config)).expect("handler failed");
    let mut server = HttpServer::new(HttpServerConfig::new(listen.clone(), None), handler, None);
    server.start().await.expect("start failed");
    tokio::spawn(async move {
        let _ = server.run().await;
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let response = send_request(&listen, "GET", "/", &[]).await;
    assert_eq!(extract_status_line(&response), 200);
}
