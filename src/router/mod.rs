//! 路由模块

use crate::config::{LocationConfig, MatchMode};
use regex::Regex;
use std::collections::HashMap;

/// 匹配结果
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// 匹配模式
    pub mode: MatchMode,
    /// 提取的参数（用于 Regex 和 PrefixRegex 模式）
    pub params: HashMap<String, String>,
    /// 剩余路径（用于 Prefix 和 PrefixRegex 模式）
    pub remaining: Option<String>,
}

impl MatchResult {
    /// 创建全匹配结果
    pub fn full() -> Self {
        MatchResult {
            mode: MatchMode::Full,
            params: HashMap::new(),
            remaining: None,
        }
    }

    /// 创建前缀匹配结果
    pub fn prefix(remaining: String) -> Self {
        MatchResult {
            mode: MatchMode::Prefix,
            params: HashMap::new(),
            remaining: Some(remaining),
        }
    }

    /// 创建正则匹配结果
    pub fn regex(params: HashMap<String, String>) -> Self {
        MatchResult {
            mode: MatchMode::Regex,
            params,
            remaining: None,
        }
    }

    /// 创建前缀正则匹配结果
    pub fn prefix_regex(params: HashMap<String, String>, remaining: String) -> Self {
        MatchResult {
            mode: MatchMode::PrefixRegex,
            params,
            remaining: Some(remaining),
        }
    }
}

/// 路由规则
#[derive(Debug)]
pub struct Route {
    /// 路由模式
    pub pattern: String,
    /// 匹配模式
    pub mode: MatchMode,
    /// 位置配置
    pub location_config: LocationConfig,
    /// 编译后的正则表达式（用于 Regex 和 PrefixRegex 模式）
    compiled_regex: Option<Regex>,
}

impl Route {
    /// 创建新的路由规则
    pub fn new(pattern: String, mode: MatchMode, location_config: LocationConfig) -> crate::Result<Self> {
        let compiled_regex = if mode == MatchMode::Regex || mode == MatchMode::PrefixRegex {
            Some(pattern_to_regex(&pattern, mode == MatchMode::Regex)?)
        } else {
            None
        };

        Ok(Route {
            pattern,
            mode,
            location_config,
            compiled_regex,
        })
    }
}

/// 路由器
#[derive(Debug)]
pub struct Router {
    routes: Vec<Route>,
}

impl Router {
    /// 创建新的路由器
    pub fn new() -> Self {
        Router {
            routes: Vec::new(),
        }
    }

    /// 添加路由规则
    pub fn add_route(&mut self, route: Route) {
        self.routes.push(route);
    }

    /// 匹配 URI
    pub fn match_uri(&self, uri: &str) -> Option<(&Route, MatchResult)> {
        for route in &self.routes {
            if let Some(result) = self.match_route(route, uri) {
                return Some((route, result));
            }
        }
        None
    }

    /// 根据路由模式匹配 URI
    fn match_route(&self, route: &Route, uri: &str) -> Option<MatchResult> {
        match route.mode {
            MatchMode::Full => self.match_full(route, uri),
            MatchMode::Prefix => self.match_prefix(route, uri),
            MatchMode::Regex => self.match_regex(route, uri),
            MatchMode::PrefixRegex => self.match_prefix_regex(route, uri),
        }
    }

    /// 全路径匹配
    /// 当 baseUri = /a/b/c, in_uri = /a/b/c 时，返回 Some(MatchResult::Full)
    /// 当 baseUri = /a/b/c, in_uri = /a/b/c/d 时，返回 None
    fn match_full(&self, route: &Route, uri: &str) -> Option<MatchResult> {
        if route.pattern == uri {
            Some(MatchResult::full())
        } else {
            None
        }
    }

    /// 前缀匹配
    /// 当 baseUri = /, in_uri = /a/b/c 时，返回 Some(MatchResult::Prefix { remaining: "a/b/c" })
    /// 当 baseUri = /a/b/c/, in_uri = /a/b/c/d/e 时，返回 Some(MatchResult::Prefix { remaining: "d/e" })
    fn match_prefix(&self, route: &Route, uri: &str) -> Option<MatchResult> {
        let pattern = route.pattern.as_str();
        
        // 检查是否是前缀匹配
        if !uri.starts_with(pattern) {
            return None;
        }

        // 获取剩余部分
        let remaining = uri[pattern.len()..].to_string();
        
        // 如果 pattern 以 / 结尾，remaining 应该去掉开头的 /
        // 但如果 pattern 不以 / 结尾，且 remaining 不为空，需要检查是否合理
        let remaining = if pattern.ends_with('/') {
            remaining
        } else {
            // 如果 pattern 不以 / 结尾，remaining 应该为空或以 / 开头
            if remaining.is_empty() {
                remaining
            } else if remaining.starts_with('/') {
                remaining[1..].to_string()
            } else {
                // 不合理的情况，例如 pattern = /a/b, uri = /a/bc
                return None;
            }
        };

        Some(MatchResult::prefix(remaining))
    }

    /// 正则匹配
    /// 当 baseUri = /a/{id}/c, in_uri = /a/b/c 时，返回 Some(MatchResult::Regex { params: {"id": "b"} })
    fn match_regex(&self, route: &Route, uri: &str) -> Option<MatchResult> {
        let regex = route.compiled_regex.as_ref()?;
        
        if let Some(captures) = regex.captures(uri) {
            let mut params = HashMap::new();
            
            // 提取所有命名捕获组
            for name in regex.capture_names() {
                if let Some(name) = name {
                    if let Some(value) = captures.name(name) {
                        params.insert(name.to_string(), value.as_str().to_string());
                    }
                }
            }
            
            // 检查是否完全匹配（整个 URI）
            if captures.get(0).map(|m| m.as_str()) == Some(uri) {
                Some(MatchResult::regex(params))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// 前缀正则匹配
    /// 当 baseUri = /a/{id}/c/, in_uri = /a/b/c/d/e 时，返回 Some(MatchResult::PrefixRegex { params: {"id": "b"}, remaining: "d/e" })
    fn match_prefix_regex(&self, route: &Route, uri: &str) -> Option<MatchResult> {
        let regex = route.compiled_regex.as_ref()?;
        
        if let Some(captures) = regex.captures(uri) {
            let mut params = HashMap::new();
            
            // 提取所有命名捕获组
            for name in regex.capture_names() {
                if let Some(name) = name {
                    if let Some(value) = captures.name(name) {
                        params.insert(name.to_string(), value.as_str().to_string());
                    }
                }
            }
            
            // 获取匹配的前缀部分
            let matched = captures.get(0)?.as_str();
            
            // 确保匹配的是前缀
            if !uri.starts_with(matched) {
                return None;
            }
            
            // 获取剩余部分
            let remaining = uri[matched.len()..].to_string();
            
            // 去掉开头的 /
            let remaining = if remaining.starts_with('/') {
                remaining[1..].to_string()
            } else {
                remaining
            };
            
            Some(MatchResult::prefix_regex(params, remaining))
        } else {
            None
        }
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

/// 将 {param} 模式转换为正则表达式
/// /a/{id}/c -> ^/a/(?P<id>[^/]+)/c$
/// /a/{id}/c/ -> ^/a/(?P<id>[^/]+)/c/
/// is_exact: 是否为完全匹配（添加 $ 结束符）
pub fn pattern_to_regex(pattern: &str, is_exact: bool) -> crate::Result<Regex> {
    let mut regex_str = String::from("^");
    let mut chars = pattern.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '{' {
            // 开始参数名
            let mut param_name = String::new();
            while let Some(&next) = chars.peek() {
                if next == '}' {
                    chars.next(); // 消费 '}'
                    break;
                }
                param_name.push(chars.next().unwrap());
            }
            
            // 添加命名捕获组，匹配非 / 的字符
            regex_str.push_str(&format!(r"(?P<{}>[^/]+)", param_name));
        } else {
            // 转义正则特殊字符
            match c {
                '.' | '^' | '$' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '\\' => {
                    regex_str.push('\\');
                    regex_str.push(c);
                }
                _ => regex_str.push(c),
            }
        }
    }
    
    // 对于完全匹配，添加 $ 结束符
    if is_exact {
        regex_str.push('$');
    }
    
    Regex::new(&regex_str).map_err(|e| crate::MystiProxyError::InvalidRegex(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{MatchMode, ProviderType};

    fn create_test_location_config() -> LocationConfig {
        LocationConfig {
            location: "/test".to_string(),
            mode: MatchMode::Full,
            provider: Some(ProviderType::Static),
            alias: None,
            condition: None,
            response: None,
            request: None,
        }
    }

    #[test]
    fn test_full_match() {
        let mut router = Router::new();
        let route = Route::new(
            "/a/b/c".to_string(),
            MatchMode::Full,
            create_test_location_config(),
        ).unwrap();
        router.add_route(route);

        // 完全匹配
        let result = router.match_uri("/a/b/c");
        assert!(result.is_some());
        let (_, match_result) = result.unwrap();
        assert_eq!(match_result.mode, MatchMode::Full);
        assert!(match_result.params.is_empty());
        assert!(match_result.remaining.is_none());

        // 不匹配（有额外路径）
        let result = router.match_uri("/a/b/c/d");
        assert!(result.is_none());

        // 不匹配（路径不同）
        let result = router.match_uri("/a/b");
        assert!(result.is_none());
    }

    #[test]
    fn test_prefix_match() {
        let mut router = Router::new();
        
        // 测试根路径前缀
        let route1 = Route::new(
            "/".to_string(),
            MatchMode::Prefix,
            create_test_location_config(),
        ).unwrap();
        router.add_route(route1);

        let result = router.match_uri("/a/b/c");
        assert!(result.is_some());
        let (_, match_result) = result.unwrap();
        assert_eq!(match_result.mode, MatchMode::Prefix);
        assert_eq!(match_result.remaining, Some("a/b/c".to_string()));

        // 测试带尾部斜杠的前缀
        let mut router2 = Router::new();
        let route2 = Route::new(
            "/a/b/c/".to_string(),
            MatchMode::Prefix,
            create_test_location_config(),
        ).unwrap();
        router2.add_route(route2);

        let result = router2.match_uri("/a/b/c/d/e");
        assert!(result.is_some());
        let (_, match_result) = result.unwrap();
        assert_eq!(match_result.mode, MatchMode::Prefix);
        assert_eq!(match_result.remaining, Some("d/e".to_string()));

        // 测试不匹配的情况
        let result = router2.match_uri("/a/b/d/e");
        assert!(result.is_none());
    }

    #[test]
    fn test_regex_match() {
        let mut router = Router::new();
        let route = Route::new(
            "/a/{id}/c".to_string(),
            MatchMode::Regex,
            create_test_location_config(),
        ).unwrap();
        router.add_route(route);

        // 匹配并提取参数
        let result = router.match_uri("/a/b/c");
        assert!(result.is_some());
        let (_, match_result) = result.unwrap();
        assert_eq!(match_result.mode, MatchMode::Regex);
        assert_eq!(match_result.params.get("id"), Some(&"b".to_string()));
        assert!(match_result.remaining.is_none());

        // 不匹配（有额外路径）
        let result = router.match_uri("/a/b/c/d");
        assert!(result.is_none());

        // 不匹配（路径格式不同）
        let result = router.match_uri("/a/b/d");
        assert!(result.is_none());
    }

    #[test]
    fn test_prefix_regex_match() {
        let mut router = Router::new();
        let route = Route::new(
            "/a/{id}/c/".to_string(),
            MatchMode::PrefixRegex,
            create_test_location_config(),
        ).unwrap();
        router.add_route(route);

        // 匹配并提取参数和剩余路径
        let result = router.match_uri("/a/b/c/d/e");
        assert!(result.is_some());
        let (_, match_result) = result.unwrap();
        assert_eq!(match_result.mode, MatchMode::PrefixRegex);
        assert_eq!(match_result.params.get("id"), Some(&"b".to_string()));
        assert_eq!(match_result.remaining, Some("d/e".to_string()));

        // 匹配但没有剩余路径
        let result = router.match_uri("/a/b/c/");
        assert!(result.is_some());
        let (_, match_result) = result.unwrap();
        assert_eq!(match_result.params.get("id"), Some(&"b".to_string()));
        assert_eq!(match_result.remaining, Some("".to_string()));

        // 不匹配
        let result = router.match_uri("/a/b/d/e");
        assert!(result.is_none());
    }

    #[test]
    fn test_pattern_to_regex() {
        // 简单参数（完全匹配）
        let regex = pattern_to_regex("/a/{id}/c", true).unwrap();
        assert!(regex.is_match("/a/b/c"));
        assert!(!regex.is_match("/a/b/c/d"));

        // 多个参数（完全匹配）
        let regex = pattern_to_regex("/a/{id}/b/{name}", true).unwrap();
        let captures = regex.captures("/a/123/b/test").unwrap();
        assert_eq!(captures.name("id").unwrap().as_str(), "123");
        assert_eq!(captures.name("name").unwrap().as_str(), "test");

        // 带特殊字符的路径（完全匹配）
        let regex = pattern_to_regex("/api/v1/users/{id}", true).unwrap();
        assert!(regex.is_match("/api/v1/users/123"));
        assert!(regex.is_match("/api/v1/users/abc"));
        
        // 前缀匹配（不添加 $）
        let regex = pattern_to_regex("/a/{id}/c/", false).unwrap();
        assert!(regex.is_match("/a/b/c/d/e"));
        assert!(regex.is_match("/a/b/c/"));
    }

    #[test]
    fn test_multiple_params() {
        let mut router = Router::new();
        let route = Route::new(
            "/users/{user_id}/posts/{post_id}".to_string(),
            MatchMode::Regex,
            create_test_location_config(),
        ).unwrap();
        router.add_route(route);

        let result = router.match_uri("/users/123/posts/456");
        assert!(result.is_some());
        let (_, match_result) = result.unwrap();
        assert_eq!(match_result.params.get("user_id"), Some(&"123".to_string()));
        assert_eq!(match_result.params.get("post_id"), Some(&"456".to_string()));
    }

    #[test]
    fn test_router_priority() {
        let mut router = Router::new();
        
        // 添加多个路由
        let route1 = Route::new(
            "/a/b/c".to_string(),
            MatchMode::Full,
            create_test_location_config(),
        ).unwrap();
        router.add_route(route1);

        let route2 = Route::new(
            "/a/".to_string(),
            MatchMode::Prefix,
            create_test_location_config(),
        ).unwrap();
        router.add_route(route2);

        // 完全匹配优先
        let result = router.match_uri("/a/b/c");
        assert!(result.is_some());
        let (_, match_result) = result.unwrap();
        assert_eq!(match_result.mode, MatchMode::Full);

        // 前缀匹配
        let result = router.match_uri("/a/d/e");
        assert!(result.is_some());
        let (_, match_result) = result.unwrap();
        assert_eq!(match_result.mode, MatchMode::Prefix);
        assert_eq!(match_result.remaining, Some("d/e".to_string()));
    }

    #[test]
    fn test_empty_remaining() {
        let mut router = Router::new();
        let route = Route::new(
            "/a/b/c/".to_string(),
            MatchMode::Prefix,
            create_test_location_config(),
        ).unwrap();
        router.add_route(route);

        let result = router.match_uri("/a/b/c/");
        assert!(result.is_some());
        let (_, match_result) = result.unwrap();
        assert_eq!(match_result.remaining, Some("".to_string()));
    }
}
