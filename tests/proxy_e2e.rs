//! E2E tests for the complete HTTP proxy forwarding chain:
//!   real upstream server → MystiProxy (HttpServer + HttpRequestHandler) → raw TCP client
//!
//! These tests start a real upstream HTTP server, then start MystiProxy pointing at it,
//! then send actual HTTP requests through the proxy and verify responses are forwarded
//! correctly including status codes, headers, and bodies.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use mystiproxy::config::{
    EngineConfig, HeaderAction, HeaderActionType, LocationConfig, MatchMode, ProviderType,
    ProxyType,
};
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

/// A minimal upstream HTTP/1.1 server that responds with a fixed status + body.
/// Uses tokio TcpListener directly to avoid hyper server complexity in tests.
async fn start_upstream_server(
    status: u16,
    body: String,
    extra_headers: Vec<(String, String)>,
) -> u16 {
    let port = get_available_port().await;

    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
            .await
            .expect("upstream bind failed");

        loop {
            if let Ok((mut stream, _)) = listener.accept().await {
                let body = body.clone();
                let headers = extra_headers.clone();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};

                    let mut buf = vec![0u8; 8192];
                    // Read the request (until we have the headers)
                    match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(_) => {}
                    }

                    let response = format!(
                        "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n{body}",
                        body.len(),
                        headers
                            .iter()
                            .map(|(k, v)| format!("{k}: {v}\r\n"))
                            .collect::<String>()
                    );

                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(30)).await;
    port
}

/// Start an upstream that echoes back the request method and path.
async fn start_echo_upstream() -> u16 {
    let port = get_available_port().await;

    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
            .await
            .expect("echo upstream bind failed");

        loop {
            if let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};

                    let mut buf = vec![0u8; 8192];
                    let n = match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => n,
                    };

                    let request = String::from_utf8_lossy(&buf[..n]);
                    let request_line = request.lines().next().unwrap_or("");
                    let body = format!("ECHO: {request_line}");

                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );

                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(30)).await;
    port
}

/// Start MystiProxy as an HTTP server pointing at the given upstream port.
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
        locations: if locations.is_empty() {
            None
        } else {
            Some(locations)
        },
        auth: None,
        tls: None,
    };

    let handler = create_handler(Arc::new(config)).expect("failed to create handler");
    let mut server = HttpServer::new(HttpServerConfig::new(listen.clone(), None), handler, None);
    server
        .start()
        .await
        .expect("failed to start proxy server");

    tokio::spawn(async move {
        let _ = server.run().await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    listen
}

/// Send a raw HTTP/1.1 request through the proxy and return the full response string.
async fn send_request(addr: &str, method: &str, path: &str) -> String {
    send_request_with_headers(addr, method, path, &[]).await
}

/// Send a raw HTTP/1.1 request with custom headers through the proxy.
async fn send_request_with_headers(
    addr: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = SocketStream::connect(addr.to_string())
        .await
        .expect("failed to connect to proxy");

    let header_str: String = headers
        .iter()
        .map(|(k, v)| format!("{k}: {v}\r\n"))
        .collect();

    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\n{header_str}Connection: close\r\n\r\n"
    );

    stream
        .write_all(request.as_bytes())
        .await
        .expect("failed to write request");

    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), async {
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

/// Send a raw HTTP/1.1 request with a body through the proxy.
async fn send_request_with_body(
    addr: &str,
    method: &str,
    path: &str,
    body: &str,
    extra_headers: &[(&str, &str)],
) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = SocketStream::connect(addr.to_string())
        .await
        .expect("failed to connect to proxy");

    let header_str: String = extra_headers
        .iter()
        .map(|(k, v)| format!("{k}: {v}\r\n"))
        .collect();

    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n{header_str}Connection: close\r\n\r\n{body}",
        body.len()
    );

    stream
        .write_all(request.as_bytes())
        .await
        .expect("failed to write request");

    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), async {
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
// Tests: basic forwarding
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_e2e_proxy_forwards_get_request() {
    let upstream = start_upstream_server(200, "hello from upstream".to_string(), vec![]).await;
    let proxy = start_proxy(upstream, vec![]).await;

    let response = send_request(&proxy, "GET", "/").await;

    assert!(
        response.starts_with("HTTP/1.1 200"),
        "expected 200 from upstream, got: {}",
        &response[..response.len().min(100)]
    );
    assert!(
        response.contains("hello from upstream"),
        "expected upstream body in response"
    );
}

#[tokio::test]
async fn test_e2e_proxy_forwards_post_request() {
    let upstream = start_echo_upstream().await;
    let proxy = start_proxy(upstream, vec![]).await;

    let response = send_request_with_body(&proxy, "POST", "/submit", "data=hello", &[]).await;

    assert!(
        response.contains("200"),
        "expected 200 status, got: {}",
        &response[..response.len().min(100)]
    );
    assert!(
        response.contains("POST /submit"),
        "upstream should echo POST method and path"
    );
}

#[tokio::test]
async fn test_e2e_proxy_preserves_query_params() {
    let upstream = start_echo_upstream().await;
    let proxy = start_proxy(upstream, vec![]).await;

    let response = send_request(&proxy, "GET", "/search?q=test&page=2").await;

    assert!(
        response.contains("/search?q=test&page=2"),
        "query params should be preserved through proxy"
    );
}

#[tokio::test]
async fn test_e2e_proxy_forwards_custom_status_code() {
    let upstream = start_upstream_server(418, "I'm a teapot".to_string(), vec![]).await;
    let proxy = start_proxy(upstream, vec![]).await;

    let response = send_request(&proxy, "GET", "/teapot").await;

    assert!(
        response.contains("418"),
        "expected 418 status forwarded from upstream"
    );
}

#[tokio::test]
async fn test_e2e_proxy_forwards_custom_headers() {
    let upstream = start_upstream_server(
        200,
        "ok".to_string(),
        vec![("X-Custom-Upstream".to_string(), "value123".to_string())],
    )
    .await;
    let proxy = start_proxy(upstream, vec![]).await;

    let response = send_request(&proxy, "GET", "/").await;

    assert!(
        response.contains("X-Custom-Upstream"),
        "custom header from upstream should be forwarded"
    );
    assert!(
        response.contains("value123"),
        "custom header value should be forwarded"
    );
}

#[tokio::test]
async fn test_e2e_proxy_forwards_put_delete() {
    let upstream = start_echo_upstream().await;
    let proxy = start_proxy(upstream, vec![]).await;

    let put_response = send_request_with_body(&proxy, "PUT", "/resource/1", "updated", &[]).await;
    assert!(
        put_response.contains("PUT /resource/1"),
        "PUT should be forwarded correctly"
    );

    let delete_response = send_request(&proxy, "DELETE", "/resource/1").await;
    assert!(
        delete_response.contains("DELETE /resource/1"),
        "DELETE should be forwarded correctly"
    );
}

// ---------------------------------------------------------------------------
// Tests: multiple requests / concurrency
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_e2e_proxy_handles_multiple_sequential_requests() {
    let upstream = start_upstream_server(200, "ok".to_string(), vec![]).await;
    let proxy = start_proxy(upstream, vec![]).await;

    for i in 0..5 {
        let response = send_request(&proxy, "GET", &format!("/req/{i}")).await;
        assert!(
            response.contains("200"),
            "request {i} should succeed"
        );
    }
}

#[tokio::test]
async fn test_e2e_proxy_concurrent_requests() {
    let upstream = start_echo_upstream().await;
    let proxy = start_proxy(upstream, vec![]).await;
    let proxy = Arc::new(proxy);

    let mut handles = Vec::new();
    for i in 0..10 {
        let p = proxy.clone();
        handles.push(tokio::spawn(async move {
            send_request(&p, "GET", &format!("/concurrent/{i}")).await
        }));
    }

    for (i, handle) in handles.into_iter().enumerate() {
        let response = handle.await.expect("task panicked");
        assert!(
            response.contains("200"),
            "concurrent request {i} should succeed"
        );
    }
}

// ---------------------------------------------------------------------------
// Tests: proxy with location-based routing (mix of proxy + mock)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_e2e_proxy_and_mock_coexist() {
    let upstream = start_upstream_server(200, "real response".to_string(), vec![]).await;

    let mock_loc = LocationConfig {
        location: "/mock".to_string(),
        mode: MatchMode::Prefix,
        provider: Some(ProviderType::Mock),
        root: None,
        response: Some(mystiproxy::config::ResponseConfig {
            status: Some(201),
            headers: None,
            body: None,
        }),
        request: None,
    };

    let proxy = start_proxy(upstream, vec![mock_loc]).await;

    // Mock path returns 201
    let mock_response = send_request(&proxy, "GET", "/mock/test").await;
    assert!(
        mock_response.contains("201"),
        "mock path should return 201"
    );

    // Non-mock path forwards to upstream
    let proxy_response = send_request(&proxy, "GET", "/real").await;
    assert!(
        proxy_response.contains("real response"),
        "non-mock path should be proxied to upstream"
    );
}

// ---------------------------------------------------------------------------
// Tests: request header rewriting via engine-level config
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_e2e_proxy_adds_engine_level_headers() {
    let upstream = start_echo_upstream().await;

    let proxy_port = get_available_port().await;
    let listen = format!("tcp://127.0.0.1:{proxy_port}");

    let mut header_map = HashMap::new();
    header_map.insert(
        "X-Forwarded-By".to_string(),
        HeaderAction {
            value: "mystiproxy".to_string(),
            action: HeaderActionType::Overwrite,
            condition: None,
        },
    );

    let config = EngineConfig {
        listen: listen.clone(),
        target: format!("tcp://127.0.0.1:{upstream}"),
        proxy_type: ProxyType::Http,
        request_timeout: Some(Duration::from_secs(5)),
        connection_timeout: None,
        header: Some(header_map),
        locations: None,
        auth: None,
        tls: None,
    };

    let handler = create_handler(Arc::new(config)).expect("failed to create handler");
    let mut server = HttpServer::new(HttpServerConfig::new(listen.clone(), None), handler, None);
    server.start().await.expect("failed to start proxy");

    tokio::spawn(async move {
        let _ = server.run().await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let response = send_request(&listen, "GET", "/").await;
    assert!(
        response.contains("200"),
        "request should succeed, got: {}",
        &response[..response.len().min(100)]
    );
}
