# F8c 配置验证框架-热重载 Watcher — Spec

## 1. 概述

F8c 为 MystiProxy 引入**配置文件热重载能力**：在进程运行期间监听配置文件变化，自动重新加载并通过回调把新 `MystiConfig` 交给调用方（默认是 `ConfigurationManager::update_config`），让"改了配置就要重启"成为过去式。

本阶段是配置验证框架的**第三阶段**，依赖 F8a（验证模型）与 F8b（增强加载器），交付 `mystiproxy/src/config/watcher.rs` 中的 `ConfigFileWatcher` 与 `start_config_watcher`。F8c 只做"监听 + 触发"，不做"编排 + UI"（留给 F8d）。

### 设计要点

| 维度 | 选择 | 理由 |
| :--- | :--- | :--- |
| 监听后端 | `notify::RecommendedWatcher` | 跨平台自动选择 FSEvents / inotify / ReadDirectoryChangesW |
| 递归模式 | `NonRecursive` | 配置是单文件，无需递归 |
| 事件过滤 | `Modify` / `Create` / `Remove` | 忽略 `Access` 等噪声事件 |
| debounce 实现 | `mpsc(1) + sleep` | 背压式合并，编辑器抖动合并为一次重载 |
| 回调签名 | `Fn(MystiConfig) -> Result<()>` | 同步、按值传 config、错误隔离 |
| 错误隔离 | 三段式 `match + error!` | 加载 / 回调 / 回调内部错误互不击穿 |
| 依赖增量 | 零新 crate | `notify` / `tokio` / `tempfile`(test) 均已在 `Cargo.toml` |

## 2. 功能说明

### 2.1 模块结构

```
mystiproxy/src/config/
├── mod.rs              ← 现有：MystiConfig / EngineConfig 等
├── validation.rs       ← F8a：ConfigValidator / ValidationResult
├── loader.rs           ← F8b：EnhancedConfigLoader / ConfigSource
├── manager.rs          ← 现有：ConfigurationManager / ConfigChangeEvent
└── watcher.rs          ← F8c（本阶段）：ConfigFileWatcher / start_config_watcher
```

`watcher.rs` 依赖 `notify` crate + `tokio` + F8b 的 `EnhancedConfigLoader` + F8a 的 `ConfigValidationError`。

### 2.2 公共类型一览

| 类型 | 用途 |
| :--- | :--- |
| `ConfigFileWatcher` | 配置文件监控器，持有 `notify::RecommendedWatcher` 与后台任务信号通道 |
| `start_config_watcher` | 便捷函数，封装 `ConfigFileWatcher` + `ConfigurationManager` 常见组合 |

### 2.3 公共 API

| 类型 / 方法 | 签名 | 用途 |
| :--- | :--- | :--- |
| `ConfigFileWatcher::new` | `fn new<F>(config_path: String, debounce_ms: u64, reload_callback: F) -> Result<Self, ConfigValidationError> where F: Fn(MystiConfig) -> Result<(), ConfigValidationError> + Send + Sync + 'static` | 创建并启动监控器 |
| `start_config_watcher` | `async fn start_config_watcher(config_path: String, debounce_ms: u64, manager: Arc<ConfigurationManager>) -> Result<tokio::task::JoinHandle<()>, ConfigValidationError>` | 便捷启动：watcher + manager 集成 |

## 3. 使用方式

### 3.1 基本用法：直接使用 `ConfigFileWatcher`

适合需要自定义回调的场景（如自定义配置应用逻辑、仅记录不应用）。

```rust
use mystiproxy::config::watcher::ConfigFileWatcher;
use mystiproxy::config::MystiConfig;

let watcher = ConfigFileWatcher::new(
    "config.yaml".to_string(),
    500, // debounce_ms
    |new_config: MystiConfig| -> Result<(), _> {
        // 自定义应用逻辑
        println!("Config reloaded: {} engines", new_config.mysti.engine.len());
        Ok(())
    },
)?;
// watcher 持有监听句柄，drop 时停止监听
```

### 3.2 生产用法：通过 `start_config_watcher` 集成 `ConfigurationManager`

适合标准热重载场景：文件变化 → 重新加载 → 更新 manager → 广播事件。

```rust
use std::sync::Arc;
use mystiproxy::config::manager::ConfigurationManager;
use mystiproxy::config::watcher::start_config_watcher;

let manager = Arc::new(ConfigurationManager::new(initial_config)?);
let handle = start_config_watcher(
    "config.yaml".to_string(),
    1000, // debounce_ms
    manager.clone(),
).await?;

// 订阅配置变更事件
let mut rx = manager.subscribe();
tokio::spawn(async move {
    while let Ok(event) = rx.recv().await {
        println!("Config changed at {:?}, validation_success={}",
                 event.timestamp, event.validation_success);
    }
});

// handle 保持 watcher 存活；drop handle 不会停止 watcher（任务 detach）
```

### 3.3 自定义回调：仅记录不应用

适合"试运行"场景：观察配置变化但不实际更新运行时配置。

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

let reload_count = Arc::new(AtomicUsize::new(0));
let reload_count_clone = reload_count.clone();

let _watcher = ConfigFileWatcher::new(
    "config.yaml".to_string(),
    500,
    move |_new_config| {
        let count = reload_count_clone.fetch_add(1, Ordering::SeqCst) + 1;
        println!("Reload #{} (not applied)", count);
        Ok(())
    },
)?;
```

### 3.4 与 F8a 验证框架衔接

`EnhancedConfigLoader` 默认 `ValidationLevel::Strict`，加载时自动跑 F8a 的 8 条规则。配置非法时加载失败，watcher `error!` 记录但不崩溃。

```rust
// watcher 内部已使用 EnhancedConfigLoader::new()，默认 Strict
// 非法配置 → load() 返回 Err → error!("Failed to reload configuration: ...")
// 合法配置 → load() 返回 Ok → callback(new_config)
```

## 4. API 说明

### 4.1 `ConfigFileWatcher::new`

```rust
pub fn new<F>(
    config_path: String,
    debounce_ms: u64,
    reload_callback: F,
) -> Result<Self, ConfigValidationError>
where
    F: Fn(MystiConfig) -> Result<(), ConfigValidationError> + Send + Sync + 'static,
```

#### 参数

| 参数 | 类型 | 说明 |
| :--- | :--- | :--- |
| `config_path` | `String` | 配置文件路径；不存在或不可读时 `watcher.watch` 返回 `Err` |
| `debounce_ms` | `u64` | debounce 时长（毫秒）；F8c 修复后真正驱动 `sleep` |
| `reload_callback` | `F: Fn(MystiConfig) -> Result<()>` | 重载回调；按值接收 `MystiConfig`，返回 `Err` 仅记录不重试 |

#### 返回

- `Ok(Self)`：watcher 已创建并启动后台任务
- `Err(ConfigValidationError::Watch(String))`：`notify::RecommendedWatcher::new` 或 `watcher.watch` 失败

#### 副作用

- 创建 `mpsc::channel(1)`
- 创建 `notify::RecommendedWatcher` 并注册 `config_path`（`NonRecursive`）
- `tokio::spawn` 一个后台任务，循环 `rx.recv() → sleep(debounce) → load → callback`

#### 生命周期

- `ConfigFileWatcher` drop 时，`reload_tx` drop
- 但 `watcher`（`RecommendedWatcher`）也持有 `tx` 副本，需等 `watcher` drop 后通道才关闭
- 通道关闭后 `rx.recv()` 返回 `None`，后台任务退出

### 4.2 `start_config_watcher`

```rust
pub async fn start_config_watcher(
    config_path: String,
    debounce_ms: u64,
    manager: Arc<ConfigurationManager>,
) -> Result<tokio::task::JoinHandle<()>, ConfigValidationError>
```

#### 参数

| 参数 | 类型 | 说明 |
| :--- | :--- | :--- |
| `config_path` | `String` | 配置文件路径 |
| `debounce_ms` | `u64` | debounce 时长（毫秒） |
| `manager` | `Arc<ConfigurationManager>` | 配置管理器；`update_config` 会被异步调用 |

#### 返回

- `Ok(JoinHandle<()>)`：watcher 已启动，handle 用于等待任务结束（实际永不结束，靠进程退出）
- `Err(ConfigValidationError::Watch(String))`：`ConfigFileWatcher::new` 失败

#### 内部回调

```rust
move |new_config| {
    let manager = manager_clone.clone();
    tokio::spawn(async move {
        if let Err(e) = manager.update_config(new_config).await {
            error!("Failed to update config: {}", e);
        }
    });
    Ok(())
}
```

- 回调内部 `tokio::spawn` 异步执行 `update_config`
- 回调立即返回 `Ok(())`，watcher 不等待 `update_config` 完成
- `update_config` 失败时 `error!` 记录，不影响 watcher

## 5. 事件处理

### 5.1 事件类型与过滤

`notify` 会产生多种事件，F8c 仅对以下三类触发重载：

| `EventKind` | 是否触发 | 典型场景 |
| :--- | :--- | :--- |
| `Modify(_)` | ✅ | 内容修改（最常见） |
| `Modify(Mode::Rename)` | ✅ | 编辑器原子保存（先写临时文件再 rename） |
| `Create(_)` | ✅ | 文件被创建（部分编辑器先删除再创建） |
| `Remove(_)` | ✅ | 文件被删除（检测配置被误删） |
| `Access(_)` | ❌ | 仅 atime 读取，无内容变化 |
| `Any` | ❌ | 兜底类型，不明确 |
| `Other` | ❌ | 平台特定事件 |

### 5.2 事件流

```
[文件变化]
   │
   ▼ notify 回调（同步闭包）
   │ 过滤：matches!(kind, Modify|Create|Remove)
   │ 通过：tx.blocking_send(())  ← 通道容量1，满时丢弃
   ▼
[后台任务]
   │ rx.recv().await → Some(())
   │ sleep(debounce_ms)  ← 合并抖动
   │ EnhancedConfigLoader::load::<MystiConfig>()
   │   ├─ Ok(config) → callback(config)
   │   │                ├─ Ok(()) → 完成，等下一轮
   │   │                └─ Err(e) → error!，等下一轮
   │   └─ Err(e) → error!，等下一轮
   ▼
[下一轮] rx.recv().await
```

### 5.3 跨平台行为

| 平台 | 后端 | 行为差异 |
| :--- | :--- | :--- |
| macOS | FSEvents | 事件延迟可能 > 100ms，建议 `debounce_ms ≥ 500` |
| Linux | inotify | 事件近乎实时，`debounce_ms = 200` 即可 |
| Windows | ReadDirectoryChangesW | 行为接近 inotify |
| NFS / FUSE | 可能 fallback 到轮询 | 事件延迟高，建议 `debounce_ms ≥ 2000` |

## 6. debounce 策略

### 6.1 debounce 语义

debounce 的目标：把编辑器抖动产生的多次事件合并为一次重载。

```
时间轴 →

事件1 ─┐
事件2 ─┤ 通道容量1，事件2 覆盖事件1
       ▼
     sleep(debounce_ms) 开始
事件3 ─┤ blocking_send 返回 Err（通道满），丢弃
事件4 ─┤ blocking_send 返回 Err，丢弃
       ▼
     sleep 结束，加载配置（此时文件已是事件4后的最终状态）
     ── 加载完成后，rx.recv().await 等待下一轮
```

### 6.2 `debounce_ms` 推荐值

| 场景 | 推荐 | 理由 |
| :--- | :--- | :--- |
| 本地开发（Vim / VSCode） | 500ms | 编辑器抖动通常 < 500ms |
| NFS / Docker volume | 1000-2000ms | 网络文件系统事件延迟更高 |
| 生产环境 | 1000ms | 平衡响应速度与稳定性 |
| 测试环境 | 100ms | 缩短测试等待时间 |

### 6.3 F8c 修复说明

当前实现（修复前）：

```rust
// ❌ debounce_ms 参数被忽略，写死 500ms
sleep(Duration::from_millis(500)).await;
```

F8c 修复后：

```rust
// ✅ 使用传入的 debounce_ms
sleep(Duration::from_millis(debounce_ms)).await;
```

`debounce_interval` 字段保留供调用方查询当前配置（未来可加 getter）。

## 7. 错误处理

### 7.1 三阶段错误隔离

| 阶段 | 错误来源 | 处置 | watcher 影响 |
| :--- | :--- | :--- | :--- |
| 加载 | `EnhancedConfigLoader::load`（YAML 语法 / F8a 校验） | `error!("Failed to reload configuration: {:?}", e)` | 不调回调，等下一轮 |
| 回调 | 回调返回 `Err` | `error!("Failed to apply config: {:?}", e)` | 不重试，等下一轮 |
| 回调内部 | `update_config` 内部 `Err` | 回调内 `error!` 记录 | 回调仍返回 `Ok(())`，watcher 不感知 |

### 7.2 错误类型

F8c 复用 F8a 的 `ConfigValidationError`：

```rust
pub enum ConfigValidationError {
    // ... F8a 已有变体
    Watch(String),  // watcher 创建 / 注册失败
}
```

`ConfigFileWatcher::new` 的错误点：
1. `RecommendedWatcher::new` 失败 → `ConfigValidationError::Watch(e.to_string())`
2. `watcher.watch(...)` 失败 → `ConfigValidationError::Watch(e.to_string())`

### 7.3 运行时错误恢复

watcher 设计为"永不退出"：

- 单次加载失败 → `error!` 记录 → 等下一轮事件
- 单次回调失败 → `error!` 记录 → 等下一轮事件
- 配置被误删 → `Remove` 事件触发 → 加载失败 `error!` → 配置恢复后自动重载

### 7.4 错误日志示例

```
ERROR mystiproxy::config::watcher: Failed to reload configuration: Parse("invalid type: string \"abc\", expected u64 at line 5 column 3")
ERROR mystiproxy::config::watcher: Failed to apply config: Validation("listen 地址必须以 tcp:// 或 unix:// 开头，当前值: 0.0.0.0:3128")
ERROR mystiproxy::config::watcher: Failed to update config: <update_config 内部错误>
```

## 8. 限制与约束

### 8.1 F8c 范围内不做

| 项 | 归属 | 原因 |
| :--- | :--- | :--- |
| 配置回滚 UI / 管理 API | F8d | `ConfigurationManager::rollback_to_previous` 已存在，UI 是独立复杂度 |
| 配置 diff 可视化 | F8d | 需要前端 + diff 算法 |
| 多文件 / 目录监听 | 后续 | F8c 仅监听单个 `config_path` |
| 显式 shutdown 信号 | 后续 / F8d | 当前靠进程退出 + handle drop |
| 跨平台行为差异测试 | 后续 | 用 debounce 统一缓解 |
| Watcher 自身指标 | 后续 | 与 `metrics.rs` 集成是独立工作 |

### 8.2 当前实现的已知限制

| 限制 | 说明 | 缓解 |
| :--- | :--- | :--- |
| `debounce_interval` 字段存储但未驱动 sleep（F8c 修复前） | `debounce_ms` 参数不生效，写死 500ms | F8c 修复为使用 `debounce_ms` |
| 后台任务无显式 shutdown | 依赖"所有 sender drop"退出，`start_config_watcher` 用 `loop { sleep }` 占位 | F8d 引入 shutdown 信号 |
| `start_config_watcher` 中 `watcher` 变量可能触发 unused 告警 | `let mut watcher = ...` 后未在闭包内使用 | 改为 `let _watcher = ...` 或显式 hold |
| `callback_clone` 死代码 | 创建后未使用 | F8c 删除 |
| 回调内部 `tokio::spawn` 异步执行 | 回调立即返回，`update_config` 结果不反馈给 watcher | 文档化此取舍 |
| 文件系统事件延迟在 CI 上可能 flaky | macOS FSEvents 延迟 > Linux inotify | 测试用 generous timeout |

### 8.3 不破坏现有类型

- `MystiConfig` / `EngineConfig` / `ConfigurationManager` 结构体**零修改**
- `EnhancedConfigLoader` 行为不变
- `ConfigValidationError` 仅复用现有 `Watch` 变体
- 不新增 crate 依赖（`notify` / `tokio` 已在 `Cargo.toml`）

## 9. FAQ

### Q1：为什么 watcher 用 `mpsc::channel(1)` 而非 `channel(100)` 或 unbounded？

容量 1 是背压式：watcher 闭包高频发信号时，`blocking_send` 在通道满时立即返回 `Err`，等价于"丢弃新信号"。这与 debounce 语义一致——已经在 debounce 等待中，再来的信号无需排队。容量大了会导致 debounce 后还有积压信号，触发多次重载，违背 debounce 初衷。

### Q2：为什么回调是同步 `Fn` 而非 `async Fn`？

Rust 的 `async fn` 闭包尚不稳定（`AsyncFn` trait 在 nightly）。同步 `Fn` + 内部 `tokio::spawn` 是当前最简方案。回调立即返回，异步工作在 spawn 的任务中执行。代价是 watcher 不感知 `update_config` 的最终结果，但错误已通过 `error!` 记录，可观测性足够。

### Q3：为什么 `start_config_watcher` 用 `loop { sleep(60s) }` 占位？

`ConfigFileWatcher::new` 内部已 `tokio::spawn` 后台任务处理重载，`start_config_watcher` 只需保持 `watcher` 不被 drop。`loop { sleep }` 是最简单的"永不出错"占位。`JoinHandle` 返回给调用方，调用方 drop 它不会终止任务（tokio 默认 detach），任务靠进程退出结束。F8d 可引入 shutdown 信号替换此占位。

### Q4：debounce 期间产生的事件会被丢弃吗？

会。debounce `sleep` 期间，`blocking_send` 在通道满时返回 `Err`，新事件被丢弃。但这是合理的——debounce 结束后加载的是"当前文件内容"，已包含所有变化。丢弃的只是"信号"，不是"变化"。

### Q5：配置文件被删除后还会重载吗？

会触发一次 `Remove` 事件，但加载会失败（文件不存在），`error!` 记录。文件恢复后下一次 `Modify` / `Create` 事件会正常重载。watcher 不会因为文件删除而退出。

### Q6：为什么不用 `notify::Config::default()` 之外的配置？

`Config::default()` 在大多数平台下足够。`notify` 提供的 `with_poll_interval` 等配置主要影响 `PollWatcher`，F8c 用 `RecommendedWatcher` 不涉及。未来若需调优（如 NFS 场景），可通过 `Config::default().with_poll_interval(...)` 扩展。

### Q7：watcher 能监听符号链接吗？

能。`notify` 默认跟随符号链接。但若符号链接目标在 watcher 启动后才创建，行为依赖平台。建议监听实际文件路径而非符号链接。

### Q8：如何手动测试热重载？

```bash
# 终端1：启动带 watcher 的 mystiproxy
RUST_LOG=info ./target/release/mystiproxy --config config.yaml --watch

# 终端2：修改配置
echo "  timeout: 30s" >> config.yaml

# 终端1 应看到日志：
# INFO mystiproxy::config::watcher: Configuration loaded successfully
```

> 注：`--watch` CLI 选项尚未实现，F8c 阶段需手动调用 `start_config_watcher`。CLI 集成留给后续。

### Q9：watcher 会重复加载相同配置吗？

会。文件 atime 变化不触发（已过滤 `Access`），但 `mtime` 变化（即使内容相同）会触发 `Modify` 事件，导致重复加载。F8c 不做内容 diff，重复加载的代价是"重新解析 + 校验 + 回调"，但 `ConfigurationManager::update_config` 内部会广播 `ConfigChangeEvent`，订阅方需自行判断是否真的变化。F8d 可引入内容 hash diff 优化。

### Q10：测试如何避免文件系统事件的 flaky？

1. 用 `tempfile::NamedTempFile` 创建临时文件，避免污染工作区
2. 用 `tokio::time::timeout(Duration::from_secs(5), ...)` 等待回调，而非固定 sleep
3. 多次写入合并测试用 `debounce_ms=100` 缩短等待
4. 极 flaky 的测试用 `#[ignore]` 标记，手动执行
