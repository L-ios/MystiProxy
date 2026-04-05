//! HTTP 请求处理模块
//!
//! 提供请求解析、路由匹配和请求转发功能

use std::convert::Infallible;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Incoming;
use hyper::service::Service;
use hyper::{Request, Response, StatusCode};
use tracing::{debug, info, warn};

use crate::config::{EngineConfig, HeaderActionType, LocationConfig, ProviderType};
use crate::error::{MystiProxyError, Result};
use crate::http::client::HttpClient;
use crate::mock::MockResponse;
use crate::router::{Route, Router};

/// BoxBody 类型别名
pub type BoxBody = http_body_util::combinators::BoxBody<Bytes, Infallible>;

/// 路由匹配结果
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum RouteMatch {
    /// 代理转发
    Proxy {
        target: String,
        location: Option<LocationConfig>,
    },
    /// Mock 响应
    Mock(MockResponse),
    /// 静态文件服务
    Static { root: String, path: String },
    /// 未匹配
    None,
}

/// HTTP 请求处理器
#[derive(Clone)]
pub struct HttpRequestHandler {
    config: Arc<EngineConfig>,
    client: Arc<HttpClient>,
    router: Arc<Router>,
}

impl HttpRequestHandler {
    /// 创建新的请求处理器
    pub fn new(config: Arc<EngineConfig>) -> Result<Self> {
        let client = Arc::new(HttpClient::new(
            config.target.clone(),
            config.request_timeout,
        ));

        let mut router = Router::new();
        if let Some(locations) = &config.locations {
            for location in locations {
                let route = Route::new(
                    location.location.clone(),
                    location.mode.clone(),
                    location.clone(),
                )?;
                router.add_route(route);
            }
        }

        Ok(Self {
            config,
            client,
            router: Arc::new(router),
        })
    }

    fn empty_body() -> BoxBody {
        Empty::<Bytes>::new()
            .map_err(|never| match never {})
            .boxed()
    }

    fn full_body(bytes: Bytes) -> BoxBody {
        Full::new(bytes).map_err(|never| match never {}).boxed()
    }
}

fn build_mock_response(location: &LocationConfig) -> MockResponse {
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

fn apply_request_modifications(
    config: &EngineConfig,
    request: Request<Incoming>,
    location: &LocationConfig,
) -> Result<Request<Incoming>> {
    if let Some(request_config) = &location.request {
        let method = if let Some(m) = &request_config.method {
            hyper::http::Method::try_from(m.as_str())
                .map_err(|e| MystiProxyError::Proxy(format!("Invalid method: {e}")))?
        } else {
            request.method().clone()
        };

        let uri = if let Some(uri_config) = &request_config.uri {
            let path = uri_config.path.as_deref().unwrap_or(request.uri().path());
            let query = uri_config.query.as_deref();

            let new_uri = hyper::http::Uri::builder().path_and_query(if let Some(q) = query {
                format!("{path}?{q}")
            } else {
                path.to_string()
            });

            new_uri.build().map_err(MystiProxyError::Http)?
        } else {
            request.uri().clone()
        };

        let mut new_request = Request::builder().method(method).uri(uri);

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
                    HeaderActionType::ForceDelete => {}
                }
            }
        }

        if let Some(headers) = &config.header {
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
                    HeaderActionType::ForceDelete => {}
                }
            }
        }

        return new_request
            .body(request.into_body())
            .map_err(MystiProxyError::Http);
    }

    if let Some(headers) = &config.header {
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
                HeaderActionType::ForceDelete => {}
            }
        }

        return new_request
            .body(request.into_body())
            .map_err(MystiProxyError::Http);
    }

    Ok(request)
}

impl Service<Request<Incoming>> for HttpRequestHandler {
    type Response = Response<BoxBody>;
    type Error = MystiProxyError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let config = self.config.clone();
        let client = self.client.clone();
        let router = self.router.clone();

        Box::pin(async move {
            let path = req.uri().path().to_string();
            debug!("Handling request: {} {}", req.method(), path);

            let route_match = match router.match_uri(&path) {
                Some((route, _match_result)) => {
                    let location = &route.location_config;
                    let provider = location.provider.as_ref().unwrap_or(&ProviderType::Proxy);
                    match provider {
                        ProviderType::Proxy => RouteMatch::Proxy {
                            target: config.target.clone(),
                            location: Some(location.clone()),
                        },
                        ProviderType::Mock => {
                            let mock = build_mock_response(location);
                            RouteMatch::Mock(mock)
                        }
                        ProviderType::Static => {
                            let root = location.root.clone().unwrap_or_else(|| ".".to_string());
                            RouteMatch::Static {
                                root,
                                path: path.clone(),
                            }
                        }
                    }
                }
                None => RouteMatch::Proxy {
                    target: config.target.clone(),
                    location: None,
                },
            };

            match route_match {
                RouteMatch::Proxy { target, location } => {
                    info!("Proxying request to: {}", target);

                    let modified_request = if let Some(loc) = &location {
                        apply_request_modifications(&config, req, loc)?
                    } else {
                        req
                    };

                    let response = client.send_request(modified_request).await?;

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

                    if mock.delay_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(mock.delay_ms)).await;
                    }

                    let mut builder =
                        Response::builder().status(StatusCode::from_u16(mock.status).map_err(
                            |e| MystiProxyError::Proxy(format!("Invalid status code: {e}")),
                        )?);

                    for (key, value) in &mock.headers {
                        builder = builder.header(key, value);
                    }

                    let body = if mock.body.is_empty() {
                        Self::empty_body()
                    } else {
                        Self::full_body(Bytes::from(mock.body))
                    };

                    let response = builder.body(body).map_err(MystiProxyError::Http)?;

                    Ok(response)
                }
                RouteMatch::Static { root, path } => {
                    info!("Serving static file: {}", path);
                    let service =
                        crate::http::static_files::StaticFileService::new(PathBuf::from(root));
                    service.serve(&path).await
                }
                RouteMatch::None => {
                    warn!("No route matched for: {}", path);
                    let response = Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Self::empty_body())
                        .map_err(MystiProxyError::Http)?;

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
    use crate::config::MatchMode;

    #[test]
    fn test_router_integration_full_match() {
        let mut router = Router::new();
        let location = LocationConfig {
            location: "/api/test".to_string(),
            mode: MatchMode::Full,
            provider: Some(ProviderType::Proxy),
            root: None,
            response: None,
            request: None,
        };
        let route = Route::new("/api/test".to_string(), MatchMode::Full, location).unwrap();
        router.add_route(route);

        let result = router.match_uri("/api/test");
        assert!(result.is_some());
        let (route, _) = result.unwrap();
        assert_eq!(route.location_config.provider, Some(ProviderType::Proxy));
    }

    #[test]
    fn test_router_integration_prefix_match() {
        let mut router = Router::new();
        let location = LocationConfig {
            location: "/api".to_string(),
            mode: MatchMode::Prefix,
            provider: Some(ProviderType::Proxy),
            root: None,
            response: None,
            request: None,
        };
        let route = Route::new("/api".to_string(), MatchMode::Prefix, location).unwrap();
        router.add_route(route);

        let result = router.match_uri("/api/users");
        assert!(result.is_some());
        let (route, match_result) = result.unwrap();
        assert_eq!(route.location_config.provider, Some(ProviderType::Proxy));
        assert_eq!(match_result.remaining, Some("users".to_string()));
    }

    #[test]
    fn test_router_no_match() {
        let router = Router::new();
        let result = router.match_uri("/nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_build_mock_response_default() {
        let location = LocationConfig {
            location: "/test".to_string(),
            mode: MatchMode::Full,
            provider: Some(ProviderType::Mock),
            root: None,
            response: None,
            request: None,
        };

        let mock = build_mock_response(&location);
        assert_eq!(mock.status, 200);
    }

    #[test]
    fn test_route_match_static_variant() {
        let route_match = RouteMatch::Static {
            root: "/var/www".to_string(),
            path: "/index.html".to_string(),
        };

        match route_match {
            RouteMatch::Static { root, path } => {
                assert_eq!(root, "/var/www");
                assert_eq!(path, "/index.html");
            }
            _ => panic!("Expected Static variant"),
        }
    }
}
