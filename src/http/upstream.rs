//! 上游代理模块
//!
//! 提供上游代理连接功能，支持：
//! - HTTP 代理转发
//! - HTTPS 代理隧道
//! - 认证代理转换（类似 cntlm）
//! - HTTPS 转 HTTP（类似 stunnel）

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
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

/// 上游代理协议类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpstreamProtocol {
    /// HTTP 代理
    Http,
    /// HTTPS 代理（TLS 连接到代理）
    Https,
    /// SOCKS5 代理
    Socks5,
}

/// 上游代理认证配置
#[derive(Debug, Clone)]
pub struct UpstreamAuth {
    /// 用户名
    pub username: String,
    /// 密码
    pub password: String,
}

impl UpstreamAuth {
    /// 创建新的认证配置
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            password: password.into(),
        }
    }

    /// 生成 Basic Auth 头
    pub fn to_proxy_authorization(&self) -> String {
        let credentials = format!("{}:{}", self.username, self.password);
        format!("Basic {}", BASE64.encode(credentials))
    }
}

/// 上游代理配置
#[derive(Debug, Clone)]
pub struct UpstreamProxyConfig {
    /// 代理地址 (host:port)
    pub host: String,
    /// 代理端口
    pub port: u16,
    /// 协议类型
    pub protocol: UpstreamProtocol,
    /// 认证信息（可选）
    pub auth: Option<UpstreamAuth>,
    /// 连接超时
    pub connect_timeout: Duration,
    /// 是否验证上游 TLS 证书
    pub tls_verify: bool,
    /// 上游 TLS CA 证书路径
    pub tls_ca_cert: Option<String>,
    /// 上游 TLS 客户端证书路径（mTLS）
    pub tls_client_cert: Option<String>,
    /// 上游 TLS 客户端私钥路径（mTLS）
    pub tls_client_key: Option<String>,
}

impl UpstreamProxyConfig {
    /// 从 URL 解析上游代理配置
    ///
    /// 支持格式:
    /// - http://proxy.example.com:8080
    /// - https://proxy.example.com:8080
    /// - socks5://proxy.example.com:1080
    /// - http://user:pass@proxy.example.com:8080
    pub fn from_url(url: &str) -> Result<Self> {
        let url = url.trim();

        // 解析协议
        let (protocol, rest) = if url.starts_with("https://") {
            (UpstreamProtocol::Https, &url[8..])
        } else if url.starts_with("http://") {
            (UpstreamProtocol::Http, &url[7..])
        } else if url.starts_with("socks5://") {
            (UpstreamProtocol::Socks5, &url[9..])
        } else {
            (UpstreamProtocol::Http, url)
        };

        // 解析认证信息
        let (auth, host_port) = if let Some(at_pos) = rest.find('@') {
            let auth_part = &rest[..at_pos];
            let host_part = &rest[at_pos + 1..];

            if let Some(colon_pos) = auth_part.find(':') {
                let username = &auth_part[..colon_pos];
                let password = &auth_part[colon_pos + 1..];
                (Some(UpstreamAuth::new(username, password)), host_part)
            } else {
                (None, host_part)
            }
        } else {
            (None, rest)
        };

        // 解析主机和端口
        let (host, port) = if let Some(colon_pos) = host_port.rfind(':') {
            let host = &host_port[..colon_pos];
            let port: u16 = host_port[colon_pos + 1..]
                .parse()
                .map_err(|e| MystiProxyError::Config(format!("Invalid port: {}", e)))?;
            (host.to_string(), port)
        } else {
            return Err(MystiProxyError::Config("Missing port in proxy URL".to_string()));
        };

        Ok(Self {
            host,
            port,
            protocol,
            auth,
            connect_timeout: Duration::from_secs(30),
            tls_verify: true,
            tls_ca_cert: None,
            tls_client_cert: None,
            tls_client_key: None,
        })
    }

    /// 创建 HTTP 上游代理
    pub fn http(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            protocol: UpstreamProtocol::Http,
            auth: None,
            connect_timeout: Duration::from_secs(30),
            tls_verify: true,
            tls_ca_cert: None,
            tls_client_cert: None,
            tls_client_key: None,
        }
    }

    /// 创建 HTTPS 上游代理
    pub fn https(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            protocol: UpstreamProtocol::Https,
            auth: None,
            connect_timeout: Duration::from_secs(30),
            tls_verify: true,
            tls_ca_cert: None,
            tls_client_cert: None,
            tls_client_key: None,
        }
    }

    /// 设置认证
    pub fn auth(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.auth = Some(UpstreamAuth::new(username, password));
        self
    }

    /// 设置连接超时
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// 设置 TLS 验证
    pub fn tls_verify(mut self, verify: bool) -> Self {
        self.tls_verify = verify;
        self
    }

    /// 获取代理地址
    pub fn proxy_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// 上游代理连接器
pub struct UpstreamProxyConnector {
    config: Arc<UpstreamProxyConfig>,
}

impl UpstreamProxyConnector {
    /// 创建新的上游代理连接器
    pub fn new(config: UpstreamProxyConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    /// 连接到上游代理
    pub async fn connect(&self) -> Result<TcpStream> {
        let addr = self.config.proxy_addr();
        log_debug!("Connecting to upstream proxy: {}", addr);

        let stream = tokio::time::timeout(
            self.config.connect_timeout,
            TcpStream::connect(&addr),
        )
        .await
        .map_err(|_| MystiProxyError::Timeout)?
        .map_err(|e| MystiProxyError::Proxy(format!("Failed to connect to upstream {}: {}", addr, e)))?;

        log_debug!("Connected to upstream proxy: {}", addr);
        Ok(stream)
    }

    /// 通过上游代理建立 CONNECT 隧道
    ///
    /// 用于 HTTPS 请求，将 HTTPS 代理转换为透明隧道
    pub async fn connect_tunnel(&self, target_host: &str, target_port: u16) -> Result<TcpStream> {
        let stream = self.connect().await?;

        // 如果是 HTTPS 代理，需要先建立 TLS 连接
        let mut stream = if self.config.protocol == UpstreamProtocol::Https {
            self.wrap_tls(stream).await?
        } else {
            stream
        };

        // 发送 CONNECT 请求
        let connect_request = format!(
            "CONNECT {}:{} HTTP/1.1\r\nHost: {}:{}\r\n",
            target_host, target_port, target_host, target_port
        );

        let connect_request = match &self.config.auth {
            Some(auth) => format!(
                "{}Proxy-Authorization: {}\r\n\r\n",
                connect_request,
                auth.to_proxy_authorization()
            ),
            None => format!("{}\r\n", connect_request),
        };

        log_debug!("Sending CONNECT request to upstream: {}:{} via {}",
            target_host, target_port, self.config.proxy_addr());

        stream
            .write_all(connect_request.as_bytes())
            .await
            .map_err(MystiProxyError::Io)?;

        // 读取响应
        let mut response_buf = vec![0u8; 4096];
        let n = stream
            .read(&mut response_buf)
            .await
            .map_err(MystiProxyError::Io)?;

        let response = String::from_utf8_lossy(&response_buf[..n]);
        let first_line = response.lines().next().unwrap_or("");

        if first_line.contains("200") {
            log_info!("CONNECT tunnel established: {}:{} via upstream {}",
                target_host, target_port, self.config.proxy_addr());
            Ok(stream)
        } else {
            log_error!("CONNECT failed: {}", first_line);
            Err(MystiProxyError::Proxy(format!(
                "Upstream proxy CONNECT failed: {}",
                first_line
            )))
        }
    }

    /// 通过上游代理转发 HTTP 请求
    ///
    /// 将请求发送到上游代理，并返回响应
    pub async fn forward_http_request(
        &self,
        method: &str,
        target_host: &str,
        target_port: u16,
        path: &str,
        headers: &HashMap<String, String>,
        body: Option<&[u8]>,
    ) -> Result<Vec<u8>> {
        let stream = self.connect().await?;

        // 如果是 HTTPS 代理，需要先建立 TLS 连接
        let mut stream = if self.config.protocol == UpstreamProtocol::Https {
            self.wrap_tls(stream).await?
        } else {
            stream
        };

        // 构建请求
        let mut request = format!("{} http://{}:{}{} HTTP/1.1\r\n", method, target_host, target_port, path);

        // 添加主机头
        request.push_str(&format!("Host: {}:{}\r\n", target_host, target_port));

        // 添加其他头
        for (key, value) in headers {
            if key.to_lowercase() != "host" && key.to_lowercase() != "proxy-authorization" {
                request.push_str(&format!("{}: {}\r\n", key, value));
            }
        }

        // 添加上游代理认证
        if let Some(auth) = &self.config.auth {
            request.push_str(&format!("Proxy-Authorization: {}\r\n", auth.to_proxy_authorization()));
        }

        // 添加连接头
        request.push_str("Connection: close\r\n");

        // 添加内容长度
        if let Some(body) = body {
            request.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }

        request.push_str("\r\n");

        log_debug!("Forwarding HTTP request to upstream: {} {} via {}",
            method, path, self.config.proxy_addr());

        // 发送请求
        stream
            .write_all(request.as_bytes())
            .await
            .map_err(MystiProxyError::Io)?;

        if let Some(body) = body {
            stream
                .write_all(body)
                .await
                .map_err(MystiProxyError::Io)?;
        }

        // 读取响应
        let mut response = Vec::new();
        let mut buf = vec![0u8; 8192];
        loop {
            let n = stream
                .read(&mut buf)
                .await
                .map_err(MystiProxyError::Io)?;
            if n == 0 {
                break;
            }
            response.extend_from_slice(&buf[..n]);
        }

        log_debug!("HTTP response received: {} bytes", response.len());
        Ok(response)
    }

    /// 包装 TLS 连接（用于 HTTPS 上游代理）
    async fn wrap_tls(&self, _stream: TcpStream) -> Result<TcpStream> {
        // 简化实现：如果需要 TLS，使用 native-tls 或 rustls
        // 这里暂时返回错误，提示需要 TLS 配置
        log_warn!("HTTPS upstream proxy requires TLS configuration");
        Err(MystiProxyError::Proxy(
            "HTTPS upstream proxy not yet implemented".to_string(),
        ))
    }

    /// 获取配置
    pub fn config(&self) -> &UpstreamProxyConfig {
        &self.config
    }
}

/// 代理转换器
///
/// 将需要认证的上游代理转换为本地无需认证的代理
/// 类似 cntlm 的功能
pub struct ProxyConverter {
    /// 上游代理连接器
    upstream: UpstreamProxyConnector,
    /// 本地监听端口
    local_port: u16,
}

impl ProxyConverter {
    /// 创建新的代理转换器
    pub fn new(upstream_config: UpstreamProxyConfig, local_port: u16) -> Self {
        Self {
            upstream: UpstreamProxyConnector::new(upstream_config),
            local_port,
        }
    }

    /// 获取本地端口
    pub fn local_port(&self) -> u16 {
        self.local_port
    }

    /// 处理客户端连接
    pub async fn handle_client(&self, mut client_stream: TcpStream) -> Result<()> {
        let mut buf = vec![0u8; 8192];
        let n = client_stream
            .read(&mut buf)
            .await
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

        // 解析请求头
        let mut headers = HashMap::new();
        for line in &lines[1..] {
            if line.is_empty() {
                break;
            }
            if let Some((key, value)) = line.split_once(':') {
                headers.insert(key.trim().to_lowercase(), value.trim().to_string());
            }
        }

        log_debug!("Proxy converter handling: {} {}", method, target);

        if method == "CONNECT" {
            // 处理 CONNECT 请求
            let (host, port) = self.parse_connect_target(target)?;
            let upstream_stream = self.upstream.connect_tunnel(&host, port).await?;

            // 发送成功响应给客户端
            let response = "HTTP/1.1 200 Connection Established\r\n\r\n";
            client_stream
                .write_all(response.as_bytes())
                .await
                .map_err(MystiProxyError::Io)?;

            // 双向转发
            self.bidirectional_forward(client_stream, upstream_stream).await?;
        } else {
            // 处理普通 HTTP 请求
            let (host, port, path) = self.parse_http_target(target)?;
            let response = self.upstream
                .forward_http_request(method, &host, port, &path, &headers, None)
                .await?;

            client_stream
                .write_all(&response)
                .await
                .map_err(MystiProxyError::Io)?;
        }

        Ok(())
    }

    /// 解析 CONNECT 目标
    fn parse_connect_target(&self, target: &str) -> Result<(String, u16)> {
        if let Some(colon_pos) = target.rfind(':') {
            let host = &target[..colon_pos];
            let port: u16 = target[colon_pos + 1..]
                .parse()
                .map_err(|e| MystiProxyError::Proxy(format!("Invalid port: {}", e)))?;
            Ok((host.to_string(), port))
        } else {
            Ok((target.to_string(), 443))
        }
    }

    /// 解析 HTTP 目标
    fn parse_http_target(&self, target: &str) -> Result<(String, u16, String)> {
        // 解析 URL
        let url: hyper::Uri = target
            .parse()
            .map_err(|e: hyper::http::uri::InvalidUri| {
                MystiProxyError::Http(hyper::http::Error::from(e))
            })?;

        let host = url.host().unwrap_or("localhost").to_string();
        let port = url.port_u16().unwrap_or(80);
        let path = url.path_and_query().map(|pq| pq.as_str()).unwrap_or("/").to_string();

        Ok((host, port, path))
    }

    /// 双向转发
    async fn bidirectional_forward(
        &self,
        client_stream: TcpStream,
        upstream_stream: TcpStream,
    ) -> Result<()> {
        let (mut client_read, mut client_write) = client_stream.into_split();
        let (mut upstream_read, mut upstream_write) = upstream_stream.into_split();

        let client_to_upstream = async {
            let mut buf = vec![0u8; 8192];
            loop {
                match client_read.read(&mut buf).await {
                    Ok(0) => break Ok::<(), std::io::Error>(()),
                    Ok(n) => {
                        if upstream_write.write_all(&buf[..n]).await.is_err() {
                            break Ok(());
                        }
                    }
                    Err(e) => break Err(e),
                }
            }
        };

        let upstream_to_client = async {
            let mut buf = vec![0u8; 8192];
            loop {
                match upstream_read.read(&mut buf).await {
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

        let _ = tokio::try_join!(client_to_upstream, upstream_to_client);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upstream_config_from_url() {
        let config = UpstreamProxyConfig::from_url("http://proxy.example.com:8080").unwrap();
        assert_eq!(config.host, "proxy.example.com");
        assert_eq!(config.port, 8080);
        assert_eq!(config.protocol, UpstreamProtocol::Http);
        assert!(config.auth.is_none());
    }

    #[test]
    fn test_upstream_config_with_auth() {
        let config = UpstreamProxyConfig::from_url("http://user:pass@proxy.example.com:8080").unwrap();
        assert_eq!(config.host, "proxy.example.com");
        assert_eq!(config.port, 8080);
        assert!(config.auth.is_some());
        let auth = config.auth.unwrap();
        assert_eq!(auth.username, "user");
        assert_eq!(auth.password, "pass");
    }

    #[test]
    fn test_upstream_config_https() {
        let config = UpstreamProxyConfig::from_url("https://proxy.example.com:8443").unwrap();
        assert_eq!(config.host, "proxy.example.com");
        assert_eq!(config.port, 8443);
        assert_eq!(config.protocol, UpstreamProtocol::Https);
    }

    #[test]
    fn test_upstream_auth_header() {
        let auth = UpstreamAuth::new("admin", "secret");
        let header = auth.to_proxy_authorization();
        assert!(header.starts_with("Basic "));
    }
}
