//! E2E tests for request rewriting (method, URI, header modifications).
//!
//! These tests verify that location-level request configs correctly modify
//! requests before they are forwarded to the upstream server.

use std::sync::Arc;
use std::time::Duration;

use mystiproxy::config::{
    EngineConfig, HeaderAction, HeaderActionType, LocationConfig, MatchMode, ProviderType,
    ProxyType, RequestConfig, UriConfig,
};
use mystiproxy::http::{create_handler, HttpServer, HttpServerConfig};
use mystiproxy::io::SocketStream;

// ---------------------------------------------------------------------------
// Infrastructure
// ---------------------------------------------------------------------------

async fn get_available_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind failed");
    listener.local_addr().expect("no addr").port()
}

/// Start an upstream that echoes the full request (request line + headers we care about).
async fn start_echo_upstream() -> u16 {
    let port = get_available_port().await;
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
            .await
            .expect("upstream bind failed");
        loop {
            if let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = vec![0u8; 16384];
                    let n = match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => n,
                    };
                    let request = String::from_utf8_lossy(&buf[..n]).to_string();
                    let body = format!("---REQUEST---\n{request}\n---END---");
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

async fn start_proxy(upstream_port: u16, locations: Vec<LocationConfig>) -> String {
    let proxy_port = get_available_port().await;
    let listen = format!("tcp://127.0.0.1:{proxy_port}");

    let config = EngineConfig {
        listen: listen.clone(),
        target: format!("tcp://127.0.0.1:{upstream_port}"),
        proxy_type: ProxyType::Http,
        request_timeout: Some(Duration::from_secs(5)),
        connection_timeout: None,
        header: None,
        locations: Some(locations),
        auth: None,
        upstream: None,
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

async fn send_request(addr: &str, method: &str, path: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = SocketStream::connect(addr.to_string())
        .await
        .expect("connect failed");

    let request =
        format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");

    stream.write_all(request.as_bytes()).await.expect("write");

    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut buf = [0u8; 16384];
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

/// Extract just the body portion (after the double CRLF) from an HTTP response.
fn extract_body(response: &str) -> &str {
    if let Some(pos) = response.find("\r\n\r\n") {
        &response[pos + 4..]
    } else {
        ""
    }
}

// ---------------------------------------------------------------------------
// Tests: URI rewriting
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_e2e_uri_path_rewrite() {
    let upstream = start_echo_upstream().await;

    let loc = LocationConfig {
        location: "/old".to_string(),
        mode: MatchMode::Prefix,
        provider: Some(ProviderType::Proxy),
        root: None,
        response: None,
        request: Some(RequestConfig {
            method: None,
            uri: Some(UriConfig {
                path: Some("/new".to_string()),
                query: None,
            }),
            headers: None,
            body: None,
        }),
        index_files: None,
        enable_directory_listing: None,
    };

    let proxy = start_proxy(upstream, vec![loc]).await;

    let response = send_request(&proxy, "GET", "/old/resource").await;
    let body = extract_body(&response);

    assert!(
        body.contains("GET /new"),
        "URI path should be rewritten from /old to /new, got: {}",
        body
    );
}

#[tokio::test]
async fn test_e2e_uri_query_rewrite() {
    let upstream = start_echo_upstream().await;

    let loc = LocationConfig {
        location: "/api".to_string(),
        mode: MatchMode::Prefix,
        provider: Some(ProviderType::Proxy),
        root: None,
        response: None,
        request: Some(RequestConfig {
            method: None,
            uri: Some(UriConfig {
                path: Some("/api".to_string()),
                query: Some("rewritten=true".to_string()),
            }),
            headers: None,
            body: None,
        }),
        index_files: None,
        enable_directory_listing: None,
    };

    let proxy = start_proxy(upstream, vec![loc]).await;

    let response = send_request(&proxy, "GET", "/api/data").await;
    let body = extract_body(&response);

    assert!(
        body.contains("rewritten=true"),
        "query should be rewritten, got: {}",
        body
    );
}

// ---------------------------------------------------------------------------
// Tests: method rewriting
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_e2e_method_rewrite_get_to_post() {
    let upstream = start_echo_upstream().await;

    let loc = LocationConfig {
        location: "/transform".to_string(),
        mode: MatchMode::Prefix,
        provider: Some(ProviderType::Proxy),
        root: None,
        response: None,
        request: Some(RequestConfig {
            method: Some("POST".to_string()),
            uri: None,
            headers: None,
            body: None,
        }),
        index_files: None,
        enable_directory_listing: None,
    };

    let proxy = start_proxy(upstream, vec![loc]).await;

    let response = send_request(&proxy, "GET", "/transform/data").await;
    let body = extract_body(&response);

    assert!(
        body.contains("POST /transform/data"),
        "method should be rewritten from GET to POST, got: {}",
        body
    );
}

// ---------------------------------------------------------------------------
// Tests: header rewriting
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_e2e_header_overwrite() {
    let upstream = start_echo_upstream().await;

    let mut headers = std::collections::HashMap::new();
    headers.insert(
        "X-Custom-Header".to_string(),
        HeaderAction {
            value: "overwritten-value".to_string(),
            action: HeaderActionType::Overwrite,
            condition: None,
        },
    );

    let loc = LocationConfig {
        location: "/rewrite".to_string(),
        mode: MatchMode::Prefix,
        provider: Some(ProviderType::Proxy),
        root: None,
        response: None,
        request: Some(RequestConfig {
            method: None,
            uri: None,
            headers: Some(headers),
            body: None,
        }),
        index_files: None,
        enable_directory_listing: None,
    };

    let proxy = start_proxy(upstream, vec![loc]).await;

    let response = send_request(&proxy, "GET", "/rewrite/path").await;
    let body = extract_body(&response);

    assert!(
        body.contains("X-Custom-Header: overwritten-value"),
        "custom header should be added/overwritten, got: {}",
        body
    );
}

#[tokio::test]
async fn test_e2e_header_missed_only_adds_if_absent() {
    let upstream = start_echo_upstream().await;

    let mut headers = std::collections::HashMap::new();
    headers.insert(
        "X-Optional".to_string(),
        HeaderAction {
            value: "default-value".to_string(),
            action: HeaderActionType::Missed,
            condition: None,
        },
    );

    let loc = LocationConfig {
        location: "/api".to_string(),
        mode: MatchMode::Prefix,
        provider: Some(ProviderType::Proxy),
        root: None,
        response: None,
        request: Some(RequestConfig {
            method: None,
            uri: None,
            headers: Some(headers),
            body: None,
        }),
        index_files: None,
        enable_directory_listing: None,
    };

    let proxy = start_proxy(upstream, vec![loc]).await;

    let response = send_request(&proxy, "GET", "/api/test").await;
    let body = extract_body(&response);

    assert!(
        body.contains("X-Optional: default-value"),
        "missed header should be added when absent, got: {}",
        body
    );
}

// ---------------------------------------------------------------------------
// Tests: combined rewrite (method + URI + header)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_e2e_combined_rewrite() {
    let upstream = start_echo_upstream().await;

    let mut headers = std::collections::HashMap::new();
    headers.insert(
        "X-Combined".to_string(),
        HeaderAction {
            value: "yes".to_string(),
            action: HeaderActionType::Overwrite,
            condition: None,
        },
    );

    let loc = LocationConfig {
        location: "/v1".to_string(),
        mode: MatchMode::Prefix,
        provider: Some(ProviderType::Proxy),
        root: None,
        response: None,
        request: Some(RequestConfig {
            method: Some("PUT".to_string()),
            uri: Some(UriConfig {
                path: Some("/v2".to_string()),
                query: Some("version=2".to_string()),
            }),
            headers: Some(headers),
            body: None,
        }),
        index_files: None,
        enable_directory_listing: None,
    };

    let proxy = start_proxy(upstream, vec![loc]).await;

    let response = send_request(&proxy, "GET", "/v1/resource").await;
    let body = extract_body(&response);

    assert!(
        body.contains("PUT /v2?version=2"),
        "method and URI should both be rewritten, got: {}",
        body
    );
    assert!(
        body.contains("X-Combined: yes"),
        "header should be added, got: {}",
        body
    );
}
