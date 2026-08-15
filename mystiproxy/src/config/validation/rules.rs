//! 配置验证规则实现

use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use url::Url;
use validator::{ValidationError, ValidationErrors};

use crate::config::{EngineConfig, LocationConfig, MatchMode, ProviderType, ProxyType, TlsConfig};

/// 验证 EngineConfig
pub fn validate_engine_config(config: &EngineConfig) -> Result<(), ValidationErrors> {
    let mut errors = ValidationErrors::new();

    // 验证 listen 地址
    if let Err(e) = validate_listen_address(&config.listen) {
        errors.add("listen", e);
    }

    // 验证 target 地址
    if let Err(e) = validate_target_address(&config.target) {
        errors.add("target", e);
    }

    // 验证代理类型匹配
    if let Err(e) =
        validate_proxy_type_match(&config.listen, &config.target, config.proxy_type.clone())
    {
        errors.add("proxy_type", e);
    }

    // 验证 locations
    if let Some(locations) = &config.locations {
        for (i, loc) in locations.iter().enumerate() {
            if let Err(e) = validate_location_config(loc) {
                // 使用静态字段名，但在错误消息中包含索引
                let mut e = e;
                e.message =
                    Some(format!("location[{}]: {}", i, e.message.unwrap_or_default()).into());
                errors.add("locations", e);
            }
        }
    }

    // 验证 TLS 配置
    if let Some(tls) = &config.tls {
        if let Err(e) = validate_tls_config(tls) {
            errors.add("tls", e);
        }
    }

    // 验证认证配置
    if let Some(auth) = &config.auth {
        if let Err(e) = validate_auth_config(auth) {
            errors.add("auth", e);
        }
    }

    // 验证上游代理
    if let Some(upstream) = &config.upstream {
        if let Err(e) = validate_upstream_proxy(upstream) {
            errors.add("upstream", e);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// 验证监听地址
fn validate_listen_address(listen: &str) -> Result<(), ValidationError> {
    if listen.is_empty() {
        return Err(ValidationError::new("listen_empty"));
    }

    // 支持 tcp:// 和 unix://
    if listen.starts_with("tcp://") {
        let addr = &listen[6..];
        SocketAddr::from_str(addr).map_err(|_| ValidationError::new("invalid_tcp_address"))?;
    } else if listen.starts_with("unix://") {
        let path = &listen[7..];
        if path.is_empty() {
            return Err(ValidationError::new("empty_unix_socket_path"));
        }
    } else {
        return Err(ValidationError::new("unsupported_protocol"));
    }

    Ok(())
}

/// 验证目标地址
fn validate_target_address(target: &str) -> Result<(), ValidationError> {
    if target.is_empty() {
        return Err(ValidationError::new("target_empty"));
    }

    if target.starts_with("tcp://") {
        let addr = &target[6..];
        SocketAddr::from_str(addr).map_err(|_| ValidationError::new("invalid_tcp_target"))?;
    } else if target.starts_with("unix://") {
        let path = &target[7..];
        if path.is_empty() {
            return Err(ValidationError::new("empty_unix_target_path"));
        }
    } else if target.starts_with("http://") || target.starts_with("https://") {
        // HTTP/HTTPS 目标用于 Forward 代理
        Url::parse(target).map_err(|_| ValidationError::new("invalid_http_target"))?;
    } else {
        return Err(ValidationError::new("unsupported_target_protocol"));
    }

    Ok(())
}

/// 验证代理类型与地址匹配
fn validate_proxy_type_match(
    listen: &str,
    target: &str,
    proxy_type: ProxyType,
) -> Result<(), ValidationError> {
    match proxy_type {
        ProxyType::Tcp => {
            if !listen.starts_with("tcp://") || !target.starts_with("tcp://") {
                return Err(ValidationError::new("tcp_proxy_requires_tcp_addresses"));
            }
        }
        ProxyType::Http => {
            if !listen.starts_with("tcp://") && !listen.starts_with("unix://") {
                return Err(ValidationError::new(
                    "http_proxy_requires_tcp_or_unix_listen",
                ));
            }
            if !target.starts_with("tcp://") && !target.starts_with("unix://") {
                return Err(ValidationError::new(
                    "http_proxy_requires_tcp_or_unix_target",
                ));
            }
        }
        ProxyType::Forward => {
            if !listen.starts_with("tcp://") {
                return Err(ValidationError::new("forward_proxy_requires_tcp_listen"));
            }
            // Forward 代理的 target 可以是 http/https（上游代理）
        }
    }
    Ok(())
}

/// 验证 Location 配置
fn validate_location_config(loc: &LocationConfig) -> Result<(), ValidationError> {
    if loc.location.is_empty() {
        return Err(ValidationError::new("empty_location_path"));
    }

    // 验证匹配模式与路径一致性
    match loc.mode {
        MatchMode::Full => {
            // 完全匹配不需要特殊处理
        }
        MatchMode::Prefix => {
            if !loc.location.starts_with('/') {
                return Err(ValidationError::new(
                    "prefix_location_must_start_with_slash",
                ));
            }
        }
        MatchMode::Regex => {
            regex::Regex::new(&loc.location)
                .map_err(|_| ValidationError::new("invalid_regex_pattern"))?;
        }
        MatchMode::PrefixRegex => {
            if !loc.location.starts_with('/') {
                return Err(ValidationError::new("prefix_regex_must_start_with_slash"));
            }
            // 后缀部分应该是合法正则
            let regex_part = &loc.location[1..];
            regex::Regex::new(regex_part)
                .map_err(|_| ValidationError::new("invalid_prefix_regex_pattern"))?;
        }
    }

    // 验证 provider 与响应/请求配置一致性
    if let Some(provider) = &loc.provider {
        match provider {
            ProviderType::Mock => {
                if loc.response.is_none() {
                    return Err(ValidationError::new("mock_provider_requires_response"));
                }
            }
            ProviderType::Static => {
                if loc.root.is_none() {
                    return Err(ValidationError::new("static_provider_requires_root"));
                }
            }
            ProviderType::Proxy => {
                // 代理 provider 使用默认转发，无额外要求
            }
        }
    }

    // 验证静态文件配置
    if loc.provider == Some(ProviderType::Static) {
        if let Some(root) = &loc.root {
            if root.is_empty() {
                return Err(ValidationError::new("static_root_empty"));
            }
        }
    }

    Ok(())
}

/// 验证 TLS 配置
fn validate_tls_config(tls: &TlsConfig) -> Result<(), ValidationError> {
    if tls.cert_path.is_empty() {
        return Err(ValidationError::new("tls_cert_path_empty"));
    }
    if tls.key_path.is_empty() {
        return Err(ValidationError::new("tls_key_path_empty"));
    }

    // 检查文件是否存在（仅在文件存在时验证，允许热重载场景）
    if !std::path::Path::new(&tls.cert_path).exists() {
        return Err(ValidationError::new("tls_cert_file_not_found"));
    }
    if !std::path::Path::new(&tls.key_path).exists() {
        return Err(ValidationError::new("tls_key_file_not_found"));
    }

    if tls.mutual_auth {
        if let Some(ca_path) = &tls.client_ca_path {
            if ca_path.is_empty() || !std::path::Path::new(ca_path).exists() {
                return Err(ValidationError::new("tls_client_ca_file_not_found"));
            }
        }
    }

    Ok(())
}

/// 验证认证配置
fn validate_auth_config(auth: &crate::config::AuthConfig) -> Result<(), ValidationError> {
    if !auth.enabled {
        return Ok(());
    }

    match auth.auth_type.as_str() {
        "header" => {
            if auth.expected_value.is_none() {
                return Err(ValidationError::new("header_auth_requires_expected_value"));
            }
        }
        "jwt" => {
            if auth.jwt_secret.is_none() || auth.jwt_secret.as_ref().unwrap().is_empty() {
                return Err(ValidationError::new("jwt_auth_requires_secret"));
            }
        }
        _ => {
            return Err(ValidationError::new("unsupported_auth_type"));
        }
    }

    Ok(())
}

/// 验证上游代理
fn validate_upstream_proxy(upstream: &str) -> Result<(), ValidationError> {
    Url::parse(upstream).map_err(|_| ValidationError::new("invalid_upstream_proxy_url"))?;
    Ok(())
}

/// 验证 CIDR 格式
pub fn validate_cidr(cidr: &str) -> Result<(), ValidationError> {
    let parts: Vec<&str> = cidr.split('/').collect();
    if parts.len() != 2 {
        return Err(ValidationError::new("cidr_must_have_prefix"));
    }

    let ip = IpAddr::from_str(parts[0]).map_err(|_| ValidationError::new("cidr_invalid_ip"))?;
    let prefix: u8 = parts[1]
        .parse()
        .map_err(|_| ValidationError::new("cidr_invalid_prefix"))?;

    let max_prefix = match ip {
        IpAddr::V4(_) => 32,
        IpAddr::V6(_) => 128,
    };

    if prefix > max_prefix {
        return Err(ValidationError::new("cidr_prefix_too_long"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_cidr() {
        assert!(validate_cidr("10.0.0.0/8").is_ok());
        assert!(validate_cidr("192.168.1.0/24").is_ok());
        assert!(validate_cidr("::1/128").is_ok());
        assert!(validate_cidr("invalid").is_err());
        assert!(validate_cidr("10.0.0.0/33").is_err());
        assert!(validate_cidr("999.1.1.1/8").is_err());
    }

    #[test]
    fn test_validate_listen_address() {
        assert!(validate_listen_address("tcp://0.0.0.0:8080").is_ok());
        assert!(validate_listen_address("unix:///tmp/socket.sock").is_ok());
        assert!(validate_listen_address("").is_err());
        assert!(validate_listen_address("http://localhost").is_err());
    }
}
