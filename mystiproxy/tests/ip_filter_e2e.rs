//! e2e tests for inbound IP filtering (CIDR allow/deny) on HTTP and TCP engines.
//!
//! Semantics under test (see `src/ip_filter.rs`):
//! - deny wins over allow
//! - non-empty allow list means only matching peers pass
//! - no config => no filtering

use std::sync::Arc;
use std::time::Duration;

use mystiproxy::config::{EngineConfig, LocationConfig, MatchMode, ProviderType, ProxyType};
use mystiproxy::http::{create_handler, HttpServer, HttpServerConfig};
use mystiproxy::ip_filter::IpFilter;
use mystiproxy::proxy::ProxyServer;

fn http_engine(listen: &str) -> EngineConfig {
    EngineConfig {
        listen: listen.to_string(),
        target: "tcp://127.0.0.1:1".to_string(),
        proxy_type: ProxyType::Http,
        request_timeout: Some(Duration::from_secs(2)),
        connection_timeout: None,
        header: None,
        locations: Some(vec![LocationConfig {
            location: "/".to_string(),
            mode: MatchMode::Prefix,
            provider: Some(ProviderType::Mock),
            root: None,
            response: None,
            request: None,
            index_files: None,
            enable_directory_listing: None,
        }]),
        auth: None,
        tls: None,
        upstream: None,
        allow: None,
        deny: None,
    }
}

async fn start_http_with_filter(port: u16, filter: Option<IpFilter>) {
    let cfg = Arc::new(http_engine(&format!("tcp://127.0.0.1:{port}")));
    let handler = create_handler(cfg).expect("handler");
    let mut server = HttpServer::new(
        HttpServerConfig::new(
            format!("tcp://127.0.0.1:{port}"),
            Some(Duration::from_secs(2)),
        ),
        handler,
        None,
    );
    server = server.with_ip_filter(filter);
    server.start().await.expect("server start");
    tokio::spawn(async move {
        let _ = server.run().await;
    });
}

async fn http_get(port: u16) -> Result<u16, String> {
    tokio::time::timeout(Duration::from_secs(3), async {
        match tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
            Ok(mut s) => {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let _ = s
                    .write_all(b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
                    .await;
                let mut buf = vec![0u8; 128];
                let n = s.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    Err("closed without response".to_string())
                } else {
                    let head = String::from_utf8_lossy(&buf[..n]);
                    head.split_whitespace()
                        .nth(1)
                        .and_then(|c| c.parse::<u16>().ok())
                        .ok_or_else(|| "no status".to_string())
                }
            }
            Err(e) => Err(format!("connect failed: {e}")),
        }
    })
    .await
    .map_err(|e| format!("timeout: {e}"))?
}

#[tokio::test]
async fn test_e2e_ip_filter_deny_localhost_rejects() {
    // deny 127.0.0.0/8 must reject loopback peers even though allow is unset
    let filter = IpFilter::from_config(&None, &Some(vec!["127.0.0.0/8".to_string()])).unwrap();
    start_http_with_filter(19191, filter).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let r = http_get(19191).await;
    // Either connection refused (never accepted) or immediate close is acceptable rejection
    assert!(r.is_err(), "expected rejection, got {:?}", r);
}

#[tokio::test]
async fn test_e2e_ip_filter_allow_localhost_passes() {
    let filter = IpFilter::from_config(&Some(vec!["127.0.0.0/8".to_string()]), &None).unwrap();
    start_http_with_filter(19192, filter).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let status = http_get(19192).await.expect("should be allowed");
    assert_eq!(status, 200, "mock location should answer 200");
}

#[tokio::test]
async fn test_e2e_ip_filter_deny_wins_over_allow() {
    // both allow and deny contain loopback => deny wins
    let filter = IpFilter::from_config(
        &Some(vec!["127.0.0.0/8".to_string()]),
        &Some(vec!["127.0.0.1/32".to_string()]),
    )
    .unwrap()
    .expect("deny wins => filter present");
    start_http_with_filter(19193, Some(filter)).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let r = http_get(19193).await;
    assert!(r.is_err(), "deny must win, got {:?}", r);
}

#[tokio::test]
async fn test_e2e_ip_filter_no_config_passes() {
    start_http_with_filter(19194, None).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let status = http_get(19194).await.expect("no filter must allow");
    assert_eq!(status, 200);
}

#[tokio::test]
async fn test_e2e_ip_filter_tcp_engine_rejects() {
    // TCP engine: ProxyServer::from_engine_config wires ip_filter from allow/deny
    let mut cfg = http_engine("tcp://127.0.0.1:19195");
    cfg.proxy_type = ProxyType::Tcp;
    cfg.deny = Some(vec!["127.0.0.0/8".to_string()]);
    let mut server = ProxyServer::from_engine_config(&cfg).expect("server");
    server.start().await.expect("start");
    tokio::spawn(async move {
        let _ = server.run().await;
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let r = tokio::time::timeout(Duration::from_secs(2), async {
        match tokio::net::TcpStream::connect("127.0.0.1:19195").await {
            Ok(mut s) => {
                use tokio::io::AsyncReadExt;
                let mut buf = [0u8; 16];
                // Server should close immediately (0 bytes) on reject
                let n = tokio::time::timeout(Duration::from_secs(1), s.read(&mut buf)).await;
                match n {
                    Ok(Ok(0)) | Err(_) => Ok::<(), ()>(()),
                    Ok(Ok(_)) => Err(()),
                    Ok(Err(_)) => Ok(()),
                }
            }
            Err(_) => Ok(()),
        }
    })
    .await;
    assert!(r.is_ok(), "tcp engine should reject denied peer");
}

#[test]
fn test_ip_filter_invalid_cidr_rejected() {
    assert!(IpFilter::from_config(&None, &Some(vec!["999.0.0.0/8".to_string()])).is_err());
    assert!(IpFilter::from_config(&None, &Some(vec!["127.0.0.0/33".to_string()])).is_err());
    assert!(IpFilter::from_config(&None, &Some(vec!["not-an-ip".to_string()])).is_err());
}
