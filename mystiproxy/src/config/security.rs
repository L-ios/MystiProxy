//! 安全验证器

use ipnetwork::IpNetwork;
use regex::Regex;
use std::collections::HashMap;
use tracing::warn;

use crate::config::validation::{ConfigValidationError, ValidationResult};

/// 危险的 HTTP 头部（不应由用户设置）
const DANGEROUS_HEADERS: &[&str] = &[
    "content-length",
    "transfer-encoding",
    "connection",
    "upgrade",
    "host",
    "authorization",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
];

/// 内部网络段（用于 SSRF 防护）
const INTERNAL_NETWORKS: &[&str] = &[
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "127.0.0.0/8",
    "169.254.0.0/16", // link-local
    "::1/128",        // IPv6 loopback
    "fe80::/10",      // IPv6 link-local
    "fc00::/7",       // IPv6 unique local
];

/// URL 黑名单模式
const URL_BLACKLIST_PATTERNS: &[&str] = &[
    r"(?i)file://",
    r"(?i)data:",
    r"(?i)ftp://",
    r"(?i)gopher://",
    r"(?i)ldap://",
    r"(?i)dict://",
];

/// 安全验证器
pub struct SecurityValidator {
    dangerous_headers: Vec<String>,
    internal_networks: Vec<IpNetwork>,
    url_blacklist_patterns: Vec<Regex>,
}

impl Default for SecurityValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl SecurityValidator {
    /// 创建新的安全验证器
    pub fn new() -> Self {
        Self {
            dangerous_headers: DANGEROUS_HEADERS.iter().map(|s| s.to_string()).collect(),
            internal_networks: INTERNAL_NETWORKS
                .iter()
                .filter_map(|s| s.parse().ok())
                .collect(),
            url_blacklist_patterns: URL_BLACKLIST_PATTERNS
                .iter()
                .filter_map(|s| Regex::new(s).ok())
                .collect(),
        }
    }

    /// 验证 HTTP 头部安全性
    pub fn validate_headers(&self, headers: &HashMap<String, String>) -> ValidationResult<()> {
        for (name, _value) in headers {
            let normalized = name.to_lowercase();
            if self.dangerous_headers.contains(&normalized) {
                return Err(ConfigValidationError::Security(format!(
                    "dangerous header '{}' is not allowed",
                    name
                )));
            }
        }
        Ok(())
    }

    /// 验证目标 URL 安全性（SSRF 防护）
    pub fn validate_target_url(&self, url: &str) -> ValidationResult<()> {
        // 检查黑名单模式
        for pattern in &self.url_blacklist_patterns {
            if pattern.is_match(url) {
                return Err(ConfigValidationError::Security(format!(
                    "URL matches blacklisted pattern: {}",
                    pattern
                )));
            }
        }

        // 解析 URL
        let parsed = url::Url::parse(url)
            .map_err(|e| ConfigValidationError::Security(format!("invalid URL: {}", e)))?;

        // 检查内部网络访问：使用 Host 枚举以正确处理 IPv6（host_str 带 [] 无法直接解析）
        if let Some(host) = parsed.host() {
            let ip_addr: Option<std::net::IpAddr> = match host {
                url::Host::Ipv4(v4) => Some(std::net::IpAddr::V4(v4)),
                url::Host::Ipv6(v6) => Some(std::net::IpAddr::V6(v6)),
                url::Host::Domain(_) => None, // 域名：跳过内网检查
            };
            if let Some(ip) = ip_addr {
                for network in &self.internal_networks {
                    if network.contains(ip) {
                        return Err(ConfigValidationError::Security(
                            "access to internal network addresses is blocked".to_string(),
                        ));
                    }
                }
            }
        }

        // 限制协议
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(ConfigValidationError::Security(
                "only HTTP and HTTPS protocols are allowed".to_string(),
            ));
        }

        Ok(())
    }

    /// 验证 CIDR 列表是否包含内网地址
    pub fn validate_cidr_list(&self, cidrs: &[String]) -> ValidationResult<()> {
        for cidr in cidrs {
            let network: IpNetwork = cidr.parse().map_err(|e| {
                ConfigValidationError::Security(format!("invalid CIDR '{}': {}", cidr, e))
            })?;

            // 检查是否过于宽泛（如 0.0.0.0/0）
            if network.prefix() == 0 {
                return Err(ConfigValidationError::Security(
                    "overly permissive CIDR (0.0.0.0/0 or ::/0) is not allowed".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// 检查配置中的敏感信息
    pub fn check_sensitive_config(&self, config: &str) -> ValidationResult<()> {
        // 检查是否包含类似密钥的模式
        const SENSITIVE_PATTERNS: &[&str] = &[
            r"(?i)password\s*[:=]\s*\S+",
            r"(?i)secret\s*[:=]\s*\S+",
            r"(?i)api[_-]?key\s*[:=]\s*\S+",
            r"(?i)token\s*[:=]\s*\S+",
            r"(?i)private[_-]?key\s*[:=]\s*\S+",
        ];

        // 预编译正则，编译失败即视为配置校验错误而非 panic
        let compiled: Vec<Regex> = SENSITIVE_PATTERNS
            .iter()
            .map(|p| Regex::new(p))
            .collect::<Result<_, _>>()
            .map_err(|e| {
                ConfigValidationError::Security(format!(
                    "failed to compile sensitive pattern: {}",
                    e
                ))
            })?;

        for (i, regex) in compiled.iter().enumerate() {
            if regex.is_match(config) {
                warn!(
                    "Configuration may contain sensitive information matching pattern: {}",
                    SENSITIVE_PATTERNS[i]
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ---------- T1 SecurityValidator 构造 ----------

    #[test]
    fn test_new_creates_validator_with_defaults() {
        let v = SecurityValidator::new();
        assert!(!v.dangerous_headers.is_empty());
        assert!(!v.internal_networks.is_empty());
        assert!(!v.url_blacklist_patterns.is_empty());
    }

    #[test]
    fn test_default_matches_new() {
        let new = SecurityValidator::new();
        let def: SecurityValidator = Default::default();
        assert_eq!(new.dangerous_headers.len(), def.dangerous_headers.len());
        assert_eq!(new.internal_networks.len(), def.internal_networks.len());
    }

    #[test]
    fn test_all_dangerous_headers_are_lowercase_normalized() {
        // 构造器里把 DANGEROUS_HEADERS 全部 lowercase 化
        let v = SecurityValidator::new();
        for h in &v.dangerous_headers {
            assert_eq!(h.to_lowercase(), h.clone());
        }
    }

    // ---------- T2 危险头部校验 ----------

    #[test]
    fn test_validate_headers() {
        let validator = SecurityValidator::new();
        let mut headers = HashMap::new();

        // 正常头部
        headers.insert("X-Custom-Header".to_string(), "value".to_string());
        assert!(validator.validate_headers(&headers).is_ok());

        // 危险头部
        headers.insert("Content-Length".to_string(), "100".to_string());
        assert!(validator.validate_headers(&headers).is_err());
    }

    #[test]
    fn test_validate_headers_empty_is_ok() {
        let v = SecurityValidator::new();
        let headers: HashMap<String, String> = HashMap::new();
        assert!(v.validate_headers(&headers).is_ok());
    }

    #[test]
    fn test_validate_headers_case_insensitive() {
        let v = SecurityValidator::new();
        // 大小写变体都应被识别为危险头部
        for variant in ["AUTHORIZATION", "Authorization", "authorization"] {
            let mut h = HashMap::new();
            h.insert(variant.to_string(), "Bearer x".to_string());
            assert!(
                v.validate_headers(&h).is_err(),
                "{variant} 应被识别为危险头部"
            );
        }
    }

    #[test]
    fn test_validate_headers_each_dangerous_one_by_one() {
        let v = SecurityValidator::new();
        // 列表里每个危险头部单独命中都应失败
        for name in &v.dangerous_headers {
            let mut h = HashMap::new();
            h.insert(name.clone(), "v".to_string());
            assert!(v.validate_headers(&h).is_err(), "{name} 应被拦截");
        }
    }

    #[test]
    fn test_validate_headers_error_message_contains_header_name() {
        let v = SecurityValidator::new();
        let mut h = HashMap::new();
        h.insert("Host".to_string(), "evil".to_string());
        let err = v.validate_headers(&h).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Host"), "错误消息应包含头部名: {msg}");
    }

    // ---------- T3 SSRF 防护（validate_target_url） ----------

    #[test]
    fn test_validate_target_url() {
        let validator = SecurityValidator::new();

        // 合法外部 URL
        assert!(validator
            .validate_target_url("https://api.example.com")
            .is_ok());
        assert!(validator
            .validate_target_url("http://example.com/path")
            .is_ok());

        // 内部地址应被拦截
        assert!(validator
            .validate_target_url("http://127.0.0.1/admin")
            .is_err());
        assert!(validator
            .validate_target_url("http://192.168.1.1/api")
            .is_err());
        assert!(validator
            .validate_target_url("http://10.0.0.1/internal")
            .is_err());

        // 危险协议
        assert!(validator.validate_target_url("file:///etc/passwd").is_err());
        assert!(validator
            .validate_target_url("data:text/html,<script>")
            .is_err());
    }

    #[test]
    fn test_validate_target_url_ipv6_loopback_blocked() {
        let v = SecurityValidator::new();
        assert!(v.validate_target_url("http://[::1]/x").is_err());
    }

    #[test]
    fn test_validate_target_url_ipv6_link_local_blocked() {
        let v = SecurityValidator::new();
        assert!(v.validate_target_url("http://[fe80::1]/x").is_err());
    }

    #[test]
    fn test_validate_target_url_hostname_skips_internal_check() {
        // 主机名（非 IP）不触发内网检查，应放行
        let v = SecurityValidator::new();
        assert!(v.validate_target_url("https://example.com").is_ok());
    }

    #[test]
    fn test_validate_target_url_invalid_url_returns_err() {
        let v = SecurityValidator::new();
        assert!(v.validate_target_url("not a url").is_err());
    }

    #[test]
    fn test_validate_target_url_ftp_blocked_by_blacklist() {
        let v = SecurityValidator::new();
        assert!(v.validate_target_url("ftp://evil.com").is_err());
    }

    #[test]
    fn test_validate_target_url_gopher_blocked() {
        let v = SecurityValidator::new();
        assert!(v.validate_target_url("gopher://evil.com").is_err());
    }

    #[test]
    fn test_validate_target_url_169_254_blocked() {
        // link-local 169.254.0.0/16
        let v = SecurityValidator::new();
        assert!(v.validate_target_url("http://169.254.1.1/x").is_err());
    }

    #[test]
    fn test_validate_target_url_fc00_ipv6_ula_blocked() {
        // IPv6 unique local fc00::/7
        let v = SecurityValidator::new();
        assert!(v.validate_target_url("http://[fc00::1]/x").is_err());
    }

    // ---------- T4 CIDR 列表校验 ----------

    #[test]
    fn test_validate_cidr_list() {
        let validator = SecurityValidator::new();

        // 正常 CIDR
        assert!(validator
            .validate_cidr_list(&["10.0.0.0/8".to_string()])
            .is_ok());
        assert!(validator
            .validate_cidr_list(&["192.168.1.0/24".to_string()])
            .is_ok());

        // 过于宽泛的 CIDR
        assert!(validator
            .validate_cidr_list(&["0.0.0.0/0".to_string()])
            .is_err());
        assert!(validator.validate_cidr_list(&["::/0".to_string()]).is_err());
    }

    #[test]
    fn test_validate_cidr_list_empty_is_ok() {
        let v = SecurityValidator::new();
        assert!(v.validate_cidr_list(&[]).is_ok());
    }

    #[test]
    fn test_validate_cidr_list_invalid_cidr_returns_err() {
        let v = SecurityValidator::new();
        assert!(v.validate_cidr_list(&["not-a-cidr".to_string()]).is_err());
    }

    #[test]
    fn test_validate_cidr_list_error_contains_value() {
        let v = SecurityValidator::new();
        let err = v
            .validate_cidr_list(&["0.0.0.0/0".to_string()])
            .unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("0.0.0.0/0"), "错误消息应含原值: {msg}");
    }

    // ---------- T5 敏感信息检查 ----------

    #[test]
    fn test_check_sensitive_config_clean_returns_ok() {
        let v = SecurityValidator::new();
        let clean = "listen: tcp://0.0.0.0:8080\ntarget: tcp://127.0.0.1:80\n";
        assert!(v.check_sensitive_config(clean).is_ok());
    }

    #[test]
    fn test_check_sensitive_config_empty_returns_ok() {
        let v = SecurityValidator::new();
        assert!(v.check_sensitive_config("").is_ok());
    }

    #[test]
    fn test_check_sensitive_config_password_pattern_returns_ok_with_warning() {
        // 敏感信息只 warn 不 fail（仍返回 Ok），保证配置加载不被阻断
        let v = SecurityValidator::new();
        let with_pw = "password: supersecret\n";
        assert!(v.check_sensitive_config(with_pw).is_ok());
    }

    #[test]
    fn test_check_sensitive_config_secret_pattern() {
        let v = SecurityValidator::new();
        assert!(v.check_sensitive_config("secret: my-key-value\n").is_ok());
    }

    #[test]
    fn test_check_sensitive_config_api_key_pattern() {
        let v = SecurityValidator::new();
        assert!(v.check_sensitive_config("api_key: abc123").is_ok());
    }

    #[test]
    fn test_check_sensitive_config_token_pattern() {
        let v = SecurityValidator::new();
        assert!(v.check_sensitive_config("token: bearer-xyz").is_ok());
    }

    #[test]
    fn test_check_sensitive_config_private_key_pattern() {
        let v = SecurityValidator::new();
        assert!(v
            .check_sensitive_config("private_key: -----BEGIN-----")
            .is_ok());
    }
}
