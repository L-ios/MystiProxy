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

        // 检查内部网络访问
        if let Some(host) = parsed.host_str() {
            use std::str::FromStr;
            let ip_addr: std::net::IpAddr = match std::net::IpAddr::from_str(host) {
                Ok(ip) => ip,
                Err(_) => {
                    // 不是 IP 地址，跳过内网检查
                    return Ok(());
                }
            };
            for network in &self.internal_networks {
                if network.contains(ip_addr) {
                    return Err(ConfigValidationError::Security(
                        "access to internal network addresses is blocked".to_string(),
                    ));
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
        let sensitive_patterns = [
            r"(?i)password\s*[:=]\s*\S+",
            r"(?i)secret\s*[:=]\s*\S+",
            r"(?i)api[_-]?key\s*[:=]\s*\S+",
            r"(?i)token\s*[:=]\s*\S+",
            r"(?i)private[_-]?key\s*[:=]\s*\S+",
        ];

        for pattern in &sensitive_patterns {
            let regex = Regex::new(pattern).unwrap();
            if regex.is_match(config) {
                warn!(
                    "Configuration may contain sensitive information matching pattern: {}",
                    pattern
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
}
