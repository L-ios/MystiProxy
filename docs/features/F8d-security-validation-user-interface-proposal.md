# F8d 配置验证框架-安全验证与用户界面 — Proposal

## 背景与动机

### 现状：F8a/F8b/F8c 之后，缺"安全验证"与"友好 UX"两块拼图

MystiProxy 配置验证框架按 4 阶段推进：

| 阶段 | 交付物 | 状态 |
| :--- | :--- | :--- |
| F8a | 验证模型与规则（`ConfigValidator` / `ValidationResult` / 8 条规则） | ✅ 已交付 |
| F8b | 加载器与管理器（`EnhancedConfigLoader` / `ConfigSource` / `ConfigurationManager`） | ✅ 已交付 |
| F8c | 配置文件监听 + 触发热重载（`ConfigFileWatcher` / `ConfigChangeEvent`） | ✅ 已交付 |
| **F8d** | **安全验证 + 用户界面（`SecurityValidator` / `ConfigUserInterface`）** | **本阶段** |

F8a 把"语义错误"前移到加载后显式校验，F8b 把"加载"做成可组合多源的能力，F8c 把"配置"升级为运行时可监控、可热重载的资源。但三者都只覆盖**语义正确性**，**不覆盖安全性**：

| 场景 | F8a–F8c 是否覆盖 | 风险 |
| :--- | :--- | :--- |
| 用户配置 `header.content-length: "100"` | ❌ F8a 的 `validate_*` 只查结构 | 运行时 `content-length` 被伪造，破穿 HTTP 语义 |
| `target: http://127.0.0.1:6379`（Redis 内网） | ❌ F8a 只校验 scheme | SSRF：代理被利用扫描内网 / 访问元数据服务（`169.254.169.254`） |
| `target: file:///etc/passwd` | ❌ F8a 不校验协议 | 本地文件读取漏洞 |
| `allow: ["0.0.0.0/0"]`（全网段放行） | ❌ F8a 的 `cidr_valid` 只校验语法 | IP 过滤策略被"全放行"绕过 |
| 配置文件明文写入 `password: secret123` | ❌ 三个阶段都不查 | 密钥泄露，无任何告警 |

同时，F8a–F8c 的错误输出是**结构化的 `ValidationResult` / `ConfigValidationError`**，对人眼不友好。运维看到 `"Validation(ValidationErrors { ... })"` 这种 Debug 输出，定位问题成本高：

| 痛点 | 现状 | F8d 解决方式 |
| :--- | :--- | :--- |
| 错误消息无颜色区分 | 终端混在一堆日志里，错/警/提示难辨 | `colored` crate 红/黄/绿/蓝分级着色 |
| 无修复建议 | 运维看到 `listen_scheme` 错误不知如何修 | `extract_validation_suggestions` 按 `(field, code)` 给出可执行建议 |
| 无配置摘要 | 启动后不知实际加载了哪些引擎 | `print_config_summary` 打印引擎列表 + locations + 证书数 |
| 校验通过无正向反馈 | "没报错"可能是"没校验" | `verbose` 模式下显式打印 ✓ 成功消息 |

### 框架定位

F8d 是配置验证框架的**第四阶段（收官阶段）**，做"安全验证 + 用户界面"两件事：

- **安全验证**（`SecurityValidator`）：在 F8a 的语义校验之上，叠加"安全规则"——危险头部、SSRF 防护、内网访问拦截、CIDR 宽泛性、敏感信息泄露检测。它**不替代** F8a 的 `ConfigValidator`，而是**正交补充**：F8a 查"对不对"，F8d 查"安不安全"。
- **用户界面**（`ConfigUserInterface`）：把 F8a/F8b/F8c 产出的 `ValidationResult` / `ConfigValidationError` / `MystiConfig` 渲染成人类可读的彩色输出，含修复建议与配置摘要。

F8d 的核心动机：**把"安全"从隐式假设变成显式校验，把"校验结果"从 Debug 字符串变成可操作的运维反馈**。它是框架的收官，把"防御纵深"补到最后一道。

## 需求深度理解

### 为什么必须做（不是可选锦上添花）

1. **代理是安全边界，SSRF 是代理的"原罪"**
   - HTTP 代理天然接收用户指定的 `target`，若不校验，攻击者可让代理访问 `http://127.0.0.1:6379`（Redis）、`http://169.254.169.254/latest/meta-data/`（云元数据）、`http://10.0.0.1/admin`（内网管理面）
   - SSRF 在 OWASP Top 10 中长期上榜，代理是 SSRF 的天然放大器
   - F8a 的 `target_scheme` 只查 `tcp://` / `unix://` 前缀，**不查目标 IP 是否内网**——这是 F8d 必须补的洞

2. **危险头部是 HTTP 语义陷阱**
   - `content-length` / `transfer-encoding` / `connection` 等头部由 HTTP 框架自动管理，用户配置会破穿请求拆包
   - `host` / `authorization` / `proxy-authorization` 由代理自身注入，用户配置会覆盖鉴权
   - F8a 不校验头部内容，F8d 的 `validate_headers` 在配置层把这些"不该被用户设置"的头部拦截

3. **"静默放行"比报错更危险**
   - `allow: ["0.0.0.0/0"]` 语法合法（F8a 的 `cidr_valid` 放行），但语义等于"全放行"，IP 过滤策略失效
   - 配置文件明文 `password: secret123` 不会触发任何告警，密钥泄露无感知
   - F8d 的 `validate_cidr_list` 查"过于宽泛"、`check_sensitive_config` 查"疑似密钥"，把这两类"合法但不安全"的情况显式化

4. **运维反馈闭环缺失**
   - F8a–F8c 的错误是 `ConfigValidationError` enum，`Debug` 输出对运维不可读
   - 没有"这里错了 + 怎么修"的建议，运维需翻源码理解 `listen_scheme` 是什么
   - 没有"配置加载了什么"的摘要，运维不知实际跑了哪些引擎
   - F8d 的 `ConfigUserInterface` 把这三件事一次补齐

### 深层需求

- **安全验证与语义验证分离**：`SecurityValidator` 不复用 F8a 的 `ConfigValidator`，而是独立结构体。原因：安全规则面向"字符串 / URL / CIDR 列表"等原子输入，不依赖 `EngineConfig` 结构；且安全验证可单独用于 CLI 子命令（如 `mystiproxy security-check config.yaml`），不强制走完整语义校验。
- **安全验证返回 `ValidationResult<T>` 而非累积式 `ValidationResult`**：安全问题通常是"第一个就拒"（如 SSRF 必须立即拦截），不需要像 F8a 那样"收集全部问题"。复用 `ConfigValidationError::Security` 变体，与 F8b 加载器的错误流自然衔接。
- **敏感信息只 `warn!` 不 `Err`**：配置中包含 `password:` 不一定是错（可能是合法的鉴权配置），但必须告警。`check_sensitive_config` 返回 `Ok(())` 但发 `tracing::warn!`，让运维知情但不阻断启动。
- **UI 着色可关**：CI / 日志收集系统不支持 ANSI 转义，`color_output: false` 时输出纯文本，保证可管道处理。
- **UI 修复建议可程序化消费**：`extract_validation_suggestions` 返回 `Vec<String>` 而非直接打印，便于管理 API（F8b 的 `ConfigurationManager`）序列化输出给前端。

## 目标与非目标

### 目标（F8d 仅做这些）

1. **`SecurityValidator` 结构体**：提供 `new()` / `Default`，持有可能复用的规则集合（危险头部表、内网 CIDR 表、URL 黑名单正则）
2. **4 个安全验证方法**：
   - `validate_headers(&HashMap<String, String>)` — 危险头部拦截
   - `validate_target_url(&str)` — SSRF 防护（黑名单协议 + 内网 IP 拦截 + scheme 白名单）
   - `validate_cidr_list(&[String])` — 过于宽泛 CIDR 拦截（`prefix == 0`）
   - `check_sensitive_config(&str)` — 敏感信息泄露告警（`warn!` 不 `Err`）
3. **3 类常量**：`DANGEROUS_HEADERS`（10 项）、`INTERNAL_NETWORKS`（8 项含 IPv6）、`URL_BLACKLIST_PATTERNS`（6 项）
4. **`ConfigUserInterface` 结构体**：提供 `new(verbose, color_output)` / `Default`，公开 `verbose` / `color_output` 字段
5. **3 个 UI 方法**：
   - `print_validation_result(&Result<(), ConfigValidationError>)` — 结果渲染 + 修复建议
   - `print_config_summary(&MystiConfig)` — 配置摘要（引擎 / locations / 证书）
   - `extract_validation_suggestions(&ValidationErrors) -> Vec<String>` — 程序化建议提取（`print_validation_result` 内部调用）
6. **新模块**：`mystiproxy/src/config/security.rs` 与 `mystiproxy/src/config/user_interface.rs`，通过 `pub mod` 从 `config/mod.rs` 导出
7. **错误衔接**：安全验证产出的 `ConfigValidationError::Security(String)` 与 F8b 加载器、F8c 热重载的错误流自然衔接，无需新增错误变体

### 非目标（明确推迟或归属其他阶段）

| 推迟项 | 归属 | 原因 |
| :--- | :--- | :--- |
| 管理 API 暴露安全验证 | F9 / 后续 | F8d 只做 CLI / 启动期渲染，API 化留给本地管理模块 |
| 配置 diff 展示 | 后续 | F8c 的热重载事件流可携带 diff，UI 渲染留给后续 |
| 自动修复配置文件 | 后续 | F8d 只给建议，不替运维改文件 |
| 跨引擎安全策略冲突检测 | 后续 | 如两个引擎的 `allow` 互斥，需全局视角，F8d 只做单引擎内安全规则 |
| DNS rebinding 防护 | 后续 | `validate_target_url` 只查 IP 字面量，域名解析后的 IP 检查需运行时 hook，超出配置层范围 |
| TLS 证书内容校验 | F8b | 证书存在性是 I/O，已由 F8b 加载器处理；证书链合法性属运行时 |
| 多文件敏感信息扫描 | 后续 | F8d 的 `check_sensitive_config` 只查单份配置字符串 |

## 利益相关方

| 角色 | 关注点 | F8d 如何满足 |
| :--- | :--- | :--- |
| 运维 / SRE | 改配置后能否提前知道哪里不安全 | 启动期 `SecurityValidator` 报 SSRF / 危险头部 / 宽泛 CIDR，含可读消息 |
| 安全审计 | SSRF / 内网访问 / 密钥泄露是否被拦截 | `validate_target_url` 拦截内网 IP，`check_sensitive_config` 告警密钥模式 |
| 平台开发者 | 校验结果能否程序化消费 | `extract_validation_suggestions` 返回 `Vec<String>`，UI 着色可关 |
| F8b–F8c 实现者 | 安全错误能否接入既有错误流 | `ConfigValidationError::Security` 已在 F8a 定义，F8d 直接复用 |
| 现有用户 | 升级后老配置还能跑吗 | `check_sensitive_config` 只 `warn!` 不 `Err`，不阻断启动；安全验证需显式调用 |
| 终端用户（CLI） | 错误提示是否好看 | `colored` 红/黄/绿/蓝分级，✓/✗ 视觉标识，修复建议可执行 |

## 信心评估

| 决策点 | 信心 | 依据 |
| :--- | :--- | :--- |
| 安全验证与语义验证分离为独立结构体 | 93% | 安全规则面向原子输入（URL/CIDR 字符串），不依赖 `EngineConfig`，可独立用于 CLI 子命令 |
| `validate_target_url` 返回 `Result` 而非累积式 | 90% | SSRF 必须"第一个就拒"，与 F8a 的"收集全部问题"语义不同；复用 `ConfigValidationError::Security` 衔接顺畅 |
| `check_sensitive_config` 只 `warn!` 不 `Err` | 88% | 配置中 `password:` 可能是合法鉴权配置，告警不阻断；但"告警而不阻断"是否够安全，取决于运维是否看日志 |
| 内网 CIDR 表覆盖 IPv4 + IPv6 | 92% | `INTERNAL_NETWORKS` 8 项含 loopback / link-local / unique local，`ipnetwork` crate 已在依赖中 |
| UI 使用 `colored` crate | 95% | `colored` 是 Rust 社区标准终端着色库，零依赖冲突，`color_output: bool` 可关 |
| **整体** | **>90%** | **无需网络调研，均为标准 Rust 模式 + 项目既有依赖（`regex`/`url`/`ipnetwork`/`colored`/`validator`）** |
