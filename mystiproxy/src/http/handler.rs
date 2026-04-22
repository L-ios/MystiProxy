//! HTTP 请求处理模块
//!
//! 提供请求解析、路由匹配和请求转发功能

use std::convert::Infallible;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Incoming;
use hyper::service::Service;
use hyper::{Request, Response, StatusCode};
use tracing::{debug, info, warn};

use crate::config::{EngineConfig, HeaderActionType, LocationConfig, ProviderType};
use crate::error::{MystiProxyError, Result};
use crate::http::auth::{AuthConfig as AuthModuleConfig, Authenticator};
use crate::http::body::BodyTransformer;
use crate::http::client::HttpClientPool;
use crate::http::header::HeaderTransformer;
use crate::http::static_files::StaticFileConfig;

use crate::metrics::MetricsManager;
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
    Static { config: StaticFileConfig, path: String },
    /// 未匹配
    None,
}

/// HTTP 请求处理器
#[derive(Clone)]
pub struct HttpRequestHandler {
    config: Arc<EngineConfig>,
    client_pool: Arc<HttpClientPool>,
    router: Arc<Router>,
    authenticator: Option<Arc<Authenticator>>,
    metrics: Arc<MetricsManager>,
}

impl HttpRequestHandler {
    /// 创建新的请求处理器
    pub fn new(config: Arc<EngineConfig>) -> Result<Self> {
        let client_pool = Arc::new(HttpClientPool::new());

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

        // 创建 Authenticator（如果配置了鉴权）
        let authenticator = if let Some(auth_config) = &config.auth {
            let auth_module_config = AuthModuleConfig {
                auth_type: match auth_config.auth_type.as_str() {
                    "jwt" => crate::http::auth::AuthType::Jwt {
                        secret: auth_config.jwt_secret.clone().ok_or_else(|| 
                            MystiProxyError::Config("JWT auth requires jwt_secret".to_string())
                        )?,
                        issuer: None,
                        audience: None,
                    },
                    _ => crate::http::auth::AuthType::Header,
                },
                header_name: auth_config.header_name.clone(),
                expected_value: auth_config.expected_value.clone(),
                enabled: auth_config.enabled,
            };
            Some(Arc::new(Authenticator::new(auth_module_config)))
        } else {
            None
        };

        // 创建 MetricsManager
        let metrics = Arc::new(MetricsManager::new());

        Ok(Self {
            config,
            client_pool,
            router: Arc::new(router),
            authenticator,
            metrics,
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

async fn apply_request_modifications(
    config: &EngineConfig,
    request: Request<Incoming>,
    location: &LocationConfig,
) -> Result<Request<Incoming>> {
    let method = if let Some(request_config) = &location.request {
        if let Some(m) = &request_config.method {
            hyper::http::Method::try_from(m.as_str())
                .map_err(|e| MystiProxyError::Proxy(format!("Invalid method: {e}")))?
        } else {
            request.method().clone()
        }
    } else {
        request.method().clone()
    };

    let uri = if let Some(request_config) = &location.request {
        if let Some(uri_config) = &request_config.uri {
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
        }
    } else {
        request.uri().clone()
    };

    let mut builder = Request::builder().method(method).uri(uri);

    for (name, value) in request.headers() {
        builder = builder.header(name, value);
    }

    let apply_header_transform = |builder: http::request::Builder, headers: &std::collections::HashMap<String, crate::config::HeaderAction>| -> Result<http::request::Builder> {
        let transformer = HeaderTransformer::new(headers.clone());
        let temp = builder.body(()).map_err(MystiProxyError::Http)?;
        let (parts, ()) = temp.into_parts();
        let mut header_map = parts.headers;
        let method = parts.method;
        let uri = parts.uri;
        transformer.apply(&mut header_map)?;

        let mut rebuilt = Request::builder().method(method).uri(uri);
        for (name, value) in &header_map {
            rebuilt = rebuilt.header(name, value);
        }
        Ok(rebuilt)
    };

    if let Some(request_config) = &location.request {
        if let Some(headers) = &request_config.headers {
            builder = apply_header_transform(builder, headers)?;
        }
    }

    if let Some(headers) = &config.header {
        builder = apply_header_transform(builder, headers)?;
    }

    builder
        .body(request.into_body())
        .map_err(MystiProxyError::Http)
}

impl Service<Request<Incoming>> for HttpRequestHandler {
    type Response = Response<BoxBody>;
    type Error = MystiProxyError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let config = self.config.clone();
        let client_pool = self.client_pool.clone();
        let router = self.router.clone();
        let authenticator = self.authenticator.clone();
        let metrics = self.metrics.clone();

        Box::pin(async move {
            let start_time = Instant::now();
            let path = req.uri().path().to_string();
            let method = req.method().to_string();
            debug!("Handling request: {} {}", req.method(), path);

            // 检查是否为 WebSocket 升级请求
            if crate::http::is_websocket_upgrade_request(&req) {
                info!("WebSocket upgrade request received");
                
                // 进行认证
                if let Some(auth) = &authenticator {
                    let auth_result = auth.authenticate(req.headers())?;
                    if !auth_result.authenticated {
                        let response = Response::builder()
                            .status(StatusCode::UNAUTHORIZED)
                            .body(Self::empty_body())
                            .map_err(MystiProxyError::Http)?;

                        let duration = start_time.elapsed();
                        metrics.record_http_request(&method, &path, response.status().as_u16(), duration);

                        return Ok(response);
                    }
                }

                // 处理 WebSocket 升级
                let response = crate::http::handle_websocket_upgrade(req).await?;

                let duration = start_time.elapsed();
                metrics.record_http_request(&method, &path, response.status().as_u16(), duration);

                // 转换响应体类型
                let (parts, _body) = response.into_parts();
                let new_response = Response::from_parts(parts, Self::empty_body());

                return Ok(new_response);
            }

            // 进行认证
            if let Some(auth) = authenticator {
                let auth_result = auth.authenticate(req.headers())?;
                if !auth_result.authenticated {
                    let response = Response::builder()
                        .status(StatusCode::UNAUTHORIZED)
                        .body(Self::empty_body())
                        .map_err(MystiProxyError::Http)?;

                    let duration = start_time.elapsed();
                    metrics.record_http_request(&method, &path, response.status().as_u16(), duration);

                    return Ok(response);
                }
                debug!("Authentication successful: {:?}", auth_result.user);
            }

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
                            let mut sf_config = StaticFileConfig {
                                root: PathBuf::from(root),
                                ..Default::default()
                            };
                            if let Some(ref index_files) = location.index_files {
                                sf_config.index_files = index_files.clone();
                            }
                            if let Some(enable) = location.enable_directory_listing {
                                sf_config.enable_directory_listing = enable;
                            }
                            RouteMatch::Static {
                                config: sf_config,
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
                        apply_request_modifications(&config, req, loc).await?
                    } else {
                        req
                    };

                    let client = client_pool.get_or_create(target.clone(), config.request_timeout).await;

                    // 检查是否需要 body 转换
                    let body_config = location.as_ref()
                        .and_then(|loc| loc.request.as_ref())
                        .and_then(|req_conf| req_conf.body.as_ref());

                    let response = if let Some(bc) = body_config {
                        let (parts, body) = modified_request.into_parts();
                        let body_bytes = body
                            .collect()
                            .await
                            .map_err(|e| MystiProxyError::Hyper(e.to_string()))?
                            .to_bytes();

                        let mut json_body: serde_json::Value = if body_bytes.is_empty() {
                            serde_json::Value::Object(serde_json::Map::new())
                        } else {
                            serde_json::from_slice(&body_bytes).map_err(|e| {
                                MystiProxyError::Proxy(format!("Failed to parse request body as JSON: {e}"))
                            })?
                        };

                        BodyTransformer::transform(&mut json_body, bc)?;

                        let new_body_bytes = serde_json::to_vec(&json_body).map_err(|e| {
                            MystiProxyError::Proxy(format!("Failed to serialize transformed body: {e}"))
                        })?;

                        let mut filtered_headers = hyper::header::HeaderMap::new();
                        for (name, value) in &parts.headers {
                            if name != "content-length" && name != "transfer-encoding" {
                                filtered_headers.insert(name.clone(), value.clone());
                            }
                        }
                        filtered_headers.insert("content-type", "application/json".parse().unwrap());
                        filtered_headers.insert("content-length", new_body_bytes.len().into());

                        let boxed_request = client.build_boxed_request(
                            parts.method,
                            parts.uri,
                            filtered_headers,
                            Bytes::from(new_body_bytes),
                        )?;

                        client.send_boxed(boxed_request).await?
                    } else {
                        client.send_request(modified_request).await?
                    };

                    let (parts, body) = response.into_parts();
                    let body_bytes = body
                        .collect()
                        .await
                        .map_err(|e| MystiProxyError::Hyper(e.to_string()))?
                        .to_bytes();

                    let new_response = Response::from_parts(parts, Self::full_body(body_bytes));

                    let duration = start_time.elapsed();
                    metrics.record_http_request(&method, &path, new_response.status().as_u16(), duration);

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

                    let duration = start_time.elapsed();
                    metrics.record_http_request(&method, &path, response.status().as_u16(), duration);

                    Ok(response)
                }
                RouteMatch::Static { config: sf_config, path } => {
                    info!("Serving static file: {}", path);
                    let service = crate::http::static_files::StaticFileService::with_config(sf_config);
                    let response = service.serve(&path).await?;

                    let duration = start_time.elapsed();
                    metrics.record_http_request(&method, &path, response.status().as_u16(), duration);

                    Ok(response)
                }
                RouteMatch::None => {
                    warn!("No route matched for: {}", path);
                    let response = Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Self::empty_body())
                        .map_err(MystiProxyError::Http)?;

                    let duration = start_time.elapsed();
                    metrics.record_http_request(&method, &path, response.status().as_u16(), duration);

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
            index_files: None,
            enable_directory_listing: None,
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
            index_files: None,
            enable_directory_listing: None,
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
            index_files: None,
            enable_directory_listing: None,
        };

        let mock = build_mock_response(&location);
        assert_eq!(mock.status, 200);
    }

    #[test]
    fn test_route_match_static_variant() {
        let route_match = RouteMatch::Static {
            config: StaticFileConfig {
                root: PathBuf::from("/var/www"),
                ..Default::default()
            },
            path: "/index.html".to_string(),
        };

        match route_match {
            RouteMatch::Static { config, path } => {
                assert_eq!(config.root, PathBuf::from("/var/www"));
                assert_eq!(path, "/index.html");
            }
            _ => panic!("Expected Static variant"),
        }
    }
}
