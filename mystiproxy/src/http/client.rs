//! HTTP 客户端模块
//!
//! 提供 HTTP 客户端功能，支持连接池和请求转发

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use http_body_util::combinators::BoxBody;
use hyper::body::Incoming;
use hyper::client::conn::http1::{Builder, SendRequest};
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

use crate::error::{MystiProxyError, Result};
use crate::http::upstream::{UpstreamProxyConfig, UpstreamProxyConnector};
use crate::io::SocketStream;

fn extract_host_from_target(target: &str) -> Option<String> {
    if target.starts_with("unix://") {
        return Some("localhost".to_string());
    }
    let addr = target.strip_prefix("tcp://").unwrap_or(target);
    if addr.is_empty() { return None; }
    Some(addr.to_string())
}

fn parse_tcp_target(target: &str) -> Option<(String, u16)> {
    let addr = target.strip_prefix("tcp://")?;
    let colon_pos = addr.rfind(':')?;
    let host = &addr[..colon_pos];
    let port: u16 = addr[colon_pos + 1..].parse().ok()?;
    Some((host.to_string(), port))
}

pub type RequestBoxBody = Request<BoxBody<Bytes, Infallible>>;

pub struct HttpClient {
    target: String,
    timeout: Option<Duration>,
    upstream_config: Option<UpstreamProxyConfig>,
}

impl HttpClient {
    pub fn new(target: String, timeout: Option<Duration>, upstream_config: Option<UpstreamProxyConfig>) -> Self {
        Self { target, timeout, upstream_config }
    }

    async fn establish_connection(&self) -> Result<SendRequest<BoxBody<Bytes, Infallible>>> {
        if let Some(ref upstream_cfg) = self.upstream_config {
            if let Some((host, port)) = parse_tcp_target(&self.target) {
                return self.establish_upstream(upstream_cfg, &host, port).await;
            }
        }
        self.establish_direct().await
    }

    async fn establish_direct(&self) -> Result<SendRequest<BoxBody<Bytes, Infallible>>> {
        let stream = SocketStream::connect(self.target.clone()).await?;
        let io = TokioIo::new(stream);

        let (sender, conn) = Builder::new()
            .preserve_header_case(true)
            .title_case_headers(true)
            .handshake(io)
            .await
            .map_err(|e| MystiProxyError::Proxy(format!("Failed to establish connection: {e}")))?;

        tokio::spawn(async move {
            if let Err(err) = conn.await {
                error!("Connection error: {:?}", err);
            }
        });

        debug!("Successfully connected to {}", self.target);
        Ok(sender)
    }

    async fn establish_upstream(
        &self,
        upstream_cfg: &UpstreamProxyConfig,
        host: &str,
        port: u16,
    ) -> Result<SendRequest<BoxBody<Bytes, Infallible>>> {
        let connector = UpstreamProxyConnector::new(upstream_cfg.clone());
        let stream = connector.connect_tunnel(host, port).await?;
        let io = TokioIo::new(stream);

        let (sender, conn) = Builder::new()
            .preserve_header_case(true)
            .title_case_headers(true)
            .handshake(io)
            .await
            .map_err(|e| MystiProxyError::Proxy(format!("Failed to establish upstream connection: {e}")))?;

        tokio::spawn(async move {
            if let Err(err) = conn.await {
                error!("Upstream connection error: {:?}", err);
            }
        });

        debug!("Successfully connected to {} via upstream proxy", self.target);
        Ok(sender)
    }

    pub async fn send_request(&self, request: Request<Incoming>) -> Result<Response<Incoming>> {
        let boxed = self.convert_incoming_request_async(request).await?;
        self.send_boxed(boxed).await
    }

    pub async fn send_boxed(&self, request: RequestBoxBody) -> Result<Response<Incoming>> {
        debug!("Sending request to {}: {} {}", self.target, request.method(), request.uri());

        let mut sender = self.establish_connection().await?;

        let response = if let Some(timeout) = self.timeout {
            tokio::time::timeout(timeout, sender.send_request(request))
                .await
                .map_err(|_| MystiProxyError::Timeout)?
                .map_err(|e| MystiProxyError::Proxy(format!("Failed to send request: {e}")))?
        } else {
            sender.send_request(request).await
                .map_err(|e| MystiProxyError::Proxy(format!("Failed to send request: {e}")))?
        };

        info!("Received response: {} from {}", response.status(), self.target);
        Ok(response)
    }

    fn rewrite_uri_and_headers(&self, method: hyper::http::Method, uri: &hyper::Uri, headers: &hyper::header::HeaderMap) -> Result<hyper::http::request::Builder> {
        let path_and_query = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
        let new_uri = hyper::http::Uri::builder()
            .path_and_query(path_and_query)
            .build()
            .map_err(MystiProxyError::Http)?;

        let mut builder = Request::builder().method(method).uri(new_uri);
        let mut has_host = false;
        for (name, value) in headers {
            if name == "host" { has_host = true; }
            builder = builder.header(name, value);
        }
        if !has_host {
            if let Some(host) = extract_host_from_target(&self.target) {
                builder = builder.header("Host", &host);
            }
        }
        Ok(builder)
    }

    pub async fn convert_incoming_request_async(&self, request: Request<Incoming>) -> Result<RequestBoxBody> {
        let (parts, body) = request.into_parts();
        let builder = self.rewrite_uri_and_headers(parts.method, &parts.uri, &parts.headers)?;
        let body_bytes = body.collect().await.map_err(|e| MystiProxyError::Hyper(e.to_string()))?.to_bytes();
        let boxed = Full::new(body_bytes).map_err(|never| match never {}).boxed();
        builder.body(boxed).map_err(MystiProxyError::Http)
    }

    pub fn build_boxed_request(
        &self,
        method: hyper::http::Method,
        uri: hyper::Uri,
        headers: hyper::header::HeaderMap,
        body_bytes: Bytes,
    ) -> Result<RequestBoxBody> {
        let builder = self.rewrite_uri_and_headers(method, &uri, &headers)?;
        let boxed = Full::new(body_bytes).map_err(|never| match never {}).boxed();
        builder.body(boxed).map_err(MystiProxyError::Http)
    }

    pub fn target(&self) -> &str {
        &self.target
    }
}

pub struct HttpClientPool {
    clients: Arc<Mutex<Vec<Arc<HttpClient>>>>,
}

impl HttpClientPool {
    pub fn new() -> Self {
        Self { clients: Arc::new(Mutex::new(Vec::new())) }
    }

    pub async fn get_or_create(&self, target: String, timeout: Option<Duration>) -> Arc<HttpClient> {
        let mut clients = self.clients.lock().await;
        for client in clients.iter() {
            if client.target() == target {
                return client.clone();
            }
        }
        let client = Arc::new(HttpClient::new(target.clone(), timeout, None));
        clients.push(client.clone());
        info!("Created new HTTP client for {}", target);
        client
    }

    pub async fn clear(&self) {
        let mut clients = self.clients.lock().await;
        clients.clear();
        info!("Cleared all HTTP clients");
    }
}

impl Default for HttpClientPool {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_client_creation() {
        let client = HttpClient::new("tcp://127.0.0.1:8080".to_string(), None, None);
        assert_eq!(client.target(), "tcp://127.0.0.1:8080");
    }

    #[test]
    fn test_extract_host_tcp() {
        assert_eq!(extract_host_from_target("tcp://127.0.0.1:8080"), Some("127.0.0.1:8080".to_string()));
    }

    #[test]
    fn test_extract_host_unix() {
        assert_eq!(extract_host_from_target("unix:///var/run/docker.sock"), Some("localhost".to_string()));
    }

    #[test]
    fn test_extract_host_empty() {
        assert_eq!(extract_host_from_target(""), None);
    }
}
