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
                MystiProxyError::Proxy(format!("Failed to establish connection: {}", e))
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

    /// 发送请求并获取响应
    pub async fn send_request(&self, request: Request<Incoming>) -> Result<Response<Incoming>> {
        // 修改请求的 URI，使其指向目标服务器
        let uri = request.uri().clone();
        let path_and_query = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");

        // 构建新的 URI
        let new_uri = hyper::http::Uri::builder()
            .path_and_query(path_and_query)
            .build()
            .map_err(|e| MystiProxyError::Http(e))?;

        // 创建新的请求
        let mut new_request = Request::builder()
            .method(request.method().clone())
            .uri(new_uri);

        // 复制所有请求头
        for (name, value) in request.headers() {
            new_request = new_request.header(name, value);
        }

        let new_request = new_request
            .body(request.into_body())
            .map_err(|e| MystiProxyError::Http(e))?;

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
                .map_err(|e| MystiProxyError::Proxy(format!("Failed to send request: {}", e)))?
        } else {
            sender
                .send_request(new_request)
                .await
                .map_err(|e| MystiProxyError::Proxy(format!("Failed to send request: {}", e)))?
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
}
