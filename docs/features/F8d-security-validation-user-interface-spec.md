# F8d 配置验证框架-安全验证与用户界面 — Spec

## 1. 概述

F8d 为 MystiProxy 引入**配置安全验证**与**用户界面渲染**两块能力，作为配置验证框架的收官阶段。

- **安全验证**（`SecurityValidator`）：在 F8a 的语义校验之上，叠加"安全规则"——危险 HTTP 头部拦截、SSRF 防护（黑名单协议 + 内网 IP + scheme 白名单）、CIDR 宽泛性拦截、敏感信息泄露告警。它**不替代** F8a 的 `ConfigValidator`，而是正交补充：F8a 查"配置对不对"，F8d 查"配置安不安全"。
- **用户界面**（`ConfigUserInterface`）：把 F8a/F8b/F8c 产出的 `ValidationResult` / `ConfigValidationError` / `MystiConfig` 渲染成人类可读的彩色输出，含修复建议与配置摘要。

本阶段封装在新模块 `mystiproxy/src/config/security.rs` 与 `mystiproxy/src/config/user_interface.rs` 中，**零修改现有配置结构体**，复用 F8a 已定义的 `ConfigValidationError::Security` 变体，不新增错误类型。

### 设计要点

| 维度 | 选择 | 理由 |
| :--- | :--- | :--- |
| 安全与语义分离 | 独立 `SecurityValidator` 结构体 | 安全规则面向原子输入（URL/CIDR 字符串），不依赖 `EngineConfig`，可独立用于 CLI 子命令 |
| 返回类型 | `Result<T, ConfigValidationError>` | SSRF 必须立即拦截，遇第一个即 `Err`，不需要累积 |
| 敏感信息处置 | 只 `warn!` 不 `Err` | 配置中 `password:` 可能是合法鉴权，告警不阻断，运维人工判断 |
| UI 着色 | `colored` crate，`color_output: bool` 可关 | 终端默认着色，CI 显式关 |
| 建议提取 | `extract_validation_suggestions -> Vec<String>` | 程序化消费，管理 API 可序列化 |
| 依赖增量 | 零新 crate | `ipnetwork` / `regex` / `url` / `colored` / `validator` / `tracing` 均已在 `Cargo.toml` |

## 2. 功能说明

### 2.1 模块结构

```
mystiproxy/src/config/
├── mod.rs              ← 修改：新增 `pub mod security;` `pub mod user_interface;`
├── security.rs         ← 新增：SecurityValidator + 3 类常量
└── user_interface.rs   ← 新增：ConfigUserInterface
```

`security.rs` 依赖 `validation::error::{ConfigValidationError, ValidationResult}` + `ipnetwork` / `regex` / `url` / `tracing`。`user_interface.rs` 依赖 `validation::error::ConfigValidationError` + `validator::ValidationErrors` + `colored` + `crate::config::MystiConfig`。

### 2.2 公共类型一览

| 类型 | 用途 |
| :--- | :--- |
| `SecurityValidator` | 安全验证器，持有预编译的危险头部表 / 内网 CIDR 表 / URL 黑名单正则 |
| `ConfigUserInterface` | UI 渲染器，持有 `verbose` / `color_output` 开关 |

### 2.3 常量

| 常量 | 类型 | 内容 | 用途 |
| :--- | :--- | :--- | :--- |
| `DANGEROUS_HEADERS` | `&[&str]` | 10 项：`content-length` / `transfer-encoding` / `connection` / `upgrade` / `host` / `authorization` / `proxy-authorization` / `proxy-connection` / `te` / `trailer` | 危险头部黑名单 |
| `INTERNAL_NETWORKS` | `&[&str]` | 8 项：`10.0.0.0/8` / `172.16.0.0/12` / `192.168.0.0/16` / `127.0.0.0/8` / `169.254.0.0/16` / `::1/128` / `fe80::/10` / `fc00::/7` | SSRF 防护内网段（含 IPv6） |
| `URL_BLACKLIST_PATTERNS` | `&[&str]` | 6 项：`(?i)file://` / `(?i)data:` / `(?i)ftp://` / `(?i)gopher://` / `(?i)ldap://` / `(?i)dict://` | URL 协议黑名单正则 |

### 2.4 公共 API

#### SecurityValidator

| 方法 | 签名 | 用途 |
| :--- | :--- | :--- |
| `SecurityValidator::new` | `fn new() -> Self` | 构造，预编译常量为 `Vec<String>` / `Vec<IpNetwork>` / `Vec<Regex>` |
| `SecurityValidator::default` | `fn default() -> Self` | 委托 `new()` |
| `validate_headers` | `fn validate_headers(&self, headers: &HashMap<String, String>) -> ValidationResult<()>` | 危险头部拦截（大小写不敏感） |
| `validate_target_url` | `fn validate_target_url(&self, url: &str) -> ValidationResult<()>` | SSRF 防护（黑名单 + 内网 + scheme） |
| `validate_cidr_list` | `fn validate_cidr_list(&self, cidrs: &[String]) -> ValidationResult<()>` | 宽泛 CIDR（`prefix == 0`）拦截 |
| `check_sensitive_config` | `fn check_sensitive_config(&self, config: &str) -> ValidationResult<()>` | 敏感信息告警（只 `warn!`，返回 `Ok`） |

#### ConfigUserInterface

| 方法 | 签名 | 用途 |
| :--- | :--- | :--- |
| `ConfigUserInterface::new` | `fn new(verbose: bool, color_output: bool) -> Self` | 构造 |
| `ConfigUserInterface::default` | `fn default() -> Self` | `verbose=false, color_output=true` |
| `print_validation_result` | `fn print_validation_result(&self, result: &Result<(), ConfigValidationError>)` | 结果渲染 + 修复建议 |
| `print_config_summary` | `fn print_config_summary(&self, config: &MystiConfig)` | 配置摘要（引擎 / locations / 证书） |
| `extract_validation_suggestions` | `fn extract_validation_suggestions(&self, errors: &ValidationErrors) -> Vec<String>` | 建议提取（私有，由 `print_validation_result` 内部调用） |

## 3. 使用方式

### 3.1 基本用法：安全验证

```rust
use mystiproxy::config::security::SecurityValidator;
use std::collections::HashMap;

let validator = SecurityValidator::new();

// 危险头部拦截
let mut headers = HashMap::new();
headers.insert("X-Custom-Header".to_string(), "value".to_string());
assert!(validator.validate_headers(&headers).is_ok());

headers.insert("Content-Length".to_string(), "100".to_string());
assert!(validator.validate_headers(&headers).is_err()); // 危险头部

// SSRF 防护
assert!(validator.validate_target_url("https://api.example.com").is_ok());
assert!(validator.validate_target_url("http://127.0.0.1/admin").is_err()); // 内网
assert!(validator.validate_target_url("file:///etc/passwd").is_err());      // 黑名单协议

// 宽泛 CIDR
assert!(validator.validate_cidr_list(&["192.168.1.0/24".to_string()]).is_ok());
assert!(validator.validate_cidr_list(&["0.0.0.0/0".to_string()]).is_err()); // 全网段

// 敏感信息（只 warn，不 Err）
validator.check_sensitive_config("password: secret123"); // warn! 但返回 Ok
```

### 3.2 在加载器中编排（F8b 预期用法）

```rust
// F8b 加载器中的预期用法（F8d 不实现，仅示例）
let security = SecurityValidator::new();
let semantic_result = ConfigValidator::with_level(Strict).validate_config(&cfg);
if !semantic_result.is_valid() { return Err(...); }

// 语义通过后，跑安全验证
for (name, engine) in &cfg.mysti.engine {
    security.validate_target_url(&engine.target)?;
    if let Some(h) = &engine.header {
        security.validate_headers(h)?;
    }
    if let Some(allow) = &engine.allow {
        security.validate_cidr_list(allow)?;
    }
}
security.check_sensitive_config(&yaml_text)?; // 只 warn
```

### 3.3 渲染验证结果

```rust
use mystiproxy::config::user_interface::ConfigUserInterface;
use mystiproxy::config::security::SecurityValidator;

let ui = ConfigUserInterface::default(); // verbose=false, color=true
let validator = SecurityValidator::new();
let result = validator.validate_target_url("http://127.0.0.1/admin");
ui.print_validation_result(&result);
// 输出（彩色）：
// ✗ Configuration validation failed: 安全验证失败: access to internal network addresses is blocked
// Suggested fixes:
//   • Security issue: access to internal network addresses is blocked
```

### 3.4 渲染配置摘要

```rust
let ui = ConfigUserInterface::new(true, true);
ui.print_config_summary(&config);
// 输出（彩色）：
// === MystiProxy Configuration Summary ===
// Engines: 2
//   web: tcp://0.0.0.0:3128 -> tcp://127.0.0.1:8080 (Http)
//     Locations: 1
//       /api [Prefix] -> None
//   api: tcp://0.0.0.0:8080 -> tcp://127.0.0.1:3000 (Http)
// Certificates: 1
```

### 3.5 CI 纯文本模式

```rust
let ui = ConfigUserInterface::new(false, false); // 无 verbose，无着色
let result = validator.validate_target_url("file:///etc/passwd");
ui.print_validation_result(&result);
// 输出（无 ANSI 转义，可管道处理）：
// ✗ Configuration validation failed: 安全验证失败: URL matches blacklisted pattern: (?i)file://
// Suggested fixes:
//   • Security issue: URL matches blacklisted pattern: (?i)file://
```

## 4. 安全规则详解

F8d 覆盖 4 条安全规则，面向原子输入（URL / CIDR / 头部 / 配置文本）。

### 4.1 规则 1：`validate_headers` — 危险头部

- **验证对象**：`HashMap<String, String>`（用户配置的 HTTP 头部）
- **条件**：头部名（大小写不敏感归一化后）不在 `DANGEROUS_HEADERS` 黑名单中
- **严重度**：`Err(Security)` 即拒
- **错误消息**：`dangerous header '{name}' is not allowed`

| 配置 | 是否合法 |
| :--- | :--- |
| `header: { X-Custom-Header: "value" }` | ✅ 合法自定义头部 |
| `header: { Content-Length: "100" }` | ❌ 危险头部（破穿 HTTP 拆包） |
| `header: { authorization: "Bearer xxx" }` | ❌ 危险头部（覆盖鉴权） |
| `header: { HOST: "evil.com" }` | ❌ 危险头部（大小写不敏感） |

> 说明：`DANGEROUS_HEADERS` 10 项均为 HTTP 框架自动管理或代理自身注入的头部，用户配置会破穿语义或覆盖鉴权。值校验（如 `authorization` 是否泄露 token）属 `check_sensitive_config` 范畴。

### 4.2 规则 2：`validate_target_url` — SSRF 防护

- **验证对象**：`&str`（目标 URL）
- **条件**：四层检查全部通过
- **严重度**：`Err(Security)` 即拒

**四层检查顺序**：

1. **黑名单协议**：URL 不匹配 `URL_BLACKLIST_PATTERNS`（`file://` / `data:` / `ftp://` / `gopher://` / `ldap://` / `dict://`）
2. **URL 解析**：`url::Url::parse(url)` 成功
3. **内网 IP 拦截**：若 host 是 IP 字面量（`IpAddr::from_str` 成功），不在 `INTERNAL_NETWORKS` 任一段内；域名跳过此层
4. **scheme 白名单**：`parsed.scheme()` 为 `http` 或 `https`

| 配置 | 是否合法 | 拦截层 |
| :--- | :--- | :--- |
| `target: http://api.example.com` | ✅ 合法外部域名 | — |
| `target: https://example.com/path` | ✅ 合法 | — |
| `target: http://127.0.0.1/admin` | ❌ 内网 IP | 第 3 层 |
| `target: http://192.168.1.1/api` | ❌ 内网 IP | 第 3 层 |
| `target: http://10.0.0.1/internal` | ❌ 内网 IP | 第 3 层 |
| `target: http://169.254.169.254/` | ❌ 云元数据 | 第 3 层 |
| `target: file:///etc/passwd` | ❌ 黑名单协议 | 第 1 层 |
| `target: data:text/html,<script>` | ❌ 黑名单协议 | 第 1 层 |
| `target: ftp://server/file` | ❌ 黑名单协议 | 第 1 层 |
| `target: javascript:alert(1)` | ❌ 非 http/https scheme | 第 4 层 |
| `target: http://localhost` | ✅ 域名放行（跳过内网检查） | — |

> 说明：`localhost` 等域名需 DNS 解析后才能查内网，超出配置层范围（DNS rebinding 防护见非目标）。云元数据服务（`169.254.169.254`）是 SSRF 高危目标，`169.254.0.0/16` 在 `INTERNAL_NETWORKS` 中显式列出。

### 4.3 规则 3：`validate_cidr_list` — 宽泛 CIDR 拦截

- **验证对象**：`&[String]`（CIDR 列表，如 `allow` / `deny`）
- **条件**：每条 `IpNetwork::parse` 成功且 `prefix > 0`
- **严重度**：`Err(Security)` 即拒
- **错误消息**：`invalid CIDR '{cidr}': {e}` 或 `overly permissive CIDR (0.0.0.0/0 or ::/0) is not allowed`

| 配置 | 是否合法 |
| :--- | :--- |
| `allow: [192.168.1.0/24]` | ✅ 合法 |
| `allow: [10.0.0.0/8]` | ✅ 合法（虽宽泛但可能是合法内网放行） |
| `allow: [2001:db8::/32]` | ✅ 合法 IPv6 |
| `allow: [0.0.0.0/0]` | ❌ 全网段放行 |
| `allow: [::/0]` | ❌ IPv6 全网段 |
| `allow: [not-an-ip/24]` | ❌ 语法错误 |
| `allow: [192.168.1.0/33]` | ❌ 前缀越界（`IpNetwork::parse` 失败） |

> 说明：只挡 `prefix == 0`（全网段），其他宽泛 CIDR（如 `/8`）可能是合法的"放行整个内网"，不强制拦截。语法错误也 `Err`，与 F8a 的 `cidr_valid` 规则重叠（防御纵深）。

### 4.4 规则 4：`check_sensitive_config` — 敏感信息告警

- **验证对象**：`&str`（配置文本，通常是 YAML 原文）
- **条件**：匹配 5 类敏感模式之一即 `warn!`
- **严重度**：**只 `warn!` 不 `Err`**，始终返回 `Ok(())`
- **告警模式**：`(?i)password\s*[:=]\s*\S+` / `(?i)secret\s*[:=]\s*\S+` / `(?i)api[_-]?key\s*[:=]\s*\S+` / `(?i)token\s*[:=]\s*\S+` / `(?i)private[_-]?key\s*[:=]\s*\S+`

| 配置文本 | 行为 |
| :--- | :--- |
| `password: secret123` | `warn!` 但 `Ok` |
| `api_key: abc123` | `warn!` 但 `Ok` |
| `token: bearer xxx` | `warn!` 但 `Ok` |
| `private_key: ...` | `warn!` 但 `Ok` |
| `name: my-service` | 无匹配，`Ok` |
| （空字符串） | 无匹配，`Ok` |

> 说明：配置中含 `password:` 不一定是错（可能是 `auth.expected_value`），告警但不阻断。运维需人工判断是否真泄露。`[_-]?` 兼容 `api_key` / `apikey`，但不覆盖驼峰 `apiKey`（YAML 惯用下划线）。

## 5. UX 设计

### 5.1 着色分级

| 信号 | 颜色 | 标识 | 场景 |
| :--- | :--- | :--- | :--- |
| 成功 | 绿 | ✓ | `verbose: true` 时校验通过 |
| 错误 | 红 | ✗ | 校验失败 |
| 建议标题 | 黄 | — | "Suggested fixes:" |
| 建议项 | 蓝 | • | 每条修复建议 |
| 结构标识 | 青粗体 | — | 配置摘要标题 / 引擎名 |
| 地址 | 黄 | — | listen / target |
| 类型 | 绿 | — | proxy_type |

### 5.2 输出示例

**校验失败**（`color_output: true`）：
```
✗ Configuration validation failed: 安全验证失败: access to internal network addresses is blocked
Suggested fixes:
  • Security issue: access to internal network addresses is blocked
```

**校验通过**（`verbose: true`）：
```
✓ Configuration validated successfully
```

**配置摘要**：
```
=== MystiProxy Configuration Summary ===
Engines: 2
  web: tcp://0.0.0.0:3128 -> tcp://127.0.0.1:8080 (Http)
    Locations: 1
      /api [Prefix] -> None
  api: tcp://0.0.0.0:8080 -> tcp://127.0.0.1:3000 (Http)
Certificates: 1
```

### 5.3 修复建议映射

`extract_validation_suggestions` 按 `(field, code)` 元组映射，覆盖 7 个字段：

| field | code | 建议 |
| :--- | :--- | :--- |
| `listen` | `listen_empty` | Listen address cannot be empty |
| `listen` | `invalid_tcp_address` | Invalid TCP address format, use 'tcp://host:port' |
| `listen` | `empty_unix_socket_path` | Unix socket path cannot be empty |
| `listen` | `unsupported_protocol` | Supported protocols: tcp://, unix:// |
| `target` | `target_empty` | Target address cannot be empty |
| `target` | `invalid_tcp_target` | Invalid TCP target format, use 'tcp://host:port' |
| `target` | `empty_unix_target_path` | Unix target path cannot be empty |
| `target` | `unsupported_target_protocol` | Supported target protocols: tcp://, unix://, http://, https:// |
| `proxy_type` | `tcp_proxy_requires_tcp_addresses` | TCP proxy requires both listen and target to use tcp:// |
| `proxy_type` | `http_proxy_requires_tcp_or_unix_listen` | HTTP proxy listen must be tcp:// or unix:// |
| `proxy_type` | `http_proxy_requires_tcp_or_unix_target` | HTTP proxy target must be tcp:// or unix:// |
| `proxy_type` | `forward_proxy_requires_tcp_listen` | Forward proxy requires tcp:// listen address |
| `tls` | `tls_cert_path_empty` | TLS certificate path cannot be empty |
| `tls` | `tls_key_path_empty` | TLS private key path cannot be empty |
| `tls` | `tls_cert_file_not_found` | TLS certificate file not found |
| `tls` | `tls_key_file_not_found` | TLS private key file not found |
| `tls` | `tls_client_ca_file_not_found` | TLS client CA file not found |
| `auth` | `header_auth_requires_expected_value` | Header authentication requires expected_value |
| `auth` | `jwt_auth_requires_secret` | JWT authentication requires jwt_secret |
| `auth` | `unsupported_auth_type` | Supported auth types: header, jwt |
| `upstream` | `invalid_upstream_proxy_url` | Invalid upstream proxy URL format |
| `locations` | （任意） | Location configuration error in '{field}' |
| 其他 | 其他 | Validation error in '{field}': {message/code} |

## 6. 限制与约束

### 6.1 F8d 范围内不做

| 项 | 归属 | 原因 |
| :--- | :--- | :--- |
| 管理 API 暴露安全验证 | F9 / 后续 | F8d 只做 CLI / 启动期渲染，API 化留给本地管理模块 |
| 配置 diff 展示 | 后续 | F8c 热重载事件流可携带 diff，UI 渲染留给后续 |
| 自动修复配置文件 | 后续 | F8d 只给建议，不替运维改文件 |
| 跨引擎安全策略冲突检测 | 后续 | 需全局视角，F8d 只做单引擎内安全规则 |
| DNS rebinding 防护 | 后续 | 域名解析后的 IP 检查需运行时 hook，超出配置层范围 |
| 多文件敏感信息扫描 | 后续 | F8d 只查单份配置字符串 |
| stdout 捕获测试 | 后续 | 需 `portable-pty` 或类似 crate，UI 测试以"不 panic + 逻辑正确"为主 |

### 6.2 不破坏现有类型

- `EngineConfig` / `MystiConfig` / `TlsConfig` / `AuthConfig` / `LocationConfig` 结构体**零修改**
- `ConfigValidationError` **不新增变体**（复用 F8a 的 `Security(String)`）
- `MystiConfig::from_yaml` / `from_yaml_file` 行为不变，不注入安全校验调用
- 不新增 crate 依赖（`ipnetwork` / `regex` / `url` / `colored` / `validator` / `tracing` 均已在 `Cargo.toml`）

### 6.3 已知问题

- **`check_sensitive_config` 中的 `Regex::new(pattern).unwrap()`**（`security.rs` 第 163 行）：常量正则编译期固定，理论上不会失败，但 `unwrap()` 是 panic 风险点。建议改为 `Regex::new(pattern).expect("sensitive pattern must compile")` 或在 `SecurityValidator::new()` 构造期预编译所有敏感正则并缓存。见 Task 文档 T5。

### 6.4 安全验证的局限

- **只挡 IP 字面量，不挡域名**：`http://localhost` / `http://internal.evil.com`（DNS 解析到内网）不会被 `validate_target_url` 拦截。DNS rebinding 防护需运行时 hook，超出配置层范围。
- **`check_sensitive_config` 是启发式**：基于正则匹配 `key: value` 模式，可能误报（如配置注释 `# password is needed`）或漏报（如 Base64 编码的密钥）。它是"告警不是阻断"，运维需结合人工判断。
- **`validate_headers` 只查名不查值**：`x-custom-header: password=secret` 不会被拦截（不是危险头部名，值校验属 `check_sensitive_config` 范畴，但 `check_sensitive_config` 接收的是配置文本，不是头部值）。

## 7. FAQ

### Q1：为什么 `SecurityValidator` 与 F8a 的 `ConfigValidator` 分离？

两者职责不同：`ConfigValidator` 查"配置对不对"（语义合法性，如 `listen` 是否有 `tcp://` 前缀），`SecurityValidator` 查"配置安不安全"（如 `target` 是否指向内网）。分离的好处：安全规则面向原子输入（URL/CIDR 字符串），不依赖 `EngineConfig` 结构，可独立用于 CLI 子命令（如 `mystiproxy security-check config.yaml`），也可单独用于运行时对请求 URL 的校验。

### Q2：为什么安全验证返回 `Result` 而非累积式 `ValidationResult`？

SSRF 必须"第一个就拒"——攻击者不需要你"收集全部安全洞"再报告。`Result` 的短路语义与"遇第一个安全问题即拦截"天然匹配。F8a 的累积式 `ValidationResult` 适合"运维一次看到所有配置错误"，但安全问题不容拖延。

### Q3：为什么 `check_sensitive_config` 只 `warn!` 不 `Err`？

配置中 `password: secret123` 不一定是错——可能是合法的 `auth.expected_value`。F8d 不能替运维判断"这是不是真泄露"，只能告警。若想"告警即拒"，可在 F8b 加载器中解析 `tracing` 日志或自行扩展 `check_sensitive_config` 返回告警计数。

### Q4：为什么 `validate_target_url` 对域名放行？

配置层只能看到 URL 字符串，无法做 DNS 解析（DNS 解析属 I/O，违反"纯函数校验"原则，且 DNS 结果可能缓存过期）。`localhost` / `internal.evil.com`（DNS 解析到内网）需运行时 hook 拦截，超出 F8d 范围。F8d 只挡 IP 字面量（`127.0.0.1` / `192.168.x` 等），这是"防御纵深"的第一道，不是全部。

### Q5：`DANGEROUS_HEADERS` 是怎么选的？

10 项都是 HTTP 框架自动管理或代理自身注入的头部：
- `content-length` / `transfer-encoding` / `te` / `trailer`：破穿 HTTP 消息拆包
- `connection` / `proxy-connection` / `upgrade`：破穿连接管理
- `host`：覆盖虚拟主机路由
- `authorization` / `proxy-authorization`：覆盖鉴权

用户配置这些头部等于绕过代理自身的协议管理，必须拦截。

### Q6：`INTERNAL_NETWORKS` 含 IPv6 吗？

含。8 项中 4 项 IPv4（`10.0.0.0/8` / `172.16.0.0/12` / `192.168.0.0/16` / `127.0.0.0/8`）、1 项 IPv4 link-local（`169.254.0.0/16`，云元数据段）、3 项 IPv6（`::1/128` loopback / `fe80::/10` link-local / `fc00::/7` unique local）。IPv6 内网访问同样是 SSRF 风险。

### Q7：`ConfigUserInterface` 的 `verbose` 和 `color_output` 有什么区别？

- `verbose: bool`：控制**是否打印成功消息**。`false`（默认）时校验通过不打印 ✓，避免噪声；`true` 时打印绿色 ✓ 反馈。
- `color_output: bool`：控制**是否使用 ANSI 转义着色**。`true`（默认）终端着色；`false` 输出纯文本，CI / 日志收集友好。

两者正交：可 `verbose=true, color=false`（CI 详细日志）或 `verbose=false, color=true`（终端静默）。

### Q8：如何对一份 YAML 配置做安全检查？

```rust
use mystiproxy::config::security::SecurityValidator;
use mystiproxy::config::user_interface::ConfigUserInterface;

let yaml = std::fs::read_to_string("config.yaml")?;
let security = SecurityValidator::new();
let ui = ConfigUserInterface::default();

// 敏感信息告警
security.check_sensitive_config(&yaml)?;

// 其他安全验证需先 from_yaml 拿到结构化配置
let cfg = MystiConfig::from_yaml(&yaml)?;
for (name, engine) in &cfg.mysti.engine {
    let result = security.validate_target_url(&engine.target);
    ui.print_validation_result(&result);
    if result.is_err() { std::process::exit(1); }
}
```
