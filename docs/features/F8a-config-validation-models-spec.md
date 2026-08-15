# F8a 配置验证框架-模型与验证规则 — Spec

## 1. 概述

F8a 为 MystiProxy 引入**配置语义校验能力**：在 `MystiConfig` / `EngineConfig` 通过 serde 解析之后，对其字段做"语义合法性"检查，将原本要在运行时才会 panic 或静默失败的问题（非法 CIDR、空必填字段、非法正则等）前移到加载阶段显式报告。

本阶段是配置验证框架的**第一阶段**，只交付"模型 + 规则"，不包含加载器、文件监听、热重载（分别归属 F8b / F8c / F8d）。F8a 的所有校验逻辑封装在新模块 `mystiproxy/src/config/validation.rs` 中，对外暴露 `ConfigValidator` 与一组结果类型，**零修改现有配置结构体**，默认 `Loose` 级别保证向后兼容。

### 设计要点

| 维度 | 选择 | 理由 |
| :--- | :--- | :--- |
| 校验语义 | 累积式（收集全部问题） | 一次报告所有错误，避免"修一个试一次"的循环 |
| 函数纯度 | 纯函数（只读 `&EngineConfig`） | 可单测、可在 CI 离线复现，不依赖运行时状态 |
| 返回类型 | `ValidationResult`（非 `Result`） | `Result` 短路语义会丢失后续问题 |
| 默认级别 | `Loose` | 现有调用方不主动 `validate_*` 即无任何行为变化 |
| 依赖增量 | 零新 crate | `regex` / `url` / `std::net` 均已在 `Cargo.toml` |

## 2. 功能说明

### 2.1 模块结构

```
mystiproxy/src/config/
├── mod.rs              ← 修改：新增 `pub mod validation;`（仅一行）
└── validation.rs       ← 新增：F8a 全部代码
```

`validation.rs` 仅依赖 `config/mod.rs` 的现有类型 + 标准库 + 已有依赖（`regex`、`url`、`std::net`）。

### 2.2 公共类型一览

| 类型 | 用途 |
| :--- | :--- |
| `ValidationLevel` | 校验级别：`Strict` / `Warn` / `Loose`（默认 `Loose`） |
| `ValidationSeverity` | 问题严重度：`Error` / `Warning`（与级别正交） |
| `ValidationIssue` | 单条校验问题（含严重度、引擎名、字段路径、规则名、消息） |
| `ValidationResult` | 校验结果（累积所有 `ValidationIssue`） |
| `ConfigValidator` | 校验器入口，提供 `new` / `with_level` / `validate_engine` / `validate_config` |

### 2.3 公共 API

| 类型 / 方法 | 签名 | 用途 |
| :--- | :--- | :--- |
| `ConfigValidator::new` | `fn new() -> Self` | 默认 `Loose` 级别 |
| `ConfigValidator::with_level` | `fn with_level(level: ValidationLevel) -> Self` | 指定级别 |
| `ConfigValidator::validate_engine` | `fn validate_engine(&self, name: &str, cfg: &EngineConfig) -> ValidationResult` | 校验单个引擎 |
| `ConfigValidator::validate_config` | `fn validate_config(&self, cfg: &MystiConfig) -> ValidationResult` | 校验整份配置（遍历引擎并合并） |
| `ValidationResult::is_valid` | `fn is_valid(&self) -> bool` | 是否通过（受 `level` 影响） |
| `ValidationResult::issues` | `fn issues(&self) -> &[ValidationIssue]` | 全部问题 |
| `ValidationResult::errors` | `fn errors(&self) -> impl Iterator<Item = &ValidationIssue>` | 仅 `Error` 严重度 |
| `ValidationResult::warnings` | `fn warnings(&self) -> impl Iterator<Item = &ValidationIssue>` | 仅 `Warning` 严重度 |
| `ValidationResult::merge` | `fn merge(&mut self, other: ValidationResult)` | 合并另一结果（级别以 self 为准） |
| `ValidationResult::from_issues` | `fn from_issues(level: ValidationLevel, issues: Vec<ValidationIssue>) -> Self` | 由问题列表构造结果 |

## 3. 使用方式

### 3.1 基本用法：校验整份配置

```rust
use mystiproxy::config::validation::{ConfigValidator, ValidationLevel};

// 默认 Loose：仅记录不阻断
let validator = ConfigValidator::new();
let result = validator.validate_config(&config);
if !result.is_valid() {
    for issue in result.errors() {
        eprintln!("{}", issue);
    }
}
```

### 3.2 严格模式：启动期拦截非法配置

```rust
use mystiproxy::config::validation::{ConfigValidator, ValidationLevel};

let validator = ConfigValidator::with_level(ValidationLevel::Strict);
let result = validator.validate_config(&config);
if !result.is_valid() {
    let msgs: Vec<_> = result
        .errors()
        .map(|i| format!("[{}] {}: {}", i.field, i.rule, i.message))
        .collect();
    return Err(MystiProxyError::Config(msgs.join("\n")));
}
```

### 3.3 校验单个引擎

```rust
use mystiproxy::config::validation::{ConfigValidator, ValidationLevel};

let validator = ConfigValidator::with_level(ValidationLevel::Strict);
let result = validator.validate_engine("web", &engine_cfg);
assert!(result.is_valid());
```

### 3.4 合并多次校验结果

```rust
use mystiproxy::config::validation::{ConfigValidator, ValidationLevel, ValidationResult};

let validator = ConfigValidator::with_level(ValidationLevel::Strict);
let mut aggregate = ValidationResult::new(ValidationLevel::Strict);
for (name, cfg) in &config.mysti.engine {
    let r = validator.validate_engine(name, cfg);
    aggregate.merge(r);
}
// aggregate.issues() 现包含全部引擎的问题
```

### 3.5 与现有 `MystiProxyError` 衔接（F8b 预期用法）

F8a **不新增** `MystiProxyError` 变体。调用方按需将 `ValidationResult` 转为 `MystiProxyError::Config(String)`：

```rust
// F8b 加载器中的预期用法（F8a 不实现，仅示例）
let result = ConfigValidator::with_level(ValidationLevel::Strict).validate_config(&cfg);
if !result.is_valid() {
    let msgs: Vec<_> = result
        .errors()
        .map(|i| format!("[{}] {}: {}", i.field, i.rule, i.message))
        .collect();
    return Err(MystiProxyError::Config(msgs.join("\n")));
}
```

## 4. 验证规则详解

F8a 覆盖 8 条规则，全部针对 `EngineConfig` 字段。每条规则的 `rule` 标识用于程序化过滤，`field` 标识出问题的字段路径。

### 4.1 规则 1：`listen_scheme`

- **验证对象**：`EngineConfig.listen`
- **条件**：非空且以 `tcp://` 或 `unix://` 开头
- **严重度**：`Error`
- **错误消息**：`listen 地址必须以 tcp:// 或 unix:// 开头，当前值: {listen}`

| 配置 | 是否合法 |
| :--- | :--- |
| `listen: tcp://0.0.0.0:3128` | ✅ 合法 |
| `listen: unix:///var/run/docker.sock` | ✅ 合法 |
| `listen: 0.0.0.0:3128` | ❌ 缺 scheme |
| `listen: ""` | ❌ 空 |

### 4.2 规则 2：`target_scheme`

- **验证对象**：`EngineConfig.target`
- **条件**：非空且以 `tcp://` 或 `unix://` 开头
- **严重度**：`Error`
- **错误消息**：`target 地址必须以 tcp:// 或 unix:// 开头，当前值: {target}`

| 配置 | 是否合法 |
| :--- | :--- |
| `target: tcp://127.0.0.1:8080` | ✅ 合法 |
| `target: unix:///tmp/upstream.sock` | ✅ 合法 |
| `target: 127.0.0.1:8080` | ❌ 缺 scheme |

### 4.3 规则 3：`cidr_valid`

- **验证对象**：`EngineConfig.allow` / `EngineConfig.deny` 每一条
- **条件**：按 `"IP/prefix"` 解析，IP 合法（`std::net::IpAddr::from_str`），前缀长度合法（IPv4 ≤ 32，IPv6 ≤ 128）。无 `/n` 时按 IP 单址解析（视为合法）
- **严重度**：`Error`
- **错误消息**：`allow[{i}] 不是合法 CIDR: {entry}（{parse_err}）`（`deny` 同理）

实现上**不引入 `cidr` crate**，使用 `std::net::IpAddr` 手动解析 `"IP/prefix"` 格式：`split('/')` 后 `IpAddr::from_str` 校验 IP，`u8::from_str` 校验前缀长度范围。解析失败产生的 `AddrParseError` 捕获后转为 `ValidationIssue`。

| 配置 | 是否合法 |
| :--- | :--- |
| `allow: [192.168.1.0/24]` | ✅ 合法 IPv4 CIDR |
| `allow: [2001:db8::/32]` | ✅ 合法 IPv6 CIDR |
| `allow: [10.0.0.1]` | ✅ 无前缀按 IP 解析 |
| `allow: [192.168.1.0/33]` | ❌ 前缀越界（IPv4 最大 32） |
| `allow: [not-an-ip/24]` | ❌ IP 解析失败 |
| `deny: [10.0.0.0/8]` | ✅ deny 同 allow 规则 |

### 4.4 规则 4：`tls_paths_nonempty`

- **验证对象**：`TlsConfig.cert_path` / `TlsConfig.key_path`
- **条件**：`tls` 为 `Some` 时两个字段非空
- **严重度**：`Error`
- **错误消息**：`tls.cert_path 不能为空` / `tls.key_path 不能为空`

> 说明：F8a 只校验"非空字符串"，**不校验文件是否存在**（文件存在性属 I/O，违反"纯函数"原则，留给 F8b 加载器处理）。

| 配置 | 是否合法 |
| :--- | :--- |
| `tls: { cert_path: "/etc/cert.pem", key_path: "/etc/key.pem" }` | ✅ 合法 |
| `tls: { cert_path: "", key_path: "/etc/key.pem" }` | ❌ cert_path 空 |
| `tls: null` | ✅ 不校验（None 跳过） |

### 4.5 规则 5：`auth_type_nonempty`

- **验证对象**：`AuthConfig.auth_type`
- **条件**：`auth` 为 `Some` 且 `enabled == true` 时 `auth_type` 非空
- **严重度**：`Error`
- **错误消息**：`auth 启用时 auth_type 不能为空`

> 说明：`enabled: false` 时不校验（鉴权被显式关闭，配置允许空值）。`header_name` 有默认值 `"Authorization"`，不强制校验。

| 配置 | 是否合法 |
| :--- | :--- |
| `auth: { auth_type: "header", enabled: true }` | ✅ 合法 |
| `auth: { auth_type: "", enabled: true }` | ❌ 启用但 auth_type 空 |
| `auth: { auth_type: "", enabled: false }` | ✅ 未启用，跳过 |
| `auth: null` | ✅ 不校验 |

### 4.6 规则 6：`upstream_url_valid`

- **验证对象**：`EngineConfig.upstream`
- **条件**：`Some` 时 `url::Url::parse` 成功且 scheme 为 `http` 或 `https`
- **严重度**：`Error`
- **错误消息**：`upstream 不是合法的 http(s) URL: {upstream}（{err}）`

> 说明：必须显式校验 scheme，避免 `tcp://` 等被误判为合法 upstream。

| 配置 | 是否合法 |
| :--- | :--- |
| `upstream: http://proxy:8080` | ✅ 合法 |
| `upstream: https://proxy.internal:8443` | ✅ 合法 |
| `upstream: proxy:8080` | ❌ 缺 scheme |
| `upstream: tcp://proxy:8080` | ❌ scheme 非 http/https |
| `upstream: null` | ✅ 不校验（None 跳过） |

### 4.7 规则 7：`regex_pattern_valid`

- **验证对象**：`LocationConfig.location`（仅当 `mode` 为 `Regex` 或 `PrefixRegex`）
- **条件**：`regex::Regex::new(location)` 成功
- **严重度**：`Error`
- **错误消息**：`locations[{i}] 的正则模式无效: {location}（{err}）`

> 说明：`Full` / `Prefix` 模式不校验（location 是字面量，非正则）。F7 已修复 `gateway.rs` 的正则 unwrap，但 `LocationConfig` 的正则在 `router` / `mock` 路径上仍可能 panic，本规则是"防御纵深"的补充。

| 配置 | 是否合法 |
| :--- | :--- |
| `locations: [{ location: "^/api/.*$", mode: Regex }]` | ✅ 合法正则 |
| `locations: [{ location: "/api/.*", mode: PrefixRegex }]` | ✅ 合法正则 |
| `locations: [{ location: "[invalid", mode: Regex }]` | ❌ 非法正则（未闭合 `[`） |
| `locations: [{ location: "/api/users", mode: Prefix }]` | ✅ 不校验（非正则模式） |

### 4.8 规则 8：`timeout_positive`

- **验证对象**：`EngineConfig.request_timeout` / `EngineConfig.connection_timeout`
- **条件**：`Some` 时 `> Duration::ZERO`
- **严重度**：`Error`
- **错误消息**：`request_timeout 必须大于 0，当前: {dur:?}`（`connection_timeout` 同理）

> 说明：`Duration::ZERO` 与负值（serde 反序列化不会产生负值，但 `parse_duration` 的 `"0s"` 会得到 `Duration::ZERO`）都判为非法。`None` 不校验（使用默认超时）。

| 配置 | 是否合法 |
| :--- | :--- |
| `request_timeout: 10s` | ✅ 合法 |
| `request_timeout: 0s` | ❌ 等于 0 |
| `request_timeout: null` | ✅ 不校验（使用默认） |
| `connection_timeout: 5s` | ✅ 合法 |
| `connection_timeout: 0s` | ❌ 等于 0 |

## 5. 验证级别说明

`ValidationLevel` 控制"发现问题后怎么办"，与规则的"严重度"正交。

### 5.1 三种级别行为对比

| 级别 | `Error` 处置 | `Warning` 处置 | `is_valid()` 语义 | 适用场景 |
| :--- | :--- | :--- | :--- | :--- |
| `Strict` | 记录并使 `is_valid()` 返回 `false` | 记录 | 无 `Error` 即为 `true` | CI / 生产启动 |
| `Warn` | 记录 | 记录 | 始终 `true` | 开发联调 |
| `Loose`（默认） | 记录 | **丢弃**（不记录） | 始终 `true` | 历史兼容、基线对比 |

### 5.2 级别选择建议

- **生产启动 / CI**：`Strict` — 有错即拒启动，把问题挡在运行前
- **本地开发联调**：`Warn` — 看到所有问题但不阻断启动，便于逐步修复
- **历史兼容 / 仅记录**：`Loose` — 默认值，老配置无感知；未来显式切 `Strict` 时可对比基线

### 5.3 升级路径

| 阶段 | 默认级别 | 行为 |
| :--- | :--- | :--- |
| F8a（本阶段） | `Loose` | 仅提供能力，不强制 |
| F8b（加载器） | `Strict`（启动时） | 加载后校验，`Error` 即拒启动 |
| F8d（UI） | 可配 | 管理 API 暴露级别切换 |

## 6. 输出格式

### 6.1 `ValidationIssue` 字段

```rust
pub struct ValidationIssue {
    pub severity: ValidationSeverity,  // Error / Warning
    pub engine: Option<String>,        // 引擎名；顶层校验时为 None
    pub field: String,                 // 字段路径：listen / allow[0] / locations[1].location
    pub rule: String,                  // 规则标识：listen_scheme / cidr_valid / ...
    pub message: String,               // 人类可读错误消息（中文）
}
```

### 6.2 典型输出示例

对一份含 3 处错误的配置，`errors()` 迭代器产出（伪展示）：

```
[web] listen / listen_scheme: listen 地址必须以 tcp:// 或 unix:// 开头，当前值: 0.0.0.0:3128
[web] allow[0] / cidr_valid: allow[0] 不是合法 CIDR: 192.168.1.0/33（前缀长度越界）
[api] locations[1].location / regex_pattern_valid: locations[1] 的正则模式无效: [invalid（未闭合的字符类）
```

### 6.3 `ValidationResult` 程序化消费

```rust
let result = validator.validate_config(&config);

// 是否通过
println!("is_valid = {}", result.is_valid());

// 问题总数
println!("total issues = {}", result.len());

// 仅 Error
for issue in result.errors() {
    println!("[{}] {} / {}: {}", issue.engine.as_deref().unwrap_or("-"),
             issue.field, issue.rule, issue.message);
}

// 仅 Warning（Strict / Warn 级别下可见；Loose 下为空）
for issue in result.warnings() {
    println!("[{}] {} / {}: {}", issue.engine.as_deref().unwrap_or("-"),
             issue.field, issue.rule, issue.message);
}
```

## 7. 限制与约束

### 7.1 F8a 范围内不做

| 项 | 归属 | 原因 |
| :--- | :--- | :--- |
| 配置加载器（多源合并、环境变量插值） | F8b | 加载是独立复杂度，F8a 先把"拿到 config 后怎么验"做对 |
| 文件监听器（notify 集成） | F8c | 依赖 F8b 的加载器 |
| 热重载 + 管理 UI | F8d | 依赖 F8c 的事件流 |
| 跨引擎冲突检测（端口重复监听等） | F8b | 需要全局视角，F8a 只做单引擎内字段校验 |
| 文件存在性校验（TLS 证书、静态 root） | F8b | 违反"纯函数"原则，属 I/O |
| 配置 schema JSON 导出 | 后续 | 锦上添花，不影响运行时安全 |
| 自动修复建议 | 后续 | 先把"诊断"做对，"治疗"下一步 |

### 7.2 不破坏现有类型

- `EngineConfig` / `MystiConfig` / `TlsConfig` / `AuthConfig` / `LocationConfig` 结构体**零修改**
- `MystiProxyError` **不新增变体**
- `MystiConfig::from_yaml` / `from_yaml_file` 行为不变，不注入校验调用
- 不新增 crate 依赖（`regex`、`url` 已在 `Cargo.toml`）

### 7.3 默认 Loose 的语义

- 丢弃所有 `Warning`（不记录）
- 记录 `Error` 但 `is_valid()` 仍返回 `true`
- 等价于"只看不拦"，现有调用方不主动 `validate_*` 即无任何行为变化

## 8. FAQ

### Q1：为什么校验返回 `ValidationResult` 而不是 `Result<ValidationResult, MystiProxyError>`？

校验语义是"收集所有问题"，`Result` 的短路语义会在遇到第一个错误时返回，丢失后续问题。运维需要一次看到全部错误，避免"修一个试一次"的低效循环。`ValidationResult` 本身已承载"是否通过"的信息（`is_valid()`），调用方可在 `Strict` 级别下自行转换为 `MystiProxyError::Config(String)`。

### Q2：为什么默认 `Loose` 而不是 `Strict`？

向后兼容。F8a 只新增模块，不修改 `from_yaml` 行为；现有能解析的配置继续可用。若默认 `Strict`，任何历史配置中的"语义小瑕疵"都会导致启动失败，造成升级阻力。`Loose` 让 F8a 可以独立交付、独立测试，F8b 再显式升级到 `Strict`。

### Q3：为什么 `Warning` 在 `Loose` 下被丢弃？

`Loose` 的语义是"仅记录 `Error`、不记录 `Warning`"。`Warning` 描述的是"可运行但有风险"的情况，对历史兼容场景而言是噪声；显式切到 `Warn` 或 `Strict` 级别即可看到。

### Q4：为什么 `cidr_valid` 不引入 `cidr` crate？

`std::net::IpAddr` + 手动解析 `"IP/prefix"` 已足够覆盖需求（IP 合法性 + 前缀长度范围）。引入新 crate 会增加依赖体积，且 `cidr` crate 的能力（CIDR 集合运算等）超出 F8a 校验范围。

### Q5：为什么 `tls_paths_nonempty` 不校验文件是否存在？

文件存在性属 I/O 操作，违反"纯函数校验"原则（给定 `&EngineConfig` 应能离线复现校验结果，不依赖文件系统状态）。文件存在性校验留给 F8b 加载器处理。

### Q6：F8a 修复了 F7 没覆盖的正则 panic 吗？

F7 修复了 `gateway.rs` 的 6 处 unwrap，但 `LocationConfig.location` 的正则在 `router` / `mock` 路径上仍可能 panic。F8a 的 `regex_pattern_valid` 规则在加载阶段提前发现非法正则，是"防御纵深"的补充，但**不替代**运行时的 `?` 传播（运行时仍需 F7 式的 unwrap 修复）。

### Q7：`ConfigValidator` 是 `Copy` 的吗？

是的。`ConfigValidator` 仅持有一个 `ValidationLevel`（`Copy` 的 enum），自身也 derive `Copy`。可以低成本克隆或按值传递。`ValidationResult` 不是 `Copy`（持有 `Vec<ValidationIssue>`），但实现 `Clone`。

### Q8：如何对一份 YAML 配置文件批量校验？

F8a 阶段需自行 `from_yaml_file` 后调 `validate_config`：

```rust
let cfg = MystiConfig::from_yaml_file("config.yaml")?;
let result = ConfigValidator::with_level(ValidationLevel::Strict).validate_config(&cfg);
if !result.is_valid() {
    for issue in result.errors() {
        eprintln!("{}", issue);
    }
    std::process::exit(1);
}
```

F8b 将提供 `load_and_validate` 一体化 API。
