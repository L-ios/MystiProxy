//! HTTP 服务器模块
//!
//! 提供 HTTP 服务器功能，支持 TCP 和 UDS 监听

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::Service;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tracing::{error, info, warn};

use crate::error::{MystiProxyError, Result};
use crate::io::{SocketStream, StreamListener};

/// BoxBody 类型别名
pub type BoxBody = http_body_util::combinators::BoxBody<Bytes, Infallible>;

/// HTTP 服务器配置
#[derive(Debug, Clone)]
pub struct HttpServerConfig {
    /// 监听地址
    pub listen: String,
    /// 超时时间
    pub timeout: Option<Duration>,
}

impl HttpServerConfig {
    /// 创建新的服务器配置
    pub fn new(listen: String, timeout: Option<Duration>) -> Self {
        Self { listen, timeout }
    }
}

/// HTTP 服务器
pub struct HttpServer<S>
where
    S: Service<Request<Incoming>, Response = Response<BoxBody>> + Clone + Send + 'static,
    S::Future: Send,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    /// 配置
    config: HttpServerConfig,
    /// 服务处理器
    service: S,
    /// 监听器
    listener: Option<StreamListener>,
}

impl<S> HttpServer<S>
where
    S: Service<Request<Incoming>, Response = Response<BoxBody>> + Clone + Send + 'static,
    S::Future: Send,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    /// 创建新的 HTTP 服务器
    pub fn new(config: HttpServerConfig, service: S) -> Self {
        Self {
            config,
            service,
            listener: None,
        }
    }

    /// 启动服务器
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting HTTP server on {}", self.config.listen);

        // 创建监听器
        let listener = StreamListener::new(self.config.listen.clone()).await?;
        self.listener = Some(listener);

        info!("HTTP server started successfully");
        Ok(())
    }

    /// 运行服务器主循环
    pub async fn run(&self) -> Result<()> {
        let listener = self.listener.as_ref().ok_or_else(|| {
            MystiProxyError::Other("Server not started. Call start() first.".to_string())
        })?;

        info!("HTTP server is running, waiting for connections...");

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    info!("Accepted HTTP connection from {}", addr);

                    let service = self.service.clone();
                    let timeout = self.config.timeout;

                    // 为每个连接创建新任务
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, service, timeout).await {
                            error!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    /// 处理单个连接
    async fn handle_connection(
        stream: SocketStream,
        service: S,
        timeout: Option<Duration>,
    ) -> Result<()> {
        let io = TokioIo::new(stream);

        // 创建 HTTP/1.1 服务
        let conn = http1::Builder::new()
            .preserve_header_case(true)
            .title_case_headers(true)
            .serve_connection(io, service);

        // 应用超时
        if let Some(duration) = timeout {
            match tokio::time::timeout(duration, conn).await {
                Ok(result) => {
                    result.map_err(|e| {
                        MystiProxyError::Proxy(format!("Connection error: {}", e))
                    })?;
                }
                Err(_) => {
                    warn!("Connection timed out after {:?}", duration);
                    return Err(MystiProxyError::Timeout);
                }
            }
        } else {
            conn.await.map_err(|e| {
                MystiProxyError::Proxy(format!("Connection error: {}", e))
            })?;
        }

        Ok(())
    }

    /// 获取监听地址
    pub fn listen_addr(&self) -> &str {
        &self.config.listen
    }
}

/// 简单的 HTTP 代理服务
#[derive(Clone)]
pub struct HttpProxyService {
    /// 目标地址
    target: String,
    /// 超时时间
    timeout: Option<Duration>,
}

impl HttpProxyService {
    /// 创建新的代理服务
    pub fn new(target: String, timeout: Option<Duration>) -> Self {
        Self { target, timeout }
    }

    /// 创建完整响应体
    fn full_body(bytes: Bytes) -> BoxBody {
        Full::new(bytes)
            .map_err(|never| match never {})
            .boxed()
    }
}

impl Service<Request<Incoming>> for HttpProxyService {
    type Response = Response<BoxBody>;
    type Error = MystiProxyError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let target = self.target.clone();
        let timeout = self.timeout;

        Box::pin(async move {
            // 建立到目标的连接
            let stream = SocketStream::connect(target.clone()).await?;
            let io = TokioIo::new(stream);

            // 创建客户端连接
            let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
                .preserve_header_case(true)
                .title_case_headers(true)
                .handshake(io)
                .await
                .map_err(|e| MystiProxyError::Proxy(format!("Handshake failed: {}", e)))?;

            // 在后台维护连接
            tokio::spawn(async move {
                if let Err(err) = conn.await {
                    error!("Connection error: {:?}", err);
                }
            });

            // 修改请求 URI
            let uri = req.uri().clone();
            let path_and_query = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");

            let new_uri = hyper::http::Uri::builder()
                .path_and_query(path_and_query)
                .build()
                .map_err(|e| MystiProxyError::Http(e))?;

            // 构建新请求
            let mut new_request = Request::builder()
                .method(req.method().clone())
                .uri(new_uri);

            // 复制请求头
            for (name, value) in req.headers() {
                new_request = new_request.header(name, value);
            }

            let new_request = new_request
                .body(req.into_body())
                .map_err(|e| MystiProxyError::Http(e))?;

            // 发送请求
            let response = if let Some(duration) = timeout {
                tokio::time::timeout(duration, sender.send_request(new_request))
                    .await
                    .map_err(|_| MystiProxyError::Timeout)?
                    .map_err(|e| MystiProxyError::Proxy(format!("Request failed: {}", e)))?
            } else {
                sender
                    .send_request(new_request)
                    .await
                    .map_err(|e| MystiProxyError::Proxy(format!("Request failed: {}", e)))?
            };

            // 转换响应体
            let (parts, body) = response.into_parts();
            let body_bytes = body
                .collect()
                .await
                .map_err(|e| MystiProxyError::Hyper(e.to_string()))?
                .to_bytes();

            let new_response = Response::from_parts(parts, Self::full_body(body_bytes));

            Ok(new_response)
        })
    }
}

/// 创建简单的 HTTP 服务器
pub async fn create_simple_server(
    listen: String,
    target: String,
    timeout: Option<Duration>,
) -> Result<HttpServer<HttpProxyService>> {
    let config = HttpServerConfig::new(listen, timeout);
    let service = HttpProxyService::new(target, timeout);
    Ok(HttpServer::new(config, service))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_creation() {
        let config = HttpServerConfig::new("tcp://0.0.0.0:8080".to_string(), None);
        assert_eq!(config.listen, "tcp://0.0.0.0:8080");
    }

    #[test]
    fn test_proxy_service_creation() {
        let service = HttpProxyService::new("tcp://127.0.0.1:9000".to_string(), None);
        assert_eq!(service.target, "tcp://127.0.0.1:9000");
    }
}
