# F8a 配置验证框架-模型与验证规则 — Proposal

## 背景与动机

### 现状：配置层零语义验证

MystiProxy 当前的配置加载链路是 `serde_yaml::from_str -> MystiConfig`，**仅做结构与类型校验**。任何"结构合法但语义非法"的配置都能通过解析，问题被推迟到运行时才暴露：

| 配置项 | 非法取值 | 运行时后果 |
| :--- | :--- | :--- |
| `listen` / `target` | 缺少 `tcp://` / `unix://` 前缀 | 启动时 `bind` 失败 panic，或连接目标解析异常 |
| `allow` / `deny` | `"192.168.1.1/33"`（非法 CIDR） | IP 过滤匹配时 panic 或静默放行 |
| `locations[].mode: Regex` | `location: "[invalid"` | 请求热路径 `Regex::new` panic（F7 已部分修复 gateway.rs，但 location 正则未覆盖） |
| `tls.cert_path` / `key_path` | 空字符串 | 运行时 `open` 失败，TLS 引擎启动崩溃 |
| `auth.auth_type` | 空字符串但 `enabled: true` | 鉴权逻辑空转，可能静默放行 |
| `upstream` | `"proxy:8080"`（缺 scheme） | 首个请求时连接失败 |
| `request_timeout` | `"0s"` 或负数 | 所有请求立即超时 |

### 框架定位

根级设计文档（`CONFIG_VALIDATION_FRAMEWORK_DESIGN.md`、`IMPLEMENTATION_ROADMAP.md`、`CONFIG_SYSTEM_ANALYSIS.md`）已提出 4 阶段框架，但**引用的是旧配置结构**，且未落地。F8a 是该框架的**第一阶段**，只做"模型 + 规则"，不触碰加载器、监听器、热重载。

F8a 的核心动机：**把"语义错误"从运行时 panic/静默失败，前移到配置加载后的显式校验阶段**，为 F8b（加载器）/ F8c（监听器）/ F8d（热重载 + UI）打地基。

## 需求深度理解

### 为什么必须做（不是可选锦上添花）

1. **代理是基础设施层，panic 不可接受**
   - 单条请求触发 `Regex::new` panic 会击穿所有并发连接的可用性
   - F7 修复了 `gateway.rs` 的 6 处 unwrap，但 `LocationConfig` 的正则在 `router` / `mock` 路径上仍可能 panic
   - 校验层是"防御纵深"的第一道，比修复单点 unwrap 更系统

2. **"静默失败"比 panic 更危险**
   - 非法 CIDR 导致过滤规则失效 → 安全策略被绕过
   - 空 `auth_type` 导致鉴权空转 → 认证被绕过
   - 这类问题不会触发告警，只在事故复盘时才被发现

3. **现有配置无任何"健康度"信号**
   - 运维改完 YAML 重启，只能靠"没崩就是对了"判断
   - 缺少结构化的 `ValidationResult` 供管理 API（F8d UI）展示

### 深层需求

- **校验必须是"纯函数"**：不读文件、不做 I/O、不依赖运行时状态。给定 `&EngineConfig` 应能离线复现校验结果。这保证校验逻辑可单测、可在 CI 中对配置文件批量校验。
- **校验必须"累积"而非"短路"**：一个配置里同时有 3 处错误，应一次性报告 3 条 `ValidationIssue`，而不是修一个才能看到下一个。这对运维效率至关重要。
- **校验级别必须可调**：CI / 生产启动要 `Strict`（有错即拒），开发联调要 `Warn`（告警但放行），历史兼容要 `Loose`（仅记录）。

## 目标与非目标

### 目标（F8a 仅做这些）

1. **验证数据模型**：`ValidationLevel`（Strict / Warn / Loose）、`ValidationSeverity`（Error / Warning）、`ValidationIssue`、`ValidationResult`
2. **验证规则**：覆盖 `EngineConfig` 的 8 类字段（listen / target / CIDR / TLS / auth / upstream / regex / timeout）
3. **`ConfigValidator` 结构体**：提供 `new()` / `with_level(level)` / `validate_engine(name, &EngineConfig)` / `validate_config(&MystiConfig)`，返回 `ValidationResult`
4. **`ValidationResult` API**：`is_valid()` / `issues()` / `errors()` / `warnings()` / `merge(other)`
5. **新模块**：`mystiproxy/src/config/validation.rs`，通过 `pub mod validation;` 从 `config/mod.rs` 导出
6. **默认 Loose 级别**：保证向后兼容，现有能解析的配置继续可用

### 非目标（明确推迟到 F8b–F8d）

| 推迟项 | 归属阶段 | 原因 |
| :--- | :--- | :--- |
| 配置加载器（多源合并、环境变量插值） | F8b | 加载是独立复杂度，F8a 先把"拿到 config 后怎么验"做对 |
| 文件监听器（notify 集成） | F8c | 依赖 F8b 的加载器 |
| 热重载 + 管理 UI | F8d | 依赖 F8c 的事件流 |
| 跨引擎冲突检测（端口重复监听等） | F8b | 需要全局视角，F8a 只做单引擎内字段校验 |
| 配置 schema JSON 导出 | 后续 | 锦上添花，不影响运行时安全 |
| 自动修复建议 | 后续 | 先把"诊断"做对，"治疗"下一步 |

## 利益相关方

| 角色 | 关注点 | F8a 如何满足 |
| :--- | :--- | :--- |
| 运维 / SRE | 改配置后能否提前知道哪里错了 | `Strict` 级别启动即报错，错误消息含引擎名 + 字段 + 原因 |
| 平台开发者 | 校验结果能否程序化消费 | `ValidationResult` 是结构化类型，可序列化供管理 API 使用 |
| F8b–F8d 实现者 | 校验模型是否稳定可依赖 | F8a 锁定数据模型与规则，后续阶段只组合不重写 |
| 安全审计 | 非法 CIDR / 空鉴权是否被拦截 | 规则 3（CIDR）、规则 5（auth）直接覆盖 |
| 现有用户 | 升级后老配置还能跑吗 | 默认 `Loose`，仅记录不阻断；显式切 `Strict` 才拒 |

## 信心评估

| 决策点 | 信心 | 依据 |
| :--- | :--- | :--- |
| 纯函数 + 累积式 `ValidationResult` 设计 | 95% | Rust 社区标准模式（参考 `validator` crate 设计思路），无外部依赖 |
| 8 条规则的技术可行性 | 92% | `regex`/`url`/`std::net` 依赖均已存在于 `Cargo.toml`，无需新增 |
| 默认 `Loose` 保证向后兼容 | 90% | 现有调用方不主动调 `validate_*` 即无任何行为变化 |
| `ValidationResult` 非 `Result` 别名 | 88% | 校验语义是"收集所有问题"而非"第一个错误即返回"，与 `Result` 短路语义冲突 |
| **整体** | **>85%** | **无需网络调研，均为标准 Rust 模式 + 项目既有依赖** |
