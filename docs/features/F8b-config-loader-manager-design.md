# F8b 配置验证框架-加载器与管理器 — Design

## 架构概述

### 设计原则

1. **加载即验证**：`EnhancedConfigLoader::load<T>()` 在反序列化后自动调用 F8a 的 `validate_engine_config`，按 `ValidationLevel` 决定处置（Strict 报错 / Warning 告警 / None 跳过）。调用方无法绕过验证（除非显式设 `None`）。
2. **多源合并，后添加优先**：`ConfigSource` 按 `add_source` 顺序入队，后添加的覆盖先添加的。底层用 `config` crate 的 `Config::builder().add_source(...)` 实现，符合 12-Factor App 的"环境变量覆盖文件"惯例。
3. **builder 模式 + 泛型加载**：`new() -> with_validation_level() -> add_source() -> load<T>()` 链式调用。`load<T: DeserializeOwned + Serialize>` 不写死 `MystiConfig`，未来可加载子配置做局部校验。
4. **管理器是单一可信源**：`ConfigurationManager` 持有 `Arc<AsyncRwLock<MystiConfig>>`，运行时所有模块从同一处读当前配置。`update_config` 是唯一的写入入口，原子地完成"验证 → 更新 → 快照 → 通知"。
5. **异步优先，不阻塞运行时**：当前配置用 `tokio::sync::RwLock`（异步读写），变更通知用 `tokio::sync::broadcast`（多订阅者）。历史快照用 `std::sync::RwLock`（不跨 await 持有，避免拖累运行时）。
6. **零破坏性接入**：F8b 只新增模块与类型，不修改现有 `MystiConfig::from_yaml` / `from_yaml_file` 行为。现有调用方不主动用 `EnhancedConfigLoader` 即无感知。

### 模块关系

```
mystiproxy/src/config/
├── mod.rs                  ← 修改：新增 `pub mod loader; pub mod manager;` 及 `pub use`
├── validation/             ← F8a 已交付
│   ├── mod.rs              ← ConfigValidationError / ValidationResult / validate_engine_config
│   ├── error.rs
│   └── rules.rs
├── loader.rs               ← 新增：ValidationLevel / ConfigSource / EnhancedConfigLoader
└── manager.rs              ← 新增：ConfigSnapshot / ConfigChangeEvent / ConfigurationManager
```

`loader.rs` 与 `manager.rs` 仅依赖 `config/mod.rs` 的现有类型 + F8a 的 `validation` 模块 + 标准库 + 已有依赖（`config`、`tokio`、`serde`、`serde_json`、`tracing`）。**不引入新 crate**（`config = "0.15.25"` 已在 `Cargo.toml`）。

### 加载与管理的协作时序

```
调用方                    EnhancedConfigLoader         config crate            F8a validation        ConfigurationManager
  │                              │                          │                       │                       │
  │── add_source(File) ─────────>│                          │                       │                       │
  │── add_source(Environment) ──>│                          │                       │                       │
  │── load::<MystiConfig>() ────>│── builder.add_source ───>│                       │                       │
  │                              │<── merged Value ─────────│                       │                       │
  │                              │── try_deserialize ───────>│                       │                       │
  │                              │<── MystiConfig ───────────│                       │                       │
  │                              │── 遍历 mysti.engine ────────────────────────────>│ validate_engine_config│
  │                              │<── Ok / Err ──────────────────────────────────────│                       │
  │<── ValidationResult<T> ──────│                          │                       │                       │
  │                                                                                                      │
  │── ConfigurationManager::new(config) ───────────────────────────────────────────────────────────────>│
  │                                                                                                      │── save_snapshot("initial")
  │<── manager ─────────────────────────────────────────────────────────────────────────────────────────│
  │                                                                                                      │
  │── manager.update_config(new) ──────────────────────────────────────────────────────────────────────>│
  │                                                                                                      │── validate_engine_config
  │                                                                                                      │── 写 current_config
  │                                                                                                      │── save_snapshot("reload")
  │                                                                                                      │── broadcast send(event)
  │<── Ok(()) ──────────────────────────────────────────────────────────────────────────────────────────│
```

## 数据模型设计

### 加载器数据模型

```rust
//! mystiproxy/src/config/loader.rs

use config::{Config, Environment, File as ConfigFile, FileFormat};
use serde::de::DeserializeOwned;
use crate::config::validation::{validate_engine_config, ConfigValidationError, ValidationResult};
use crate::config::{EngineConfig, MystiConfig, ProxyType};

/// 验证级别：控制加载后验证失败的处置策略
///
/// - `Strict`：验证失败立即返回 `Err`（默认值，用于生产 / CI）
/// - `Warning`：验证失败仅 `tracing::warn!` 记录，仍返回 `Ok`
/// - `None`：完全跳过验证
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationLevel {
    Strict,
    Warning,
    None,
}

/// 配置源：`add_source` 按 enum 变体决定底层 `config` crate 的 Source 类型
pub enum ConfigSource {
    /// YAML 文件路径
    File(String),
    /// 环境变量前缀（如 `MYSTI`，分隔符 `__`）
    Environment(String),
    /// 默认值（序列化为 JSON 后注入 builder）
    Default(serde_json::Value),
}

/// 增强配置加载器：builder 模式，链式添加源后 `load<T>` 一次性加载 + 验证
pub struct EnhancedConfigLoader {
    sources: Vec<ConfigSource>,
    validation_level: ValidationLevel,
}

impl Default for EnhancedConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}
```

### 管理器数据模型

```rust
//! mystiproxy/src/config/manager.rs

use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, RwLock as AsyncRwLock};

use crate::config::loader::EnhancedConfigLoader;
use crate::config::validation::{ConfigValidationError, ValidationResult};
use crate::config::{EngineConfig, MystiConfig};

/// 配置快照：历史中的一份不可变配置
#[derive(Debug, Clone)]
pub struct ConfigSnapshot {
    /// 配置内容
    pub config: MystiConfig,
    /// 快照时间
    pub timestamp: SystemTime,
    /// 版本号（纳秒时间戳字符串）
    pub version: String,
    /// 来源标识（"initial" / "reload"）
    pub source: String,
}

/// 配置变更事件：广播给所有订阅者
#[derive(Debug, Clone)]
pub struct ConfigChangeEvent {
    /// 变更前配置
    pub old_config: MystiConfig,
    /// 变更后配置
    pub new_config: MystiConfig,
    /// 变更时间
    pub timestamp: SystemTime,
    /// 验证是否成功（当前实现：验证失败直接 Err，事件只在成功时发出，故恒为 true）
    pub validation_success: bool,
}

/// 配置管理器：运行时配置的单一可信源
pub struct ConfigurationManager {
    /// 当前配置（异步读写，运行时热路径）
    current_config: Arc<AsyncRwLock<MystiConfig>>,
    /// 配置历史快照（同步读写，不跨 await 持有）
    config_history: Arc<RwLock<Vec<ConfigSnapshot>>>,
    /// 变更通知广播发送端
    reload_notifier: broadcast::Sender<ConfigChangeEvent>,
    /// 历史上限（超出时丢弃最旧）
    max_history_size: usize,
}
```

## 加载器设计

### `EnhancedConfigLoader` API

| 方法 | 签名 | 用途 |
| :--- | :--- | :--- |
| `new` | `fn new() -> Self` | 创建空加载器，默认 `Strict` |
| `with_validation_level` | `fn with_validation_level(self, level: ValidationLevel) -> Self` | 链式设置验证级别 |
| `add_source` | `fn add_source(self, source: ConfigSource) -> Self` | 链式添加配置源（后添加优先级高） |
| `load<T>` | `fn load<T>(self) -> ValidationResult<T> where T: DeserializeOwned + Serialize` | 加载并验证，泛型返回 |
| `load_mysti_config` | `fn load_mysti_config(path: &str) -> ValidationResult<MystiConfig>` | 便捷方法：单文件加载 `MystiConfig` |

### `load<T>` 内部流程

```rust
pub fn load<T>(self) -> ValidationResult<T>
where
    T: DeserializeOwned + serde::Serialize,
{
    let mut builder = Config::builder();

    // 1. 按优先级加载配置源（后添加的优先级更高）
    for source in self.sources {
        match source {
            ConfigSource::File(path) => {
                builder = builder.add_source(ConfigFile::new(&path, FileFormat::Yaml));
            }
            ConfigSource::Environment(prefix) => {
                builder = builder.add_source(
                    Environment::with_prefix(&prefix).separator("__")
                );
            }
            ConfigSource::Default(value) => {
                let json_str = serde_json::to_string(&value)
                    .map_err(|e| ConfigValidationError::Parse(e.to_string()))?;
                builder = builder.add_source(ConfigFile::from_str(&json_str, FileFormat::Json));
            }
        }
    }

    // 2. 构建合并后的 Value
    let config = builder
        .build()
        .map_err(|e| ConfigValidationError::Load(e.to_string()))?;

    // 3. 反序列化为目标类型
    let parsed: T = config
        .try_deserialize()
        .map_err(|e| ConfigValidationError::Parse(e.to_string()))?;

    // 4. 根据验证级别执行验证
    match self.validation_level {
        ValidationLevel::Strict => {
            // 反射出 mysti.engine 映射，逐个校验 EngineConfig
            if let Ok(mysti_config) = serde_json::to_value(&parsed) {
                if let Some(engines_map) = mysti_config
                    .get("mysti").and_then(|m| m.get("engine")).and_then(|e| e.as_object())
                {
                    for (_name, engine_value) in engines_map {
                        if let Ok(engine) = serde_json::from_value::<EngineConfig>(engine_value.clone()) {
                            if let Err(e) = validate_engine_config(&engine) {
                                return Err(ConfigValidationError::Validation(e));
                            }
                        }
                    }
                }
            }
            Ok(parsed)
        }
        ValidationLevel::Warning => {
            // 同上遍历，但验证失败仅 tracing::warn!
            // ... tracing::warn!("Engine '{}' validation warnings: {}", name, e);
            Ok(parsed)
        }
        ValidationLevel::None => Ok(parsed),
    }
}
```

### 设计要点

- **`ConfigSource::Default` 用 JSON 字符串注入**：`config` crate 的 `File::from_str` 接受 JSON 文本，所以先把 `serde_json::Value` 序列化为字符串再注入。这避免了 `config` crate 不直接接受 `serde_json::Value` 的限制。
- **验证只针对 `EngineConfig`**：`load<T>` 通过 `serde_json::to_value(&parsed)` 反射出 `mysti.engine` 映射，逐个 `from_value::<EngineConfig>` 后调 `validate_engine_config`。这保证泛型 `T` 可以是 `MystiConfig` 或其超集，只要包含 `mysti.engine` 字段即可校验。
- **`Strict` 用 `?` 短路返回**：遇到第一个验证失败即 `return Err`，不收集所有错误。这与 F8a 的"累积式"不同——F8b 加载器是"加载入口"，第一个错误就应阻断启动，无需累积。
- **`Warning` 用 `tracing::warn!`**：记录引擎名 + 错误，不阻断加载。适合本地联调。
- **`None` 完全跳过**：连 `serde_json::to_value` 都不调，性能最优。适合已知配置可靠的场景（如内部测试）。

## 管理器设计

### `ConfigurationManager` API

| 方法 | 签名 | 用途 |
| :--- | :--- | :--- |
| `new` | `fn new(initial_config: MystiConfig) -> Result<Self, ConfigValidationError>` | 创建管理器，保存初始快照 |
| `get_current` | `async fn get_current(&self) -> MystiConfig` | 异步读当前配置（克隆返回） |
| `update_config` | `async fn update_config(&self, new_config: MystiConfig) -> ValidationResult<()>` | 验证 + 更新 + 快照 + 通知 |
| `rollback_to_previous` | `async fn rollback_to_previous(&self) -> ValidationResult<()>` | 回滚到上一个版本 |
| `get_history` | `fn get_history(&self) -> Vec<ConfigSnapshot>` | 同步读历史快照列表 |
| `subscribe` | `fn subscribe(&self) -> broadcast::Receiver<ConfigChangeEvent>` | 订阅变更事件 |

### `new` 流程

```rust
pub fn new(initial_config: MystiConfig) -> Result<Self, ConfigValidationError> {
    let (sender, _) = broadcast::channel(100);

    let manager = Self {
        current_config: Arc::new(AsyncRwLock::new(initial_config.clone())),
        config_history: Arc::new(RwLock::new(Vec::new())),
        reload_notifier: sender,
        max_history_size: 10,
    };

    // 保存初始配置快照
    manager.save_snapshot(&initial_config, "initial".to_string())?;
    Ok(manager)
}
```

- **broadcast 容量 100**：默认容量，足够多订阅者（metrics / log / watcher）；超出时新订阅者收到 `RecvError::Lagged`。
- **`max_history_size: 10`**：硬编码上限，超出时 `history.remove(0)` 丢弃最旧。未来可配（F8d UI 暴露）。
- **初始快照 source = "initial"**：与后续 reload 快照区分，便于 F8d UI 展示时间线起点。

### `update_config` 流程

```rust
pub async fn update_config(&self, new_config: MystiConfig) -> ValidationResult<()> {
    // 1. 获取旧配置（用于事件）
    let old_config = self.get_current().await;

    // 2. 验证新配置：反射出 mysti.engine，逐个 validate_engine_config
    let mysti_json: serde_json::Value = serde_json::to_value(&new_config)
        .map_err(|e| ConfigValidationError::Parse(e.to_string()))?;
    let engines_map = mysti_json
        .get("mysti").and_then(|m| m.get("engine")).and_then(|e| e.as_object())
        .ok_or(ConfigValidationError::Parse("missing mysti.engine".to_string()))?;

    for (_name, engine_value) in engines_map {
        if let Ok(engine) = serde_json::from_value::<EngineConfig>(engine_value.clone()) {
            crate::config::validation::validate_engine_config(&engine)?;
        }
    }

    // 3. 更新当前配置（短临界区，仅写指针）
    {
        let mut config_guard = self.current_config.write().await;
        *config_guard = new_config.clone();
    }

    // 4. 保存快照
    self.save_snapshot(&new_config, "reload".to_string())?;

    // 5. 广播变更事件
    let event = ConfigChangeEvent {
        old_config,
        new_config,
        timestamp: SystemTime::now(),
        validation_success: true,  // 验证失败已在上文 ? 返回，此处恒 true
    };
    let _ = self.reload_notifier.send(event);

    Ok(())
}
```

### 设计要点

- **验证逻辑与加载器一致**：`update_config` 复用 `validate_engine_config`，保证"加载时"和"热重载时"验证规则相同。区别是加载器支持 `ValidationLevel`，`update_config` 恒为 `Strict` 语义（验证失败即 Err，不告警放行）——热重载不能引入未验证配置。
- **写临界区最小化**：`current_config.write().await` 只持有到赋值完成，不跨 `save_snapshot` / `send`，避免长时间阻塞读。
- **`validation_success` 恒 true**：当前实现下，事件只在验证通过后发出，故字段恒 true。保留字段是为未来"软验证"（warning 不阻断但标记 `validation_success: false`）预留。
- **`send` 忽略接收端错误**：`let _ = self.reload_notifier.send(event)` 无订阅者时返回 `Err(SendError)`，忽略即可（无订阅者不是错误）。

### `rollback_to_previous` 流程

```rust
pub async fn rollback_to_previous(&self) -> ValidationResult<()> {
    let history = self.config_history.read().unwrap();
    if history.len() < 2 {
        return Err(ConfigValidationError::Load(
            "No previous version to rollback to".to_string(),
        ));
    }
    let previous_config = history[history.len() - 2].config.clone();
    drop(history);  // 显式释放锁，避免跨 await 持有

    self.update_config(previous_config).await
}
```

- **回滚目标 = 倒数第二个快照**：`history[len-1]` 是当前配置，`history[len-2]` 是上一个版本。
- **`drop(history)` 在 await 前**：`std::sync::RwLock` 不能跨 await 持有，否则可能死锁。显式 drop 保证锁在 `update_config` 调用前释放。
- **回滚经 `update_config`**：回滚也走"验证 → 更新 → 快照 → 通知"全流程，保证回滚后的配置同样被验证、被快照、被广播。副作用是回滚会在历史中再增加一条 "reload" 快照（含回滚后的配置），这符合"每次变更都留痕"的审计诉求。

### `save_snapshot` 流程

```rust
fn save_snapshot(&self, config: &MystiConfig, source: String) -> Result<(), ConfigValidationError> {
    let mut history = self.config_history.write().unwrap();

    let version = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        .to_string();

    let snapshot = ConfigSnapshot {
        config: config.clone(),
        timestamp: SystemTime::now(),
        version,
        source,
    };

    history.push(snapshot);

    if history.len() > self.max_history_size {
        history.remove(0);  // 丢弃最旧
    }

    Ok(())
}
```

- **版本号 = 纳秒时间戳字符串**：单进程内冲突概率极低；字符串形式便于 F8d UI 展示与比较。
- **FIFO 淘汰**：`history.remove(0)` 是 O(n) 操作，但 `max_history_size = 10` 量级小，可接受。若未来需要大量历史，可换 `VecDeque`。
- **`unwrap()` 在 `duration_since`**：`UNIX_EPOCH` 之前的系统时间会返回 Err，当前实现直接 panic。生产环境假设时钟正常（NTP 同步），此风险可接受；未来可改为 fallback 到 `0`。

## 代码设计

### 模块结构

```
mystiproxy/src/config/
├── mod.rs              ← 修改：新增 `pub mod loader; pub mod manager;` 及 `pub use`
├── validation/         ← F8a 已交付（不动）
├── loader.rs           ← 新增：F8b 加载器全部代码
└── manager.rs          ← 新增：F8b 管理器全部代码
```

### `config/mod.rs` 接入点

在 `mod.rs` 顶部新增模块声明与 `pub use`（不改动任何现有代码）：

```rust
//! 配置模块

pub mod loader;        // ← 新增
pub mod manager;       // ← 新增
pub mod validation;    // F8a 已有
// ... 其他现有模块

pub use loader::{ConfigSource, EnhancedConfigLoader, ValidationLevel};        // ← 新增
pub use manager::{ConfigChangeEvent, ConfigSnapshot, ConfigurationManager};   // ← 新增
pub use validation::{
    validate_cidr, validate_engine_config, ConfigValidationError, ValidationResult,
};
// ... 其他现有 pub use
```

### 公共 API 总结

| 类型 / 方法 | 签名 | 用途 |
| :--- | :--- | :--- |
| `ValidationLevel` | `enum { Strict, Warning, None }` | 验证级别 |
| `ConfigSource` | `enum { File(String), Environment(String), Default(serde_json::Value) }` | 配置源 |
| `EnhancedConfigLoader::new` | `fn new() -> Self` | 创建加载器，默认 Strict |
| `EnhancedConfigLoader::with_validation_level` | `fn with_validation_level(self, ValidationLevel) -> Self` | 设置级别 |
| `EnhancedConfigLoader::add_source` | `fn add_source(self, ConfigSource) -> Self` | 添加源 |
| `EnhancedConfigLoader::load` | `fn load<T: DeserializeOwned + Serialize>(self) -> ValidationResult<T>` | 加载 + 验证 |
| `EnhancedConfigLoader::load_mysti_config` | `fn load_mysti_config(path: &str) -> ValidationResult<MystiConfig>` | 便捷单文件加载 |
| `ConfigSnapshot` | `struct { config, timestamp, version, source }` | 配置快照 |
| `ConfigChangeEvent` | `struct { old_config, new_config, timestamp, validation_success }` | 变更事件 |
| `ConfigurationManager::new` | `fn new(MystiConfig) -> Result<Self, ConfigValidationError>` | 创建管理器 |
| `ConfigurationManager::get_current` | `async fn get_current(&self) -> MystiConfig` | 读当前配置 |
| `ConfigurationManager::update_config` | `async fn update_config(&self, MystiConfig) -> ValidationResult<()>` | 更新配置 |
| `ConfigurationManager::rollback_to_previous` | `async fn rollback_to_previous(&self) -> ValidationResult<()>` | 回滚 |
| `ConfigurationManager::get_history` | `fn get_history(&self) -> Vec<ConfigSnapshot>` | 读历史 |
| `ConfigurationManager::subscribe` | `fn subscribe(&self) -> broadcast::Receiver<ConfigChangeEvent>` | 订阅事件 |

## 错误处理

### 加载器错误归一到 `ConfigValidationError`

`EnhancedConfigLoader::load<T>` 返回 `ValidationResult<T>`（即 `Result<T, ConfigValidationError>`）。所有错误统一映射到 F8a 已有的 `ConfigValidationError` 变体，**不新增变体**：

| 错误来源 | 映射变体 | 触发场景 |
| :--- | :--- | :--- |
| `config::Config::build()` 失败 | `ConfigValidationError::Load(String)` | 文件不存在 / YAML 语法错误 |
| `serde_json::to_string` 失败（Default 源） | `ConfigValidationError::Parse(String)` | `serde_json::Value` 不可序列化（理论不会发生） |
| `Config::try_deserialize` 失败 | `ConfigValidationError::Parse(String)` | 类型不匹配 / 缺失必填字段 |
| `validate_engine_config` 失败（Strict） | `ConfigValidationError::Validation(ValidationErrors)` | F8a 8 条规则之一未通过 |

### 管理器错误复用 `ConfigValidationError`

`ConfigurationManager` 的所有 `Result` 返回也用 `ConfigValidationError`，**不新增变体**：

| 方法 | 错误变体 | 触发场景 |
| :--- | :--- | :--- |
| `new` | `Load` (via `save_snapshot`) | 实际不会失败（`save_snapshot` 当前恒返回 Ok） |
| `update_config` | `Parse` / `Validation` | 反射 `mysti.engine` 失败 / `validate_engine_config` 失败 |
| `rollback_to_previous` | `Load` / 同 `update_config` | 历史不足 2 条 / 回滚目标验证失败 |
| `get_current` / `get_history` / `subscribe` | 无 | 只读操作，不返回 Result |

### 与 `MystiProxyError` 的衔接

F8b 不修改 `MystiProxyError`。调用方按需将 `ConfigValidationError` 转 `MystiProxyError::Config(String)`（`mod.rs:418` 已有此转换模式）：

```rust
let cfg = EnhancedConfigLoader::load_mysti_config("config.yaml")
    .map_err(|e| crate::MystiProxyError::Config(e.to_string()))?;
```

### broadcast 的 lagged 处理

`subscribe()` 返回的 `broadcast::Receiver` 在消费慢时会收到 `RecvError::Lagged(n)`，表示跳过了 `n` 条事件。订阅者应显式处理：

```rust
loop {
    match receiver.recv().await {
        Ok(event) => { /* 处理事件 */ }
        Err(broadcast::error::RecvError::Lagged(n)) => {
            tracing::warn!("config event subscriber lagged, skipped {} events", n);
            continue;
        }
        Err(broadcast::error::RecvError::Closed) => break,
    }
}
```

## 向后兼容策略

### 默认 Strict，但不强制接入

- `EnhancedConfigLoader::new()` 默认 `Strict`，但 F8b **不修改** `MystiConfig::from_yaml` / `from_yaml_file`，不注入加载器调用
- 现有调用方继续用 `from_yaml`，行为完全不变（无验证）
- F8b 是"新增 API"，调用方主动 `use EnhancedConfigLoader` 才进入新链路
- 现有 `cargo test` 全部保持通过（F8b 只新增测试，不改现有测试）

### `ValidationLevel::None` 提供逃生口

- 已知配置可靠（如内部测试、固定环境）时，可 `with_validation_level(ValidationLevel::None)` 跳过验证，性能最优
- 这保证 F8b 不会因"默认 Strict"导致历史测试场景受影响

### `ConfigurationManager` 是可选组件

- 运行时不强制使用 `ConfigurationManager`，调用方仍可直接持有一份 `MystiConfig` 值
- `ConfigurationManager` 是为"需要热重载 / 回滚 / 订阅"的场景准备的，简单场景可不用
- 这保证 F8b 可独立交付、独立测试，不绑架现有运行时架构

### 升级路径

| 阶段 | 加载方式 | 管理方式 | 验证级别 |
| :--- | :--- | :--- | :--- |
| F8a（已交付） | `from_yaml` + 手动 `validate_*` | 无管理器 | 调用方自选 |
| F8b（本阶段） | `EnhancedConfigLoader::load` | `ConfigurationManager`（可选） | 默认 Strict |
| F8c（监听器） | F8b 加载器 | F8b 管理器 + 自动 `update_config` | Strict |
| F8d（UI） | F8b 加载器 | F8b 管理器 + REST API | 可配 |

### 不破坏现有类型

- `EngineConfig` / `MystiConfig` 结构体**零修改**
- `ConfigValidationError` **不新增变体**（复用 F8a 的 `Load` / `Parse` / `Validation`）
- `MystiProxyError` **不新增变体**
- 不新增 crate 依赖（`config = "0.15.25"` 已在 `Cargo.toml`，`tokio` / `serde_json` / `tracing` 均已有）

## 测试策略

### TDD 流程

加载器的每个 `ValidationLevel` 分支、管理器的每个公开方法，先写失败测试（红），再实现使测试通过。多源合并、回滚、订阅等关键路径必须有端到端用例。

### 单元测试矩阵

测试统一放在 `loader.rs` 与 `manager.rs` 的 `#[cfg(test)] mod tests` 内。

#### 加载器测试（`loader.rs`）

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EngineConfig, ProxyType};

    // ===== 加载器基础 =====
    #[test]
    fn test_load_valid_config() {
        // YAML 字符串解析为 MystiConfig，含合法 engine
        let yaml = r#"
mysti:
  engine:
    test:
      listen: tcp://0.0.0.0:8080
      target: tcp://127.0.0.1:80
      proxy_type: http
"#;
        let config: MystiConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.mysti.engine.contains_key("test"));
    }

    #[test]
    fn test_validate_engine_config_valid() {
        // 合法 EngineConfig 通过 validate_engine_config
        let engine = EngineConfig { /* ... */ };
        assert!(validate_engine_config(&engine).is_ok());
    }

    #[test]
    fn test_validate_engine_config_invalid() {
        // target 为 http:// 但 proxy_type 为 Tcp → 验证失败
        let mut engine = EngineConfig { /* ... proxy_type: Tcp ... */ };
        engine.target = "http://example.com".to_string();
        assert!(validate_engine_config(&engine).is_err());
    }

    // ===== ValidationLevel 行为（应补全） =====
    // test_load_strict_rejects_invalid_engine
    // test_load_warning_allows_invalid_engine
    // test_load_none_skips_validation
    // test_add_source_priority_env_overrides_file
    // test_load_mysti_config_convenience
}
```

#### 管理器测试（`manager.rs`）

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EngineConfig, MystiConfig, ProxyType};
    use std::collections::HashMap;

    fn create_test_config() -> MystiConfig { /* 构造含 1 个 engine 的配置 */ }

    #[tokio::test]
    async fn test_config_manager_creation() {
        let config = create_test_config();
        let manager = ConfigurationManager::new(config.clone()).unwrap();
        let current = manager.get_current().await;
        assert_eq!(current.mysti.engine.len(), 1);
    }

    #[tokio::test]
    async fn test_config_update() {
        let config = create_test_config();
        let manager = ConfigurationManager::new(config).unwrap();

        let mut new_config = create_test_config();
        new_config.mysti.engine.get_mut("test").unwrap().request_timeout =
            Some(std::time::Duration::from_secs(30));

        manager.update_config(new_config).await.unwrap();
        let current = manager.get_current().await;
        assert_eq!(
            current.mysti.engine["test"].request_timeout,
            Some(std::time::Duration::from_secs(30))
        );
    }

    #[tokio::test]
    async fn test_config_history() {
        let config = create_test_config();
        let manager = ConfigurationManager::new(config).unwrap();

        assert_eq!(manager.get_history().len(), 1); // 初始快照

        let mut new_config = create_test_config();
        new_config.mysti.engine.get_mut("test").unwrap().request_timeout =
            Some(std::time::Duration::from_secs(30));
        manager.update_config(new_config).await.unwrap();

        assert_eq!(manager.get_history().len(), 2); // 初始 + reload
    }

    // ===== 应补全的测试 =====
    // test_rollback_to_previous
    // test_rollback_with_insufficient_history
    // test_subscribe_receives_event
    // test_history_max_size_eviction
    // test_update_config_rejects_invalid
}
```

### 覆盖率目标

- `loader.rs` 行覆盖率 ≥ 70%（`cargo llvm-cov -p mystiproxy config::loader`）
- `manager.rs` 行覆盖率 ≥ 70%（`cargo llvm-cov -p mystiproxy config::manager`）
- `ValidationLevel` 三档分支均有用例
- `ConfigSource` 三种源均有用例（File / Environment / Default）
- `update_config` 的成功 / 验证失败 / 通知路径均覆盖
- `rollback_to_previous` 的成功 / 历史不足两条路径均覆盖
- `subscribe` 收到事件 / lagged / closed 路径均覆盖

### 集成验证

```bash
cargo test -p mystiproxy config::loader::tests       # 加载器单测
cargo test -p mystiproxy config::manager::tests      # 管理器单测
cargo test --workspace                                # 全量不回归
cargo clippy --workspace --all-targets -- -D warnings # 无新告警
cargo fmt --check                                     # 格式通过
```
