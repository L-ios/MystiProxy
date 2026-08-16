# F8b 配置验证框架-加载器与管理器 — Spec

## 1. 概述

F8b 为 MystiProxy 引入**配置加载器**与**运行时配置管理器**：在 F8a（验证模型与规则）之上，把"加载 + 验证"封装成一体化入口（`EnhancedConfigLoader`），并把加载产出的 `MystiConfig` 包成可观测、可回滚、可订阅的运行时容器（`ConfigurationManager`）。

本阶段是配置验证框架的**第二阶段**，依赖 F8a 的 `validate_engine_config` / `ConfigValidationError` / `ValidationResult<T>`，不包含文件监听、自动热重载、管理 UI（分别归属 F8c / F8c / F8d）。F8b 的全部代码位于 `mystiproxy/src/config/loader.rs` 与 `mystiproxy/src/config/manager.rs`，**零修改现有配置结构体**，默认 `Strict` 级别保证"加载即验证"。

### 设计要点

| 维度 | 选择 | 理由 |
| :--- | :--- | :--- |
| 加载语义 | 加载即验证（`load` 内部调 `validate_engine_config`） | 调用方无法绕过验证，从机制上消除遗漏 |
| 多源合并 | `config` crate builder，后添加优先 | 支持 K8s 文件 + 环境变量分层配置 |
| 返回类型 | `ValidationResult<T>`（即 `Result<T, ConfigValidationError>`） | 复用 F8a 错误类型，不新增变体 |
| 默认级别 | `Strict` | F8b 是加载入口，"默认严格"符合基础设施防御原则 |
| 管理器状态 | `Arc<AsyncRwLock<MystiConfig>>` 单一可信源 | 运行时所有模块从同一处读，避免状态发散 |
| 变更通知 | `tokio::sync::broadcast` 广播 | 多订阅者（metrics / log / watcher）都能收到 |
| 历史快照 | `Vec<ConfigSnapshot>`，上限 10，FIFO 淘汰 | 支持回滚与审计，内存占用可控 |
| 依赖增量 | 零新 crate | `config = "0.15.25"` 已在 `Cargo.toml` |

## 2. 功能说明

### 2.1 模块结构

```
mystiproxy/src/config/
├── mod.rs              ← 修改：新增 `pub mod loader; pub mod manager;` 及 `pub use`
├── validation/         ← F8a 已交付（不动）
│   ├── mod.rs
│   ├── error.rs        ← ConfigValidationError / ValidationResult
│   └── rules.rs        ← validate_engine_config
├── loader.rs           ← 新增：F8b 加载器
└── manager.rs          ← 新增：F8b 管理器
```

`loader.rs` 与 `manager.rs` 仅依赖 `config/mod.rs` 现有类型 + F8a `validation` 模块 + 标准库 + 已有依赖（`config`、`tokio`、`serde`、`serde_json`、`tracing`）。

### 2.2 公共类型一览

| 类型 | 模块 | 用途 |
| :--- | :--- | :--- |
| `ValidationLevel` | `loader` | 验证级别：`Strict` / `Warning` / `None`（默认 `Strict`） |
| `ConfigSource` | `loader` | 配置源：`File(String)` / `Environment(String)` / `Default(serde_json::Value)` |
| `EnhancedConfigLoader` | `loader` | 加载器入口，builder 模式 |
| `ConfigSnapshot` | `manager` | 配置快照（含 config / timestamp / version / source） |
| `ConfigChangeEvent` | `manager` | 变更事件（含 old / new / timestamp / validation_success） |
| `ConfigurationManager` | `manager` | 管理器入口，持有当前配置 / 历史 / 通知发送端 |

### 2.3 公共 API

#### 加载器 API

| 方法 | 签名 | 用途 |
| :--- | :--- | :--- |
| `EnhancedConfigLoader::new` | `fn new() -> Self` | 创建加载器，默认 `Strict` |
| `EnhancedConfigLoader::default` | `fn default() -> Self` | 等同 `new()`（实现 `Default`） |
| `with_validation_level` | `fn with_validation_level(self, level: ValidationLevel) -> Self` | 链式设置验证级别 |
| `add_source` | `fn add_source(self, source: ConfigSource) -> Self` | 链式添加配置源（后添加优先级高） |
| `load<T>` | `fn load<T>(self) -> ValidationResult<T> where T: DeserializeOwned + Serialize` | 加载并验证，泛型返回 |
| `load_mysti_config` | `fn load_mysti_config(path: &str) -> ValidationResult<MystiConfig>` | 便捷方法：单文件加载 `MystiConfig` |

#### 管理器 API

| 方法 | 签名 | 用途 |
| :--- | :--- | :--- |
| `ConfigurationManager::new` | `fn new(initial_config: MystiConfig) -> Result<Self, ConfigValidationError>` | 创建管理器，保存初始快照 |
| `get_current` | `async fn get_current(&self) -> MystiConfig` | 异步读当前配置（克隆返回） |
| `update_config` | `async fn update_config(&self, new_config: MystiConfig) -> ValidationResult<()>` | 验证 + 更新 + 快照 + 通知 |
| `rollback_to_previous` | `async fn rollback_to_previous(&self) -> ValidationResult<()>` | 回滚到上一个版本 |
| `get_history` | `fn get_history(&self) -> Vec<ConfigSnapshot>` | 同步读历史快照列表 |
| `subscribe` | `fn subscribe(&self) -> broadcast::Receiver<ConfigChangeEvent>` | 订阅变更事件 |

## 3. 使用示例

### 3.1 基本用法：单文件加载 + 验证

```rust
use mystiproxy::config::loader::EnhancedConfigLoader;

// 默认 Strict：未通过验证的配置会返回 Err
let config = EnhancedConfigLoader::load_mysti_config("config.yaml")?;
// config: MystiConfig，已通过 validate_engine_config
```

### 3.2 多源合并：文件 + 环境变量

```rust
use mystiproxy::config::loader::{ConfigSource, EnhancedConfigLoader, ValidationLevel};

let config = EnhancedConfigLoader::new()
    .add_source(ConfigSource::File("config.yaml".to_string()))
    .add_source(ConfigSource::Environment("MYSTI".to_string()))
    .load::<MystiConfig>()?;
// 环境变量 MYSTI__ENGINE__WEB__LISTEN=tcp://0.0.0.0:9090 会覆盖文件中的 listen
```

> 环境变量分隔符为 `__`（双下划线），前缀 `MYSTI` 会被剥离。如 `MYSTI__ENGINE__WEB__LISTEN` 对应 `mysti.engine.web.listen`。

### 3.3 警告模式：本地联调不阻断

```rust
use mystiproxy::config::loader::{ConfigSource, EnhancedConfigLoader, ValidationLevel};

let config = EnhancedConfigLoader::new()
    .with_validation_level(ValidationLevel::Warning)
    .add_source(ConfigSource::File("config.yaml".to_string()))
    .load::<MystiConfig>()?;
// 验证失败会 tracing::warn! 记录，但仍返回 Ok(config)
```

### 3.4 默认值兜底

```rust
use mystiproxy::config::loader::{ConfigSource, EnhancedConfigLoader};
use serde_json::json;

let defaults = json!({
    "mysti": {
        "engine": {
            "default": {
                "listen": "tcp://0.0.0.0:3128",
                "target": "tcp://127.0.0.1:8080",
                "proxy_type": "http"
            }
        }
    }
});

let config = EnhancedConfigLoader::new()
    .add_source(ConfigSource::Default(defaults))
    .add_source(ConfigSource::File("config.yaml".to_string()))
    .load::<MystiConfig>()?;
// 文件中的配置覆盖默认值；文件未定义的字段使用默认值
```

### 3.5 管理器：创建 + 更新 + 订阅

```rust
use mystiproxy::config::manager::ConfigurationManager;
use mystiproxy::config::loader::EnhancedConfigLoader;

let initial = EnhancedConfigLoader::load_mysti_config("config.yaml")?;
let manager = ConfigurationManager::new(initial)?;

// 订阅变更事件
let mut rx = manager.subscribe();
tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        tracing::info!("config changed at {:?}", event.timestamp);
    }
});

// 热更新配置（手动触发；F8c 将自动触发）
let new_config = EnhancedConfigLoader::load_mysti_config("config-v2.yaml")?;
manager.update_config(new_config).await?;  // 验证 + 更新 + 快照 + 广播
```

### 3.6 回滚到上一版本

```rust
// 假设已经 update_config 过一次，历史中有 2 条快照
let history = manager.get_history();
assert_eq!(history.len(), 2);

manager.rollback_to_previous().await?;  // 回到上一个版本
let current = manager.get_current().await;
// current 现在等于 history[0].config（回滚前的"上一个版本"）
```

### 3.7 查看配置历史

```rust
for snapshot in manager.get_history() {
    println!(
        "version={}, source={}, timestamp={:?}, engines={}",
        snapshot.version,
        snapshot.source,
        snapshot.timestamp,
        snapshot.config.mysti.engine.len()
    );
}
// 输出示例：
// version=1786769492000000000, source=initial, timestamp=..., engines=1
// version=1786769495000000000, source=reload, timestamp=..., engines=2
```

## 4. 加载器 API 详解

### 4.1 `ValidationLevel`

```rust
pub enum ValidationLevel {
    /// 严格模式：验证失败即返回 Err（默认）
    Strict,
    /// 警告模式：验证失败仅 tracing::warn! 记录，仍返回 Ok
    Warning,
    /// 无验证模式：完全跳过验证
    None,
}
```

| 级别 | 验证失败处置 | `load` 返回 | 适用场景 |
| :--- | :--- | :--- | :--- |
| `Strict`（默认） | `return Err(ConfigValidationError::Validation(...))` | `Err` | CI / 生产启动 |
| `Warning` | `tracing::warn!("Engine '{}' validation warnings: {}", name, e)` | `Ok(config)` | 本地联调 |
| `None` | 跳过验证逻辑（连 `serde_json::to_value` 都不调） | `Ok(config)` | 已知配置可靠 / 内部测试 |

### 4.2 `ConfigSource`

```rust
pub enum ConfigSource {
    /// YAML 文件路径（如 "config.yaml"）
    File(String),
    /// 环境变量前缀（如 "MYSTI"，分隔符 "__"）
    Environment(String),
    /// 默认值（serde_json::Value，内部序列化为 JSON 注入 builder）
    Default(serde_json::Value),
}
```

| 源类型 | 底层 `config` crate Source | 优先级 |
| :--- | :--- | :--- |
| `File(path)` | `File::new(path, FileFormat::Yaml)` | 按 `add_source` 顺序，后添加优先级高 |
| `Environment(prefix)` | `Environment::with_prefix(prefix).separator("__")` | 同上 |
| `Default(value)` | `File::from_str(json_str, FileFormat::Json)`（value 先序列化为 JSON 字符串） | 同上 |

**优先级规则**：`add_source` 越晚调用的，优先级越高（覆盖先前源的相同字段）。典型用法：

```rust
// 文件为基础，环境变量覆盖，默认值兜底（顺序：默认 → 文件 → 环境变量）
.add_source(ConfigSource::Default(defaults))      // 优先级最低
.add_source(ConfigSource::File("config.yaml"))    // 中
.add_source(ConfigSource::Environment("MYSTI"))   // 优先级最高
```

### 4.3 `EnhancedConfigLoader::load<T>`

```rust
pub fn load<T>(self) -> ValidationResult<T>
where
    T: DeserializeOwned + serde::Serialize,
```

**泛型约束**：`T: DeserializeOwned + Serialize`。`DeserializeOwned` 保证能从 `config` crate 的中间 `Value` 反序列化；`Serialize` 用于 `serde_json::to_value` 反射出 `mysti.engine` 做验证。

**验证范围**：仅校验 `mysti.engine` 映射中的每个 `EngineConfig`。若 `T` 不含 `mysti.engine` 字段（如加载子配置），验证逻辑静默跳过（`serde_json::to_value` 成功但 `.get("mysti")` 返回 `None`）。

**错误变体**（复用 F8a，不新增）：

| 错误 | 变体 | 触发条件 |
| :--- | :--- | :--- |
| 文件不存在 / YAML 语法错误 | `ConfigValidationError::Load(String)` | `Config::builder().build()` 失败 |
| 类型不匹配 / 缺失必填 | `ConfigValidationError::Parse(String)` | `try_deserialize` 失败 |
| F8a 规则未通过（仅 Strict） | `ConfigValidationError::Validation(ValidationErrors)` | `validate_engine_config` 返回 `Err` |

### 4.4 `load_mysti_config` 便捷方法

```rust
pub fn load_mysti_config(path: &str) -> ValidationResult<MystiConfig> {
    Self::new()
        .add_source(ConfigSource::File(path.to_string()))
        .load()
}
```

等价于"单文件 + 默认 Strict"。最常用入口，适合不需要多源合并的场景。

## 5. 管理器 API 详解

### 5.1 `ConfigSnapshot`

```rust
#[derive(Debug, Clone)]
pub struct ConfigSnapshot {
    pub config: MystiConfig,
    pub timestamp: SystemTime,
    pub version: String,    // 纳秒时间戳字符串
    pub source: String,     // "initial" / "reload"
}
```

| 字段 | 类型 | 说明 |
| :--- | :--- | :--- |
| `config` | `MystiConfig` | 快照时的完整配置（克隆） |
| `timestamp` | `SystemTime` | 快照生成时刻 |
| `version` | `String` | 版本号，`SystemTime::now().duration_since(UNIX_EPOCH).as_nanos()` 转字符串 |
| `source` | `String` | `"initial"`（`new` 时）或 `"reload"`（`update_config` 时） |

### 5.2 `ConfigChangeEvent`

```rust
#[derive(Debug, Clone)]
pub struct ConfigChangeEvent {
    pub old_config: MystiConfig,
    pub new_config: MystiConfig,
    pub timestamp: SystemTime,
    pub validation_success: bool,  // 当前实现恒为 true
}
```

| 字段 | 类型 | 说明 |
| :--- | :--- | :--- |
| `old_config` | `MystiConfig` | 变更前配置 |
| `new_config` | `MystiConfig` | 变更后配置 |
| `timestamp` | `SystemTime` | 变更时刻 |
| `validation_success` | `bool` | 验证是否成功（当前恒 `true`，因验证失败时 `update_config` 直接返回 `Err`，不发出事件） |

> `validation_success` 字段为未来"软验证"预留：未来若 `update_config` 支持 `Warning` 级别（验证失败仅告警但仍更新），此字段会标记为 `false`，订阅者可据此决定是否告警。

### 5.3 `ConfigurationManager::new`

```rust
pub fn new(initial_config: MystiConfig) -> Result<Self, ConfigValidationError>
```

- **创建 `broadcast::channel(100)`**：容量 100，足够多订阅者
- **初始化 `current_config`**：`Arc::new(AsyncRwLock::new(initial_config.clone()))`
- **保存初始快照**：`save_snapshot(&initial_config, "initial")`
- **`max_history_size = 10`**：硬编码上限

### 5.4 `get_current`

```rust
pub async fn get_current(&self) -> MystiConfig
```

- 异步读 `current_config`，返回克隆（避免持有锁）
- 无错误返回（读操作不会失败）

### 5.5 `update_config`

```rust
pub async fn update_config(&self, new_config: MystiConfig) -> ValidationResult<()>
```

**流程**：
1. `get_current()` 获取 `old_config`（用于事件）
2. `serde_json::to_value(&new_config)` 反射出 `mysti.engine`
3. 遍历 `engines_map`，对每个 `EngineConfig` 调 `validate_engine_config`（恒 Strict 语义，`?` 短路）
4. `current_config.write().await` 更新为新配置（短临界区）
5. `save_snapshot(&new_config, "reload")` 保存快照
6. `reload_notifier.send(event)` 广播变更事件

**验证语义**：`update_config` 恒为 Strict（验证失败即 `Err`），不支持 `Warning` / `None`。这是设计选择——热重载不能引入未验证配置，否则运行时可能因非法配置 panic。

### 5.6 `rollback_to_previous`

```rust
pub async fn rollback_to_previous(&self) -> ValidationResult<()>
```

- **前置条件**：`history.len() >= 2`（当前 + 上一版）
- **回滚目标**：`history[len - 2].config`（倒数第二个快照）
- **实现**：取到 `previous_config` 后调 `update_config(previous_config)`，复用全流程
- **副作用**：回滚会在历史中再增加一条 `"reload"` 快照（含回滚后的配置），符合"每次变更都留痕"

**错误**：
- 历史不足 2 条：`ConfigValidationError::Load("No previous version to rollback to")`
- 回滚目标验证失败：透传 `validate_engine_config` 的 `Validation` 错误

### 5.7 `get_history`

```rust
pub fn get_history(&self) -> Vec<ConfigSnapshot>
```

- 同步读 `config_history`，返回克隆
- 顺序：从旧到新（`history[0]` 最旧，`history[len-1]` 最新）
- 无错误返回

### 5.8 `subscribe`

```rust
pub fn subscribe(&self) -> broadcast::Receiver<ConfigChangeEvent>
```

- 返回新的 `broadcast::Receiver`，可多次调用获取多个订阅者
- 容量 100，消费慢会收到 `RecvError::Lagged(n)`
- 发送端关闭（manager 被 drop）后收到 `RecvError::Closed`

**推荐消费模式**：

```rust
let mut rx = manager.subscribe();
tokio::spawn(async move {
    loop {
        match rx.recv().await {
            Ok(event) => { /* 处理事件 */ }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("config event subscriber lagged, skipped {} events", n);
                continue;
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
});
```

## 6. 配置源详解

### 6.1 `ConfigSource::File`

```rust
ConfigSource::File("config.yaml".to_string())
```

- 底层：`config::File::new(path, FileFormat::Yaml)`
- 支持：YAML 文件，相对 / 绝对路径
- 不支持：glob 模式（如 `conf.d/*.yaml`）、远程 URL（需 F8c 或自定义 Source）

### 6.2 `ConfigSource::Environment`

```rust
ConfigSource::Environment("MYSTI".to_string())
```

- 底层：`config::Environment::with_prefix("MYSTI").separator("__")`
- 前缀 `MYSTI` 会被剥离，分隔符 `__` 对应嵌套层级
- 示例映射：

| 环境变量 | 配置路径 |
| :--- | :--- |
| `MYSTI__ENGINE__WEB__LISTEN` | `mysti.engine.web.listen` |
| `MYSTI__ENGINE__WEB__REQUEST_TIMEOUT` | `mysti.engine.web.request_timeout` |
| `MYSTI__CERT__0__PATH` | `mysti.cert[0].path`（数组索引） |

> 环境变量覆盖是 K8s 部署的核心能力：基础配置走 ConfigMap（文件），敏感参数走 Secret 环境变量注入。

### 6.3 `ConfigSource::Default`

```rust
use serde_json::json;

ConfigSource::Default(json!({
    "mysti": { "engine": { "default": { /* ... */ } } }
}))
```

- 底层：`serde_json::to_string(&value)` 转 JSON 字符串，再 `config::File::from_str(json_str, FileFormat::Json)`
- 用途：默认值兜底（优先级最低）、测试夹具注入

## 7. 验证级别详解

### 7.1 三种级别行为对比

| 级别 | 验证失败处置 | `load` 返回 | `update_config` 行为 | 适用场景 |
| :--- | :--- | :--- | :--- | :--- |
| `Strict`（默认） | `return Err(Validation(...))` | `Err` | 验证失败即 `Err`（恒 Strict） | CI / 生产启动 |
| `Warning` | `tracing::warn!` 记录，继续 | `Ok(config)` | 不适用（`update_config` 不支持） | 本地联调 |
| `None` | 跳过验证逻辑 | `Ok(config)` | 不适用 | 已知配置可靠 / 内部测试 |

### 7.2 加载器 vs 管理器的级别差异

| 入口 | 默认级别 | 可配置 | 原因 |
| :--- | :--- | :--- | :--- |
| `EnhancedConfigLoader` | `Strict` | 是（`with_validation_level`） | 加载是启动期行为，可按场景选级别 |
| `ConfigurationManager::update_config` | `Strict`（硬编码） | 否 | 热重载是运行期行为，不能引入未验证配置 |

### 7.3 升级路径

| 阶段 | 加载默认级别 | 管理器级别 | 行为 |
| :--- | :--- | :--- | :--- |
| F8a（已交付） | 不接入加载器 | 无管理器 | 调用方手动 `validate_*` |
| F8b（本阶段） | `Strict` | `Strict`（硬编码） | 加载即验证；热重载强制验证 |
| F8c（监听器） | `Strict` | `Strict` | F8b + 文件变化自动触发 `update_config` |
| F8d（UI） | 可配 | 可配 | REST API 暴露级别切换 |

## 8. 限制与约束

### 8.1 F8b 范围内不做

| 项 | 归属 | 原因 |
| :--- | :--- | :--- |
| 文件监听器（notify 集成） | F8c | 依赖 F8b 的 `update_config` 作为变更入口 |
| 热重载自动触发 | F8c | F8b 只提供"手动调 `update_config`"能力 |
| 管理 UI / REST API | F8d | 依赖 F8b 的事件流与历史快照 |
| 跨引擎冲突检测（端口重复监听等） | 后续 | 需要全局视角，F8b 沿用 F8a 的单引擎校验范围 |
| 文件存在性校验（TLS 证书、静态 root） | 后续 | 属 I/O，加载器只校验路径非空（F8a 规则 4） |
| 配置 schema JSON 导出 | 后续 | 锦上添花 |
| 多节点配置同步 | 后续 | 分布式范畴 |
| `max_history_size` 可配 | 后续 | 当前硬编码 10，未来由 F8d UI 暴露 |

### 8.2 不破坏现有类型

- `EngineConfig` / `MystiConfig` 结构体**零修改**
- `ConfigValidationError` **不新增变体**（复用 F8a 的 `Load` / `Parse` / `Validation`）
- `MystiProxyError` **不新增变体**
- `MystiConfig::from_yaml` / `from_yaml_file` 行为不变，不注入加载器调用
- 不新增 crate 依赖（`config = "0.15.25"` 已在 `Cargo.toml`）

### 8.3 已知限制

| 限制 | 影响 | 缓解 |
| :--- | :--- | :--- |
| `update_config` 中 `validation_success` 恒 `true` | 订阅者无法区分"硬验证通过"与"软验证告警" | 当前 `update_config` 不支持软验证；未来扩展时填充字段 |
| `rollback_to_previous` 会在历史中再增加一条快照 | 历史可能快速增长到 `max_history_size` 上限后淘汰最旧 | 设计选择："每次变更都留痕"优先于"历史纯净" |
| 版本号用纳秒时间戳 | 时钟回拨可能产生重复版本号 | 单进程 + NTP 同步下概率极低；未来可换 ULID |
| `save_snapshot` 中 `duration_since(UNIX_EPOCH).unwrap()` | 系统时间早于 1970 会 panic | 生产环境假设时钟正常；未来可 fallback 到 `"0"` |
| `config_history` 用 `std::sync::RwLock` | 在 async 上下文持锁需谨慎（不能跨 await） | 代码已显式 `drop(history)` 后再 `await` |
| `ConfigSource::Default` 经 JSON 序列化注入 | `serde_json::Value` 中的非 JSON 类型（如 NaN）会失败 | 实际 `serde_json::Value` 不产生 NaN，理论安全 |
| `load<T>` 验证只针对 `mysti.engine` | 加载子配置（不含 engine）时不验证 | 设计选择：F8a 规则只覆盖 `EngineConfig` |

## 9. FAQ

### Q1：为什么 `ValidationLevel` 是 `Strict` / `Warning` / `None`，而 F8a 文档设计的是 `Strict` / `Warn` / `Loose`？

F8b 加载器实际实现的命名是 `Strict` / `Warning` / `None`，与 F8a 文档设计的 `Strict` / `Warn` / `Loose` 不同。语义对应关系：

| F8b 实现 | F8a 文档设计 | 语义 |
| :--- | :--- | :--- |
| `Strict` | `Strict` | 验证失败即拒 |
| `Warning` | `Warn` | 验证失败仅告警 |
| `None` | `Loose` | 跳过 / 放行 |

F8b 用 `None` 替代 `Loose` 更直白（"无验证"），`Warning` 比 `Warn` 更完整。这是反向工程文档时发现的命名差异，代码以实际实现为准。

### Q2：为什么 `EnhancedConfigLoader` 默认 `Strict`，而 F8a 的 `ConfigValidator` 默认 `Loose`？

F8a 的 `ConfigValidator` 是"可选调用"的校验器，默认 `Loose` 保证向后兼容（不主动调即无影响）。F8b 的 `EnhancedConfigLoader` 是"加载入口"，调用方主动使用即表示接受新链路，默认 `Strict` 符合基础设施防御原则——"加载即验证，未通过不进入运行时"。需要逃生口时可 `with_validation_level(ValidationLevel::None)`。

### Q3：为什么 `update_config` 不支持 `ValidationLevel`，恒为 `Strict`？

热重载是运行期行为，引入未验证配置可能导致运行时 panic（如非法正则、空必填字段）。`update_config` 是运行时唯一的配置写入入口，必须强制验证。若需"软验证"（告警但放行），未来可扩展 `update_config_with_level` 方法，但当前不支持。

### Q4：为什么 `load<T>` 是泛型的，而不是直接返回 `MystiConfig`？

未来可能需要加载子配置（如单独的 `EngineConfig`、`CertConfig`）做局部校验或测试。泛型 `T: DeserializeOwned + Serialize` 保证灵活性。验证逻辑通过 `serde_json::to_value` 反射出 `mysti.engine`，对不含该字段的 `T` 静默跳过，不会误报。

### Q5：为什么 `ConfigurationManager` 用 `Arc<AsyncRwLock<MystiConfig>>` 而不是 `Arc<Mutex<MystiConfig>>`？

配置是"读多写少"的资源：运行时各模块频繁 `get_current`，`update_config` 只在热重载时偶发。`RwLock` 允许多个读并发，`Mutex` 会让读之间互斥。`AsyncRwLock`（tokio 版）保证读不阻塞异步运行时。

### Q6：为什么历史用 `std::sync::RwLock` 而不是 `tokio::sync::RwLock`？

`save_snapshot` / `get_history` / `rollback_to_previous` 内部不跨 `await`（`rollback_to_previous` 在调 `update_config` 前显式 `drop(history)`），用 `std::sync::RwLock` 更轻量，避免 tokio 运行时开销。若未来需要在持锁期间 `await`，需切换为 `tokio::sync::RwLock`。

### Q7：broadcast 容量 100 够吗？订阅者消费慢怎么办？

容量 100 足够典型场景（metrics / log / watcher 共 3-5 个订阅者，每秒变更 < 10 次）。订阅者消费慢会收到 `RecvError::Lagged(n)`，表示跳过了 `n` 条事件。订阅者应显式处理（`tracing::warn!` 后 `continue`），不要把 lagged 当致命错误。若需更高容量，未来可配置。

### Q8：回滚会保留回滚前的快照吗？

会。`rollback_to_previous` 调用 `update_config(previous_config)`，后者会 `save_snapshot("reload")` 保存回滚后的配置。因此回滚后历史变为：`[..., 当前, 回滚后]`，回滚前的"当前"仍保留在历史中。这符合"每次变更都留痕"的审计诉求，运维可在 F8d UI 中看到完整时间线。

### Q9：`load_mysti_config` 和 `MystiConfig::from_yaml_file` 有什么区别？

| 维度 | `load_mysti_config` | `from_yaml_file` |
| :--- | :--- | :--- |
| 多源合并 | 否（单文件便捷方法） | 否 |
| 验证 | 是（默认 Strict） | 否 |
| 错误类型 | `ConfigValidationError` | `MystiProxyError` |
| 推荐场景 | 生产 / CI（需验证） | 历史 / 简单场景 |

`load_mysti_config` 内部等价于 `Self::new().add_source(File).load()`，强制 Strict 验证。需要多源合并时用 `EnhancedConfigLoader::new()` 链式构建。

### Q10：`ConfigSource::Default` 为什么先序列化为 JSON 字符串再注入？

`config` crate 的 `File::from_str` 接受 JSON / YAML / TOML 文本，不直接接受 `serde_json::Value`。因此先把 `serde_json::Value` 序列化为 JSON 字符串，再以 `FileFormat::Json` 注入 builder。这是 `config` crate API 限制的变通方案，对调用方透明。
