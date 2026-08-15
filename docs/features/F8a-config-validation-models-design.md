# F8a 配置验证框架-模型与验证规则 — Design

## 架构概述

### 设计原则

1. **纯函数校验**：`ConfigValidator` 的所有 `validate_*` 方法只读 `&EngineConfig` / `&MystiConfig`，不触文件、不做 I/O、不依赖运行时状态。给定输入必有确定输出，可单测、可在 CI 离线复现。
2. **累积式而非短路式**：一次校验收集全部 `ValidationIssue`，而非遇到第一个错误即返回。运维一次看到所有问题，避免"修一个试一次"的低效循环。
3. **级别可调**：`ValidationLevel` 控制"发现问题后怎么办"——`Strict` 让 `is_valid()` 对任何 Error 返回 false，`Warn` 仅记录，`Loose` 几乎全放行。规则本身的"严重度"与级别正交。
4. **零破坏性接入**：F8a 只新增模块与类型，不修改现有 `EngineConfig` / `MystiConfig` 结构，不改变 `from_yaml` 行为。默认 `Loose`，现有调用方无感知。

### 模块关系

```
mystiproxy/src/config/
├── mod.rs              ← 现有：MystiConfig / EngineConfig 等
└── validation.rs       ← 新增：ValidationLevel / ValidationSeverity /
                          ValidationIssue / ValidationResult / ConfigValidator
```

`validation.rs` 仅依赖 `config/mod.rs` 的现有类型 + 标准库 + 已有依赖（`regex`、`url`、`std::net`）。**不引入新 crate**。

### 校验时机（F8a 不强制）

F8a 只提供 `ConfigValidator`，不规定调用时机。F8b 加载器会在 `from_yaml` 后调用 `validate_config`；F8a 阶段可由调用方自行决定是否校验。这保证 F8a 可独立交付、独立测试。

## 数据模型设计

```rust
//! mystiproxy/src/config/validation.rs

use std::net::IpAddr;
use std::time::Duration;

use regex::Regex;

use crate::config::{EngineConfig, MatchMode, MystiConfig};

/// 校验级别：控制发现问题后的处置策略
///
/// - `Strict`：任何 `Error` 严重度问题都使 `ValidationResult::is_valid()` 返回 false
/// - `Warn`：所有问题仅记录，`is_valid()` 始终返回 true（用于开发联调）
/// - `Loose`：默认值，仅记录 `Error`，忽略 `Warning`，`is_valid()` 始终返回 true
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationLevel {
    Strict,
    Warn,
    #[default]
    Loose,
}

/// 问题严重度：与 `ValidationLevel` 正交，描述问题本身而非处置策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationSeverity {
    /// 错误：配置在语义上无法工作（如非法 CIDR、空必填字段）
    Error,
    /// 警告：配置可运行但有风险（如 timeout 过小、mutual_auth 未启用 client_ca）
    Warning,
}

/// 单条校验问题
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    /// 严重度
    pub severity: ValidationSeverity,
    /// 出问题的引擎名（顶层校验时为 `None`）
    pub engine: Option<String>,
    /// 出问题的字段路径，如 `listen`、`allow[0]`、`locations[1].location`
    pub field: String,
    /// 规则标识，如 `listen_scheme`、`cidr_valid`
    pub rule: String,
    /// 人类可读的错误消息（中文）
    pub message: String,
}

impl ValidationIssue {
    fn error(engine: Option<String>, field: &str, rule: &str, message: impl Into<String>) -> Self {
        Self {
            severity: ValidationSeverity::Error,
            engine,
            field: field.to_string(),
            rule: rule.to_string(),
            message: message.into(),
        }
    }

    fn warning(engine: Option<String>, field: &str, rule: &str, message: impl Into<String>) -> Self {
        Self {
            severity: ValidationSeverity::Warning,
            engine,
            field: field.to_string(),
            rule: rule.to_string(),
            message: message.into(),
        }
    }
}

/// 校验结果：累积所有 `ValidationIssue`
#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    level: ValidationLevel,
    issues: Vec<ValidationIssue>,
}

impl ValidationResult {
    /// 创建指定级别的空结果
    pub fn new(level: ValidationLevel) -> Self {
        Self { level, issues: Vec::new() }
    }

    /// 是否有效
    ///
    /// - `Strict`：无 `Error` 严重度问题即为有效
    /// - `Warn` / `Loose`：始终返回 true（仅记录不阻断）
    pub fn is_valid(&self) -> bool {
        match self.level {
            ValidationLevel::Strict => !self.issues.iter().any(|i| i.severity == ValidationSeverity::Error),
            ValidationLevel::Warn | ValidationLevel::Loose => true,
        }
    }

    /// 全部问题
    pub fn issues(&self) -> &[ValidationIssue] {
        &self.issues
    }

    /// 仅 Error 严重度问题
    pub fn errors(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues.iter().filter(|i| i.severity == ValidationSeverity::Error)
    }

    /// 仅 Warning 严重度问题
    pub fn warnings(&self) -> impl Iterator<Item = &ValidationIssue> {
        self.issues.iter().filter(|i| i.severity == ValidationSeverity::Warning)
    }

    /// 追加一条问题
    fn push(&mut self, issue: ValidationIssue) {
        // Loose 级别丢弃 Warning，仅保留 Error 用于记录
        if self.level == ValidationLevel::Loose && issue.severity == ValidationSeverity::Warning {
            return;
        }
        self.issues.push(issue);
    }

    /// 合并另一结果（级别以 self 为准）
    pub fn merge(&mut self, other: ValidationResult) {
        for issue in other.issues {
            self.push(issue);
        }
    }

    /// 问题总数
    pub fn len(&self) -> usize {
        self.issues.len()
    }

    /// 是否无任何问题
    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }
}

/// 配置校验器：应用规则并累积结果
#[derive(Debug, Clone, Copy, Default)]
pub struct ConfigValidator {
    level: ValidationLevel,
}

impl ConfigValidator {
    /// 默认 Loose 级别
    pub fn new() -> Self {
        Self::default()
    }

    /// 指定级别
    pub fn with_level(level: ValidationLevel) -> Self {
        Self { level }
    }

    /// 校验单个引擎配置
    pub fn validate_engine(&self, name: &str, cfg: &EngineConfig) -> ValidationResult {
        let mut result = ValidationResult::new(self.level);
        let engine = Some(name.to_string());
        validate_listen(&mut result, &engine, cfg);
        validate_target(&mut result, &engine, cfg);
        validate_cidr(&mut result, &engine, cfg);
        validate_tls(&mut result, &engine, cfg);
        validate_auth(&mut result, &engine, cfg);
        validate_upstream(&mut result, &engine, cfg);
        validate_regex_locations(&mut result, &engine, cfg);
        validate_timeouts(&mut result, &engine, cfg);
        result
    }

    /// 校验整份配置（遍历所有引擎并合并结果）
    pub fn validate_config(&self, cfg: &MystiConfig) -> ValidationResult {
        let mut result = ValidationResult::new(self.level);
        for (name, engine_cfg) in &cfg.mysti.engine {
            let engine_result = self.validate_engine(name, engine_cfg);
            result.merge(engine_result);
        }
        result
    }
}
```

## 验证规则设计

| 规则名 | 验证对象 | 条件 | 严重度 | 错误消息 |
| :--- | :--- | :--- | :--- | :--- |
| `listen_scheme` | `EngineConfig.listen` | 非空且以 `tcp://` 或 `unix://` 开头 | Error | `listen 地址必须以 tcp:// 或 unix:// 开头，当前值: {listen}` |
| `target_scheme` | `EngineConfig.target` | 非空且以 `tcp://` 或 `unix://` 开头 | Error | `target 地址必须以 tcp:// 或 unix:// 开头，当前值: {target}` |
| `cidr_valid` | `EngineConfig.allow` / `deny` 每条 | `cidr::parse` 成功（IP/前缀长度合法，前缀 0–32 或 0–128） | Error | `allow[{i}] 不是合法 CIDR: {entry}（{parse_err}）` |
| `tls_paths_nonempty` | `TlsConfig.cert_path` / `key_path` | `tls` 为 `Some` 时两个字段非空 | Error | `tls.cert_path 不能为空` / `tls.key_path 不能为空` |
| `auth_type_nonempty` | `AuthConfig.auth_type` | `auth` 为 `Some` 且 `enabled == true` 时 `auth_type` 非空 | Error | `auth 启用时 auth_type 不能为空` |
| `upstream_url_valid` | `EngineConfig.upstream` | `Some` 时 `url::Url::parse` 成功且 scheme 为 `http` 或 `https` | Error | `upstream 不是合法的 http(s) URL: {upstream}（{err}）` |
| `regex_pattern_valid` | `LocationConfig.location`（mode 为 `Regex` / `PrefixRegex`） | `regex::Regex::new(location)` 成功 | Error | `locations[{i}] 的正则模式无效: {location}（{err}）` |
| `timeout_positive` | `EngineConfig.request_timeout` / `connection_timeout` | `Some` 时 `> Duration::ZERO` | Error | `request_timeout 必须大于 0，当前: {dur:?}` |

### 规则实现要点

- **CIDR 校验**：不引入 `cidr` crate，使用 `std::net::IpAddr` 手动解析 `"IP/prefix"` 格式。`split('/')` 后 `IpAddr::from_str` 校验 IP，`u8::from_str` 校验前缀长度范围（IPv4 ≤32，IPv6 ≤128）。若解析失败产生 `AddrParseError`，捕获后转为 `ValidationIssue`。
- **正则校验**：`regex::Regex::new(location)` 失败返回 `regex::Error`，捕获后转入 `ValidationIssue`。复用 `MystiProxyError::Regex` 的 `#[from]` 关系不适用（校验不返回 `Result`，而是累积 `ValidationIssue`）。
- **upstream 校验**：`url::Url::parse(upstream)` 成功后检查 `url.scheme()` 是否为 `http` / `https`，避免 `tcp://` 等被误判为合法 upstream。
- **timeout 校验**：`Duration::ZERO` 与负值（serde 反序列化不会产生负值，但 `parse_duration` 的 `"0s"` 会得到 `Duration::ZERO`）都判为非法。

## 代码设计

### 模块结构

```
mystiproxy/src/config/
├── mod.rs              ← 修改：新增 `pub mod validation;`
└── validation.rs       ← 新增：全部 F8a 代码
```

`validation.rs` 内部组织（私有函数 + 公有类型）：

```rust
// 公有类型（见"数据模型设计"）
pub enum ValidationLevel { ... }
pub enum ValidationSeverity { ... }
pub struct ValidationIssue { ... }
pub struct ValidationResult { ... }
pub struct ConfigValidator { ... }

// 私有规则函数（每个返回 Vec<ValidationIssue> 或直接 push 到 result）
fn validate_listen(result: &mut ValidationResult, engine: &Option<String>, cfg: &EngineConfig);
fn validate_target(result: &mut ValidationResult, engine: &Option<String>, cfg: &EngineConfig);
fn validate_cidr(result: &mut ValidationResult, engine: &Option<String>, cfg: &EngineConfig);
fn validate_tls(result: &mut ValidationResult, engine: &Option<String>, cfg: &EngineConfig);
fn validate_auth(result: &mut ValidationResult, engine: &Option<String>, cfg: &EngineConfig);
fn validate_upstream(result: &mut ValidationResult, engine: &Option<String>, cfg: &EngineConfig);
fn validate_regex_locations(result: &mut ValidationResult, engine: &Option<String>, cfg: &EngineConfig);
fn validate_timeouts(result: &mut ValidationResult, engine: &Option<String>, cfg: &EngineConfig);

// 私有辅助
fn is_valid_cidr(s: &str) -> Result<(), String>;
fn has_valid_scheme(addr: &str) -> bool;
```

### `config/mod.rs` 接入点

在 `mod.rs` 顶部新增一行（不改动任何现有代码）：

```rust
//! 配置模块

pub mod validation;   // ← 新增

use serde::{Deserialize, Deserializer, Serialize, Serializer};
// ... 现有代码不变
```

### 公共 API 总结

| 类型 / 方法 | 签名 | 用途 |
| :--- | :--- | :--- |
| `ConfigValidator::new` | `fn new() -> Self` | 默认 Loose |
| `ConfigValidator::with_level` | `fn with_level(level: ValidationLevel) -> Self` | 指定级别 |
| `ConfigValidator::validate_engine` | `fn validate_engine(&self, name: &str, cfg: &EngineConfig) -> ValidationResult` | 单引擎校验 |
| `ConfigValidator::validate_config` | `fn validate_config(&self, cfg: &MystiConfig) -> ValidationResult` | 整份校验 |
| `ValidationResult::is_valid` | `fn is_valid(&self) -> bool` | 是否通过（受 level 影响） |
| `ValidationResult::issues` | `fn issues(&self) -> &[ValidationIssue]` | 全部问题 |
| `ValidationResult::errors` | `fn errors(&self) -> impl Iterator<Item = &ValidationIssue>` | 仅 Error |
| `ValidationResult::warnings` | `fn warnings(&self) -> impl Iterator<Item = &ValidationIssue>` | 仅 Warning |
| `ValidationResult::merge` | `fn merge(&mut self, other: ValidationResult)` | 合并结果 |

## 错误处理

### 校验层不抛 `Result`

`ConfigValidator::validate_*` 返回 `ValidationResult` 而非 `Result<ValidationResult, MystiProxyError>`。原因：

1. 校验语义是"收集所有问题"，`Result` 的短路语义会丢失后续问题
2. `ValidationResult` 本身已承载"是否通过"的信息（`is_valid()`）
3. 调用方可在 `Strict` 级别下 `if !result.is_valid() { return Err(MystiProxyError::Config(format!("{:?}", result.errors().collect::<Vec<_>>()))); }` 自行转换

### 与 `MystiProxyError` 的衔接

F8a 不新增错误变体。调用方（F8b 加载器）按需将 `ValidationResult` 转 `MystiProxyError::Config(String)`：

```rust
// F8b 中的预期用法（F8a 不实现，仅示例）
let result = ConfigValidator::with_level(ValidationLevel::Strict).validate_config(&cfg);
if !result.is_valid() {
    let msgs: Vec<_> = result.errors().map(|i| format!("[{}] {}: {}", i.field, i.rule, i.message)).collect();
    return Err(MystiProxyError::Config(msgs.join("\n")));
}
```

### 规则内部的错误捕获

每条规则内部用 `Result` 捕获底层解析错误，再转为 `ValidationIssue` push 到结果，**不向上传播**：

```rust
fn validate_upstream(result: &mut ValidationResult, engine: &Option<String>, cfg: &EngineConfig) {
    if let Some(upstream) = &cfg.upstream {
        match url::Url::parse(upstream) {
            Ok(url) if matches!(url.scheme(), "http" | "https") => {}
            Ok(url) => {
                result.push(ValidationIssue::error(
                    engine.clone(), "upstream", "upstream_url_valid",
                    format!("upstream scheme 必须为 http/https，当前: {}", url.scheme()),
                ));
            }
            Err(e) => {
                result.push(ValidationIssue::error(
                    engine.clone(), "upstream", "upstream_url_valid",
                    format!("upstream 不是合法 URL: {upstream}（{e}）"),
                ));
            }
        }
    }
}
```

## 向后兼容策略

### 默认 Loose，零行为变化

- `ConfigValidator::new()` 默认 `Loose`
- F8a **不修改** `MystiConfig::from_yaml` / `from_yaml_file`，不注入校验调用
- 现有调用方不主动 `validate_*` 即无任何影响
- 现有 `cargo test` 全部保持通过（F8a 只新增测试，不改现有测试）

### Loose 级别的语义

- 丢弃所有 `Warning`（不记录）
- 记录 `Error` 但 `is_valid()` 仍返回 `true`
- 等价于"只看不拦"，供未来 F8b 显式升级到 `Strict` 时对比基线

### 升级路径

| 阶段 | 默认级别 | 行为 |
| :--- | :--- | :--- |
| F8a（本阶段） | Loose | 仅提供能力，不强制 |
| F8b（加载器） | Strict（启动时） | 加载后校验，Error 即拒启动 |
| F8d（UI） | 可配 | 管理 API 暴露级别切换 |

### 不破坏现有类型

- `EngineConfig` / `MystiConfig` 结构体**零修改**
- `MystiProxyError` **不新增变体**
- 不新增 crate 依赖（`regex`、`url` 已在 `Cargo.toml`）

## 测试策略

### TDD 流程

每条规则先写"非法输入应产生对应 `ValidationIssue`"的失败测试，再实现规则使测试通过。

### 单元测试矩阵

每条规则至少 2 个用例（合法 + 非法），关键规则增加边界用例。统一放在 `validation.rs` 的 `#[cfg(test)] mod tests` 内。

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;
    use std::collections::HashMap;

    // ===== 数据模型测试 =====
    #[test]
    fn test_validation_level_default_is_loose() {
        assert_eq!(ValidationLevel::default(), ValidationLevel::Loose);
    }

    #[test]
    fn test_is_valid_strict_with_error_is_false() {
        let mut r = ValidationResult::new(ValidationLevel::Strict);
        r.push(ValidationIssue::error(None, "f", "r", "m"));
        assert!(!r.is_valid());
    }

    #[test]
    fn test_is_valid_loose_with_error_is_true() {
        let mut r = ValidationResult::new(ValidationLevel::Loose);
        r.push(ValidationIssue::error(None, "f", "r", "m"));
        assert!(r.is_valid());
    }

    #[test]
    fn test_loose_drops_warnings() {
        let mut r = ValidationResult::new(ValidationLevel::Loose);
        r.push(ValidationIssue::warning(None, "f", "r", "m"));
        assert!(r.issues().is_empty());
    }

    #[test]
    fn test_merge_combines_issues() {
        let mut a = ValidationResult::new(ValidationLevel::Strict);
        a.push(ValidationIssue::error(None, "a", "ra", "ma"));
        let mut b = ValidationResult::new(ValidationLevel::Strict);
        b.push(ValidationIssue::error(None, "b", "rb", "mb"));
        a.merge(b);
        assert_eq!(a.len(), 2);
    }

    // ===== 规则 1: listen_scheme =====
    #[test]
    fn test_listen_valid_scheme() {
        let cfg = engine_with_listen("tcp://0.0.0.0:3128");
        let r = ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &cfg);
        assert!(r.errors().all(|i| i.rule != "listen_scheme"));
    }

    #[test]
    fn test_listen_invalid_scheme() {
        let cfg = engine_with_listen("0.0.0.0:3128");
        let r = ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &cfg);
        assert!(r.errors().any(|i| i.rule == "listen_scheme"));
    }

    #[test]
    fn test_listen_empty() {
        let cfg = engine_with_listen("");
        let r = ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &cfg);
        assert!(r.errors().any(|i| i.rule == "listen_scheme"));
    }

    #[test]
    fn test_listen_unix_scheme() {
        let cfg = engine_with_listen("unix:///var/run/docker.sock");
        let r = ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &cfg);
        assert!(r.errors().all(|i| i.rule != "listen_scheme"));
    }

    // ===== 规则 3: cidr_valid =====
    #[test]
    fn test_cidr_valid_ipv4() {
        let mut cfg = base_engine();
        cfg.allow = Some(vec!["192.168.1.0/24".into()]);
        let r = ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &cfg);
        assert!(r.errors().all(|i| i.rule != "cidr_valid"));
    }

    #[test]
    fn test_cidr_invalid_prefix() {
        let mut cfg = base_engine();
        cfg.allow = Some(vec!["192.168.1.0/33".into()]);
        let r = ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &cfg);
        assert!(r.errors().any(|i| i.rule == "cidr_valid"));
    }

    #[test]
    fn test_cidr_invalid_ip() {
        let mut cfg = base_engine();
        cfg.deny = Some(vec!["not-an-ip/24".into()]);
        let r = ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &cfg);
        assert!(r.errors().any(|i| i.rule == "cidr_valid"));
    }

    // ===== 规则 7: regex_pattern_valid =====
    #[test]
    fn test_regex_location_valid() {
        let mut cfg = base_engine();
        cfg.locations = Some(vec![LocationConfig {
            location: "^/api/.*$".into(),
            mode: MatchMode::Regex,
            provider: None, root: None, response: None, request: None,
            index_files: None, enable_directory_listing: None,
        }]);
        let r = ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &cfg);
        assert!(r.errors().all(|i| i.rule != "regex_pattern_valid"));
    }

    #[test]
    fn test_regex_location_invalid() {
        let mut cfg = base_engine();
        cfg.locations = Some(vec![LocationConfig {
            location: "[invalid".into(),
            mode: MatchMode::Regex,
            provider: None, root: None, response: None, request: None,
            index_files: None, enable_directory_listing: None,
        }]);
        let r = ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &cfg);
        assert!(r.errors().any(|i| i.rule == "regex_pattern_valid"));
    }

    // ===== 规则 8: timeout_positive =====
    #[test]
    fn test_timeout_zero_is_error() {
        let mut cfg = base_engine();
        cfg.request_timeout = Some(Duration::ZERO);
        let r = ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &cfg);
        assert!(r.errors().any(|i| i.rule == "timeout_positive"));
    }

    #[test]
    fn test_timeout_positive_ok() {
        let mut cfg = base_engine();
        cfg.request_timeout = Some(Duration::from_secs(10));
        let r = ConfigValidator::with_level(ValidationLevel::Strict).validate_engine("e", &cfg);
        assert!(r.errors().all(|i| i.rule != "timeout_positive"));
    }

    // ===== validate_config 端到端 =====
    #[test]
    fn test_validate_config_merges_engines() {
        let mut cfg = MystiConfig { mysti: Mysti { engine: HashMap::new() }, cert: vec![] };
        cfg.mysti.engine.insert("good".into(), base_engine());
        let mut bad = base_engine();
        bad.listen = "bad-addr".into();
        cfg.mysti.engine.insert("bad".into(), bad);
        let r = ConfigValidator::with_level(ValidationLevel::Strict).validate_config(&cfg);
        assert!(!r.is_valid());
        assert!(r.errors().any(|i| i.engine.as_deref() == Some("bad") && i.rule == "listen_scheme"));
    }

    // ===== 辅助构造器 =====
    fn base_engine() -> EngineConfig {
        EngineConfig {
            listen: "tcp://0.0.0.0:3128".into(),
            target: "tcp://127.0.0.1:8080".into(),
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

    fn engine_with_listen(listen: &str) -> EngineConfig {
        let mut e = base_engine();
        e.listen = listen.into();
        e
    }
}
```

### 覆盖率目标

- `validation.rs` 行覆盖率 ≥ 70%（`cargo llvm-cov -p mystiproxy config::validation`）
- 每条规则的合法 / 非法路径均有用例
- `ValidationResult::merge` / `is_valid` 在三个 level 下均断言
- `validate_config` 端到端用例覆盖多引擎合并

### 集成验证

```bash
cargo test -p mystiproxy config::validation::tests   # 模块单测
cargo test --workspace                                # 全量不回归
cargo clippy --workspace --all-targets -- -D warnings # 无新告警
cargo fmt --check                                     # 格式通过
```
