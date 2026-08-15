//! 配置验证框架 — 模型与验证规则（F8a）
//!
//! 提供对 `MystiConfig` / `EngineConfig` 的语义级验证。
//! serde 仅做结构反序列化，本模块补充业务规则校验
//! （地址 scheme、CIDR 合法性、TLS 路径非空、auth 字段、upstream URL、
//! regex 模式有效性、timeout 正数）。
//!
//! 验证是**累积式**的：不会在第一条错误时短路返回，而是收集所有
//! 问题后一次性返回 `ValidationResult`，便于用户一次修复全部配置缺陷。
//!
//! 三档验证级别控制 `is_valid()` 的语义：
//! - `Strict`：任何 `Error` 或 `Warning` 均判为不可用
//! - `Warn`：仅 `Error` 判为不可用，`Warning` 记录但不阻断
//! - `Loose`（默认）：即使有 `Error` 也判为可用，仅记录问题

use std::net::IpAddr;
use std::str::FromStr;
use std::time::Duration;

use regex::Regex;

use super::{EngineConfig, MatchMode, MystiConfig};

// ---------------------------------------------------------------------------
// 数据模型
// ---------------------------------------------------------------------------

/// 验证级别，控制 `ValidationResult::is_valid()` 的严格程度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationLevel {
    /// 最严格：Error 和 Warning 都会使 `is_valid()` 返回 `false`。
    Strict,
    /// 中等：Error 使 `is_valid()` 返回 `false`，Warning 仅记录。
    Warn,
    /// 最宽松（默认）：即使有 Error，`is_valid()` 仍返回 `true`。
    /// Loose 级别下 Warning 会被丢弃（不保留在 issues 中）。
    #[default]
    Loose,
}

/// 单条验证问题的严重度。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationSeverity {
    Error,
    Warning,
}

/// 一条验证问题。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    /// 引擎名称（来自 `Mysti.engine` 的 key）。
    pub engine: String,
    /// 配置字段路径，如 `listen`、`allow[0]`、`tls.cert_path`。
    pub field: String,
    /// 规则标识符，如 `listen_scheme`、`cidr_valid`。
    pub rule: String,
    /// 人类可读的错误描述（中文）。
    pub message: String,
    /// 严重度。
    pub severity: ValidationSeverity,
}

impl ValidationIssue {
    fn error(engine: &str, field: &str, rule: &str, message: impl Into<String>) -> Self {
        Self {
            engine: engine.to_string(),
            field: field.to_string(),
            rule: rule.to_string(),
            message: message.into(),
            severity: ValidationSeverity::Error,
        }
    }

    #[allow(dead_code)]
    fn warning(engine: &str, field: &str, rule: &str, message: impl Into<String>) -> Self {
        Self {
            engine: engine.to_string(),
            field: field.to_string(),
            rule: rule.to_string(),
            message: message.into(),
            severity: ValidationSeverity::Warning,
        }
    }
}

impl std::fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let level = match self.severity {
            ValidationSeverity::Error => "ERROR",
            ValidationSeverity::Warning => "WARN",
        };
        write!(
            f,
            "[{level}] engine={engine} field={field} rule={rule}: {message}",
            engine = self.engine,
            field = self.field,
            rule = self.rule,
            message = self.message,
        )
    }
}

/// 验证结果，包含验证级别和发现的所有问题。
#[derive(Debug, Clone)]
pub struct ValidationResult {
    level: ValidationLevel,
    issues: Vec<ValidationIssue>,
}

impl ValidationResult {
    /// 创建空结果。
    pub fn new(level: ValidationLevel) -> Self {
        Self {
            level,
            issues: Vec::new(),
        }
    }

    /// 从已有 issues 列表构造结果。
    pub fn from_issues(level: ValidationLevel, issues: Vec<ValidationIssue>) -> Self {
        let mut result = Self::new(level);
        for issue in issues {
            result.push(issue);
        }
        result
    }

    /// 追加一条问题。Loose 级别下 Warning 会被丢弃。
    pub fn push(&mut self, issue: ValidationIssue) {
        if self.level == ValidationLevel::Loose && issue.severity == ValidationSeverity::Warning {
            return;
        }
        self.issues.push(issue);
    }

    /// 合并另一个结果。使用 self 的级别决定 Warning 去留。
    pub fn merge(&mut self, other: ValidationResult) {
        for issue in other.issues {
            self.push(issue);
        }
    }

    /// 是否有效。语义取决于 `level`。
    ///
    /// - `Strict`：有任何问题（Error 或 Warning）即不可用
    /// - `Warn`：有 Error 即不可用；Warning 不阻断
    /// - `Loose`：始终可用，问题仅记录
    pub fn is_valid(&self) -> bool {
        match self.level {
            ValidationLevel::Strict => self.issues.is_empty(),
            ValidationLevel::Warn => !self
                .issues
                .iter()
                .any(|i| i.severity == ValidationSeverity::Error),
            ValidationLevel::Loose => true,
        }
    }

    /// 全部问题。
    pub fn issues(&self) -> &[ValidationIssue] {
        &self.issues
    }

    /// 仅 Error 级别问题的迭代器。
    pub fn errors(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues
            .iter()
            .filter(|i| i.severity == ValidationSeverity::Error)
    }

    /// 仅 Warning 级别问题的迭代器。
    pub fn warnings(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues
            .iter()
            .filter(|i| i.severity == ValidationSeverity::Warning)
    }

    /// 问题数量。
    pub fn len(&self) -> usize {
        self.issues.len()
    }

    /// 是否没有问题。
    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }
}

// ---------------------------------------------------------------------------
// 验证器
// ---------------------------------------------------------------------------

/// 配置验证器。对 `EngineConfig` / `MystiConfig` 执行全部验证规则。
#[derive(Debug, Clone, Copy)]
pub struct ConfigValidator {
    level: ValidationLevel,
}

impl ConfigValidator {
    /// 创建验证器，默认 `Loose` 级别。
    pub fn new() -> Self {
        Self {
            level: ValidationLevel::default(),
        }
    }

    /// 指定验证级别。
    pub fn with_level(level: ValidationLevel) -> Self {
        Self { level }
    }

    /// 验证整个配置。遍历所有引擎，合并结果。
    pub fn validate_config(&self, config: &MystiConfig) -> ValidationResult {
        let mut result = ValidationResult::new(self.level);
        for (name, engine) in &config.mysti.engine {
            let engine_result = self.validate_engine(name, engine);
            result.merge(engine_result);
        }
        result
    }

    /// 验证单个引擎配置。执行全部 8 条规则。
    pub fn validate_engine(&self, name: &str, cfg: &EngineConfig) -> ValidationResult {
        let mut result = ValidationResult::new(self.level);
        validate_listen(&mut result, name, cfg);
        validate_target(&mut result, name, cfg);
        validate_cidr(&mut result, name, cfg);
        validate_tls(&mut result, name, cfg);
        validate_auth(&mut result, name, cfg);
        validate_upstream(&mut result, name, cfg);
        validate_regex_locations(&mut result, name, cfg);
        validate_timeouts(&mut result, name, cfg);
        result
    }
}

impl Default for ConfigValidator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// 验证规则（私有）
// ---------------------------------------------------------------------------

/// 检查地址是否以合法 scheme 开头。
fn has_valid_scheme(addr: &str) -> bool {
    addr.starts_with("tcp://") || addr.starts_with("unix://")
}

/// 规则 1：listen 地址非空且 scheme 合法。
fn validate_listen(result: &mut ValidationResult, engine: &str, cfg: &EngineConfig) {
    if !has_valid_scheme(&cfg.listen) {
        result.push(ValidationIssue::error(
            engine,
            "listen",
            "listen_scheme",
            format!(
                "listen 地址必须以 tcp:// 或 unix:// 开头，当前值: {:?}",
                cfg.listen
            ),
        ));
    }
}

/// 规则 2：target 地址非空且 scheme 合法。
fn validate_target(result: &mut ValidationResult, engine: &str, cfg: &EngineConfig) {
    if !has_valid_scheme(&cfg.target) {
        result.push(ValidationIssue::error(
            engine,
            "target",
            "target_scheme",
            format!(
                "target 地址必须以 tcp:// 或 unix:// 开头，当前值: {:?}",
                cfg.target
            ),
        ));
    }
}

/// 规则 3：allow / deny 中的 CIDR 合法。
fn validate_cidr(result: &mut ValidationResult, engine: &str, cfg: &EngineConfig) {
    if let Some(allow) = &cfg.allow {
        for (i, cidr) in allow.iter().enumerate() {
            if let Err(msg) = is_valid_cidr(cidr) {
                result.push(ValidationIssue::error(
                    engine,
                    &format!("allow[{i}]"),
                    "cidr_valid",
                    format!("无效的 CIDR: {msg}"),
                ));
            }
        }
    }
    if let Some(deny) = &cfg.deny {
        for (i, cidr) in deny.iter().enumerate() {
            if let Err(msg) = is_valid_cidr(cidr) {
                result.push(ValidationIssue::error(
                    engine,
                    &format!("deny[{i}]"),
                    "cidr_valid",
                    format!("无效的 CIDR: {msg}"),
                ));
            }
        }
    }
}

/// 规则 4：TLS 配置的 cert_path / key_path 非空。
fn validate_tls(result: &mut ValidationResult, engine: &str, cfg: &EngineConfig) {
    if let Some(tls) = &cfg.tls {
        if tls.cert_path.is_empty() {
            result.push(ValidationIssue::error(
                engine,
                "tls.cert_path",
                "tls_paths_nonempty",
                "tls.cert_path 不能为空",
            ));
        }
        if tls.key_path.is_empty() {
            result.push(ValidationIssue::error(
                engine,
                "tls.key_path",
                "tls_paths_nonempty",
                "tls.key_path 不能为空",
            ));
        }
    }
}

/// 规则 5：auth 启用时 auth_type 非空。
fn validate_auth(result: &mut ValidationResult, engine: &str, cfg: &EngineConfig) {
    if let Some(auth) = &cfg.auth {
        if auth.enabled && auth.auth_type.is_empty() {
            result.push(ValidationIssue::error(
                engine,
                "auth.auth_type",
                "auth_type_nonempty",
                "auth 启用时 auth_type 不能为空",
            ));
        }
    }
}

/// 规则 6：upstream 为合法的 http/https URL。
fn validate_upstream(result: &mut ValidationResult, engine: &str, cfg: &EngineConfig) {
    if let Some(upstream) = &cfg.upstream {
        match url::Url::parse(upstream) {
            Ok(url) => {
                let scheme = url.scheme();
                if scheme != "http" && scheme != "https" {
                    result.push(ValidationIssue::error(
                        engine,
                        "upstream",
                        "upstream_url_valid",
                        format!("upstream URL scheme 必须为 http 或 https，当前: {scheme}"),
                    ));
                }
            }
            Err(e) => {
                result.push(ValidationIssue::error(
                    engine,
                    "upstream",
                    "upstream_url_valid",
                    format!("upstream URL 解析失败: {e}"),
                ));
            }
        }
    }
}

/// 规则 7：Regex / PrefixRegex 模式的 location 有合法正则。
fn validate_regex_locations(result: &mut ValidationResult, engine: &str, cfg: &EngineConfig) {
    if let Some(locations) = &cfg.locations {
        for (i, loc) in locations.iter().enumerate() {
            if loc.mode == MatchMode::Regex || loc.mode == MatchMode::PrefixRegex {
                if let Err(e) = Regex::new(&loc.location) {
                    result.push(ValidationIssue::error(
                        engine,
                        &format!("locations[{i}].location"),
                        "regex_pattern_valid",
                        format!("无效的正则表达式: {e}"),
                    ));
                }
            }
        }
    }
}

/// 规则 8：timeout 为正数。
fn validate_timeouts(result: &mut ValidationResult, engine: &str, cfg: &EngineConfig) {
    if let Some(timeout) = cfg.request_timeout {
        if timeout <= Duration::ZERO {
            result.push(ValidationIssue::error(
                engine,
                "request_timeout",
                "timeout_positive",
                "request_timeout 必须大于零",
            ));
        }
    }
    if let Some(timeout) = cfg.connection_timeout {
        if timeout <= Duration::ZERO {
            result.push(ValidationIssue::error(
                engine,
                "connection_timeout",
                "timeout_positive",
                "connection_timeout 必须大于零",
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// CIDR 辅助
// ---------------------------------------------------------------------------

/// 校验 CIDR 字符串。不引入 `cidr` crate，手动解析。
///
/// - 无 `/`：按 IP 地址解析即可。
/// - 有 `/`：解析 IP + 前缀长度，前缀范围 IPv4 ≤ 32 / IPv6 ≤ 128。
fn is_valid_cidr(s: &str) -> Result<(), String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("CIDR 为空".to_string());
    }
    if let Some((ip_part, prefix_part)) = s.split_once('/') {
        let ip = IpAddr::from_str(ip_part).map_err(|e| format!("IP 部分无效: {e}"))?;
        let prefix: u8 = prefix_part
            .parse()
            .map_err(|_| format!("前缀部分不是数字: {prefix_part}"))?;
        let max = match ip {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        if prefix > max {
            return Err(format!("前缀长度 {prefix} 超过上限 {max}"));
        }
    } else {
        IpAddr::from_str(s).map_err(|e| format!("不是有效的 IP 或 CIDR: {e}"))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AuthConfig, EngineConfig, LocationConfig, MatchMode, ProxyType, TlsConfig,
    };
    use std::collections::HashMap;

    // --- 测试辅助构造器 ---

    fn base_engine() -> EngineConfig {
        EngineConfig {
            listen: "tcp://0.0.0.0:3128".to_string(),
            target: "tcp://127.0.0.1:8080".to_string(),
            proxy_type: ProxyType::Http,
            request_timeout: None,
            connection_timeout: None,
            header: None,
            locations: None,
            tls: None,
            auth: None,
            upstream: None,
            allow: None,
            deny: None,
        }
    }

    fn make_config(engine: EngineConfig) -> MystiConfig {
        let mut map = HashMap::new();
        map.insert("test".to_string(), engine);
        MystiConfig {
            mysti: crate::config::Mysti { engine: map },
            cert: vec![],
        }
    }

    // --- T1 模型层测试 ---

    #[test]
    fn test_validation_level_default_is_loose() {
        assert_eq!(ValidationLevel::default(), ValidationLevel::Loose);
    }

    #[test]
    fn test_is_valid_strict_with_error_is_false() {
        let mut result = ValidationResult::new(ValidationLevel::Strict);
        result.push(ValidationIssue::error("e", "f", "r", "msg"));
        assert!(!result.is_valid());
    }

    #[test]
    fn test_is_valid_warn_with_error_is_false() {
        let mut result = ValidationResult::new(ValidationLevel::Warn);
        result.push(ValidationIssue::error("e", "f", "r", "msg"));
        assert!(!result.is_valid());
    }

    #[test]
    fn test_is_valid_warn_with_warning_is_true() {
        let mut result = ValidationResult::new(ValidationLevel::Warn);
        result.push(ValidationIssue::warning("e", "f", "r", "msg"));
        assert!(result.is_valid());
    }

    #[test]
    fn test_is_valid_loose_with_error_is_true() {
        let mut result = ValidationResult::new(ValidationLevel::Loose);
        result.push(ValidationIssue::error("e", "f", "r", "msg"));
        assert!(result.is_valid());
    }

    #[test]
    fn test_loose_drops_warnings() {
        let mut result = ValidationResult::new(ValidationLevel::Loose);
        result.push(ValidationIssue::warning("e", "f", "r", "msg"));
        assert!(result.issues().is_empty());
    }

    #[test]
    fn test_warn_keeps_warnings() {
        let mut result = ValidationResult::new(ValidationLevel::Warn);
        result.push(ValidationIssue::warning("e", "f", "r", "msg"));
        assert_eq!(result.issues().len(), 1);
    }

    #[test]
    fn test_strict_keeps_warnings() {
        let mut result = ValidationResult::new(ValidationLevel::Strict);
        result.push(ValidationIssue::warning("e", "f", "r", "msg"));
        assert_eq!(result.issues().len(), 1);
    }

    #[test]
    fn test_merge_combines_issues() {
        let mut a = ValidationResult::new(ValidationLevel::Strict);
        a.push(ValidationIssue::error("e1", "f", "r", "msg"));
        let mut b = ValidationResult::new(ValidationLevel::Strict);
        b.push(ValidationIssue::error("e2", "f", "r", "msg"));
        a.merge(b);
        assert_eq!(a.len(), 2);
    }

    #[test]
    fn test_merge_level_uses_self() {
        let mut a = ValidationResult::new(ValidationLevel::Loose);
        let mut b = ValidationResult::new(ValidationLevel::Strict);
        b.push(ValidationIssue::warning("e", "f", "r", "msg"));
        a.merge(b);
        assert!(a.issues().is_empty());
    }

    #[test]
    fn test_from_issues_constructs_result() {
        let issues = vec![
            ValidationIssue::error("e", "f1", "r", "msg"),
            ValidationIssue::error("e", "f2", "r", "msg"),
        ];
        let result = ValidationResult::from_issues(ValidationLevel::Strict, issues);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_errors_filter_only_errors() {
        let mut result = ValidationResult::new(ValidationLevel::Strict);
        result.push(ValidationIssue::error("e", "f1", "r", "msg"));
        result.push(ValidationIssue::warning("e", "f2", "r", "msg"));
        assert_eq!(result.errors().count(), 1);
    }

    #[test]
    fn test_warnings_filter_only_warnings() {
        let mut result = ValidationResult::new(ValidationLevel::Strict);
        result.push(ValidationIssue::error("e", "f1", "r", "msg"));
        result.push(ValidationIssue::warning("e", "f2", "r", "msg"));
        assert_eq!(result.warnings().count(), 1);
    }

    // --- T2 listen ---

    #[test]
    fn test_listen_valid_tcp_scheme() {
        let mut engine = base_engine();
        engine.listen = "tcp://0.0.0.0:3128".to_string();
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "listen_scheme"));
    }

    #[test]
    fn test_listen_valid_unix_scheme() {
        let mut engine = base_engine();
        engine.listen = "unix:///var/run/docker.sock".to_string();
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "listen_scheme"));
    }

    #[test]
    fn test_listen_invalid_scheme() {
        let mut engine = base_engine();
        engine.listen = "0.0.0.0:3128".to_string();
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(result
            .errors()
            .any(|i| i.rule == "listen_scheme" && i.field == "listen"));
    }

    #[test]
    fn test_listen_empty() {
        let mut engine = base_engine();
        engine.listen = String::new();
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(result.errors().any(|i| i.rule == "listen_scheme"));
    }

    // --- T3 target ---

    #[test]
    fn test_target_valid_tcp_scheme() {
        let mut engine = base_engine();
        engine.target = "tcp://127.0.0.1:8080".to_string();
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "target_scheme"));
    }

    #[test]
    fn test_target_valid_unix_scheme() {
        let mut engine = base_engine();
        engine.target = "unix:///tmp/upstream.sock".to_string();
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "target_scheme"));
    }

    #[test]
    fn test_target_invalid_scheme() {
        let mut engine = base_engine();
        engine.target = "127.0.0.1:8080".to_string();
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(result.errors().any(|i| i.rule == "target_scheme"));
    }

    #[test]
    fn test_target_empty() {
        let mut engine = base_engine();
        engine.target = String::new();
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(result.errors().any(|i| i.rule == "target_scheme"));
    }

    // --- T4 CIDR ---

    #[test]
    fn test_cidr_valid_ipv4() {
        let mut engine = base_engine();
        engine.allow = Some(vec!["192.168.1.0/24".to_string()]);
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "cidr_valid"));
    }

    #[test]
    fn test_cidr_valid_ipv6() {
        let mut engine = base_engine();
        engine.allow = Some(vec!["2001:db8::/32".to_string()]);
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "cidr_valid"));
    }

    #[test]
    fn test_cidr_valid_no_prefix() {
        let mut engine = base_engine();
        engine.allow = Some(vec!["10.0.0.1".to_string()]);
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "cidr_valid"));
    }

    #[test]
    fn test_cidr_invalid_prefix_ipv4() {
        let mut engine = base_engine();
        engine.allow = Some(vec!["192.168.1.0/33".to_string()]);
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(result.errors().any(|i| i.rule == "cidr_valid"));
    }

    #[test]
    fn test_cidr_invalid_prefix_ipv6() {
        let mut engine = base_engine();
        engine.allow = Some(vec!["2001:db8::/129".to_string()]);
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(result.errors().any(|i| i.rule == "cidr_valid"));
    }

    #[test]
    fn test_cidr_invalid_ip() {
        let mut engine = base_engine();
        engine.deny = Some(vec!["not-an-ip/24".to_string()]);
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(result.errors().any(|i| i.rule == "cidr_valid"));
    }

    #[test]
    fn test_cidr_deny_also_validated() {
        let mut engine = base_engine();
        engine.deny = Some(vec!["10.0.0.0/8".to_string()]);
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "cidr_valid"));
    }

    #[test]
    fn test_cidr_none_skipped() {
        let engine = base_engine();
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "cidr_valid"));
    }

    // --- T5 TLS ---

    #[test]
    fn test_tls_paths_valid() {
        let mut engine = base_engine();
        engine.tls = Some(TlsConfig {
            cert_path: "/etc/cert.pem".to_string(),
            key_path: "/etc/key.pem".to_string(),
            client_ca_path: None,
            mutual_auth: false,
        });
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "tls_paths_nonempty"));
    }

    #[test]
    fn test_tls_cert_path_empty() {
        let mut engine = base_engine();
        engine.tls = Some(TlsConfig {
            cert_path: String::new(),
            key_path: "/etc/key.pem".to_string(),
            client_ca_path: None,
            mutual_auth: false,
        });
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(result
            .errors()
            .any(|i| i.rule == "tls_paths_nonempty" && i.field == "tls.cert_path"));
    }

    #[test]
    fn test_tls_key_path_empty() {
        let mut engine = base_engine();
        engine.tls = Some(TlsConfig {
            cert_path: "/etc/cert.pem".to_string(),
            key_path: String::new(),
            client_ca_path: None,
            mutual_auth: false,
        });
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(result
            .errors()
            .any(|i| i.rule == "tls_paths_nonempty" && i.field == "tls.key_path"));
    }

    #[test]
    fn test_tls_none_skipped() {
        let engine = base_engine();
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "tls_paths_nonempty"));
    }

    // --- T6 auth ---

    #[test]
    fn test_auth_type_valid_when_enabled() {
        let mut engine = base_engine();
        engine.auth = Some(AuthConfig {
            auth_type: "header".to_string(),
            header_name: "Authorization".to_string(),
            expected_value: None,
            jwt_secret: None,
            enabled: true,
        });
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "auth_type_nonempty"));
    }

    #[test]
    fn test_auth_type_empty_when_enabled() {
        let mut engine = base_engine();
        engine.auth = Some(AuthConfig {
            auth_type: String::new(),
            header_name: "Authorization".to_string(),
            expected_value: None,
            jwt_secret: None,
            enabled: true,
        });
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(result.errors().any(|i| i.rule == "auth_type_nonempty"));
    }

    #[test]
    fn test_auth_type_empty_when_disabled() {
        let mut engine = base_engine();
        engine.auth = Some(AuthConfig {
            auth_type: String::new(),
            header_name: "Authorization".to_string(),
            expected_value: None,
            jwt_secret: None,
            enabled: false,
        });
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "auth_type_nonempty"));
    }

    #[test]
    fn test_auth_none_skipped() {
        let engine = base_engine();
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "auth_type_nonempty"));
    }

    // --- T7 upstream ---

    #[test]
    fn test_upstream_valid_http() {
        let mut engine = base_engine();
        engine.upstream = Some("http://proxy:8080".to_string());
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "upstream_url_valid"));
    }

    #[test]
    fn test_upstream_valid_https() {
        let mut engine = base_engine();
        engine.upstream = Some("https://proxy:8443".to_string());
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "upstream_url_valid"));
    }

    #[test]
    fn test_upstream_invalid_scheme() {
        let mut engine = base_engine();
        engine.upstream = Some("tcp://proxy:8080".to_string());
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(result.errors().any(|i| i.rule == "upstream_url_valid"));
    }

    #[test]
    fn test_upstream_missing_scheme() {
        let mut engine = base_engine();
        engine.upstream = Some("proxy:8080".to_string());
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(result.errors().any(|i| i.rule == "upstream_url_valid"));
    }

    #[test]
    fn test_upstream_none_skipped() {
        let engine = base_engine();
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "upstream_url_valid"));
    }

    // --- T8 regex location ---

    fn base_location(mode: MatchMode, location: &str) -> LocationConfig {
        LocationConfig {
            location: location.to_string(),
            mode,
            provider: None,
            root: None,
            response: None,
            request: None,
            index_files: None,
            enable_directory_listing: None,
        }
    }

    #[test]
    fn test_regex_location_valid() {
        let mut engine = base_engine();
        engine.locations = Some(vec![base_location(MatchMode::Regex, "^/api/.*$")]);
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "regex_pattern_valid"));
    }

    #[test]
    fn test_prefix_regex_location_valid() {
        let mut engine = base_engine();
        engine.locations = Some(vec![base_location(MatchMode::PrefixRegex, "/api/.*")]);
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "regex_pattern_valid"));
    }

    #[test]
    fn test_regex_location_invalid() {
        let mut engine = base_engine();
        engine.locations = Some(vec![base_location(MatchMode::Regex, "[invalid")]);
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(result.errors().any(|i| i.rule == "regex_pattern_valid"));
    }

    #[test]
    fn test_full_mode_not_validated() {
        let mut engine = base_engine();
        engine.locations = Some(vec![base_location(MatchMode::Full, "[invalid")]);
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "regex_pattern_valid"));
    }

    #[test]
    fn test_prefix_mode_not_validated() {
        let mut engine = base_engine();
        engine.locations = Some(vec![base_location(MatchMode::Prefix, "[invalid")]);
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "regex_pattern_valid"));
    }

    #[test]
    fn test_regex_location_index_in_field() {
        let mut engine = base_engine();
        engine.locations = Some(vec![
            base_location(MatchMode::Regex, "^/ok$"),
            base_location(MatchMode::Regex, "[bad"),
        ]);
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(result
            .errors()
            .any(|i| i.rule == "regex_pattern_valid" && i.field == "locations[1].location"));
    }

    // --- T9 timeout ---

    #[test]
    fn test_request_timeout_positive_ok() {
        let mut engine = base_engine();
        engine.request_timeout = Some(Duration::from_secs(10));
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "timeout_positive"));
    }

    #[test]
    fn test_request_timeout_zero_is_error() {
        let mut engine = base_engine();
        engine.request_timeout = Some(Duration::ZERO);
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(result
            .errors()
            .any(|i| i.rule == "timeout_positive" && i.field == "request_timeout"));
    }

    #[test]
    fn test_connection_timeout_positive_ok() {
        let mut engine = base_engine();
        engine.connection_timeout = Some(Duration::from_secs(5));
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "timeout_positive"));
    }

    #[test]
    fn test_connection_timeout_zero_is_error() {
        let mut engine = base_engine();
        engine.connection_timeout = Some(Duration::ZERO);
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(result
            .errors()
            .any(|i| i.rule == "timeout_positive" && i.field == "connection_timeout"));
    }

    #[test]
    fn test_timeouts_none_skipped() {
        let engine = base_engine();
        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert!(!result.errors().any(|i| i.rule == "timeout_positive"));
    }

    // --- T10 集成测试 ---

    #[test]
    fn test_validate_engine_collects_all_errors() {
        let mut engine = base_engine();
        engine.listen = "bad-scheme".to_string(); // listen_scheme
        engine.target = "also-bad".to_string(); // target_scheme
        engine.allow = Some(vec!["not-an-ip".to_string()]); // cidr_valid
        engine.request_timeout = Some(Duration::ZERO); // timeout_positive

        let result =
            ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &engine);
        assert_eq!(result.errors().count(), 4);
        assert!(!result.is_valid());
    }

    #[test]
    fn test_validate_config_merges_engines() {
        let good = base_engine();
        let mut bad = base_engine();
        bad.listen = "no-scheme".to_string();

        let mut engines = HashMap::new();
        engines.insert("good".to_string(), good);
        engines.insert("bad".to_string(), bad);
        let config = MystiConfig {
            mysti: crate::config::Mysti { engine: engines },
            cert: vec![],
        };

        let result = ConfigValidator::with_level(ValidationLevel::Strict).validate_config(&config);
        let bad_issues: Vec<_> = result.errors().filter(|i| i.engine == "bad").collect();
        assert_eq!(bad_issues.len(), 1);
        let good_issues: Vec<_> = result.errors().filter(|i| i.engine == "good").collect();
        assert!(good_issues.is_empty());
    }

    #[test]
    fn test_validate_config_all_good_engines() {
        let config = make_config(base_engine());
        let result = ConfigValidator::with_level(ValidationLevel::Strict).validate_config(&config);
        assert!(result.is_valid());
        assert!(result.issues().is_empty());
    }

    #[test]
    fn test_validate_config_loose_is_valid_even_with_errors() {
        let mut engine = base_engine();
        engine.listen = "bad".to_string();
        let config = make_config(engine);

        let result = ConfigValidator::with_level(ValidationLevel::Loose).validate_config(&config);
        assert!(result.is_valid());
        assert!(result.errors().count() > 0);
    }

    #[test]
    fn test_validate_config_warn_keeps_warnings() {
        // Trigger a warning-level issue: we don't have warning rules in F8a,
        // but we can verify that Warn level keeps warnings via model test.
        let mut result = ValidationResult::new(ValidationLevel::Warn);
        result.push(ValidationIssue::warning("e", "f", "r", "msg"));
        assert_eq!(result.warnings().count(), 1);
        assert!(result.is_valid());
    }

    // --- CIDR 辅助直接测试 ---

    #[test]
    fn test_is_valid_cidr_empty() {
        assert!(is_valid_cidr("").is_err());
        assert!(is_valid_cidr("   ").is_err());
    }

    #[test]
    fn test_is_valid_cidr_bare_ipv4() {
        assert!(is_valid_cidr("10.0.0.1").is_ok());
    }

    #[test]
    fn test_is_valid_cidr_bare_ipv6() {
        assert!(is_valid_cidr("::1").is_ok());
    }

    #[test]
    fn test_is_valid_cidr_prefix_zero() {
        assert!(is_valid_cidr("0.0.0.0/0").is_ok());
        assert!(is_valid_cidr("::/0").is_ok());
    }
}
