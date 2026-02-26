//! HTTP 请求处理模块
//!
//! 提供请求解析、路由匹配和请求转发功能

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Incoming;
use hyper::service::Service;
use hyper::{Request, Response, StatusCode};
use regex::Regex;
use tracing::{debug, info, warn};

use crate::config::{EngineConfig, HeaderActionType, LocationConfig, MatchMode, ProviderType};
use crate::error::{MystiProxyError, Result};
use crate::http::client::HttpClient;
use crate::mock::MockResponse;

/// BoxBody 类型别名
pub type BoxBody = http_body_util::combinators::BoxBody<Bytes, Infallible>;

/// 路由匹配结果
#[derive(Debug, Clone)]
pub enum RouteMatch {
    /// 代理转发
    Proxy {
        /// 目标地址
        target: String,
        /// 位置配置
        location: Option<LocationConfig>,
    },
    /// Mock 响应
    Mock(MockResponse),
    /// 未匹配
    None,
}

/// HTTP 请求处理器
#[derive(Clone)]
pub struct HttpRequestHandler {
    /// 引擎配置
    config: Arc<EngineConfig>,
    /// HTTP 客户端
    client: Arc<HttpClient>,
    /// 路由规则
    routes: Vec<RouteRule>,
}

/// 路由规则
#[derive(Debug, Clone)]
struct RouteRule {
    /// 匹配模式
    pattern: String,
    /// 匹配模式类型
    mode: MatchMode,
    /// 正则表达式（如果需要）
    regex: Option<Regex>,
    /// 提供者类型
    provider: ProviderType,
    /// 位置配置
    location: LocationConfig,
}

impl HttpRequestHandler {
    /// 创建新的请求处理器
    pub fn new(config: Arc<EngineConfig>) -> Result<Self> {
        let client = Arc::new(HttpClient::new(config.target.clone(), config.timeout));
        let mut routes = Vec::new();

        // 构建路由规则
        if let Some(locations) = &config.locations {
            for location in locations {
                let regex =
                    if location.mode == MatchMode::Regex || location.mode == MatchMode::PrefixRegex
                    {
                        Some(Regex::new(&location.location).map_err(|e| {
                            MystiProxyError::Router(format!("Invalid regex pattern: {}", e))
                        })?)
                    } else {
                        None
                    };

                let provider = location.provider.clone().unwrap_or(ProviderType::Proxy);

                routes.push(RouteRule {
                    pattern: location.location.clone(),
                    mode: location.mode.clone(),
                    regex,
                    provider,
                    location: location.clone(),
                });
            }
        }

        Ok(Self {
            config,
            client,
            routes,
        })
    }

    /// 匹配路由
    fn match_route(&self, path: &str) -> RouteMatch {
        for rule in &self.routes {
            let matched = match rule.mode {
                MatchMode::Full => path == rule.pattern,
                MatchMode::Prefix => path.starts_with(&rule.pattern),
                MatchMode::Regex => rule.regex.as_ref().map(|r| r.is_match(path)).unwrap_or(false),
                MatchMode::PrefixRegex => {
                    // 前缀正则匹配：先检查前缀，再用正则匹配剩余部分
                    if path.starts_with(&rule.pattern) {
                        true
                    } else if let Some(regex) = &rule.regex {
                        regex.is_match(path)
                    } else {
                        false
                    }
                }
            };

            if matched {
                debug!("Route matched: {} with mode {:?}", rule.pattern, rule.mode);

                return match rule.provider {
                    ProviderType::Proxy => RouteMatch::Proxy {
                        target: self.config.target.clone(),
                        location: Some(rule.location.clone()),
                    },
                    ProviderType::Mock => {
                        let mock = self.build_mock_response(&rule.location);
                        RouteMatch::Mock(mock)
                    }
                    ProviderType::Static => {
                        // 静态响应，类似于 Mock
                        let mock = self.build_mock_response(&rule.location);
                        RouteMatch::Mock(mock)
                    }
                };
            }
        }

        // 没有匹配的路由，使用默认代理
        RouteMatch::Proxy {
            target: self.config.target.clone(),
            location: None,
        }
    }

    /// 构建 Mock 响应
    fn build_mock_response(&self, location: &LocationConfig) -> MockResponse {
        let mut mock = MockResponse::new();

        if let Some(response) = &location.response {
            if let Some(status) = response.status {
                mock = mock.status(status);
            }

            if let Some(headers) = &response.headers {
                for (key, action) in headers {
                    if action.action == HeaderActionType::Overwrite {
                        mock = mock.header(key.clone(), action.value.clone());
                    }
                }
            }

            if let Some(body) = &response.body {
                if let Some(body_type) = &body.body_type {
                    if body_type == &crate::config::BodyType::Static {
                        mock = mock.body(String::new());
                    }
                }
            }
        }

        mock
    }

    /// 应用请求修改
    fn apply_request_modifications(
        &self,
        request: Request<Incoming>,
        location: &LocationConfig,
    ) -> Result<Request<Incoming>> {
        if let Some(request_config) = &location.request {
            // 修改方法
            let method = if let Some(m) = &request_config.method {
                hyper::http::Method::try_from(m.as_str())
                    .map_err(|e| MystiProxyError::Proxy(format!("Invalid method: {}", e)))?
            } else {
                request.method().clone()
            };

            // 修改 URI
            let uri = if let Some(uri_config) = &request_config.uri {
                let path = uri_config.path.as_deref().unwrap_or(request.uri().path());
                let query = uri_config.query.as_deref();

                let new_uri = hyper::http::Uri::builder().path_and_query(
                    if let Some(q) = query {
                        format!("{}?{}", path, q)
                    } else {
                        path.to_string()
                    },
                );

                new_uri.build().map_err(|e| MystiProxyError::Http(e))?
            } else {
                request.uri().clone()
            };

            // 构建新请求
            let mut new_request = Request::builder().method(method).uri(uri);

            // 应用头部修改
            for (name, value) in request.headers() {
                new_request = new_request.header(name, value);
            }

            if let Some(headers) = &request_config.headers {
                for (key, action) in headers {
                    match action.action {
                        HeaderActionType::Overwrite => {
                            new_request = new_request.header(key, &action.value);
                        }
                        HeaderActionType::Missed => {
                            if !request.headers().contains_key(key) {
                                new_request = new_request.header(key, &action.value);
                            }
                        }
                        HeaderActionType::ForceDelete => {
                            // 删除头部（通过不添加）
                        }
                    }
                }
            }

            // 应用全局头部修改
            if let Some(headers) = &self.config.header {
                for (key, action) in headers {
                    match action.action {
                        HeaderActionType::Overwrite => {
                            new_request = new_request.header(key, &action.value);
                        }
                        HeaderActionType::Missed => {
                            if !request.headers().contains_key(key) {
                                new_request = new_request.header(key, &action.value);
                            }
                        }
                        HeaderActionType::ForceDelete => {
                            // 删除头部
                        }
                    }
                }
            }

            return new_request
                .body(request.into_body())
                .map_err(|e| MystiProxyError::Http(e));
        }

        // 应用全局头部修改
        if let Some(headers) = &self.config.header {
            let mut new_request = Request::builder()
                .method(request.method().clone())
                .uri(request.uri().clone());

            for (name, value) in request.headers() {
                new_request = new_request.header(name, value);
            }

            for (key, action) in headers {
                match action.action {
                    HeaderActionType::Overwrite => {
                        new_request = new_request.header(key, &action.value);
                    }
                    HeaderActionType::Missed => {
                        if !request.headers().contains_key(key) {
                            new_request = new_request.header(key, &action.value);
                        }
                    }
                    HeaderActionType::ForceDelete => {
                        // 删除头部
                    }
                }
            }

            return new_request
                .body(request.into_body())
                .map_err(|e| MystiProxyError::Http(e));
        }

        Ok(request)
    }

    /// 创建空响应体
    fn empty_body() -> BoxBody {
        Empty::<Bytes>::new()
            .map_err(|never| match never {})
            .boxed()
    }

    /// 创建完整响应体
    fn full_body(bytes: Bytes) -> BoxBody {
        Full::new(bytes)
            .map_err(|never| match never {})
            .boxed()
    }
}

impl Service<Request<Incoming>> for HttpRequestHandler {
    type Response = Response<BoxBody>;
    type Error = MystiProxyError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let handler = self.clone();
        let path = req.uri().path().to_string();

        Box::pin(async move {
            debug!("Handling request: {} {}", req.method(), path);

            // 匹配路由
            let route_match = handler.match_route(&path);

            match route_match {
                RouteMatch::Proxy { target, location } => {
                    info!("Proxying request to: {}", target);

                    // 应用请求修改
                    let modified_request = if let Some(loc) = &location {
                        handler.apply_request_modifications(req, loc)?
                    } else {
                        req
                    };

                    // 发送请求
                    let response = handler.client.send_request(modified_request).await?;

                    // 转换响应体
                    let (parts, body) = response.into_parts();
                    let body_bytes = body
                        .collect()
                        .await
                        .map_err(|e| MystiProxyError::Hyper(e.to_string()))?
                        .to_bytes();

                    let new_response = Response::from_parts(parts, Self::full_body(body_bytes));

                    Ok(new_response)
                }
                RouteMatch::Mock(mock) => {
                    info!("Returning mock response: {}", mock.status);

                    // 应用延迟
                    if mock.delay_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(mock.delay_ms)).await;
                    }

                    // 构建响应
                    let mut builder = Response::builder().status(
                        StatusCode::from_u16(mock.status).map_err(|e| {
                            MystiProxyError::Proxy(format!("Invalid status code: {}", e))
                        })?,
                    );

                    for (key, value) in &mock.headers {
                        builder = builder.header(key, value);
                    }

                    // 创建响应体
                    let body = if mock.body.is_empty() {
                        Self::empty_body()
                    } else {
                        Self::full_body(Bytes::from(mock.body))
                    };

                    let response = builder.body(body).map_err(|e| MystiProxyError::Http(e))?;

                    Ok(response)
                }
                RouteMatch::None => {
                    warn!("No route matched for: {}", path);
                    let response = Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Self::empty_body())
                        .map_err(|e| MystiProxyError::Http(e))?;

                    Ok(response)
                }
            }
        })
    }
}

/// 创建简单的请求处理器
pub fn create_handler(config: Arc<EngineConfig>) -> Result<HttpRequestHandler> {
    HttpRequestHandler::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_match_full() {
        let rule = RouteRule {
            pattern: "/api/test".to_string(),
            mode: MatchMode::Full,
            regex: None,
            provider: ProviderType::Proxy,
            location: LocationConfig {
                location: "/api/test".to_string(),
                mode: MatchMode::Full,
                provider: Some(ProviderType::Proxy),
                alias: None,
                condition: None,
                response: None,
                request: None,
            },
        };

        assert!(matches!(rule.mode, MatchMode::Full));
    }

    #[test]
    fn test_route_match_prefix() {
        let rule = RouteRule {
            pattern: "/api".to_string(),
            mode: MatchMode::Prefix,
            regex: None,
            provider: ProviderType::Proxy,
            location: LocationConfig {
                location: "/api".to_string(),
                mode: MatchMode::Prefix,
                provider: Some(ProviderType::Proxy),
                alias: None,
                condition: None,
                response: None,
                request: None,
            },
        };

        assert!(matches!(rule.mode, MatchMode::Prefix));
    }
}
