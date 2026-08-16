//! e2e test: HTTP engine routes through a real upstream HTTP proxy (CONNECT tunnel).
//!
//! Chain: client → MystiProxy(http engine) → upstream proxy(python-like CONNECT impl) → target HTTP server.
//! Verifies `upstream:` engine config is actually wired through HttpClientPool (regression for the
//! previously-dead upstream path).

use std::sync::Arc;
use std::time::Duration;

use mystiproxy::config::{EngineConfig, LocationConfig, MatchMode, ProviderType, ProxyType};

/// Minimal CONNECT-only upstream proxy (same wire protocol as squid/tinyproxy).
async fn start_upstream_proxy(port: u16) {
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .expect("upstream bind");
        loop {
            let (mut client, _) = listener.accept().await.expect("accept");
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = vec![0u8; 4096];
                // Read until end of headers
                let mut req = String::new();
                loop {
                    let n = client.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    req.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if req.contains("\r\n\r\n") {
                        break;
                    }
                }
                let first = req.lines().next().unwrap_or_default().to_string();
                // Parse "CONNECT host:port HTTP/1.1"
                let target = first
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();
                if !first.starts_with("CONNECT") || target.is_empty() {
                    let _ = client.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
                    return;
                }
                let upstream_tcp = tokio::net::TcpStream::connect(&target).await;
                match upstream_tcp {
                    Ok(mut remote) => {
                        let _ = client
                            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                            .await;
                        // Bidirectional copy
                        let (mut cr, mut cw) = client.split();
                        let (mut rr, mut rw) = remote.split();
                        let a = tokio::io::copy(&mut cr, &mut rw);
                        let b = tokio::io::copy(&mut rr, &mut cw);
                        let _ = tokio::join!(a, b);
                    }
                    Err(_) => {
                        let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
                    }
                }
            });
        }
    });
}

/// Plain HTTP upstream (no TLS) answering on the target port.
async fn start_target_server(port: u16) {
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .expect("target bind");
        loop {
            let (mut sock, _) = listener.accept().await.expect("accept");
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = vec![0u8; 2048];
                let _ = sock.read(&mut buf).await;
                let body = "upstream-chain-ok";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
}

#[tokio::test]
async fn test_e2e_http_engine_via_upstream_connect_tunnel() {
    start_upstream_proxy(19251).await;
    start_target_server(19252).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let engine = EngineConfig {
        listen: "tcp://127.0.0.1:19250".to_string(),
        target: "tcp://127.0.0.1:19252".to_string(),
        proxy_type: ProxyType::Http,
        request_timeout: Some(Duration::from_secs(5)),
        connection_timeout: None,
        header: None,
        locations: Some(vec![LocationConfig {
            location: "/".to_string(),
            mode: MatchMode::Prefix,
            provider: Some(ProviderType::Proxy),
            root: None,
            response: None,
            request: None,
            index_files: None,
            enable_directory_listing: None,
        }]),
        auth: None,
        tls: None,
        upstream: Some("http://127.0.0.1:19251".to_string()),
        allow: None,
        deny: None,
        management: None,
    };

    let handler = mystiproxy::http::create_handler(Arc::new(engine)).expect("handler");
    let mut server = mystiproxy::http::HttpServer::new(
        mystiproxy::http::HttpServerConfig::new(
            "tcp://127.0.0.1:19250".to_string(),
            Some(Duration::from_secs(5)),
        ),
        handler,
        None,
    );
    server.start().await.expect("server start");
    tokio::spawn(async move {
        let _ = server.run().await;
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let resp = tokio::time::timeout(Duration::from_secs(5), async {
        let mut s = tokio::net::TcpStream::connect("127.0.0.1:19250")
            .await
            .expect("connect mystiproxy");
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        s.write_all(b"GET /path HTTP/1.1\r\nHost: target\r\nConnection: close\r\n\r\n")
            .await
            .expect("send");
        let mut buf = vec![0u8; 2048];
        let n = s.read(&mut buf).await.expect("read");
        String::from_utf8_lossy(&buf[..n]).to_string()
    })
    .await
    .expect("timeout");

    assert!(
        resp.contains("200 OK"),
        "expected 200 via upstream tunnel, got: {resp}"
    );
    assert!(
        resp.contains("upstream-chain-ok"),
        "expected target body through tunnel, got: {resp}"
    );
}
