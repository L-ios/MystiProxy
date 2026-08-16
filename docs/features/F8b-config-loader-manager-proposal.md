# F8b 配置验证框架-加载器与管理器 — Proposal

## 背景与动机

### 现状：配置加载与运行时管理脱节

MystiProxy 在 F8a 之前，配置加载链路是 `serde_yaml::from_str -> MystiConfig`，**只做结构与类型校验**。F8a 已交付"模型 + 规则"层（`validation/` 模块、`validate_engine_config`、`ConfigValidationError`、`ValidationResult<T>`），把语义错误前移到加载后显式校验。

但 F8a 的设计明确将以下两件事推迟到 F8b：

1. **"拿到 config 后怎么加载"** —— F8a 不规定调用时机，调用方需自行 `from_yaml_file` 后再调 `validate_*`，缺少一体化 API
2. **"加载之后怎么管理"** —— F8a 只产出一个 `MystiConfig` 值，没有"当前配置 / 历史版本 / 变更通知 / 回滚"的运行时容器

实际生产中暴露的痛点：

| 痛点 | 现状后果 | F8b 解决方式 |
| :--- | :--- | :--- |
| 单一文件源 | 无法用环境变量覆盖（如 K8s ConfigMap 注入）、无默认值兜底 | `ConfigSource` 多源合并，后添加优先级高 |
| 加载与验证割裂 | 调用方需手写 `from_yaml + validate` 两步，易遗漏验证 | `EnhancedConfigLoader::load<T>()` 一体化加载 + 验证 |
| 验证级别不可调 | CI / 生产启动要"有错即拒"，本地联调要"告警但放行" | `ValidationLevel`（Strict / Warning / None）三档 |
| 无配置快照 | 改完配置出问题无法回滚到上一版本 | `ConfigurationManager` 维护 `Vec<ConfigSnapshot>` |
| 无变更通知 | 热重载（F8c/F8d）需要"配置变了"事件流 | `broadcast::Sender<ConfigChangeEvent>` 订阅模型 |
| 无当前配置容器 | 运行时各模块各持一份 `MystiConfig` 副本，状态发散 | `Arc<AsyncRwLock<MystiConfig>>` 单一可信源 |

### 框架定位

根级设计文档（`CONFIG_VALIDATION_FRAMEWORK_DESIGN.md`、`IMPLEMENTATION_ROADMAP.md`、`CONFIG_SYSTEM_ANALYSIS.md`）已提出 4 阶段框架。F8a 是第一阶段（模型 + 规则），**F8b 是第二阶段**，做"加载器 + 管理器"：

- **加载器**（`EnhancedConfigLoader`）：把 F8a 的 `validate_engine_config` 嵌入加载链路，提供多源合并 + 级别可调的一体化 API
- **管理器**（`ConfigurationManager`）：把加载产出的 `MystiConfig` 包成运行时容器，提供快照 / 历史 / 回滚 / 订阅能力，为 F8c（文件监听）/ F8d（热重载 + UI）打地基

F8b 的核心动机：**把"配置"从一次性加载的值，升级为可观测、可回滚、可订阅的运行时资源**，同时让"加载 + 验证"成为不可绕过的一体化入口。

## 需求深度理解

### 为什么必须做（不是可选锦上添花）

1. **多源合并是云原生部署的硬需求**
   - K8s 部署中，基础配置走 ConfigMap（YAML 文件），敏感参数走环境变量（`MYSTI__ENGINE__WEB__LISTEN=tcp://0.0.0.0:8080`），两者必须能合并
   - 当前 `serde_yaml::from_str` 只能读单文件，环境变量覆盖需手写代码，每个调用方重复造轮子
   - `config` crate 已在 `Cargo.toml`（`config = { version = "0.15.25", features = ["yaml"] }`），F8b 把它封装成 MystiProxy 风格的 API

2. **"加载即验证"必须强制，不能靠调用方自觉**
   - F8a 已提供 `validate_engine_config`，但调用方可能忘记调
   - F8b 让 `EnhancedConfigLoader::load` 内部自动调验证，`Strict` 级别下"未通过验证的配置无法加载"，从机制上消除遗漏

3. **运行时配置必须有"单一可信源"**
   - 当前每个引擎模块各持一份 `MystiConfig` 副本，热重载时需逐个通知更新，状态发散
   - F8b 的 `ConfigurationManager` 持有 `Arc<AsyncRwLock<MystiConfig>>`，所有模块从同一处读，热重载只需更新这一处

4. **"改坏了能回滚"是运维底线**
   - 生产环境改配置出问题，最朴素的诉求是"回到上一版"
   - F8b 维护 `Vec<ConfigSnapshot>`（默认上限 10），提供 `rollback_to_previous()`
   - 没有 F8b，运维只能 `git revert` + 重启，时间长、易出错

### 深层需求

- **加载器必须是 builder 模式**：`new() -> with_validation_level() -> add_source() -> load()` 链式调用，配置源顺序即优先级顺序（后添加覆盖先添加），可单测、可组合。
- **加载器必须泛型**：`load<T: DeserializeOwned + Serialize>()` 不写死 `MystiConfig`，未来可加载子配置（如单独的 `EngineConfig`、`CertConfig`）做局部校验。
- **管理器必须异步**：基于 `tokio::sync::RwLock`（当前配置）+ `tokio::sync::broadcast`（变更通知），不阻塞异步运行时。历史快照用 `std::sync::RwLock`（不跨 await 持有）。
- **变更通知必须是广播**：多个订阅者（如 metrics 模块、日志模块、F8c 监听器）都能收到 `ConfigChangeEvent`，而非单消费者队列。
- **快照版本必须可追溯**：每个 `ConfigSnapshot` 带 `version`（纳秒时间戳）、`timestamp`、`source`（"initial" / "reload"），供 F8d UI 展示历史时间线。

## 目标与非目标

### 目标（F8b 仅做这些）

1. **`ValidationLevel` 枚举**：`Strict`（验证失败即 Err）/ `Warning`（验证失败仅 `tracing::warn`）/ `None`（跳过验证），位于 `loader.rs`
2. **`ConfigSource` 枚举**：`File(String)` / `Environment(String)` / `Default(serde_json::Value)`，三种配置源
3. **`EnhancedConfigLoader` 结构体**：builder 模式，提供 `new()` / `with_validation_level(level)` / `add_source(source)` / `load<T>()` / `load_mysti_config(path)` 便捷方法
4. **多源合并加载**：内部使用 `config` crate 的 `Config::builder`，后添加的源优先级更高（覆盖先添加的）
5. **加载即验证**：`load<T>()` 在反序列化后，对 `mysti.engine` 中每个 `EngineConfig` 调 `validate_engine_config`，按 `ValidationLevel` 决定处置
6. **`ConfigSnapshot` 结构体**：`config` / `timestamp` / `version` / `source`，记录配置历史
7. **`ConfigChangeEvent` 结构体**：`old_config` / `new_config` / `timestamp` / `validation_success`，广播给订阅者
8. **`ConfigurationManager` 结构体**：提供 `new(initial_config)` / `get_current()` / `update_config(new_config)` / `rollback_to_previous()` / `get_history()` / `subscribe()`
9. **新模块**：`mystiproxy/src/config/loader.rs` 与 `mystiproxy/src/config/manager.rs`，通过 `pub mod loader; pub mod manager;` 从 `config/mod.rs` 导出，并 `pub use` 关键类型
10. **默认 Strict 级别**：`EnhancedConfigLoader::new()` 默认 `Strict`，与 F8a 的"默认 Loose"不同——F8b 是加载入口，应默认严格

### 非目标（明确推迟到 F8c–F8d）

| 推迟项 | 归属阶段 | 原因 |
| :--- | :--- | :--- |
| 文件监听器（notify 集成） | F8c | 依赖 F8b 的 `ConfigurationManager::update_config` 作为变更入口 |
| 热重载自动触发 | F8c | F8b 只提供"手动调 `update_config`"能力，自动触发需 F8c 监听文件变化 |
| 管理 UI / REST API | F8d | 依赖 F8b 的事件流与历史快照 |
| 跨引擎冲突检测（端口重复监听等） | 后续 | 需要全局视角，F8b 只做单引擎内字段校验（沿用 F8a 范围） |
| 文件存在性校验（TLS 证书、静态 root） | 后续 | 属 I/O，加载器目前只校验路径非空（F8a 规则 4），不打开文件 |
| 配置 schema JSON 导出 | 后续 | 锦上添花，不影响运行时安全 |
| 配置加密 / 密钥管理 | 后续 | 属安全层，由 `security.rs` 独立模块负责 |
| 多节点配置同步 | 后续 | 分布式范畴，F8b 只做单进程内管理 |

## 利益相关方

| 角色 | 关注点 | F8b 如何满足 |
| :--- | :--- | :--- |
| 运维 / SRE | 改配置后能否回滚、能否用环境变量覆盖 | `rollback_to_previous()` 一键回滚；`ConfigSource::Environment` 支持 K8s 环境变量注入 |
| 平台开发者 | 加载配置能否一步到位（加载 + 验证） | `EnhancedConfigLoader::load_mysti_config(path)` 一行完成加载 + Strict 验证 |
| F8c–F8d 实现者 | 管理器 API 是否稳定可依赖 | F8b 锁定 `ConfigurationManager` 接口，F8c 只调 `update_config`，F8d 只读 `get_history` / `subscribe` |
| K8s 部署者 | 能否用 ConfigMap + 环境变量分层配置 | `add_source(File).add_source(Environment)` 链式合并，环境变量覆盖文件 |
| 安全审计 | 加载时是否强制验证 | 默认 `Strict`，未通过验证的配置 `load` 返回 `Err`，无法进入运行时 |
| 现有用户 | 升级后老配置还能跑吗 | `ValidationLevel::None` 可关闭验证；现有 `from_yaml` 路径不变，F8b 是新增 API |

## 信心评估

| 决策点 | 信心 | 依据 |
| :--- | :--- | :--- |
| builder 模式 + 泛型 `load<T>` 设计 | 95% | Rust 社区标准模式（参考 `config` crate 自身 API），`config` crate 已在依赖中 |
| 多源合并用 `config` crate | 92% | `config = "0.15.25"` 已在 `Cargo.toml`，`Config::builder + add_source` 原生支持 File / Environment / JSON 字符串 |
| `ValidationLevel` 三档（Strict / Warning / None） | 90% | 与 F8a 的 Strict / Warn / Loose 命名不同但语义对应；F8b 用 `None` 替代 `Loose` 更直白（"无验证"） |
| `Arc<AsyncRwLock<MystiConfig>>` 单一可信源 | 93% | tokio 官方推荐的共享可变状态模式，`get_current` 异步读不阻塞 |
| `broadcast::Sender<ConfigChangeEvent>` 广播通知 | 88% | tokio broadcast 支持多订阅者，容量 100 足够；潜在风险是订阅者消费慢导致 lagged |
| 历史用 `std::sync::RwLock` 而非 async | 85% | `save_snapshot` / `get_history` 内无 await，`std::sync::RwLock` 更轻量；需保证不跨 await 持有 |
| 版本号用纳秒时间戳 | 80% | 单进程内纳秒级冲突概率极低；多进程或时钟回拨场景需额外保护（F8b 单进程，可接受） |
| 默认 `Strict`（与 F8a 默认 `Loose` 不同） | 88% | F8b 是加载入口，"默认严格"符合基础设施层防御原则；调用方可显式切 `None` 回退 |
| **整体** | **>88%** | **无需网络调研，均为标准 tokio + config crate 模式** |
