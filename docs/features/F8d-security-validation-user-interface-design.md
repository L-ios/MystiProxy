# F8d 配置验证框架-安全验证与用户界面 — Design

## 架构概述

### 设计原则

1. **安全与语义正交**：`SecurityValidator` 不复用 F8a 的 `ConfigValidator`。F8a 查"配置对不对"（语义合法性），F8d 查"配置安不安全"（安全风险）。两者可独立调用、独立测试，组合时由 F8b 加载器编排（先语义后安全）。
2. **安全验证返回 `Result`**：`validate_headers` / `validate_target_url` / `validate_cidr_list` 返回 `ValidationResult<T>`（即 `Result<T, ConfigValidationError>`），遇第一个安全问题即 `Err`。这与 F8a 的"累积式 `ValidationResult`"不同——SSRF 必须立即拦截，不需要"收集全部安全洞"。
3. **敏感信息只告警不阻断**：`check_sensitive_config` 返回 `Ok(())` 但发 `tracing::warn!`。配置中含 `password:` 不一定是错（可能是合法鉴权），但必须让运维知情。
4. **UI 与验证解耦**：`ConfigUserInterface` 只接收 `Result` / `MystiConfig` 并渲染，不持有 `SecurityValidator` 或 `ConfigValidator`。验证逻辑由调用方决定，UI 只负责"把结果说清楚"。
5. **着色可关、建议可程序化**：`color_output: false` 输出纯文本（CI 友好）；`extract_validation_suggestions` 返回 `Vec<String>`（管理 API 友好）。

### 模块关系

```
mystiproxy/src/config/
├── mod.rs              ← 现有：MystiConfig / EngineConfig / pub mod 声明
├── validation/         ← F8a：ConfigValidator / ValidationResult / 规则
│   ├── mod.rs
│   ├── error.rs        ← ConfigValidationError（含 Security 变体，F8a 已定义）
│   └── rules.rs
├── loader.rs           ← F8b：EnhancedConfigLoader
├── manager.rs          ← F8b：ConfigurationManager
├── watcher.rs          ← F8c：ConfigFileWatcher
├── security.rs         ← F8d 新增：SecurityValidator + 安全常量
└── user_interface.rs   ← F8d 新增：ConfigUserInterface
```

`security.rs` 依赖 `validation::error::{ConfigValidationError, ValidationResult}`（F8a 已定义的 `Security` 变体）+ `ipnetwork` / `regex` / `url` / `tracing`。`user_interface.rs` 依赖 `validation::error::ConfigValidationError` + `validator::ValidationErrors` + `colored` + `crate::config::MystiConfig`。**不引入新 crate**（`ipnetwork` / `colored` / `validator` 均已在 `Cargo.toml`）。

### 验证时机（F8d 不强制）

F8d 只提供 `SecurityValidator` 与 `ConfigUserInterface`，不规定调用时机。预期用法：

- **F8b 加载器**：在 `EnhancedConfigLoader::load` 中，F8a 语义校验通过后，调用 `SecurityValidator` 做安全校验；`Err(Security)` 即拒启动
- **F8c 热重载**：watcher 触发重载后，对新配置跑安全校验；`Err(Security)` 则保留旧配置，`warn!` 告警
- **CLI 子命令**：`mystiproxy security-check config.yaml` 可单独跑安全验证，不强制走完整语义校验
- **UI 渲染**：任何产出了 `ConfigValidationError` 或 `MystiConfig` 的地方，都可调 `ConfigUserInterface` 渲染

## SecurityValidator 设计

### 数据结构

```rust
//! mystiproxy/src/config/security.rs

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
```

设计要点：
- **常量私有，构造时转存**：`new()` 把 `&[&str]` 常量转为 `Vec<String>` / `Vec<IpNetwork>` / `Vec<Regex>`，避免每次验证都重复解析。`IpNetwork::parse` 与 `Regex::new` 在构造期完成，验证期零分配。
- **`filter_map` 容错**：构造期若某条常量解析失败，`filter_map(|s| s.parse().ok())` 静默跳过。这是有意的——常量是编译期固定的，不应失败；若失败说明常量本身写错，应在测试期发现（见 T1 测试 `test_internal_networks_all_parse`）。
- **`Default` 委托 `new()`**：`impl Default for SecurityValidator { fn default() -> Self { Self::new() } }`，符合 Rust 惯例。

### 方法 1：`validate_headers` — 危险头部拦截

```rust
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
```

设计要点：
- **大小写不敏感**：HTTP 头部名大小写不敏感（`Content-Length` 与 `content-length` 等价），校验前 `to_lowercase()` 归一化。
- **只查名不查值**：危险头部是"不该被用户设置"的，无论值是什么都拒。值校验（如 `authorization` 是否泄露 token）属 `check_sensitive_config` 范畴。
- **遇第一个即 `Err`**：不累积，与 `Result` 语义一致。
- **错误消息含原始名**：用 `name`（未归一化）拼消息，让运维知道配置里写的是哪个大小写形式。

### 方法 2：`validate_target_url` — SSRF 防护

```rust
pub fn validate_target_url(&self, url: &str) -> ValidationResult<()> {
    // 1. 检查黑名单模式（file:// / data: / ftp:// / gopher:// / ldap:// / dict://）
    for pattern in &self.url_blacklist_patterns {
        if pattern.is_match(url) {
            return Err(ConfigValidationError::Security(format!(
                "URL matches blacklisted pattern: {}",
                pattern
            )));
        }
    }

    // 2. 解析 URL
    let parsed = url::Url::parse(url)
        .map_err(|e| ConfigValidationError::Security(format!("invalid URL: {}", e)))?;

    // 3. 检查内部网络访问（仅对 IP 字面量）
    if let Some(host) = parsed.host_str() {
        use std::str::FromStr;
        let ip_addr: std::net::IpAddr = match std::net::IpAddr::from_str(host) {
            Ok(ip) => ip,
            Err(_) => return Ok(()), // 域名跳过内网检查
        };
        for network in &self.internal_networks {
            if network.contains(ip_addr) {
                return Err(ConfigValidationError::Security(
                    "access to internal network addresses is blocked".to_string(),
                ));
            }
        }
    }

    // 4. 限制协议（仅 http / https）
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ConfigValidationError::Security(
            "only HTTP and HTTPS protocols are allowed".to_string(),
        ));
    }

    Ok(())
}
```

设计要点（SSRF 防护纵深）：
- **四层检查顺序**：黑名单协议 → URL 解析 → 内网 IP → scheme 白名单。黑名单放最前，避免对 `file:///etc/passwd` 这类无 host 的 URL 做 `Url::parse` 后再查 host（虽然 `Url::parse` 能解析，但黑名单正则更快短路）。
- **只对 IP 字面量查内网**：`127.0.0.1` / `192.168.1.1` 等 IP 字面量可直接判断；域名（如 `localhost`）需 DNS 解析后才能查，超出配置层范围（DNS rebinding 防护见非目标）。
- **`IpAddr::from_str` 失败视为域名**：`Err(_) => return Ok(())`——不是 IP 就跳过内网检查，继续走 scheme 白名单。这是有意的权衡：域名走运行时 hook，配置层只挡 IP 字面量。
- **scheme 白名单在最后**：即使前面都通过，scheme 非 `http`/`https` 仍拒。这兜底了黑名单未覆盖的协议（如 `javascript:`）。
- **`169.254.0.0/16` 是云元数据服务段**：AWS / GCP / Azure 的元数据服务都在 `169.254.169.254`，必须挡。

### 方法 3：`validate_cidr_list` — 宽泛 CIDR 拦截

```rust
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
```

设计要点：
- **复用 `ipnetwork` crate**：`IpNetwork::parse` 同时校验 IP 合法性与前缀长度范围（IPv4 ≤32，IPv6 ≤128），无需手写解析。
- **只挡 `prefix == 0`**：`0.0.0.0/0` 与 `::/0` 是"全网段放行"，等于 IP 过滤策略失效。其他宽泛 CIDR（如 `/8`）虽宽松但可能是合法的"放行整个内网"，不强制拦截。
- **语法错误也 `Err`**：`cidr.parse()` 失败说明不是合法 CIDR，直接 `Err`。这与 F8a 的 `cidr_valid` 规则重叠，但 F8d 在安全层再查一次（防御纵深）。
- **遇第一个即 `Err`**：与 `Result` 语义一致。

### 方法 4：`check_sensitive_config` — 敏感信息告警

```rust
pub fn check_sensitive_config(&self, config: &str) -> ValidationResult<()> {
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
```

设计要点：
- **5 类敏感模式**：`password` / `secret` / `api_key` / `token` / `private_key`，覆盖常见密钥字段名。`(?i)` 大小写不敏感。`[_-]?` 兼容 `api_key` / `apiKey`（驼峰不在覆盖范围，YAML 惯用下划线）。
- **只 `warn!` 不 `Err`**：配置中 `password: secret123` 可能是合法的鉴权配置（如 `auth.expected_value`），告警但不阻断。运维需人工判断是否真泄露。
- **返回 `Ok(())`**：与 `validate_*` 方法签名一致，调用方无需特殊处理。
- **⚠️ 已知问题**：`Regex::new(pattern).unwrap()` 在 `security.rs` 第 163 行——常量正则编译期固定，理论上不会失败，但 `unwrap()` 是 panic 风险点。建议改为 `Regex::new(pattern).expect("sensitive pattern must compile")` 或构造期预编译。见"限制与缓解"。

## ConfigUserInterface 设计

### 数据结构

```rust
//! mystiproxy/src/config/user_interface.rs

use colored::*;
use validator::ValidationErrors;

use crate::config::validation::ConfigValidationError;

/// 用户界面配置
pub struct ConfigUserInterface {
    pub verbose: bool,
    pub color_output: bool,
}

impl Default for ConfigUserInterface {
    fn default() -> Self {
        Self {
            verbose: false,
            color_output: true,
        }
    }
}
```

设计要点：
- **字段公开**：`verbose` / `color_output` 是 `pub`，调用方可直接构造或修改。不强制走 `new()`，便于 `ConfigUserInterface { verbose: true, color_output: false }` 字面量构造。
- **默认 `color_output: true`**：终端默认着色，CI 显式设 `false`。
- **默认 `verbose: false`**：校验通过时不打印 ✓，避免噪声；调试时显式开。

### 方法 1：`print_validation_result` — 结果渲染

```rust
pub fn print_validation_result(&self, result: &Result<(), ConfigValidationError>) {
    match result {
        Ok(()) => {
            if self.verbose {
                self.print_success("Configuration validated successfully");
            }
        }
        Err(error) => {
            self.print_error(&format!("Configuration validation failed: {}", error));
            self.print_fix_suggestions(error);
        }
    }
}
```

设计要点：
- **`Ok` 仅 `verbose` 时打印**：默认不打印成功，避免噪声；`verbose: true` 时打印绿色 ✓ 反馈。
- **`Err` 总是打印 + 给建议**：红色 ✗ 错误消息 + 黄色"Suggested fixes:"标题 + 蓝色 • 建议项。
- **委托私有方法**：`print_success` / `print_error` / `print_fix_suggestions` 是私有辅助，统一着色逻辑。

### 方法 2：`print_config_summary` — 配置摘要

```rust
pub fn print_config_summary(&self, config: &crate::config::MystiConfig) {
    // 青色粗体标题
    println!("=== MystiProxy Configuration Summary ===");
    println!("Engines: {}", config.mysti.engine.len());
    for (name, engine) in &config.mysti.engine {
        // 青色引擎名、黄色 listen/target、绿色 proxy_type
        println!("  {}: {} -> {} ({:?})", name, engine.listen, engine.target, engine.proxy_type);
        if let Some(locations) = &engine.locations {
            println!("    Locations: {}", locations.len());
            for loc in locations {
                println!("      {} [{:?}] -> {:?}", loc.location, loc.mode, loc.provider);
            }
        }
    }
    if !config.cert.is_empty() {
        println!("Certificates: {}", config.cert.len());
    }
}
```

设计要点：
- **摘要层级**：顶层标题（青粗体）→ 引擎数 → 每个引擎（名/监听/目标/类型）→ locations（路径/模式/provider）→ 证书数。
- **着色按语义**：青色=结构标识（引擎名/标题），黄色=地址（listen/target），绿色=类型（proxy_type），便于视觉扫读。
- **可选段省略**：无 locations 的引擎不打印 `Locations:` 行；无证书不打印 `Certificates:` 行，避免噪声。

### 方法 3：`extract_validation_suggestions` — 建议提取

```rust
fn extract_validation_suggestions(&self, errors: &ValidationErrors) -> Vec<String> {
    let field_errors = errors.field_errors();
    for (field, error_vec) in field_errors {
        for error in error_vec {
            let code: &str = error.code.as_ref();
            let suggestion = match (field.as_ref(), code) {
                ("listen", "listen_empty") => "Listen address cannot be empty".to_string(),
                ("listen", "invalid_tcp_address") => "Invalid TCP address format, use 'tcp://host:port'".to_string(),
                // ... 25+ 条 (field, code) 映射
                _ => format!("Validation error in '{}': {}", field, error.message.as_deref().unwrap_or(code)),
            };
            suggestions.push(suggestion);
        }
    }
    suggestions
}
```

设计要点：
- **按 `(field, code)` 元组匹配**：F8a 的 `validator` crate 产出的 `ValidationErrors` 含字段名与错误码，UI 按 `(field, code)` 给出针对性建议。如 `("listen", "invalid_tcp_address")` → "Invalid TCP address format, use 'tcp://host:port'"。
- **覆盖 7 个字段**：`listen` / `target` / `proxy_type` / `locations` / `tls` / `auth` / `upstream`，每字段多条 code 映射。
- **兜底分支**：未匹配的 `(field, code)` 走 `_ => format!("Validation error in '{}': {}", field, ...)`，保证不遗漏。
- **返回 `Vec<String>` 而非直接打印**：调用方（`print_fix_suggestions`）负责着色打印；管理 API 可直接序列化此 `Vec` 给前端。

### 私有辅助：着色打印

```rust
fn print_success(&self, msg: &str) {
    if self.color_output {
        println!("{} {}", "✓".green(), msg.green());
    } else {
        println!("✓ {}", msg);
    }
}

fn print_error(&self, msg: &str) {
    if self.color_output {
        println!("{} {}", "✗".red(), msg.red());
    } else {
        println!("✗ {}", msg);
    }
}

fn print_fix_suggestions(&self, error: &ConfigValidationError) {
    let suggestions = match error {
        ConfigValidationError::Validation(errors) => self.extract_validation_suggestions(errors),
        ConfigValidationError::Security(msg) => vec![format!("Security issue: {}", msg)],
        ConfigValidationError::Load(msg) => vec![format!("Load error: {}", msg)],
        ConfigValidationError::Parse(msg) => vec![format!("Parse error: {}", msg)],
        ConfigValidationError::HotReload(msg) => vec![format!("Hot reload error: {}", msg)],
        ConfigValidationError::Watch(msg) => vec![format!("Watch error: {}", msg)],
    };
    if self.color_output {
        println!("{}", "Suggested fixes:".yellow());
    } else {
        println!("Suggested fixes:");
    }
    for suggestion in suggestions {
        if self.color_output {
            println!("  • {}", suggestion.blue());
        } else {
            println!("  • {}", suggestion);
        }
    }
}
```

设计要点：
- **按 `ConfigValidationError` 变体分发**：6 个变体各有专属建议模板。`Validation` 调 `extract_validation_suggestions` 走详细映射；其他 5 个（Security/Load/Parse/HotReload/Watch）给一句话模板。
- **着色分级**：✓ 绿（成功）、✗ 红（错误）、标题黄（建议）、• 蓝（建议项），视觉层级清晰。
- **`color_output: false` 纯文本**：CI / 日志收集无 ANSI 转义，输出可管道处理。

## 代码设计

### 模块结构

```
mystiproxy/src/config/
├── mod.rs              ← 修改：新增 `pub mod security;` `pub mod user_interface;`
├── security.rs         ← 新增：SecurityValidator + DANGEROUS_HEADERS / INTERNAL_NETWORKS / URL_BLACKLIST_PATTERNS
└── user_interface.rs   ← 新增：ConfigUserInterface + 着色/建议/摘要
```

### `config/mod.rs` 接入点

在 `mod.rs` 顶部新增两行（不改动任何现有代码）：

```rust
//! 配置模块

pub mod security;          // ← 新增 F8d
pub mod user_interface;    // ← 新增 F8d
pub mod validation;        // ← F8a 已有

// ... 现有代码不变
```

### 公共 API 总结

| 类型 / 方法 | 签名 | 用途 |
| :--- | :--- | :--- |
| `SecurityValidator::new` | `fn new() -> Self` | 构造，预编译常量 |
| `SecurityValidator::default` | `fn default() -> Self` | 委托 `new()` |
| `SecurityValidator::validate_headers` | `fn validate_headers(&self, headers: &HashMap<String, String>) -> ValidationResult<()>` | 危险头部拦截 |
| `SecurityValidator::validate_target_url` | `fn validate_target_url(&self, url: &str) -> ValidationResult<()>` | SSRF 防护 |
| `SecurityValidator::validate_cidr_list` | `fn validate_cidr_list(&self, cidrs: &[String]) -> ValidationResult<()>` | 宽泛 CIDR 拦截 |
| `SecurityValidator::check_sensitive_config` | `fn check_sensitive_config(&self, config: &str) -> ValidationResult<()>` | 敏感信息告警（只 `warn!`） |
| `ConfigUserInterface::new` | `fn new(verbose: bool, color_output: bool) -> Self` | 构造 |
| `ConfigUserInterface::default` | `fn default() -> Self` | `verbose=false, color_output=true` |
| `ConfigUserInterface::print_validation_result` | `fn print_validation_result(&self, result: &Result<(), ConfigValidationError>)` | 结果渲染 |
| `ConfigUserInterface::print_config_summary` | `fn print_config_summary(&self, config: &MystiConfig)` | 配置摘要 |
| `ConfigUserInterface::extract_validation_suggestions` | `fn extract_validation_suggestions(&self, errors: &ValidationErrors) -> Vec<String>` | 建议提取（私有，仅 `print_fix_suggestions` 内部调用，但方法签名公开以便测试） |

> 注：`extract_validation_suggestions` 在当前实现中是私有方法（`fn` 而非 `pub fn`）。设计上可考虑改为 `pub` 以便管理 API 直接消费，但当前实现保持私有，由 `print_validation_result` 间接调用。

## 错误处理

### 安全验证产出 `ConfigValidationError::Security`

F8d **不新增** `ConfigValidationError` 变体，直接复用 F8a 已定义的 `Security(String)` 变体：

```rust
// F8a 已定义（validation/error.rs），F8d 直接复用
#[error("安全验证失败: {0}")]
Security(String),
```

所有 4 个安全验证方法返回 `Result<T, ConfigValidationError>`，遇安全问题 `Err(ConfigValidationError::Security(msg))`。这与 F8b 加载器、F8c 热重载的错误流自然衔接：

```rust
// F8b 加载器中的预期用法（F8d 不实现，仅示例）
let security = SecurityValidator::new();
security.validate_target_url(&engine.target)?;  // Err(Security) 直接传播
security.validate_headers(&engine.header.unwrap_or_default())?;
```

### `check_sensitive_config` 的特殊语义

`check_sensitive_config` 返回 `Ok(())` 但发 `warn!`。这与 `Result` 语义"返回 `Ok` 即无问题"看似矛盾，实则是**有意的权衡**：

- 配置中含 `password:` 不一定是错（可能是 `auth.expected_value`），不能 `Err` 阻断
- 但必须让运维知情，`tracing::warn!` 是最低成本的告警方式
- 调用方若想"告警即拒"，可自行解析 `warn!` 日志，但 F8d 不替运维做这个决定

### 与 `MystiProxyError` 的衔接

F8d 不直接产 `MystiProxyError`。`ConfigValidationError` 可由 F8b 加载器转为 `MystiProxyError::Config(String)`：

```rust
// F8b 中的预期用法（F8d 不实现，仅示例）
match security.validate_target_url(&engine.target) {
    Ok(()) => {},
    Err(e) => return Err(MystiProxyError::Config(e.to_string())),
}
```

## 测试策略

### TDD 流程

每个安全规则先写"非法输入应 `Err`"的失败测试，再实现使测试通过。UI 方法因涉及 `println!` 输出，测试以"构造不 panic + 建议提取逻辑正确"为主，不验证 stdout 内容（stdout 捕获需 `portable-pty` 或类似 crate，超出 F8d 范围）。

### 单元测试矩阵

详见 Task 文档。关键覆盖：

- **`validate_headers`**：合法头部放行、危险头部（`content-length` / `authorization` / 大小写混合）拦截
- **`validate_target_url`**：合法外部 URL 放行、内网 IP（`127.0.0.1` / `192.168.x` / `10.x` / `169.254.x`）拦截、黑名单协议（`file://` / `data:`）拦截、非 http/https scheme 拦截、域名放行（跳过内网检查）
- **`validate_cidr_list`**：合法 CIDR 放行、`0.0.0.0/0` 与 `::/0` 拦截、非法 CIDR 拦截
- **`check_sensitive_config`**：含 `password:` / `api_key:` / `token:` 触发 `warn!`（验证不 panic）、无敏感信息通过、返回 `Ok`
- **`ConfigUserInterface`**：`new` 构造、`default` 默认值、`extract_validation_suggestions` 对各类 `(field, code)` 映射正确

### 覆盖率目标

- `security.rs` 行覆盖率 ≥ 70%
- `user_interface.rs` 行覆盖率 ≥ 60%（`println!` 分支难以全覆盖，着重 `extract_validation_suggestions` 的 match 分支）
- 每个安全规则的合法 / 非法路径均有用例
- `DANGEROUS_HEADERS` / `INTERNAL_NETWORKS` / `URL_BLACKLIST_PATTERNS` 常量完整性有断言

### 集成验证

```bash
cargo test -p mystiproxy config::security::tests          # 安全验证单测
cargo test -p mystiproxy config::user_interface::tests     # UI 单测
cargo test --workspace                                     # 全量不回归
cargo clippy --workspace --all-targets -- -D warnings     # 无新告警
cargo fmt --check                                          # 格式通过
```
