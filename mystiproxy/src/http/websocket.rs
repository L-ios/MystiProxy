//! WebSocket 模块
//!
//! 提供真正的 WebSocket 代理：升级请求转发到上游，成功后双向字节桥接。
//! 上游不可达或拒绝升级时向客户端返回 502。

use std::convert::Infallible;
use std::time::Duration;

use http_body_util::Empty;
use hyper::header;
use hyper::{Request, Response, StatusCode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{info, warn};

use crate::error::{MystiProxyError, Result};

/// 检查是否为 WebSocket 升级请求
pub fn is_websocket_upgrade_request(req: &Request<hyper::body::Incoming>) -> bool {
    if let Some(upgrade) = req.headers().get(header::UPGRADE) {
        if let Ok(upgrade_str) = upgrade.to_str() {
            return upgrade_str.eq_ignore_ascii_case("websocket");
        }
    }
    false
}

/// 代理 WebSocket 升级请求：连接上游、转发握手、双向桥接。
///
/// - 上游握手成功（101）→ 返回给客户端 101，随后透传字节流
/// - 上游不可达/拒绝 → 返回 502
pub async fn proxy_websocket(
    req: Request<hyper::body::Incoming>,
    target: &str,
    timeout: Option<Duration>,
) -> Result<Response<Empty<Infallible>>> {
    let key = req
        .headers()
        .get(header::SEC_WEBSOCKET_KEY)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    if key.is_empty() {
        return bad_gateway("missing Sec-WebSocket-Key");
    }

    // 提取转发要素
    let path_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.to_string())
        .unwrap_or_else(|| "/".to_string());
    let origin = header_str(req.headers(), header::ORIGIN);
    let subproto = header_str(req.headers(), header::SEC_WEBSOCKET_PROTOCOL);

    // 连接上游并完成握手（受 engine 超时约束）
    let handshake = upstream_handshake(target, &path_query, &key, origin.as_deref(), subproto.as_deref());
    let (mut upstream_stream, upstream_headers) = match timeout {
        Some(t) => match tokio::time::timeout(t, handshake).await {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                warn!("websocket upstream handshake failed: {e}");
                return bad_gateway("upstream rejected websocket upgrade");
            }
            Err(_) => return bad_gateway("upstream handshake timeout"),
        },
        None => match handshake.await {
            Ok(v) => v,
            Err(e) => {
                warn!("websocket upstream handshake failed: {e}");
                return bad_gateway("upstream rejected websocket upgrade");
            }
        },
    };

    info!("websocket tunnel established via {target}{path_query}");

    // 客户端侧 101：Accept 基于客户端 Key；上游协商头透传
    let accept = compute_websocket_accept(&key);
    let mut builder = Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(header::UPGRADE, "websocket")
        .header(header::CONNECTION, "upgrade")
        .header(header::SEC_WEBSOCKET_ACCEPT, accept);
    for name in [header::SEC_WEBSOCKET_PROTOCOL, header::SEC_WEBSOCKET_EXTENSIONS] {
        if let Some(v) = upstream_headers.iter().find(|(n, _)| *n == name) {
            builder = builder.header(name, v.1.clone());
        }
    }
    let response = builder.body(Empty::new()).map_err(MystiProxyError::Http)?;

    // 升级后的客户端 IO 与上游字节流双向桥接
    tokio::spawn(async move {
        match hyper::upgrade::on(req).await {
            Ok(upgraded) => {
                let mut client = hyper_util::rt::TokioIo::new(upgraded);
                match tokio::io::copy_bidirectional(&mut client, &mut upstream_stream).await {
                    Ok((a, b)) => info!("websocket tunnel closed (c2u={a}, u2c={b})"),
                    Err(e) => warn!("websocket tunnel error: {e}"),
                }
            }
            Err(e) => warn!("websocket client upgrade failed: {e}"),
        }
    });

    Ok(response)
}

/// 与上游完成 WebSocket 握手，返回字节流与响应头。
async fn upstream_handshake(
    target: &str,
    path_query: &str,
    key: &str,
    origin: Option<&str>,
    subproto: Option<&str>,
) -> Result<(tokio::net::TcpStream, hyper::HeaderMap)> {
    let addr = crate::proxy::address::Address::parse(target)?;
    let socket_addr = addr
        .as_tcp()
        .ok_or_else(|| MystiProxyError::Proxy("websocket over UDS target not supported".into()))?;

    let mut stream = tokio::net::TcpStream::connect(socket_addr).await?;

    let host = socket_addr.to_string();
    let mut req_bytes = format!(
        "GET {path_query} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\n\
         Sec-WebSocket-Version: 13\r\n"
    );
    if let Some(o) = origin {
        req_bytes.push_str(&format!("Origin: {o}\r\n"));
    }
    if let Some(s) = subproto {
        req_bytes.push_str(&format!("Sec-WebSocket-Protocol: {s}\r\n"));
    }
    req_bytes.push_str("\r\n");

    stream.write_all(req_bytes.as_bytes()).await?;

    // 读取响应头（读到 \r\n\r\n）
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 512];
    loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Err(MystiProxyError::Proxy("upstream closed during handshake".into()));
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = find_header_end(&buf) {
            let head = String::from_utf8_lossy(&buf[..pos]).to_string();
            let mut lines = head.split("\r\n");
            let status_line = lines.next().unwrap_or_default();
            if !status_line.contains("101") {
                return Err(MystiProxyError::Proxy(format!(
                    "upstream refused upgrade: {status_line}"
                )));
            }
            let mut headers = hyper::HeaderMap::new();
            for line in lines {
                if let Some((k, v)) = line.split_once(':') {
                    if let (Ok(name), Ok(value)) = (
                        k.trim().parse::<hyper::header::HeaderName>(),
                        v.trim().parse::<http::HeaderValue>(),
                    ) {
                        headers.append(name, value);
                    }
                }
            }
            return Ok((stream, headers));
        }
        if buf.len() > 16 * 1024 {
            return Err(MystiProxyError::Proxy("upstream handshake headers too large".into()));
        }
    }
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

fn header_str(headers: &hyper::HeaderMap, name: hyper::header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

fn bad_gateway(msg: &str) -> Result<Response<Empty<Infallible>>> {
    Ok(Response::builder()
        .status(StatusCode::BAD_GATEWAY)
        .header(header::CONNECTION, "close")
        .body(Empty::new())
        .unwrap_or_else(|_| Response::new(Empty::new())))
        .map(|r: Response<Empty<Infallible>>| {
            let _ = msg;
            r
        })
}

/// 计算 WebSocket 接受密钥
fn compute_websocket_accept(key: &str) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use sha1::{Digest, Sha1};

    let mut hasher = Sha1::new();
    hasher.update(key);
    hasher.update("258EAFA5-E914-47DA-95CA-C5AB0DC85B11"); // WebSocket 魔术字符串
    let hash = hasher.finalize();
    STANDARD.encode(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_websocket_accept_rfc_example() {
        // RFC 6455 示例
        assert_eq!(
            compute_websocket_accept("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn test_find_header_end() {
        assert_eq!(find_header_end(b"HTTP/1.1 101\r\n\r\nrest"), Some(16));
        assert_eq!(find_header_end(b"no end here"), None);
        assert_eq!(find_header_end(b""), None);
    }

    #[tokio::test]
    async fn test_upstream_handshake_success_and_reject() {
        // 真 TCP 假上游：接受后回 101
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let n = s.read(&mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            s.write_all(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: abc\r\n\r\n")
                .await
                .unwrap();
            req
        });

        let (_stream, headers) = upstream_handshake(
            &format!("tcp://{addr}"),
            "/chat",
            "dGhlIHNhbXBsZSBub25jZQ==",
            Some("http://example.com"),
            Some("chat.v2"),
        )
        .await
        .unwrap();

        let req = server.await.unwrap();
        // 握手请求头断言
        assert!(req.starts_with("GET /chat HTTP/1.1"), "{req}");
        assert!(req.contains(&format!("Host: {addr}")));
        assert!(req.contains("Upgrade: websocket"));
        assert!(req.contains("Connection: Upgrade"));
        assert!(req.contains("Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ=="));
        assert!(req.contains("Sec-WebSocket-Version: 13"));
        assert!(req.contains("Origin: http://example.com"));
        assert!(req.contains("Sec-WebSocket-Protocol: chat.v2"));
        // 上游响应头解析
        assert_eq!(
            headers.get(header::SEC_WEBSOCKET_ACCEPT).unwrap(),
            "abc"
        );
    }

    #[tokio::test]
    async fn test_upstream_reject_non_101() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 512];
            let _ = s.read(&mut buf).await;
            s.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
        });

        let r = upstream_handshake(&format!("tcp://{addr}"), "/", "k", None, None).await;
        assert!(r.is_err());
        assert!(r.unwrap_err().to_string().contains("403"));
    }

    #[tokio::test]
    async fn test_upstream_unreachable() {
        // 保留端口未监听
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l);
        let r = upstream_handshake(&format!("tcp://127.0.0.1:{port}"), "/", "k", None, None).await;
        assert!(r.is_err());
    }
}
