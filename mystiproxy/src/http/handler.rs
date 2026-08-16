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

use crate::config::{EngineConfig, HeaderAction, HeaderActionType, LocationConfig, ProviderType};
use crate::error::{MystiProxyError, Result};
use crate::http::auth::{AuthConfig as AuthModuleConfig, Authenticator};
use crate::http::client::HttpClientPool;
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
    Static {
        config: StaticFileConfig,
        path: String,
    },
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
                        secret: auth_config.jwt_secret.clone().ok_or_else(|| {
                            MystiProxyError::Config("JWT auth requires jwt_secret".to_string())
                        })?,
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

        // 使用进程级共享 MetricsManager（与 main.rs 的导出服务同一实例）
        let metrics = crate::metrics::global_metrics();

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

fn build_mock_response(location: &LocationConfig, uri: &str) -> MockResponse {
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
            match body.body_type.as_ref() {
                Some(crate::config::BodyType::Static) => {
                    // 静态内容：优先 content，向后兼容空体
                    let content = body.content.clone().unwrap_or_default();
                    mock = mock.body(content);
                }
                Some(crate::config::BodyType::Template) => {
                    // 模版：基于请求 URI 渲染占位符（body 上下文由条件网关按需注入，见 handler）
                    let tpl = body.template.clone().unwrap_or_default();
                    mock = mock.body(crate::mock::render_template(&tpl, uri, None));
                }
                _ => {
                    // 未指定类型但给了 content：同样作为静态体返回（与 config.example.yaml 对齐）
                    if let Some(content) = &body.content {
                        mock = mock.body(content.clone());
                    }
                }
            }
        }
    }

    mock
}

/// 请求修改结果：未修改 body 或已转换 body
pub enum ModifiedRequest {
    /// Body 未转换（仅 headers/URI/method 可能已修改）
    Incoming(Request<Incoming>),
    /// Body 已转换（JSON 变换）
    Bytes(Request<http_body_util::Full<bytes::Bytes>>),
}

async fn apply_request_modifications(
    config: &EngineConfig,
    request: Request<Incoming>,
    location: &LocationConfig,
) -> Result<ModifiedRequest> {
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

        let (mut parts, body) = request.into_parts();

        // Apply header actions
        if let Some(headers) = &request_config.headers {
            apply_header_actions(&mut parts.headers, headers);
        }
        if let Some(headers) = &config.header {
            apply_header_actions(&mut parts.headers, headers);
        }

        parts.method = method;
        parts.uri = uri;

        // Body JSON transformation
        if let Some(body_config) = &request_config.body {
            if let Some(json_config) = &body_config.json {
                let body_bytes = body
                    .collect()
                    .await
                    .map_err(|e| MystiProxyError::Hyper(e.to_string()))?
                    .to_bytes();

                if !body_bytes.is_empty() {
                    if let Ok(mut json_value) =
                        serde_json::from_slice::<serde_json::Value>(&body_bytes)
                    {
                        let transform_config = crate::config::BodyConfig {
                            json: Some(json_config.clone()),
                            body_type: None,
                            content: None,
                            template: None,
                        };
                        if let Err(e) = crate::http::body::BodyTransformer::transform(
                            &mut json_value,
                            &transform_config,
                        ) {
                            warn!("Body transformation failed: {}", e);
                        }

                        let new_bytes = bytes::Bytes::from(
                            serde_json::to_vec(&json_value).unwrap_or_else(|_| body_bytes.to_vec()),
                        );

                        parts.headers.remove("content-length");
                        parts.headers.insert(
                            "content-length",
                            hyper::header::HeaderValue::from_str(&new_bytes.len().to_string())
                                .map_err(|e| {
                                    MystiProxyError::Proxy(format!("Invalid content-length: {e}"))
                                })?,
                        );

                        return Ok(ModifiedRequest::Bytes(Request::from_parts(
                            parts,
                            http_body_util::Full::new(new_bytes),
                        )));
                    }
                }

                // Body consumed but not JSON - return raw bytes
                return Ok(ModifiedRequest::Bytes(Request::from_parts(
                    parts,
                    http_body_util::Full::new(bytes::Bytes::from(body_bytes.to_vec())),
                )));
            }
        }

        // No body transformation configured
        return Ok(ModifiedRequest::Incoming(Request::from_parts(parts, body)));
    }

    // Engine-level headers only (no location match)
    if let Some(headers) = &config.header {
        let (mut parts, body) = request.into_parts();
        apply_header_actions(&mut parts.headers, headers);
        return Ok(ModifiedRequest::Incoming(Request::from_parts(parts, body)));
    }

    Ok(ModifiedRequest::Incoming(request))
}

/// Apply engine-level header modifications when no location matches.
async fn apply_engine_header_modifications(
    config: &EngineConfig,
    request: Request<Incoming>,
) -> Result<Request<Incoming>> {
    if let Some(headers) = &config.header {
        let (mut parts, body) = request.into_parts();
        apply_header_actions(&mut parts.headers, headers);
        return Ok(Request::from_parts(parts, body));
    }
    Ok(request)
}

/// Apply header actions (Overwrite, Missed, ForceDelete) to a HeaderMap.
fn apply_header_actions(
    headers: &mut hyper::HeaderMap,
    actions: &std::collections::HashMap<String, HeaderAction>,
) {
    for (key, action) in actions {
        match action.action {
            HeaderActionType::Overwrite => {
                if let Ok(name) = key.parse::<hyper::header::HeaderName>() {
                    if let Ok(value) = action.value.parse() {
                        headers.insert(&name, value);
                    }
                }
            }
            HeaderActionType::Missed => {
                if !headers.contains_key(key.as_str()) {
                    if let Ok(name) = key.parse::<hyper::header::HeaderName>() {
                        if let Ok(value) = action.value.parse() {
                            headers.insert(&name, value);
                        }
                    }
                }
            }
            HeaderActionType::ForceDelete => {
                if let Ok(name) = key.parse::<hyper::header::HeaderName>() {
                    headers.remove(&name);
                }
            }
        }
    }
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
                        metrics.record_http_request(
                            &method,
                            &path,
                            response.status().as_u16(),
                            duration,
                        );

                        return Ok(response);
                    }
                }

                // WebSocket 真正代理：转发握手到 engine.target 并桥接
                let response = crate::http::websocket::proxy_websocket(
                    req,
                    &config.target,
                    config.request_timeout,
                )
                .await?;

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
                    metrics.record_http_request(
                        &method,
                        &path,
                        response.status().as_u16(),
                        duration,
                    );

                    return Ok(response);
                }
                debug!("Authentication successful: {:?}", auth_result.user);
            }

            // 依序遍历候选 location：mock 条件不命中时回退下一候选，其余 provider 保持第一命中语义
            let mut route_match: Option<RouteMatch> = None;
            for (route, _match_result) in router.match_uri_candidates(&path) {
                let location = &route.location_config;
                let provider = location.provider.as_ref().unwrap_or(&ProviderType::Proxy);
                match provider {
                    ProviderType::Mock => {
                        let conditions: Vec<crate::mock::Condition> = location
                            .response
                            .as_ref()
                            .and_then(|r| r.conditions.as_ref())
                            .map(|cs| {
                                cs.iter()
                                    .map(|c| crate::mock::Condition {
                                        condition_type: c.condition_type.clone(),
                                        value: c.value.clone(),
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();

                        if conditions.is_empty()
                            || crate::mock::MockBuilder::matches_conditions(
                                &req.uri().to_string(),
                                req.headers(),
                                None,
                                &conditions,
                            )
                        {
                            let mock = build_mock_response(location, &req.uri().to_string());
                            route_match = Some(RouteMatch::Mock(mock));
                            break;
                        }
                        // 条件不命中：尝试下一候选
                        debug!(
                            "Mock conditions not matched for location {}, trying next",
                            location.location
                        );
                    }
                    ProviderType::Proxy => {
                        route_match = Some(RouteMatch::Proxy {
                            target: config.target.clone(),
                            location: Some(location.clone()),
                        });
                        break;
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
                        // 前缀匹配时剥离 location 前缀（与上游 09312dd 语义一致）
                        let stripped = match _match_result.remaining.as_deref() {
                            Some(remaining) if !remaining.is_empty() => {
                                format!("/{}", remaining.trim_start_matches('/'))
                            }
                            _ => "/".to_string(),
                        };
                        route_match = Some(RouteMatch::Static {
                            config: sf_config,
                            path: stripped,
                        });
                        break;
                    }
                }
            }
            let route_match = route_match.unwrap_or(RouteMatch::Proxy {
                target: config.target.clone(),
                location: None,
            });

            match route_match {
                RouteMatch::Proxy { target, location } => {
                    info!("Proxying request to: {}", target);

                    let client = client_pool
                        .get_or_create_with_upstream(
                            target.clone(),
                            config.request_timeout,
                            config.upstream.as_deref(),
                        )
                        .await;

                    // Get response parts + body bytes, handling both Incoming and Bytes body types
                    let (resp_parts, body_bytes) = if let Some(loc) = &location {
                        match apply_request_modifications(&config, req, loc).await? {
                            ModifiedRequest::Incoming(r) => {
                                let resp = client.send_request(r).await?;
                                let (parts, body) = resp.into_parts();
                                let bytes = body
                                    .collect()
                                    .await
                                    .map_err(|e| MystiProxyError::Hyper(e.to_string()))?
                                    .to_bytes();
                                (parts, bytes)
                            }
                            ModifiedRequest::Bytes(r) => {
                                let (parts, body) = r.into_parts();
                                let boxed = body.map_err(|never| match never {}).boxed();
                                let boxed_req = Request::from_parts(parts, boxed);
                                let resp = client.send_boxed(boxed_req).await?;
                                let (parts, body) = resp.into_parts();
                                let bytes = body
                                    .collect()
                                    .await
                                    .map_err(|e| MystiProxyError::Hyper(e.to_string()))?
                                    .to_bytes();
                                (parts, bytes)
                            }
                        }
                    } else if config.header.is_some() {
                        let r = apply_engine_header_modifications(&config, req).await?;
                        let resp = client.send_request(r).await?;
                        let (parts, body) = resp.into_parts();
                        let bytes = body
                            .collect()
                            .await
                            .map_err(|e| MystiProxyError::Hyper(e.to_string()))?
                            .to_bytes();
                        (parts, bytes)
                    } else {
                        let resp = client.send_request(req).await?;
                        let (parts, body) = resp.into_parts();
                        let bytes = body
                            .collect()
                            .await
                            .map_err(|e| MystiProxyError::Hyper(e.to_string()))?
                            .to_bytes();
                        (parts, bytes)
                    };

                    let new_response =
                        Response::from_parts(resp_parts, Self::full_body(body_bytes));

                    let duration = start_time.elapsed();
                    metrics.record_http_request(
                        &method,
                        &path,
                        new_response.status().as_u16(),
                        duration,
                    );

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
                    metrics.record_http_request(
                        &method,
                        &path,
                        response.status().as_u16(),
                        duration,
                    );

                    Ok(response)
                }
                RouteMatch::Static {
                    config: sf_config,
                    path: static_path,
                } => {
                    info!("Serving static file: {}", static_path);
                    let service =
                        crate::http::static_files::StaticFileService::with_config(sf_config);
                    // Pass Range header if present
                    let range_header = req
                        .headers()
                        .get("range")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.to_string());
                    let response = if let Some(range) = range_header {
                        service.serve_with_range(&static_path, Some(&range)).await?
                    } else {
                        service.serve(&static_path).await?
                    };

                    let duration = start_time.elapsed();
                    metrics.record_http_request(
                        &method,
                        &path,
                        response.status().as_u16(),
                        duration,
                    );

                    Ok(response)
                }
                RouteMatch::None => {
                    warn!("No route matched for: {}", path);
                    let response = Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Self::empty_body())
                        .map_err(MystiProxyError::Http)?;

                    let duration = start_time.elapsed();
                    metrics.record_http_request(
                        &method,
                        &path,
                        response.status().as_u16(),
                        duration,
                    );

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

        let mock = build_mock_response(&location, "/test");
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
