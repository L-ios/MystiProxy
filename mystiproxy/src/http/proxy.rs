//! HTTP Proxy 模块
//!
//! 提供完整的 HTTP(S) 代理功能，包括：
//! - HTTP 转发代理
//! - HTTPS CONNECT 隧道
//! - 代理认证（Basic Auth）

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use bytes::Bytes;
use http_body::Body as HttpBody;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::Service;
use hyper::{Method, Request, Response, StatusCode, Uri};
use hyper_util::rt::TokioIo;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, error, info, warn};

use crate::context::thread_identity;
use crate::error::{MystiProxyError, Result};

/// 带线程标识的日志宏
macro_rules! log_debug {
    ($($arg:tt)*) => {
        debug!("[{}] {}", thread_identity(), format!($($arg)*))
    };
}

macro_rules! log_info {
    ($($arg:tt)*) => {
        info!("[{}] {}", thread_identity(), format!($($arg)*))
    };
}

macro_rules! log_warn {
    ($($arg:tt)*) => {
        warn!("[{}] {}", thread_identity(), format!($($arg)*))
    };
}

macro_rules! log_error {
    ($($arg:tt)*) => {
        error!("[{}] {}", thread_identity(), format!($($arg)*))
    };
}

/// BoxBody 类型别名
pub type BoxBody = http_body_util::combinators::BoxBody<Bytes, MystiProxyError>;

/// 创建文本响应体
fn text_body(text: impl Into<String>) -> BoxBody {
    Full::new(Bytes::from(text.into()))
        .map_err(|never| match never {})
        .boxed()
}

/// 代理认证配置
#[derive(Debug, Clone)]
pub struct ProxyAuthConfig {
    /// 是否启用认证
    pub enabled: bool,
    /// 用户名密码映射
    pub users: HashMap<String, String>,
    /// 认证域
    pub realm: String,
}

impl Default for ProxyAuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            users: HashMap::new(),
            realm: "MystiProxy".to_string(),
        }
    }
}

impl ProxyAuthConfig {
    /// 创建新的认证配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加用户
    pub fn add_user(mut self, username: String, password: String) -> Self {
        let password_hash = Self::hash_password(&password);
        self.users.insert(username, password_hash);
        self
    }

    /// 启用认证
    pub fn enable(mut self) -> Self {
        self.enabled = true;
        self
    }

    /// 设置认证域
    pub fn realm(mut self, realm: impl Into<String>) -> Self {
        self.realm = realm.into();
        self
    }

    /// 哈希密码
    fn hash_password(password: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(password.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// 验证密码
    pub fn verify_password(&self, username: &str, password: &str) -> bool {
        if let Some(stored_hash) = self.users.get(username) {
            let password_hash = Self::hash_password(password);
            stored_hash == &password_hash
        } else {
            false
        }
    }

    /// 验证 Proxy-Authorization header
    pub fn authenticate(&self, headers: &hyper::header::HeaderMap) -> Option<String> {
        if !self.enabled {
            return Some("anonymous".to_string());
        }

        let auth_header = headers.get("Proxy-Authorization")?.to_str().ok()?;

        if !auth_header.starts_with("Basic ") {
            return None;
        }

        let encoded = &auth_header[6..];
        let decoded = BASE64.decode(encoded).ok()?;
        let credentials = String::from_utf8(decoded).ok()?;

        let parts: Vec<&str> = credentials.splitn(2, ':').collect();
        if parts.len() != 2 {
            return None;
        }

        let username = parts[0];
        let password = parts[1];

        if self.verify_password(username, password) {
            Some(username.to_string())
        } else {
            None
        }
    }

    /// 生成 407 Proxy Authentication Required 响应
    pub fn create_auth_required_response(&self) -> Response<BoxBody> {
        Response::builder()
            .status(StatusCode::PROXY_AUTHENTICATION_REQUIRED)
            .header(
                "Proxy-Authenticate",
                format!("Basic realm=\"{}\"", self.realm),
            )
            .header("Proxy-Connection", "close")
            .body(text_body("Proxy Authentication Required"))
            .unwrap()
    }
}

/// HTTP 代理配置
#[derive(Debug, Clone)]
pub struct HttpProxyConfig {
    /// 认证配置
    pub auth: ProxyAuthConfig,
    /// 连接超时
    pub connect_timeout: Duration,
    /// 请求超时
    pub request_timeout: Duration,
    /// 允许的目标主机（空表示允许所有）
    pub allowed_hosts: Vec<String>,
    /// 禁止的目标主机
    pub blocked_hosts: Vec<String>,
    /// 是否允许 CONNECT 方法
    pub allow_connect: bool,
    /// 上游代理（可选）
    pub upstream_proxy: Option<String>,
}

impl Default for HttpProxyConfig {
    fn default() -> Self {
        Self {
            auth: ProxyAuthConfig::default(),
            connect_timeout: Duration::from_secs(30),
            request_timeout: Duration::from_secs(60),
            allowed_hosts: vec![],
            blocked_hosts: vec![],
            allow_connect: true,
            upstream_proxy: None,
        }
    }
}

impl HttpProxyConfig {
    /// 创建新的代理配置
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置认证配置
    pub fn auth(mut self, auth: ProxyAuthConfig) -> Self {
        self.auth = auth;
        self
    }

    /// 设置连接超时
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// 设置请求超时
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// 添加允许的主机
    pub fn allow_host(mut self, host: impl Into<String>) -> Self {
        self.allowed_hosts.push(host.into());
        self
    }

    /// 添加禁止的主机
    pub fn block_host(mut self, host: impl Into<String>) -> Self {
        self.blocked_hosts.push(host.into());
        self
    }

    /// 设置是否允许 CONNECT
    pub fn allow_connect(mut self, allow: bool) -> Self {
        self.allow_connect = allow;
        self
    }

    /// 设置上游代理
    pub fn upstream_proxy(mut self, proxy: impl Into<String>) -> Self {
        self.upstream_proxy = Some(proxy.into());
        self
    }

    /// 检查主机是否允许
    pub fn is_host_allowed(&self, host: &str) -> bool {
        if !self.blocked_hosts.is_empty() && self.blocked_hosts.iter().any(|h| host.contains(h)) {
            return false;
        }
        if !self.allowed_hosts.is_empty() {
            return self.allowed_hosts.iter().any(|h| host.contains(h));
        }
        true
    }
}

/// HTTP 代理服务
#[derive(Clone)]
pub struct HttpProxyService {
    /// 配置
    config: Arc<HttpProxyConfig>,
}

impl HttpProxyService {
    /// 创建新的代理服务
    pub fn new(config: HttpProxyConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    /// 处理普通 HTTP 请求（支持任意 body 类型）
    async fn handle_http_request<B>(
        config: Arc<HttpProxyConfig>,
        req: Request<B>,
    ) -> Result<Response<BoxBody>>
    where
        B: HttpBody<Data = Bytes, Error: Into<Box<dyn std::error::Error + Send + Sync>>>
            + Send
            + 'static,
    {
        let uri = req.uri().clone();
        let method = req.method().clone();
        let host = uri.host().unwrap_or("unknown");
        let port = uri.port_u16().unwrap_or(80);

        log_debug!("Forwarding HTTP request: {} {}", method, uri);

        if !config.is_host_allowed(host) {
            log_warn!("Host {} is not allowed", host);
            return Ok(Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(text_body("Access to this host is forbidden"))
                .unwrap());
        }

        let target_addr = format!("{host}:{port}");

        let target_stream =
            tokio::time::timeout(config.connect_timeout, TcpStream::connect(&target_addr))
                .await
                .map_err(|_| MystiProxyError::Timeout)?
                .map_err(|e| {
                    MystiProxyError::Proxy(format!("Failed to connect to {target_addr}: {e}"))
                })?;

        let io = TokioIo::new(target_stream);

        let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
            .preserve_header_case(true)
            .title_case_headers(true)
            .handshake(io)
            .await
            .map_err(|e| MystiProxyError::Proxy(format!("Handshake failed: {e}")))?;

        tokio::spawn(async move {
            if let Err(err) = conn.await {
                log_error!("Connection error: {:?}", err);
            }
        });

        let path_and_query = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");

        let new_uri: Uri = path_and_query
            .parse()
            .map_err(|e: hyper::http::uri::InvalidUri| {
                MystiProxyError::Http(hyper::http::Error::from(e))
            })?;

        let mut new_request = Request::builder().method(method.clone()).uri(new_uri);

        for (name, value) in req.headers() {
            if name != "Proxy-Authorization" && name != "Proxy-Connection" {
                new_request = new_request.header(name, value);
            }
        }

        new_request = new_request.header("Connection", "close");

        let (parts, body) = req.into_parts();
        let new_request = new_request.body(body).map_err(MystiProxyError::Http)?;

        let response =
            tokio::time::timeout(config.request_timeout, sender.send_request(new_request))
                .await
                .map_err(|_| MystiProxyError::Timeout)?
                .map_err(|e| MystiProxyError::Proxy(format!("Request failed: {e}")))?;

        let (parts, body) = response.into_parts();
        let body_bytes = body
            .collect()
            .await
            .map_err(|e| MystiProxyError::Hyper(e.to_string()))?
            .to_bytes();

        let new_response = Response::from_parts(
            parts,
            Full::new(body_bytes)
                .map_err(|never| match never {})
                .boxed(),
        );

        log_debug!("HTTP request completed: {} {}", method, uri);
        Ok(new_response)
    }

    /// 创建错误响应
    fn create_error_response(status: StatusCode, message: &str) -> Response<BoxBody> {
        Response::builder()
            .status(status)
            .body(text_body(message))
            .unwrap()
    }
}

impl Service<Request<Incoming>> for HttpProxyService {
    type Response = Response<BoxBody>;
    type Error = MystiProxyError;
    type Future =
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let config = self.config.clone();

        Box::pin(async move {
            if let Some(user) = config.auth.authenticate(req.headers()) {
                log_debug!("Proxy authenticated as user: {}", user);
            } else {
                log_warn!("Proxy authentication failed");
                return Ok(config.auth.create_auth_required_response());
            }

            let method = req.method().clone();

            if method == Method::CONNECT {
                if !config.allow_connect {
                    return Ok(Self::create_error_response(
                        StatusCode::METHOD_NOT_ALLOWED,
                        "CONNECT method is not allowed",
                    ));
                }

                let uri = req.uri();
                let host = uri.host().unwrap_or("");
                if !config.is_host_allowed(host) {
                    return Ok(Self::create_error_response(
                        StatusCode::FORBIDDEN,
                        "Access to this host is forbidden",
                    ));
                }

                return Ok(Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(text_body("CONNECT requires direct TCP connection. Use HttpProxyAcceptor for HTTPS proxy."))
                    .unwrap());
            }

            if req.uri().host().is_none() {
                return Ok(Self::create_error_response(
                    StatusCode::BAD_REQUEST,
                    "Missing host in request URI",
                ));
            }

            Self::handle_http_request(config, req).await
        })
    }
}

/// HTTP 代理接受器（支持 CONNECT 隧道）
#[derive(Clone)]
pub struct HttpProxyAcceptor {
    config: Arc<HttpProxyConfig>,
}

impl HttpProxyAcceptor {
    /// 创建新的代理接受器
    pub fn new(config: HttpProxyConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    /// 处理客户端连接（支持 CONNECT 隧道和普通 HTTP 代理请求）
    pub async fn handle_connection(&self, mut client_stream: tokio::net::TcpStream) -> Result<()> {
        let mut buf = vec![0u8; 8192];
        let n = tokio::time::timeout(self.config.request_timeout, client_stream.read(&mut buf))
            .await
            .map_err(|_| MystiProxyError::Timeout)?
            .map_err(MystiProxyError::Io)?;

        let request_str = String::from_utf8_lossy(&buf[..n]);
        let lines: Vec<&str> = request_str.lines().collect();

        if lines.is_empty() {
            return Err(MystiProxyError::Proxy("Empty request".to_string()));
        }

        let request_line = lines[0];
        let parts: Vec<&str> = request_line.split_whitespace().collect();

        if parts.len() < 3 {
            return Err(MystiProxyError::Proxy("Invalid request line".to_string()));
        }

        let method = parts[0];
        let target = parts[1];
        let _version = parts[2]; // HTTP version

        let mut headers = HashMap::new();
        let mut header_end = 0;
        for (i, line) in lines.iter().enumerate().skip(1) {
            if line.is_empty() {
                header_end = i + 1;
                break;
            }
            if let Some((key, value)) = line.split_once(':') {
                headers.insert(key.trim().to_lowercase(), value.trim().to_string());
            }
        }

        // 认证
        if let Some(user) = self.authenticate(&headers) {
            log_debug!("Proxy authenticated as user: {}", user);
        } else {
            let response = "HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"MystiProxy\"\r\n\r\n";
            client_stream
                .write_all(response.as_bytes())
                .await
                .map_err(MystiProxyError::Io)?;
            return Ok(());
        }

        // CONNECT 方法：建立 TLS 隧道
        if method == "CONNECT" {
            return self.handle_connect(target, client_stream).await;
        }

        // 非 CONNECT 方法：检查是否为绝对 URI（HTTP 代理标准形态）
        // 形如 GET http://host:port/path HTTP/1.1
        let target_uri = match Uri::try_from(target) {
            Ok(uri) if uri.scheme().is_some() && uri.host().is_some() => {
                // 这是绝对 URI 请求，转发给目标
                uri
            }
            _ => {
                // 相对路径请求（如 GET /path）在正向代理语义下无 Host 可寻
                // 返回 400 Bad Request，这符合 RFC 7230
                let response = "HTTP/1.1 400 Bad Request\r\n\r\nInvalid request target for forward proxy: absolute URI required";
                client_stream
                    .write_all(response.as_bytes())
                    .await
                    .map_err(MystiProxyError::Io)?;
                return Ok(());
            }
        };

        // 解析完整的 HTTP 请求头（用于重建请求）
        let mut header_bytes = Vec::new();
        for line in &lines[1..header_end] {
            header_bytes.extend_from_slice(line.as_bytes());
            header_bytes.extend_from_slice(b"\r\n");
        }
        header_bytes.extend_from_slice(b"\r\n");

        // 读取请求体（如果有）
        let body_start = header_bytes.len() + request_line.len() + 2; // +2 for \r\n
        let body = if body_start < buf[..n].len() {
            buf[body_start..n].to_vec()
        } else {
            Vec::new()
        };

        // 重建完整请求用于转发
        let request_uri = target_uri.clone();
        let mut request_builder = Request::builder().method(method).uri(request_uri);

        // 复制头部（跳过 Proxy-Authorization 和 Proxy-Connection）
        for (name, value) in &headers {
            if *name != "proxy-authorization" && *name != "proxy-connection" {
                request_builder = request_builder.header(name, value);
            }
        }

        // 添加 Host 头（如果原请求没有）
        if !headers.contains_key("host") {
            if let (Some(host), Some(port)) = (target_uri.host(), target_uri.port_u16()) {
                let host_header = if port == 80 || port == 443 {
                    host.to_string()
                } else {
                    format!("{}:{}", host, port)
                };
                request_builder = request_builder.header("Host", host_header);
            }
        }

        request_builder = request_builder.header("Connection", "close");

        let request = request_builder
            .body(http_body_util::Full::new(Bytes::from(body)))
            .map_err(MystiProxyError::Http)?;

        // 使用现有的 HttpProxyService::handle_http_request 处理转发
        let config = self.config.clone();
        let response = HttpProxyService::handle_http_request(config, request).await?;

        // 将响应写回客户端
        let (parts, body) = response.into_parts();
        let mut response_bytes = Vec::new();

        // 写状态行
        let status_line = format!(
            "HTTP/1.1 {} {}\r\n",
            parts.status.as_u16(),
            parts.status.canonical_reason().unwrap_or("")
        );
        response_bytes.extend_from_slice(status_line.as_bytes());

        // 写头部
        for (name, value) in parts.headers.iter() {
            response_bytes.extend_from_slice(name.as_str().as_bytes());
            response_bytes.extend_from_slice(b": ");
            response_bytes.extend_from_slice(value.as_bytes());
            response_bytes.extend_from_slice(b"\r\n");
        }
        response_bytes.extend_from_slice(b"\r\n");

        // 写 body
        let body_bytes = body
            .collect()
            .await
            .map_err(|e| MystiProxyError::Hyper(e.to_string()))?
            .to_bytes();
        response_bytes.extend_from_slice(&body_bytes);

        client_stream
            .write_all(&response_bytes)
            .await
            .map_err(MystiProxyError::Io)?;

        Ok(())
    }

    /// 认证
    fn authenticate(&self, headers: &HashMap<String, String>) -> Option<String> {
        if !self.config.auth.enabled {
            return Some("anonymous".to_string());
        }

        let auth_header = headers.get("proxy-authorization")?;
        if !auth_header.starts_with("Basic ") {
            return None;
        }

        let encoded = &auth_header[6..];
        let decoded = BASE64.decode(encoded).ok()?;
        let credentials = String::from_utf8(decoded).ok()?;

        let parts: Vec<&str> = credentials.splitn(2, ':').collect();
        if parts.len() != 2 {
            return None;
        }

        let username = parts[0];
        let password = parts[1];

        if self.config.auth.verify_password(username, password) {
            Some(username.to_string())
        } else {
            None
        }
    }

    /// 处理 CONNECT 请求（HTTPS 隧道）
    async fn handle_connect(
        &self,
        target_host: &str,
        mut client_stream: tokio::net::TcpStream,
    ) -> Result<()> {
        log_debug!("Establishing CONNECT tunnel to {}", target_host);

        let target_addr = if target_host.contains(':') {
            target_host.to_string()
        } else {
            format!("{target_host}:443")
        };

        let mut target_stream = tokio::time::timeout(
            self.config.connect_timeout,
            TcpStream::connect(&target_addr),
        )
        .await
        .map_err(|_| MystiProxyError::Timeout)?
        .map_err(|e| MystiProxyError::Proxy(format!("Failed to connect to {target_addr}: {e}")))?;

        log_debug!("Connected to {}", target_addr);

        let success_response = "HTTP/1.1 200 Connection Established\r\n\r\n";
        client_stream
            .write_all(success_response.as_bytes())
            .await
            .map_err(MystiProxyError::Io)?;

        log_info!("CONNECT tunnel established: {}", target_host);

        let (mut client_read, mut client_write) = client_stream.split();
        let (mut target_read, mut target_write) = target_stream.split();

        let client_to_target = async {
            let mut buf = vec![0u8; 8192];
            loop {
                match client_read.read(&mut buf).await {
                    Ok(0) => break Ok::<(), std::io::Error>(()),
                    Ok(n) => {
                        if target_write.write_all(&buf[..n]).await.is_err() {
                            break Ok(());
                        }
                    }
                    Err(e) => break Err(e),
                }
            }
        };

        let target_to_client = async {
            let mut buf = vec![0u8; 8192];
            loop {
                match target_read.read(&mut buf).await {
                    Ok(0) => break Ok::<(), std::io::Error>(()),
                    Ok(n) => {
                        if client_write.write_all(&buf[..n]).await.is_err() {
                            break Ok(());
                        }
                    }
                    Err(e) => break Err(e),
                }
            }
        };

        let _ = tokio::try_join!(client_to_target, target_to_client);

        log_debug!("CONNECT tunnel closed: {}", target_host);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_auth_config() {
        let config = ProxyAuthConfig::new()
            .add_user("admin".to_string(), "password".to_string())
            .enable();

        assert!(config.enabled);
        assert!(config.users.contains_key("admin"));
    }

    #[test]
    fn test_proxy_auth_verify() {
        let config = ProxyAuthConfig::new()
            .add_user("admin".to_string(), "secret".to_string())
            .enable();

        assert!(config.verify_password("admin", "secret"));
        assert!(!config.verify_password("admin", "wrong"));
        assert!(!config.verify_password("unknown", "secret"));
    }

    #[test]
    fn test_proxy_config_host_filter() {
        let config = HttpProxyConfig::new()
            .allow_host("example.com")
            .block_host("blocked.com");

        assert!(config.is_host_allowed("example.com"));
        assert!(config.is_host_allowed("api.example.com"));
        assert!(!config.is_host_allowed("blocked.com"));
        assert!(!config.is_host_allowed("sub.blocked.com"));
    }

    #[test]
    fn test_authenticate_header() {
        let config = ProxyAuthConfig::new()
            .add_user("test".to_string(), "pass".to_string())
            .enable();

        let mut headers = hyper::header::HeaderMap::new();
        let credentials = BASE64.encode("test:pass");
        headers.insert(
            "Proxy-Authorization",
            format!("Basic {credentials}").parse().unwrap(),
        );

        let result = config.authenticate(&headers);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "test");
    }

    #[test]
    fn test_authenticate_invalid() {
        let config = ProxyAuthConfig::new()
            .add_user("test".to_string(), "pass".to_string())
            .enable();

        let mut headers = hyper::header::HeaderMap::new();
        let credentials = BASE64.encode("test:wrong");
        headers.insert(
            "Proxy-Authorization",
            format!("Basic {credentials}").parse().unwrap(),
        );

        let result = config.authenticate(&headers);
        assert!(result.is_none());
    }

    #[test]
    fn test_authenticate_disabled() {
        let config = ProxyAuthConfig::new();

        let headers = hyper::header::HeaderMap::new();
        let result = config.authenticate(&headers);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "anonymous");
    }
}
