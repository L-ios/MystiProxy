# F8c 配置验证框架-热重载 Watcher — Design

## 架构概述

### 设计原则

1. **监听与编排解耦**：`ConfigFileWatcher` 只负责"文件变了 → 重新加载 → 调回调"，不知道 `ConfigurationManager` 的存在。`start_config_watcher` 是把两者粘合的便捷函数。这保证 watcher 可用 mock 回调独立单测。
2. **debounce 必须可配且生效**：当前实现 debounce 写死 `500ms`，`debounce_interval` 字段被存储但未使用。F8c 修复此 bug，让 `debounce_ms` 参数真正驱动 sleep 时长。
3. **错误隔离三段式**：加载（`EnhancedConfigLoader::load`）、回调（`callback(config)`）两阶段独立 `match` + `error!`，任一失败都不影响 watcher 主循环。回调内部（如 `update_config`）的错误由回调自身处理。
4. **事件过滤显式化**：只对 `EventKind::Modify(_)` / `EventKind::Create(_)` / `EventKind::Remove(_)` 触发，忽略 `Access` / `Other`。这避免 atime 读取等噪声触发无谓重载。
5. **零破坏性接入**：F8c 不修改 `MystiConfig` / `EngineConfig` 结构，不改变 `EnhancedConfigLoader` 行为。`start_config_watcher` 是显式调用，不调用即无 watcher。

### 模块关系

```
mystiproxy/src/config/
├── mod.rs              ← 现有：MystiConfig / EngineConfig 等
├── validation.rs       ← F8a：ConfigValidator / ValidationResult
├── loader.rs           ← F8b：EnhancedConfigLoader / ConfigSource / ValidationLevel
├── manager.rs          ← 现有：ConfigurationManager / ConfigChangeEvent
└── watcher.rs          ← F8c（本阶段）：ConfigFileWatcher / start_config_watcher
```

`watcher.rs` 依赖：
- `notify` crate（已在 `Cargo.toml`）
- `tokio::sync::mpsc` / `tokio::time::sleep` / `tokio::spawn`（已在 `Cargo.toml`）
- `crate::config::loader::EnhancedConfigLoader` + `ConfigSource`（F8b）
- `crate::config::manager::ConfigurationManager`（仅 `start_config_watcher` 用到）
- `crate::config::validation::ConfigValidationError`（错误类型）
- `crate::config::MystiConfig`（回调参数类型）

**不引入新 crate**。

### 调用链路

```
[文件系统] 
   │ notify 事件 (Modify/Create/Remove)
   ▼
[RecommendedWatcher 闭包]
   │ tx.blocking_send(())   ← 过滤后的事件
   ▼
[mpsc::Receiver 后台任务]
   │ rx.recv().await
   │ sleep(debounce_interval) ← F8c 修复：使用 debounce_ms 参数
   ▼
[EnhancedConfigLoader::load::<MystiConfig>()]
   │ Ok(config) / Err(e)
   ▼
[callback(config)]   ← Arc<dyn Fn(MystiConfig) -> Result<()>>
   │ Ok(()) / Err(e)
   ▼
[ConfigurationManager::update_config]  ← 仅 start_config_watcher 路径
   │ 广播 ConfigChangeEvent
   ▼
[subscribe() 的 Receiver]  ← F8d 消费
```

## 数据模型设计

```rust
//! mystiproxy/src/config/watcher.rs

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{error, info, warn};

use crate::config::loader::EnhancedConfigLoader;
use crate::config::manager::ConfigurationManager;
use crate::config::validation::ConfigValidationError;
use crate::config::MystiConfig;

/// 配置文件监控器
///
/// 持有 `notify::RecommendedWatcher` 与一个后台 tokio 任务，
/// 文件变化时通过 `EnhancedConfigLoader` 重新加载并调用回调。
pub struct ConfigFileWatcher {
    watcher: RecommendedWatcher,
    config_path: String,
    debounce_interval: Duration,
    reload_tx: mpsc::Sender<()>,
}
```

### 字段说明

| 字段 | 类型 | 用途 |
| :--- | :--- | :--- |
| `watcher` | `notify::RecommendedWatcher` | 底层文件监听句柄；drop 时自动取消监听 |
| `config_path` | `String` | 被监听的配置文件路径，传给 `EnhancedConfigLoader` 重新加载 |
| `debounce_interval` | `Duration` | debounce 时长；F8c 修复后实际驱动 `sleep` |
| `reload_tx` | `mpsc::Sender<()>` | 后台任务的信号发送端；watcher 闭包通过它通知"有事件" |

> **注意**：`ConfigFileWatcher` 不持有后台任务的 `JoinHandle`。后台任务在 `tokio::spawn` 后即"脱管"，依赖 `mpsc::Receiver` 在 watcher drop 时被 drop 而自然退出（`rx.recv().await` 返回 `None`）。

## Watcher 设计

### `ConfigFileWatcher::new` 详解

```rust
impl ConfigFileWatcher {
    pub fn new<F>(
        config_path: String,
        debounce_ms: u64,
        reload_callback: F,
    ) -> Result<Self, ConfigValidationError>
    where
        F: Fn(MystiConfig) -> Result<(), ConfigValidationError> + Send + Sync + 'static,
    {
        // 1. 创建 mpsc 通道（容量 1：背压式，事件积压时丢弃旧信号）
        let (tx, mut rx) = mpsc::channel(1);

        // 2. 把回调装入 Arc 以便在后台任务中共享
        let callback = Arc::new(reload_callback);

        // 3. 创建 notify watcher，事件闭包过滤后通过 tx 发信号
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    if matches!(
                        event.kind,
                        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                    ) {
                        // blocking_send：watcher 闭包是同步的，不能用 .await
                        // 通道满时 blocking_send 返回 Err，等价于"丢弃这次信号"，符合 debounce 语义
                        let _ = tx.blocking_send(());
                    }
                }
            },
            Config::default(),
        )
        .map_err(|e| ConfigValidationError::Watch(e.to_string()))?;

        // 4. 注册监听路径，非递归（只监听 config_path 本身）
        watcher
            .watch(Path::new(&config_path), RecursiveMode::NonRecursive)
            .map_err(|e| ConfigValidationError::Watch(e.to_string()))?;

        // 5. 启动后台任务：收信号 → debounce → 加载 → 回调
        let callback_bg = callback; // 重命名，语义清晰
        let config_path_bg = config_path.clone();
        tokio::spawn(async move {
            while rx.recv().await.is_some() {
                // F8c 修复：使用 debounce_interval 而非写死 500ms
                sleep(Duration::from_millis(debounce_ms)).await;

                let loader = EnhancedConfigLoader::new().add_source(
                    crate::config::loader::ConfigSource::File(config_path_bg.clone()),
                );

                match loader.load::<MystiConfig>() {
                    Ok(new_config) => {
                        info!("Configuration loaded successfully");
                        if let Err(e) = callback_bg(new_config) {
                            error!("Failed to apply config: {:?}", e);
                        }
                    }
                    Err(e) => {
                        error!("Failed to reload configuration: {:?}", e);
                    }
                }
            }
        });

        Ok(Self {
            watcher,
            config_path,
            debounce_interval: Duration::from_millis(debounce_ms),
            reload_tx: tx,
        })
    }
}
```

### 设计要点

1. **`mpsc::channel(1)` 容量选择**：背压式。watcher 闭包高频发信号时，`blocking_send` 在通道满时立即返回 `Err`，等价于"丢弃新信号"。这与 debounce 语义一致——已经在 debounce 等待中，再来的信号无需排队。
2. **`blocking_send` 而非 `send().await`**：notify 的回调是同步函数（`FnMut(Result<Event>)`），不能 `.await`。`blocking_send` 会阻塞当前线程直到通道有空间或返回 `Err`；容量 1 时几乎不阻塞。
3. **`Arc<dyn Fn>` 共享回调**：回调被 `Arc` 包装，watcher 闭包与后台任务都持有副本。`Send + Sync + 'static` 约束保证可跨线程。
4. **后台任务脱管**：`tokio::spawn` 返回的 `JoinHandle` 被丢弃。任务依赖 `rx.recv()` 在 sender 全部 drop 时返回 `None` 退出。`ConfigFileWatcher` drop 时 `reload_tx` drop，但 watcher 闭包还持有一份 `tx`，需等 `watcher` 也 drop 后通道才真正关闭。这是当前实现的隐式生命周期，F8c 文档化此限制。

## notify 集成

### `RecommendedWatcher` 选择

`notify` 提供多种后端：

| 后端 | 平台 | 特点 |
| :--- | :--- | :--- |
| `RecommendedWatcher` | 跨平台 | 自动选择最优后端（FSEvents / inotify / ReadDirectoryChangesW） |
| `PollWatcher` | 跨平台 | 轮询，兼容性最高但 CPU 开销大 |
| `FsWatcher` | macOS | FSEvents，原生 |
| `INotifyWatcher` | Linux | inotify，原生 |

F8c 选 `RecommendedWatcher`：跨平台、零配置、性能足够。轮询后端仅在 NFS / FUSE 等特殊文件系统下才需要，F8c 不覆盖。

### `RecursiveMode::NonRecursive`

只监听 `config_path` 本身，不递归子目录。原因：

- 配置文件是单文件，无需递归
- 递归会监听 `cert_path` / `key_path` 所在目录的无关变化，触发误重载
- 非递归性能更好

### 事件过滤

```rust
if matches!(
    event.kind,
    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
) {
    let _ = tx.blocking_send(());
}
```

| 事件类型 | 是否触发 | 原因 |
| :--- | :--- | :--- |
| `EventKind::Modify(_)` | ✅ | 内容修改，核心场景 |
| `EventKind::Create(_)` | ✅ | 编辑器原子 rename 保存（先写临时文件再 rename 覆盖）会触发 Create |
| `EventKind::Remove(_)` | ✅ | 部分编辑器先删除再创建；也用于检测配置被误删 |
| `EventKind::Access(_)` | ❌ | 仅 atime 读取，无内容变化 |
| `EventKind::Any` | ❌ | 兜底类型，不明确，忽略 |
| `EventKind::Other` | ❌ | 平台特定事件，不明确，忽略 |

## debounce 策略

### 当前实现的问题

```rust
// ❌ 当前代码（watcher.rs 第 66 行）
sleep(Duration::from_millis(500)).await; // debounce
```

`debounce_ms` 参数被传入但未使用，`debounce_interval` 字段被存储但仅作记录。调用方传 `debounce_ms=2000` 仍按 500ms debounce，行为不可预期。

### F8c 修复

```rust
// ✅ F8c 修复后
sleep(Duration::from_millis(debounce_ms)).await;
```

让 `debounce_ms` 参数真正驱动 sleep 时长。`debounce_interval` 字段保留供调用方查询当前配置。

### debounce 语义

```
时间轴 →

事件1 ─┐
事件2 ─┤ 通道容量1，事件2 覆盖事件1
       ▼
     sleep(debounce_ms) 开始
事件3 ─┤ blocking_send 返回 Err（通道满），丢弃
事件4 ─┤ blocking_send 返回 Err，丢弃
       ▼
     sleep 结束，加载配置
     ── 此时若事件3/4 是有效变化，已在加载的 config 中反映
     ── 加载完成后，rx.recv().await 等待下一轮
```

**关键性质**：
- 多次事件合并为一次重载（debounce）
- 通道容量 1 + `blocking_send` 实现背压（不积压）
- sleep 期间的事件被丢弃，但下一次 `rx.recv()` 仍能收到 sleep 期间产生的新信号（如果通道有空位）

### debounce_ms 推荐值

| 场景 | 推荐 | 理由 |
| :--- | :--- | :--- |
| 本地开发（Vim / VSCode） | 500ms | 编辑器抖动通常 < 500ms |
| NFS / Docker volume | 1000-2000ms | 网络文件系统事件延迟更高 |
| 生产环境 | 1000ms | 平衡响应速度与稳定性 |

## 回调机制

### 回调签名

```rust
F: Fn(MystiConfig) -> Result<(), ConfigValidationError> + Send + Sync + 'static
```

- `Fn`（非 `FnMut` / `FnOnce`）：可被多次调用，无状态
- `MystiConfig` 入参按值传递：调用方拿到所有权，可自由消费
- `Result<(), ConfigValidationError>`：回调失败仅 `error!` 记录，不重试，不传播
- `Send + Sync + 'static`：可跨线程，可装入 `Arc<dyn Fn>`

### 回调的两种典型用法

#### 用法 1：mock 回调（单测）

```rust
let called = Arc::new(AtomicUsize::new(0));
let called_clone = called.clone();
let watcher = ConfigFileWatcher::new(
    config_path,
    100,
    move |_new_config| {
        called_clone.fetch_add(1, Ordering::SeqCst);
        Ok(())
    },
)?;
// 触发文件变化，等待 debounce + 加载，断言 called > 0
```

#### 用法 2：`ConfigurationManager::update_config`（生产）

```rust
let manager_clone = manager.clone();
ConfigFileWatcher::new(
    config_path,
    1000,
    move |new_config| {
        let manager = manager_clone.clone();
        tokio::spawn(async move {
            if let Err(e) = manager.update_config(new_config).await {
                error!("Failed to update config: {}", e);
            }
        });
        Ok(())
    },
)?;
```

> **注意**：回调内部 `tokio::spawn` 是因为 `update_config` 是 `async fn`，而回调是同步 `Fn`。`spawn` 后立即返回 `Ok(())`，实际更新异步进行。这是当前实现的取舍：回调签名保持同步，简化 watcher 逻辑。

## 错误处理

### 三阶段错误隔离

```rust
match loader.load::<MystiConfig>() {
    Ok(new_config) => {
        info!("Configuration loaded successfully");
        if let Err(e) = callback_bg(new_config) {
            error!("Failed to apply config: {:?}", e);  // 阶段2：回调错误
        }
    }
    Err(e) => {
        error!("Failed to reload configuration: {:?}", e);  // 阶段1：加载错误
    }
}
// 阶段3：回调内部的 update_config 错误由回调自身处理（spawn 内 error!）
```

| 阶段 | 错误来源 | 处置 | 影响 |
| :--- | :--- | :--- | :--- |
| 加载 | `EnhancedConfigLoader::load`（YAML 语法 / F8a 校验） | `error!` 记录 | 不调回调，等下一轮事件 |
| 回调 | 回调返回 `Err` | `error!` 记录 | 不重试，等下一轮事件 |
| 回调内部 | `update_config` 内部 `Err` | 回调内 `error!` 记录 | 回调仍返回 `Ok(())`，watcher 不感知 |

### 错误类型

F8c 复用 F8a 的 `ConfigValidationError`：

```rust
pub enum ConfigValidationError {
    // ... F8a 已有变体
    Watch(String),  // watcher 创建 / 注册失败
    // Load(String) / Parse(String) / Validation(String) 由 loader 阶段产生
}
```

`ConfigFileWatcher::new` 的两个错误点：
1. `RecommendedWatcher::new` 失败 → `ConfigValidationError::Watch(e.to_string())`
2. `watcher.watch(...)` 失败 → `ConfigValidationError::Watch(e.to_string())`

> **注意**：`notify::Error` 在运行时（事件回调中）的错误被忽略（`if let Ok(event) = res`）。这是有意为之：单次事件错误不应击穿 watcher。

## 生命周期

### 创建

```
ConfigFileWatcher::new
  ├─ mpsc::channel(1) 创建通道
  ├─ RecommendedWatcher::new 创建底层 watcher
  ├─ watcher.watch 注册路径
  ├─ tokio::spawn 启动后台任务
  └─ 返回 ConfigFileWatcher { watcher, config_path, debounce_interval, reload_tx }
```

### 运行

```
[文件变化] → notify 回调 → tx.blocking_send(()) → 后台任务 rx.recv() → sleep(debounce) → load → callback → 循环
```

### 销毁

```
ConfigFileWatcher drop
  ├─ reload_tx drop（但 watcher 闭包还持有 tx 副本）
  ├─ watcher drop → notify 停止监听 → watcher 闭包 drop → tx 副本 drop
  └─ 通道所有 sender drop → rx.recv() 返回 None → 后台任务退出
```

> **限制**：当前实现没有显式 shutdown 信号。后台任务依赖"所有 sender drop"退出。若调用方长期持有 `reload_tx`（理论上不会，因为它是私有字段），任务不会退出。F8c 文档化此限制，显式 shutdown 留给 F8d。

### `start_config_watcher` 的生命周期

```rust
pub async fn start_config_watcher(
    config_path: String,
    debounce_ms: u64,
    manager: Arc<ConfigurationManager>,
) -> Result<tokio::task::JoinHandle<()>, ConfigValidationError> {
    let manager_clone = manager.clone();

    let mut watcher = ConfigFileWatcher::new(
        config_path.clone(),
        debounce_ms,
        move |new_config| {
            let manager = manager_clone.clone();
            tokio::spawn(async move {
                if let Err(e) = manager.update_config(new_config).await {
                    error!("Failed to update config: {}", e);
                }
            });
            Ok(())
        },
    )?;

    let handle = tokio::spawn(async move {
        // watcher 已在 new() 中启动监控，这里只需保持任务存活
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });

    Ok(handle)
}
```

**关键设计**：`watcher` 被 move 进 `tokio::spawn` 的闭包，但闭包内并未使用它，仅靠 `loop { sleep }` 保持任务存活。Rust 编译器会警告"unused variable"，当前代码用 `let mut watcher = ...` 而非 `let _watcher = ...`，可能有告警。F8c 建议改为 `let _watcher = ...` 或在闭包内显式 `drop(watcher)` 之前 hold 它。

**`JoinHandle` 的语义**：调用方 drop `JoinHandle` 不会终止任务（tokio 默认 detach）。任务依赖 `loop { sleep }` 永不退出，直到进程结束。这是当前实现的简化，F8d 可引入 shutdown 信号。

## 向后兼容策略

### F8c 不破坏现有行为

- `ConfigFileWatcher` / `start_config_watcher` 是显式调用，不调用即无 watcher
- `EnhancedConfigLoader` / `ConfigurationManager` 行为不变
- 现有 `cargo test` 全部保持通过（F8c 只新增测试，不改现有测试）

### F8c 修复的 bug

| bug | 修复前 | 修复后 |
| :--- | :--- | :--- |
| debounce 写死 500ms | `debounce_ms` 参数不生效 | `sleep(Duration::from_millis(debounce_ms))` |
| `callback_clone` 死代码 | 创建后未使用 | 删除，仅用 `callback` |

### 升级路径

| 阶段 | 行为 |
| :--- | :--- |
| F8c（本阶段） | watcher 可用，debounce 可配，测试覆盖补全 |
| F8d（编排 + UI） | 在 `start_config_watcher` 之上包装管理 API，暴露 shutdown 信号、重载历史、diff 可视化 |

## 测试策略

### TDD 流程

watcher 涉及文件系统事件，测试需特殊处理：

1. **临时文件**：用 `tempfile::NamedTempFile` 创建临时配置文件，避免污染工作区
2. **触发变化**：`std::fs::write` 覆盖文件内容
3. **等待事件**：`tokio::time::timeout(Duration::from_secs(2), ...)` 等待 watcher 响应
4. **mock 回调**：`Arc<AtomicUsize>` 计数器或 `Arc<Mutex<Vec<MystiConfig>>>` 收集传入 config

### 单元测试矩阵

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tempfile::NamedTempFile;
    use tokio::time::timeout;

    // ===== T2 事件过滤 =====
    #[tokio::test]
    async fn test_modify_triggers_reload() {
        // 写入初始配置 → 创建 watcher → 修改文件 → 等待回调被调用
    }

    #[tokio::test]
    async fn test_create_event_triggers_reload() {
        // 部分编辑器用 rename 保存，触发 Create
    }

    // ===== T3 debounce =====
    #[tokio::test]
    async fn test_debounce_merges_rapid_writes() {
        // 短时间内多次写入 → 回调只被调用 1 次（或显著少于写入次数）
    }

    #[tokio::test]
    async fn test_debounce_uses_configured_interval() {
        // 传 debounce_ms=1000，测量回调触发延迟 ≥ 1000ms
    }

    // ===== T4 回调 =====
    #[tokio::test]
    async fn test_callback_receives_new_config() {
        // 修改文件后，mock 回调收到的 MystiConfig 应反映新内容
    }

    #[tokio::test]
    async fn test_callback_error_does_not_crash_watcher() {
        // 回调返回 Err → watcher 仍存活 → 下次修改仍能触发
    }

    // ===== T5 加载错误 =====
    #[tokio::test]
    async fn test_invalid_yaml_does_not_crash_watcher() {
        // 写入非法 YAML → watcher 加载失败但存活 → 改回合法 YAML 仍能重载
    }

    // ===== T6 start_config_watcher =====
    #[tokio::test]
    async fn test_start_config_watcher_updates_manager() {
        // 创建 manager + watcher → 修改文件 → manager.get_current() 反映新配置
    }
}
```

### 覆盖率目标

- `watcher.rs` 行覆盖率 ≥ 60%（文件系统事件测试天然有 flaky 风险，目标低于纯函数模块）
- 事件过滤、debounce、回调、错误路径均有用例
- `start_config_watcher` 端到端用例覆盖 `manager.update_config` 集成

### 集成验证

```bash
cargo test -p mystiproxy config::watcher::tests   # 模块单测
cargo test --workspace                             # 全量不回归
cargo clippy --workspace --all-targets -- -D warnings  # 无新告警
cargo fmt --check                                  # 格式通过
```

### flaky 测试缓解

文件系统事件在不同平台 / CI 环境下延迟差异大，测试可能 flaky。缓解策略：

1. ** generous timeout**：用 `timeout(Duration::from_secs(5), ...)` 而非 `Duration::from_millis(100)`
2. **重试断言**：关键断言失败时 sleep 后重试 1-2 次
3. **CI 标记**：极 flaky 的测试用 `#[ignore]` 标记，手动执行
4. **平台差异**：macOS FSEvents 事件延迟通常 > Linux inotify，timeout 需覆盖最慢平台
