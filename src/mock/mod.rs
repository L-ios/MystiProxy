//! Mock 模块
//!
//! 提供 Mock 响应构建和条件匹配功能

use std::collections::HashMap;
use std::convert::Infallible;

use bytes::Bytes;
use http_body_util::{BodyExt, Empty, Full};
use hyper::header::HeaderMap;
use hyper::{Response, StatusCode};
use regex::Regex;
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::config::{BodyType, Condition, HeaderActionType, LocationConfig, MatchMode, ResponseConfig};
use crate::error::{MystiProxyError, Result};

/// BoxBody 类型别名
pub type BoxBody = http_body_util::combinators::BoxBody<Bytes, Infallible>;

/// Mock 响应
#[derive(Debug, Clone)]
pub struct MockResponse {
    /// 状态码
    pub status: u16,
    /// 响应头
    pub headers: HashMap<String, String>,
    /// 响应体
    pub body: String,
    /// 延迟（毫秒）
    pub delay_ms: u64,
}

impl Default for MockResponse {
    fn default() -> Self {
        MockResponse {
            status: 200,
            headers: HashMap::new(),
            body: String::new(),
            delay_ms: 0,
        }
    }
}

impl MockResponse {
    /// 创建新的 Mock 响应
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置状态码
    pub fn status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    /// 设置响应体
    pub fn body(mut self, body: String) -> Self {
        self.body = body;
        self
    }

    /// 添加响应头
    pub fn header(mut self, key: String, value: String) -> Self {
        self.headers.insert(key, value);
        self
    }

    /// 设置延迟
    pub fn delay(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self
    }
}

/// Mock 响应构建器
pub struct MockBuilder;

impl MockBuilder {
    /// 检查请求是否匹配所有条件
    ///
    /// # 参数
    /// - `uri`: 请求 URI
    /// - `headers`: 请求头
    /// - `body`: 请求体（可选）
    /// - `conditions`: 条件列表
    ///
    /// # 返回
    /// 如果所有条件都匹配则返回 true，否则返回 false
    pub fn matches_conditions(
        uri: &str,
        headers: &HeaderMap,
        body: Option<&Value>,
        conditions: &[Condition],
    ) -> bool {
        // 如果没有条件，默认匹配
        if conditions.is_empty() {
            return true;
        }

        // 检查所有条件
        for condition in conditions {
            if !Self::matches_single_condition(uri, headers, body, condition) {
                debug!("Condition not matched: {:?}", condition);
                return false;
            }
        }

        true
    }

    /// 检查单个条件是否匹配
    fn matches_single_condition(
        uri: &str,
        headers: &HeaderMap,
        body: Option<&Value>,
        condition: &Condition,
    ) -> bool {
        let condition_type = condition.condition_type.to_lowercase();

        match condition_type.as_str() {
            // URI 路径匹配
            "uri" | "path" => Self::matches_uri(uri, &condition.value),

            // Query 参数匹配
            "query" => Self::matches_query(uri, &condition.value),

            // Header 匹配
            "header" => Self::matches_header(headers, &condition.value),

            // Body 字段匹配
            "body" | "json" => Self::matches_body(body, &condition.value),

            // 未知条件类型
            _ => {
                warn!("Unknown condition type: {}", condition.condition_type);
                false
            }
        }
    }

    /// 匹配 URI 路径
    ///
    /// 支持格式：
    /// - 精确匹配: `/api/test`
    /// - 前缀匹配: `/api/*`
    /// - 正则匹配: `regex:/api/.*`
    fn matches_uri(uri: &str, pattern: &str) -> bool {
        // 解析 URI，提取路径部分
        let path = if let Ok(parsed) = url::Url::parse(&format!("http://localhost{}", uri)) {
            parsed.path().to_string()
        } else {
            uri.to_string()
        };

        // 检查是否为正则匹配
        if let Some(regex_pattern) = pattern.strip_prefix("regex:") {
            if let Ok(re) = Regex::new(regex_pattern) {
                return re.is_match(&path);
            }
            return false;
        }

        // 检查是否为前缀匹配
        if let Some(prefix) = pattern.strip_suffix('*') {
            return path.starts_with(prefix);
        }

        // 精确匹配
        path == pattern
    }

    /// 匹配 Query 参数
    ///
    /// 支持格式：
    /// - 存在检查: `key`
    /// - 值匹配: `key=value`
    /// - 正则匹配: `key=regex:pattern`
    fn matches_query(uri: &str, pattern: &str) -> bool {
        // 解析 URI，提取查询参数
        let query_string = if let Ok(parsed) = url::Url::parse(&format!("http://localhost{}", uri)) {
            parsed.query().unwrap_or("").to_string()
        } else {
            // 尝试从 URI 中提取查询部分
            if let Some(pos) = uri.find('?') {
                uri[pos + 1..].to_string()
            } else {
                String::new()
            }
        };

        // 解析查询参数
        let params: HashMap<String, String> = urlencoding::decode(&query_string)
            .unwrap_or_default()
            .split('&')
            .filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                let key = parts.next()?.to_string();
                let value = parts.next().unwrap_or("").to_string();
                Some((key, value))
            })
            .collect();

        // 解析模式
        if let Some(eq_pos) = pattern.find('=') {
            let key = &pattern[..eq_pos];
            let value_pattern = &pattern[eq_pos + 1..];

            // 检查参数是否存在
            if let Some(param_value) = params.get(key) {
                // 检查是否为正则匹配
                if let Some(regex_pattern) = value_pattern.strip_prefix("regex:") {
                    if let Ok(re) = Regex::new(regex_pattern) {
                        return re.is_match(param_value);
                    }
                    return false;
                }

                // 精确匹配
                return param_value == value_pattern;
            }

            false
        } else {
            // 只检查参数是否存在
            params.contains_key(pattern)
        }
    }

    /// 匹配 Header
    ///
    /// 支持格式：
    /// - 存在检查: `Header-Name`
    /// - 值匹配: `Header-Name=value`
    /// - 正则匹配: `Header-Name=regex:pattern`
    fn matches_header(headers: &HeaderMap, pattern: &str) -> bool {
        // 解析模式
        if let Some(eq_pos) = pattern.find('=') {
            let header_name = &pattern[..eq_pos];
            let value_pattern = &pattern[eq_pos + 1..];

            // 检查 header 是否存在
            if let Some(header_value) = headers.get(header_name) {
                let header_str = header_value.to_str().unwrap_or("");

                // 检查是否为正则匹配
                if let Some(regex_pattern) = value_pattern.strip_prefix("regex:") {
                    if let Ok(re) = Regex::new(regex_pattern) {
                        return re.is_match(header_str);
                    }
                    return false;
                }

                // 精确匹配
                return header_str == value_pattern;
            }

            false
        } else {
            // 只检查 header 是否存在
            headers.contains_key(pattern)
        }
    }

    /// 匹配 Body 字段
    ///
    /// 支持格式：
    /// - JSONPath 匹配: `$.field=value`
    /// - 正则匹配: `$.field=regex:pattern`
    fn matches_body(body: Option<&Value>, pattern: &str) -> bool {
        let body = match body {
            Some(b) => b,
            None => return false,
        };

        // 解析模式
        if let Some(eq_pos) = pattern.find('=') {
            let path = &pattern[..eq_pos];
            let value_pattern = &pattern[eq_pos + 1..];

            // 使用 JSONPath 提取值
            if let Some(field_value) = Self::get_value_by_path(body, path) {
                let field_str = match field_value {
                    Value::String(s) => s.clone(),
                    _ => field_value.to_string(),
                };

                // 检查是否为正则匹配
                if let Some(regex_pattern) = value_pattern.strip_prefix("regex:") {
                    if let Ok(re) = Regex::new(regex_pattern) {
                        return re.is_match(&field_str);
                    }
                    return false;
                }

                // 精确匹配
                return field_str == value_pattern;
            }

            false
        } else {
            // 只检查字段是否存在
            Self::get_value_by_path(body, pattern).is_some()
        }
    }

    /// 根据 JSONPath 获取值
    fn get_value_by_path(body: &Value, path: &str) -> Option<Value> {
        if path == "$" {
            return Some(body.clone());
        }

        let path = path.strip_prefix("$.")?;
        let parts: Vec<&str> = path.split('.').collect();

        let mut current = body;
        for part in parts {
            // 处理数组索引
            if part.contains('[') {
                current = Self::navigate_array(current, part)?;
            } else if let Some(obj) = current.as_object() {
                current = obj.get(part)?;
            } else {
                return None;
            }
        }

        Some(current.clone())
    }

    /// 导航到数组元素
    fn navigate_array<'a>(body: &'a Value, part: &str) -> Option<&'a Value> {
        let idx_parts: Vec<&str> = part.split('[').collect();
        let field = idx_parts[0];

        let mut current = body;

        // 如果有字段名，先导航到字段
        if !field.is_empty() {
            if let Some(obj) = current.as_object() {
                current = obj.get(field)?;
            } else {
                return None;
            }
        }

        // 处理数组索引
        for idx_part in idx_parts.iter().skip(1) {
            if let Some(idx_str) = idx_part.strip_suffix(']') {
                let idx: usize = idx_str.parse().ok()?;
                if let Some(arr) = current.as_array() {
                    if idx < arr.len() {
                        current = &arr[idx];
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
        }

        Some(current)
    }

    /// 构建 Mock 响应
    ///
    /// # 参数
    /// - `config`: 响应配置
    ///
    /// # 返回
    /// 成功返回 Response<BoxBody>，失败返回错误
    pub fn build_response(config: &ResponseConfig) -> Result<Response<BoxBody>> {
        let mut builder = Response::builder()
            .status(StatusCode::from_u16(config.status.unwrap_or(200))
                .map_err(|e| MystiProxyError::Mock(format!("Invalid status code: {}", e)))?);

        // 添加 headers
        if let Some(headers) = &config.headers {
            for (name, action) in headers {
                if action.action == HeaderActionType::Overwrite {
                    builder = builder.header(name, &action.value);
                }
            }
        }

        // 添加 body
        let body = if let Some(body_config) = &config.body {
            // 根据类型构建 body
            if let Some(body_type) = &body_config.body_type {
                match body_type {
                    BodyType::Static => {
                        // 静态响应体
                        if let Some(json_config) = &body_config.json {
                            Self::full_body(Bytes::from(json_config.value.clone()))
                        } else {
                            Self::empty_body()
                        }
                    }
                    BodyType::Json => {
                        // JSON 响应体
                        if let Some(json_config) = &body_config.json {
                            // 尝试解析为 JSON，如果失败则作为字符串
                            let body_str = if let Ok(value) = serde_json::from_str::<Value>(&json_config.value) {
                                serde_json::to_string(&value).unwrap_or_else(|_| json_config.value.clone())
                            } else {
                                json_config.value.clone()
                            };
                            Self::full_body(Bytes::from(body_str))
                        } else {
                            Self::empty_body()
                        }
                    }
                }
            } else {
                Self::empty_body()
            }
        } else {
            Self::empty_body()
        };

        builder.body(body).map_err(MystiProxyError::Http)
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

/// Mock 位置配置
#[derive(Debug, Clone)]
pub struct MockLocation {
    /// 位置路径
    pub location: String,
    /// 匹配模式
    pub mode: MatchMode,
    /// 正则表达式（如果需要）
    pub regex: Option<Regex>,
    /// 条件列表
    pub conditions: Vec<Condition>,
    /// 响应配置
    pub response: ResponseConfig,
}

impl MockLocation {
    /// 从 LocationConfig 创建 MockLocation
    pub fn from_config(config: &LocationConfig) -> Result<Self> {
        // 编译正则表达式（如果需要）
        let regex = if config.mode == MatchMode::Regex || config.mode == MatchMode::PrefixRegex {
            Some(Regex::new(&config.location).map_err(|e| {
                MystiProxyError::Mock(format!("Invalid regex pattern: {}", e))
            })?)
        } else {
            None
        };

        // 获取响应配置，如果没有则创建默认配置
        let response = config.response.clone().unwrap_or_else(|| ResponseConfig {
            status: Some(200),
            headers: None,
            body: None,
        });

        Ok(MockLocation {
            location: config.location.clone(),
            mode: config.mode.clone(),
            regex,
            conditions: config.condition.clone().unwrap_or_default(),
            response,
        })
    }

    /// 检查路径是否匹配
    pub fn matches_path(&self, path: &str) -> bool {
        match self.mode {
            MatchMode::Full => path == self.location,
            MatchMode::Prefix => path.starts_with(&self.location),
            MatchMode::Regex => self.regex.as_ref().map(|r| r.is_match(path)).unwrap_or(false),
            MatchMode::PrefixRegex => {
                // 前缀正则匹配：先检查前缀，再用正则匹配剩余部分
                if path.starts_with(&self.location) {
                    true
                } else if let Some(regex) = &self.regex {
                    regex.is_match(path)
                } else {
                    false
                }
            }
        }
    }

    /// 检查请求是否完全匹配（路径 + 条件）
    pub fn matches_request(
        &self,
        uri: &str,
        headers: &HeaderMap,
        body: Option<&Value>,
    ) -> bool {
        // 首先检查路径匹配
        let path = if let Ok(parsed) = url::Url::parse(&format!("http://localhost{}", uri)) {
            parsed.path().to_string()
        } else {
            uri.to_string()
        };

        if !self.matches_path(&path) {
            return false;
        }

        // 然后检查条件匹配
        MockBuilder::matches_conditions(uri, headers, body, &self.conditions)
    }
}

/// Mock 服务
pub struct MockService {
    /// Mock 位置列表
    locations: Vec<MockLocation>,
}

impl MockService {
    /// 创建新的 Mock 服务
    pub fn new(locations: Vec<MockLocation>) -> Self {
        MockService { locations }
    }

    /// 从配置创建 Mock 服务
    pub fn from_configs(configs: &[LocationConfig]) -> Result<Self> {
        let mut locations = Vec::new();

        for config in configs {
            // 只处理 Mock 和 Static 类型的位置
            if config.provider == Some(crate::config::ProviderType::Mock)
                || config.provider == Some(crate::config::ProviderType::Static)
                || config.response.is_some()
            {
                locations.push(MockLocation::from_config(config)?);
            }
        }

        Ok(MockService { locations })
    }

    /// 匹配请求并返回 Mock 响应
    ///
    /// # 参数
    /// - `uri`: 请求 URI
    /// - `headers`: 请求头
    /// - `body`: 请求体（可选）
    ///
    /// # 返回
    /// 如果匹配成功返回 Some(Result<Response<BoxBody>>)，否则返回 None
    pub fn match_and_respond(
        &self,
        uri: &str,
        headers: &HeaderMap,
        body: Option<&Value>,
    ) -> Option<Result<Response<BoxBody>>> {
        // 查找匹配的 Mock 位置
        for location in &self.locations {
            if location.matches_request(uri, headers, body) {
                info!("Mock matched for URI: {}", uri);
                debug!("Mock location: {:?}", location);

                // 构建 Mock 响应
                return Some(MockBuilder::build_response(&location.response));
            }
        }

        None
    }

    /// 检查是否有匹配的 Mock 位置
    pub fn has_match(&self, uri: &str, headers: &HeaderMap, body: Option<&Value>) -> bool {
        for location in &self.locations {
            if location.matches_request(uri, headers, body) {
                return true;
            }
        }
        false
    }

    /// 获取所有 Mock 位置
    pub fn locations(&self) -> &[MockLocation] {
        &self.locations
    }

    /// 添加 Mock 位置
    pub fn add_location(&mut self, location: MockLocation) {
        self.locations.push(location);
    }

    /// 清空所有 Mock 位置
    pub fn clear(&mut self) {
        self.locations.clear();
    }
}

/// URL 编码解码辅助模块
mod urlencoding {
    /// 解码 URL 编码字符串
    pub fn decode(s: &str) -> Option<String> {
        let mut result = String::new();
        let mut chars = s.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '%' {
                let hex: String = chars.by_ref().take(2).collect();
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                } else {
                    return None;
                }
            } else if c == '+' {
                result.push(' ');
            } else {
                result.push(c);
            }
        }

        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::header::HeaderValue;
    use serde_json::json;

    #[test]
    fn test_mock_response_builder() {
        let response = MockResponse::new()
            .status(201)
            .body("test body".to_string())
            .header("Content-Type".to_string(), "application/json".to_string())
            .delay(100);

        assert_eq!(response.status, 201);
        assert_eq!(response.body, "test body");
        assert_eq!(response.headers.get("Content-Type"), Some(&"application/json".to_string()));
        assert_eq!(response.delay_ms, 100);
    }

    #[test]
    fn test_matches_uri_exact() {
        assert!(MockBuilder::matches_uri("/api/test", "/api/test"));
        assert!(!MockBuilder::matches_uri("/api/test", "/api/other"));
    }

    #[test]
    fn test_matches_uri_prefix() {
        assert!(MockBuilder::matches_uri("/api/test/123", "/api/*"));
        assert!(MockBuilder::matches_uri("/api/", "/api/*"));
        assert!(!MockBuilder::matches_uri("/other/test", "/api/*"));
    }

    #[test]
    fn test_matches_uri_regex() {
        assert!(MockBuilder::matches_uri("/api/test/123", "regex:/api/test/\\d+"));
        assert!(!MockBuilder::matches_uri("/api/test/abc", "regex:/api/test/\\d+"));
    }

    #[test]
    fn test_matches_header() {
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", HeaderValue::from_static("application/json"));
        headers.insert("Authorization", HeaderValue::from_static("Bearer token123"));

        // 存在检查
        assert!(MockBuilder::matches_header(&headers, "Content-Type"));

        // 值匹配
        assert!(MockBuilder::matches_header(&headers, "Content-Type=application/json"));
        assert!(!MockBuilder::matches_header(&headers, "Content-Type=text/html"));

        // 正则匹配
        assert!(MockBuilder::matches_header(&headers, "Authorization=regex:Bearer .*"));
        assert!(!MockBuilder::matches_header(&headers, "Authorization=regex:Basic .*"));
    }

    #[test]
    fn test_matches_query() {
        let uri = "/api/test?name=john&age=30";

        // 存在检查
        assert!(MockBuilder::matches_query(uri, "name"));

        // 值匹配
        assert!(MockBuilder::matches_query(uri, "name=john"));
        assert!(!MockBuilder::matches_query(uri, "name=jane"));

        // 正则匹配
        assert!(MockBuilder::matches_query(uri, "name=regex:j.*"));
        assert!(!MockBuilder::matches_query(uri, "name=regex:M.*"));
    }

    #[test]
    fn test_matches_body() {
        let body = json!({
            "name": "john",
            "age": 30,
            "address": {
                "city": "New York"
            }
        });

        // 存在检查
        assert!(MockBuilder::matches_body(Some(&body), "$.name"));

        // 值匹配
        assert!(MockBuilder::matches_body(Some(&body), "$.name=john"));
        assert!(!MockBuilder::matches_body(Some(&body), "$.name=jane"));

        // 嵌套字段
        assert!(MockBuilder::matches_body(Some(&body), "$.address.city=New York"));

        // 正则匹配
        assert!(MockBuilder::matches_body(Some(&body), "$.name=regex:j.*"));
    }

    #[test]
    fn test_matches_conditions() {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", HeaderValue::from_static("Bearer token123"));

        let body = json!({
            "user": "admin"
        });

        let conditions = vec![
            Condition {
                condition_type: "header".to_string(),
                value: "Authorization=Bearer token123".to_string(),
            },
            Condition {
                condition_type: "body".to_string(),
                value: "$.user=admin".to_string(),
            },
        ];

        assert!(MockBuilder::matches_conditions("/api/test", &headers, Some(&body), &conditions));

        // 测试条件不匹配
        let conditions2 = vec![
            Condition {
                condition_type: "header".to_string(),
                value: "Authorization=Basic token123".to_string(),
            },
        ];

        assert!(!MockBuilder::matches_conditions("/api/test", &headers, Some(&body), &conditions2));
    }

    #[test]
    fn test_mock_location_matches() {
        let location = MockLocation {
            location: "/api/test".to_string(),
            mode: MatchMode::Prefix,
            regex: None,
            conditions: vec![],
            response: ResponseConfig {
                status: Some(200),
                headers: None,
                body: None,
            },
        };

        assert!(location.matches_path("/api/test"));
        assert!(location.matches_path("/api/test/123"));
        assert!(!location.matches_path("/api/other"));
    }

    #[test]
    fn test_mock_service() {
        let location = MockLocation {
            location: "/api/mock".to_string(),
            mode: MatchMode::Full,
            regex: None,
            conditions: vec![],
            response: ResponseConfig {
                status: Some(201),
                headers: None,
                body: None,
            },
        };

        let service = MockService::new(vec![location]);

        let headers = HeaderMap::new();

        // 测试匹配
        let result = service.match_and_respond("/api/mock", &headers, None);
        assert!(result.is_some());

        // 测试不匹配
        let result = service.match_and_respond("/api/other", &headers, None);
        assert!(result.is_none());
    }

    #[test]
    fn test_build_response() {
        let config = ResponseConfig {
            status: Some(201),
            headers: Some({
                let mut map = HashMap::new();
                map.insert(
                    "Content-Type".to_string(),
                    crate::config::HeaderAction {
                        value: "application/json".to_string(),
                        action: HeaderActionType::Overwrite,
                        condition: None,
                    },
                );
                map
            }),
            body: None,
        };

        let response = MockBuilder::build_response(&config).unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "application/json"
        );
    }

    #[test]
    fn test_get_value_by_path() {
        let body = json!({
            "user": {
                "name": "john",
                "age": 30,
                "tags": ["admin", "user"]
            }
        });

        // 简单路径
        let value = MockBuilder::get_value_by_path(&body, "$.user.name").unwrap();
        assert_eq!(value, "john");

        // 嵌套路径
        let value = MockBuilder::get_value_by_path(&body, "$.user.age").unwrap();
        assert_eq!(value, 30);

        // 数组访问
        let value = MockBuilder::get_value_by_path(&body, "$.user.tags[0]").unwrap();
        assert_eq!(value, "admin");
    }

    #[test]
    fn test_urlencoding_decode() {
        assert_eq!(urlencoding::decode("hello%20world"), Some("hello world".to_string()));
        assert_eq!(urlencoding::decode("name=john&age=30"), Some("name=john&age=30".to_string()));
        assert_eq!(urlencoding::decode("a+b"), Some("a b".to_string()));
    }
}
