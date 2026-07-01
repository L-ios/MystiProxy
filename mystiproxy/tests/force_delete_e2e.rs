//! E2E tests for ForceDelete header action.
//!
//! These tests verify that the ForceDelete header action correctly removes
//! headers from requests before forwarding them to the upstream server.
//! Before the fix, ForceDelete was a no-op (empty match arm `{}`).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use mystiproxy::config::{
    EngineConfig, HeaderAction, HeaderActionType, LocationConfig, MatchMode, ProviderType,
    ProxyType, RequestConfig,
};
use mystiproxy::http::{create_handler, HttpServer, HttpServerConfig};
use mystiproxy::io::SocketStream;

async fn get_available_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind failed");
    listener.local_addr().expect("no addr").port()
}

/// Start an upstream that echoes back the full request (request line + headers).
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
        .expect("connect failed");

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

fn extract_body(response: &str) -> &str {
    if let Some(pos) = response.find("\r\n\r\n") {
        &response[pos + 4..]
    } else {
        ""
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_e2e_force_delete_removes_header() {
    let upstream = start_echo_upstream().await;

    let mut headers = HashMap::new();
    headers.insert(
        "X-Sensitive".to_string(),
        HeaderAction {
            value: "".to_string(),
            action: HeaderActionType::ForceDelete,
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

    // Send a request WITH the header that should be deleted
    let response = send_request_with_headers(
        &proxy,
        "GET",
        "/api/test",
        &[("X-Sensitive", "secret-data")],
    )
    .await;
    let body = extract_body(&response);

    assert!(
        !body.contains("X-Sensitive"),
        "X-Sensitive header should be DELETED by ForceDelete, but it appears in: {}",
        body
    );
    assert!(
        !body.contains("secret-data"),
        "secret-data value should be removed from request"
    );
}

#[tokio::test]
async fn test_e2e_force_delete_does_not_affect_other_headers() {
    let upstream = start_echo_upstream().await;

    let mut headers = HashMap::new();
    headers.insert(
        "X-Delete-Me".to_string(),
        HeaderAction {
            value: "".to_string(),
            action: HeaderActionType::ForceDelete,
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

    let response = send_request_with_headers(
        &proxy,
        "GET",
        "/api/test",
        &[("X-Delete-Me", "bye"), ("X-Keep-Me", "hello")],
    )
    .await;
    let body = extract_body(&response);

    assert!(
        !body.contains("X-Delete-Me"),
        "X-Delete-Me should be removed"
    );
    assert!(
        body.contains("X-Keep-Me"),
        "X-Keep-Me should be preserved: {}",
        body
    );
    assert!(
        body.contains("hello"),
        "X-Keep-Me value should be preserved"
    );
}

#[tokio::test]
async fn test_e2e_engine_level_force_delete() {
    let upstream = start_echo_upstream().await;

    let mut engine_headers = HashMap::new();
    engine_headers.insert(
        "X-Engine-Delete".to_string(),
        HeaderAction {
            value: "".to_string(),
            action: HeaderActionType::ForceDelete,
            condition: None,
        },
    );

    let proxy_port = get_available_port().await;
    let listen = format!("tcp://127.0.0.1:{proxy_port}");

    let config = EngineConfig {
        listen: listen.clone(),
        target: format!("tcp://127.0.0.1:{upstream}"),
        proxy_type: ProxyType::Http,
        request_timeout: Some(Duration::from_secs(5)),
        connection_timeout: None,
        header: Some(engine_headers),
        locations: None,
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

    let response =
        send_request_with_headers(&listen, "GET", "/", &[("X-Engine-Delete", "data")]).await;
    let body = extract_body(&response);

    assert!(
        !body.contains("X-Engine-Delete"),
        "Engine-level ForceDelete should remove header"
    );
}
