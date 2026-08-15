//! E2E tests for request body JSON transformation.
//!
//! These tests verify that JSONPath-based body transformations are correctly
//! applied to request bodies before forwarding to the upstream server.

use std::sync::Arc;
use std::time::Duration;

use mystiproxy::config::{
    BodyConfig, EngineConfig, JsonBodyAction, JsonBodyConfig, LocationConfig, MatchMode,
    ProviderType, ProxyType, RequestConfig,
};
use mystiproxy::http::{create_handler, HttpServer, HttpServerConfig};
use mystiproxy::io::SocketStream;

async fn get_available_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind failed");
    listener.local_addr().expect("no addr").port()
}

/// Start an upstream that echoes back the request body.
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

                    // Extract just the body (after \r\n\r\n)
                    let body = if let Some(pos) = request.find("\r\n\r\n") {
                        &request[pos + 4..]
                    } else {
                        ""
                    };

                    let resp_body = format!("UPSTREAM_RECEIVED:{body}");
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n{resp_body}",
                        resp_body.len()
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

async fn send_json_request(addr: &str, method: &str, path: &str, json_body: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = SocketStream::connect(addr.to_string())
        .await
        .expect("connect failed");

    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json_body}",
        json_body.len()
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
async fn test_e2e_body_transform_overwrite_field() {
    let upstream = start_echo_upstream().await;

    let loc = LocationConfig {
        location: "/api".to_string(),
        mode: MatchMode::Prefix,
        provider: Some(ProviderType::Proxy),
        root: None,
        response: None,
        request: Some(RequestConfig {
            method: None,
            uri: None,
            headers: None,
            body: Some(BodyConfig {
                json: Some(JsonBodyConfig {
                    path: "$.version".to_string(),
                    value: "2".to_string(),
                    action: JsonBodyAction::Overwrite,
                }),
                body_type: None,
                template: None,
                content: None,
            }),
        }),
        index_files: None,
        enable_directory_listing: None,
    };

    let proxy = start_proxy(upstream, vec![loc]).await;

    let response = send_json_request(
        &proxy,
        "POST",
        "/api/data",
        r#"{"name":"test","version":1}"#,
    )
    .await;
    let body = extract_body(&response);

    assert!(
        body.contains("UPSTREAM_RECEIVED:"),
        "should get response from upstream"
    );
    // The version field should be transformed from 1 to "2"
    assert!(
        body.contains(r#""version":"2""#) || body.contains(r#""version":2"#),
        "version should be overwritten to 2, got: {body}"
    );
}

#[tokio::test]
async fn test_e2e_body_transform_nested_field() {
    let upstream = start_echo_upstream().await;

    let loc = LocationConfig {
        location: "/api".to_string(),
        mode: MatchMode::Prefix,
        provider: Some(ProviderType::Proxy),
        root: None,
        response: None,
        request: Some(RequestConfig {
            method: None,
            uri: None,
            headers: None,
            body: Some(BodyConfig {
                json: Some(JsonBodyConfig {
                    path: "$.user.name".to_string(),
                    value: "transformed".to_string(),
                    action: JsonBodyAction::Overwrite,
                }),
                body_type: None,
                template: None,
                content: None,
            }),
        }),
        index_files: None,
        enable_directory_listing: None,
    };

    let proxy = start_proxy(upstream, vec![loc]).await;

    let response = send_json_request(
        &proxy,
        "POST",
        "/api/update",
        r#"{"user":{"name":"original","id":123}}"#,
    )
    .await;
    let body = extract_body(&response);

    assert!(
        body.contains(r#""name":"transformed""#),
        "nested field should be transformed, got: {body}"
    );
    assert!(
        body.contains(r#""id":123"#),
        "other fields should be preserved, got: {body}"
    );
}

#[tokio::test]
async fn test_e2e_body_transform_delete_field() {
    let upstream = start_echo_upstream().await;

    let loc = LocationConfig {
        location: "/api".to_string(),
        mode: MatchMode::Prefix,
        provider: Some(ProviderType::Proxy),
        root: None,
        response: None,
        request: Some(RequestConfig {
            method: None,
            uri: None,
            headers: None,
            body: Some(BodyConfig {
                json: Some(JsonBodyConfig {
                    path: "$.secret".to_string(),
                    value: String::new(),
                    action: JsonBodyAction::Delete,
                }),
                body_type: None,
                template: None,
                content: None,
            }),
        }),
        index_files: None,
        enable_directory_listing: None,
    };

    let proxy = start_proxy(upstream, vec![loc]).await;

    let response = send_json_request(
        &proxy,
        "POST",
        "/api/data",
        r#"{"data":"keep","secret":"sensitive-value"}"#,
    )
    .await;
    let body = extract_body(&response);

    assert!(
        !body.contains("secret"),
        "secret field should be deleted, got: {body}"
    );
    assert!(
        body.contains(r#""data":"keep""#),
        "data field should be preserved, got: {body}"
    );
}
