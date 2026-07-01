//! HTTP 客户端模块
//!
//! 提供 HTTP 客户端功能，支持连接池和请求转发

use std::sync::Arc;
use std::time::Duration;

use hyper::body::Incoming;
use hyper::client::conn::http1::{Builder, SendRequest};
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

use crate::error::{MystiProxyError, Result};
use crate::io::SocketStream;

/// 从目标地址字符串中提取 Host 值
///
/// 支持格式：
/// - `"tcp://host:port"` → `"host:port"`
/// - `"unix:///path/to/socket"` → `"localhost"`
/// - `"host:port"` → `"host:port"`
fn extract_host_from_target(target: &str) -> Option<String> {
    if target.starts_with("unix://") {
        return Some("localhost".to_string());
    }

    let addr = target.strip_prefix("tcp://").unwrap_or(target);

    if addr.is_empty() {
        return None;
    }

    Some(addr.to_string())
}

/// HTTP 客户端连接
pub struct HttpClient {
    /// 目标地址
    target: String,
    /// 超时时间
    timeout: Option<Duration>,
}

impl HttpClient {
    /// 创建新的 HTTP 客户端
    pub fn new(target: String, timeout: Option<Duration>) -> Self {
        Self { target, timeout }
    }

    /// 建立到目标服务器的连接
    async fn establish_connection(&self) -> Result<SendRequest<Incoming>> {
        let stream = SocketStream::connect(self.target.clone()).await?;
        let io = TokioIo::new(stream);

        // 创建 HTTP/1.1 客户端连接
        let (sender, conn) = Builder::new()
            .preserve_header_case(true)
            .title_case_headers(true)
            .handshake(io)
            .await
            .map_err(|e| {
                MystiProxyError::Proxy(format!("Failed to establish connection: {e}"))
            })?;

        // 在后台任务中维护连接
        tokio::spawn(async move {
            if let Err(err) = conn.await {
                error!("Connection error: {:?}", err);
            }
        });

        debug!("Successfully connected to {}", self.target);
        Ok(sender)
    }

    /// 发送请求（支持任意 body 类型）。
    ///
    /// 通过 raw TCP 序列化发送，绕过 hyper `SendRequest` 对 `Incoming` 类型的限制。
    /// 用于 handler 在转发前修改请求体（如 JSON 变换）的场景。
    /// 返回的 response body 类型是 `Full<Bytes>`，handler 通过 `collect()` 统一处理。
    pub async fn send_request_with_body<B>(
        &self,
        request: Request<B>,
    ) -> Result<Response<bytes::Bytes>>
    where
        B: hyper::body::Body + Send + 'static,
        B::Data: Send,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        use http_body_util::BodyExt;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Extract request metadata BEFORE consuming the body
        let method = request.method().clone();
        let uri = request.uri().clone();
        let headers = request.headers().clone();

        // Collect body to bytes
        let body_bytes = request
            .into_body()
            .collect()
            .await
            .map_err(|e| {
                let boxed: Box<dyn std::error::Error + Send + Sync> = e.into();
                MystiProxyError::Hyper(boxed.to_string())
            })?
            .to_bytes();

        // Connect via raw TCP
        let mut stream = SocketStream::connect(self.target.clone()).await?;
        let host = extract_host_from_target(&self.target).unwrap_or_default();

        // Serialize request line
        let path = uri
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or("/");

        // Serialize headers
        let mut header_str = String::new();
        let mut has_host = false;
        let mut has_cl = false;
        for (name, value) in &headers {
            let name_str = name.as_str();
            if name_str.eq_ignore_ascii_case("host") {
                has_host = true;
            }
            if name_str.eq_ignore_ascii_case("content-length") {
                has_cl = true;
            }
            header_str.push_str(&format!(
                "{name_str}: {}\r\n",
                value.to_str().unwrap_or("")
            ));
        }
        if !has_host {
            header_str.push_str(&format!("Host: {host}\r\n"));
        }
        if !has_cl {
            header_str.push_str(&format!("Content-Length: {}\r\n", body_bytes.len()));
        }

        // Write request
        let full_request = format!("{} {path} HTTP/1.1\r\n{header_str}\r\n", method.as_str());
        stream
            .write_all(full_request.as_bytes())
            .await
            .map_err(MystiProxyError::Io)?;
        if !body_bytes.is_empty() {
            stream
                .write_all(&body_bytes)
                .await
                .map_err(MystiProxyError::Io)?;
        }
        stream.flush().await.map_err(MystiProxyError::Io)?;

        // Read response until we have complete headers
        let mut buf = vec![0u8; 16384];
        let mut total = 0;
        let timeout = self.timeout;

        let read_fn = async {
            loop {
                if total >= buf.len() {
                    buf.resize(buf.len() * 2, 0);
                }
                let n = stream.read(&mut buf[total..]).await?;
                if n == 0 {
                    break;
                }
                total += n;
                if buf[..total].windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            Ok::<(), std::io::Error>(())
        };

        if let Some(t) = timeout {
            tokio::time::timeout(t, read_fn)
                .await
                .map_err(|_| MystiProxyError::Timeout)?
                .map_err(MystiProxyError::Io)?;
        } else {
            read_fn.await.map_err(MystiProxyError::Io)?;
        }

        // Parse response headers
        let header_end = buf[..total]
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or_else(|| MystiProxyError::Proxy("Incomplete response headers".to_string()))?;

        let headers_str = std::str::from_utf8(&buf[..header_end])
            .map_err(|_| MystiProxyError::Proxy("Invalid UTF-8 in response headers".to_string()))?;

        let mut lines = headers_str.lines();
        let status_line = lines
            .next()
            .ok_or_else(|| MystiProxyError::Proxy("Empty response".to_string()))?;
        let status_parts: Vec<&str> = status_line.split_whitespace().collect();
        let status_code: u16 = status_parts
            .get(1)
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| MystiProxyError::Proxy(format!("Invalid status: {status_line}")))?;

        let status = hyper::StatusCode::from_u16(status_code)
            .map_err(|e| MystiProxyError::Proxy(format!("Invalid status code: {e}")))?;

        let mut response_builder = Response::builder().status(status);
        for line in lines {
            if let Some(colon_pos) = line.find(':') {
                let name = line[..colon_pos].trim();
                let value = line[colon_pos + 1..].trim();
                if let (Ok(hn), Ok(hv)) = (
                    name.parse::<hyper::header::HeaderName>(),
                    value.parse::<hyper::header::HeaderValue>(),
                ) {
                    response_builder = response_builder.header(hn, hv);
                }
            }
        }

        // Extract body bytes (everything after headers, plus read remaining if needed)
        let body_start = header_end + 4;
        let mut body_data = Vec::new();
        if body_start < total {
            body_data.extend_from_slice(&buf[body_start..total]);
        }
        // Read remaining body until EOF or Content-Length satisfied
        loop {
            let n = stream.read(&mut buf).await.unwrap_or(0);
            if n == 0 {
                break;
            }
            body_data.extend_from_slice(&buf[..n]);
        }

        info!("Received response: {} from {}", status_code, self.target);

        response_builder
            .body(bytes::Bytes::from(body_data))
            .map_err(MystiProxyError::Http)
    }

    /// 发送请求并获取响应
    pub async fn send_request(&self, request: Request<Incoming>) -> Result<Response<Incoming>> {
        // 修改请求的 URI，使其指向目标服务器
        let uri = request.uri().clone();
        let path_and_query = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");

        // 构建新的 URI
        let new_uri = hyper::http::Uri::builder()
            .path_and_query(path_and_query)
            .build()
            .map_err(MystiProxyError::Http)?;

        // 创建新的请求
        let mut new_request = Request::builder()
            .method(request.method().clone())
            .uri(new_uri);

        let mut has_host_header = false;
        for (name, value) in request.headers() {
            if name == "host" {
                has_host_header = true;
            }
            new_request = new_request.header(name, value);
        }

        if !has_host_header {
            if let Some(host) = extract_host_from_target(&self.target) {
                new_request = new_request.header("Host", &host);
            }
        }

        let new_request = new_request
            .body(request.into_body())
            .map_err(MystiProxyError::Http)?;

        debug!(
            "Sending request to {}: {} {}",
            self.target,
            new_request.method(),
            new_request.uri()
        );

        // 建立连接并发送请求
        let mut sender = self.establish_connection().await?;

        // 应用超时
        let response = if let Some(timeout) = self.timeout {
            tokio::time::timeout(timeout, sender.send_request(new_request))
                .await
                .map_err(|_| MystiProxyError::Timeout)?
                .map_err(|e| MystiProxyError::Proxy(format!("Failed to send request: {e}")))?
        } else {
            sender
                .send_request(new_request)
                .await
                .map_err(|e| MystiProxyError::Proxy(format!("Failed to send request: {e}")))?
        };

        info!(
            "Received response: {} from {}",
            response.status(),
            self.target
        );
        Ok(response)
    }

    /// 获取目标地址
    pub fn target(&self) -> &str {
        &self.target
    }
}

/// HTTP 客户端池管理器
pub struct HttpClientPool {
    /// 客户端映射
    clients: Arc<Mutex<Vec<Arc<HttpClient>>>>,
}

impl HttpClientPool {
    /// 创建新的客户端池
    pub fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 获取或创建客户端
    pub async fn get_or_create(
        &self,
        target: String,
        timeout: Option<Duration>,
    ) -> Arc<HttpClient> {
        let mut clients = self.clients.lock().await;

        // 查找现有客户端
        for client in clients.iter() {
            if client.target() == target {
                return client.clone();
            }
        }

        // 创建新客户端
        let client = Arc::new(HttpClient::new(target.clone(), timeout));
        clients.push(client.clone());

        info!("Created new HTTP client for {}", target);
        client
    }

    /// 清理所有连接
    pub async fn clear(&self) {
        let mut clients = self.clients.lock().await;
        clients.clear();
        info!("Cleared all HTTP clients");
    }
}

impl Default for HttpClientPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_client_creation() {
        let client = HttpClient::new("tcp://127.0.0.1:8080".to_string(), None);
        assert_eq!(client.target(), "tcp://127.0.0.1:8080");
    }

    #[test]
    fn test_http_client_pool_creation() {
        let pool = HttpClientPool::new();
        assert!(true);
    }

    #[test]
    fn test_extract_host_from_target_tcp() {
        assert_eq!(
            extract_host_from_target("tcp://127.0.0.1:8080"),
            Some("127.0.0.1:8080".to_string())
        );
    }

    #[test]
    fn test_extract_host_from_target_unix() {
        assert_eq!(
            extract_host_from_target("unix:///var/run/docker.sock"),
            Some("localhost".to_string())
        );
    }

    #[test]
    fn test_extract_host_from_target_bare_host_port() {
        assert_eq!(
            extract_host_from_target("localhost:3000"),
            Some("localhost:3000".to_string())
        );
    }

    #[test]
    fn test_extract_host_from_target_empty() {
        assert_eq!(extract_host_from_target(""), None);
    }
}
