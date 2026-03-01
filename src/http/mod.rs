//! HTTP 处理模块
//!
//! 提供 HTTP 代理的核心功能，包括服务器、客户端和请求处理

mod auth;
mod body;
mod client;
mod handler;
mod proxy;
mod server;
mod static_files;
mod upstream;

use http_body_util::BodyExt;
use hyper::body::{Bytes, Incoming};

// 重导出公共接口
pub use auth::{
    AuthConfig, AuthResult, AuthType, Authenticator, Claims,
};
pub use body::{read_json_body, write_json_body, BodyTransformer};
pub use client::{HttpClient, HttpClientPool};
pub use handler::{create_handler, BoxBody, HttpRequestHandler, RouteMatch};
pub use proxy::{
    HttpProxyAcceptor, HttpProxyConfig, HttpProxyService, ProxyAuthConfig,
};
pub use server::{
    create_simple_server, BoxBody as ServerBoxBody, HttpProxyService as SimpleHttpProxyService, HttpServer, HttpServerConfig,
};
pub use static_files::{StaticFileConfig, StaticFileService};
pub use upstream::{
    ProxyConverter, UpstreamAuth, UpstreamProtocol, UpstreamProxyConfig, UpstreamProxyConnector,
};

/// HTTP 请求处理工具
pub struct HttpHandler;

impl HttpHandler {
    /// 读取请求体
    pub async fn read_body(body: Incoming) -> crate::Result<Bytes> {
        body.collect()
            .await
            .map(|collected| collected.to_bytes())
            .map_err(|e| crate::MystiProxyError::Hyper(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_handler_creation() {
        let handler = HttpHandler;
        assert!(true);
    }
}
