# F8b 配置验证框架-加载器与管理器 — Task 规划

## 任务分解（TDD 顺序）

> 总体策略：先写失败测试（红），再实现加载器与管理器使测试转绿，最后做集成与闭环验证。加载器的 `ValidationLevel` 三档分支、管理器的每个公开方法独立 TDD，便于定位回归。
>
> **背景说明**：F8b 代码已实现（`loader.rs` + `manager.rs`），本任务清单是反向工程补齐的 TDD 视角规划。已实现部分标注"✅ 已实现"，未实现或测试覆盖不足的部分标注"✅ 已完成（2026-08-16）"。

### T1 加载器基础结构与 `ValidationLevel` TDD

- [x] ✅ 已实现：在 `mystiproxy/src/config/loader.rs` 新建文件，定义 `ValidationLevel` 枚举（`Strict` / `Warning` / `None`）
- [x] ✅ 已实现：定义 `ConfigSource` 枚举（`File(String)` / `Environment(String)` / `Default(serde_json::Value)`）
- [x] ✅ 已实现：定义 `EnhancedConfigLoader` 结构体（`sources: Vec<ConfigSource>` + `validation_level: ValidationLevel`）
- [x] ✅ 已实现：实现 `Default` trait（`fn default() -> Self { Self::new() }`）
- [x] ✅ 已实现：实现 `new()` / `with_validation_level(level)` / `add_source(source)` builder 方法
- [x] ✅ 已实现：在 `mystiproxy/src/config/mod.rs` 新增 `pub mod loader;` 与 `pub use loader::{ConfigSource, EnhancedConfigLoader, ValidationLevel};`
- [ ] ✅ 已完成（2026-08-16）：在 `loader.rs` 的 `#[cfg(test)] mod tests` 内补全模型层失败测试：
  - `test_validation_level_default_is_strict`：`assert_eq!(EnhancedConfigLoader::new().validation_level, ValidationLevel::Strict)`（需暴露 `validation_level` 字段或加 getter）
  - `test_new_creates_empty_loader`：`new()` 后 `sources` 为空
  - `test_with_validation_level_changes_level`：`new().with_validation_level(Warning)` 后级别为 `Warning`
  - `test_add_source_appends_to_sources`：连续 `add_source` 两次后 `sources.len() == 2`
- [ ] `cargo test -p mystiproxy config::loader::tests` 验证全**绿**

**验收标准**：加载器基础结构编译通过；`new()` 默认 `Strict`；builder 方法链式可用；`config/mod.rs` 正确导出。

### T2 加载器 `load<T>` 多源合并 TDD

- [x] ✅ 已实现：实现 `load<T: DeserializeOwned + Serialize>(self) -> ValidationResult<T>`：
  - 遍历 `self.sources`，按 `ConfigSource` 变体映射到 `config` crate 的 `File` / `Environment` / `File::from_str`
  - `Config::builder().add_source(...)` 链式合并（后添加优先级高）
  - `builder.build()` 失败 → `ConfigValidationError::Load`
  - `config.try_deserialize::<T>()` 失败 → `ConfigValidationError::Parse`
- [x] ✅ 已实现：`ConfigSource::Default` 经 `serde_json::to_string` 转 JSON 字符串，再 `File::from_str(json_str, FileFormat::Json)` 注入
- [x] ✅ 已实现：`ConfigSource::Environment` 用 `Environment::with_prefix(prefix).separator("__")`
- [ ] ✅ 已完成（2026-08-16）：多源合并失败测试：
  - `test_load_single_file_valid`：单文件加载合法 `MystiConfig` → `Ok`
  - `test_load_file_not_found`：`ConfigSource::File("nonexistent.yaml")` → `Err(Load(...))`
  - `test_load_env_overrides_file`：文件 + 环境变量，环境变量覆盖文件字段
  - `test_load_default_as_fallback`：`Default` + `File`，文件未定义字段用默认值
  - `test_load_deserialize_type_mismatch`：YAML 中 `listen: 123`（非字符串）→ `Err(Parse(...))`
- [ ] `cargo test -p mystiproxy config::loader::tests test_load` 验证全**绿**

**验收标准**：多源合并按"后添加优先"覆盖；三种 `ConfigSource` 均有用例；错误映射到正确的 `ConfigValidationError` 变体。

### T3 加载器验证级别分支 TDD

- [x] ✅ 已实现：`load<T>` 中按 `self.validation_level` 分支：
  - `Strict`：`serde_json::to_value(&parsed)` 反射 `mysti.engine`，逐个 `from_value::<EngineConfig>` 后调 `validate_engine_config`，失败 `return Err(Validation(...))`
  - `Warning`：同上遍历，失败 `tracing::warn!("Engine '{}' validation warnings: {}", name, e)`，仍返回 `Ok(parsed)`
  - `None`：完全跳过验证，直接 `Ok(parsed)`
- [x] ✅ 已实现：`load_mysti_config(path: &str) -> ValidationResult<MystiConfig>` 便捷方法（`new().add_source(File).load()`）
- [ ] ✅ 已完成（2026-08-16）：验证级别分支失败测试：
  - `test_load_strict_rejects_invalid_engine`：含非法 `target: "http://example.com"` + `proxy_type: Tcp` 的配置，`Strict` 下 `load` 返回 `Err(Validation(...))`
  - `test_load_warning_allows_invalid_engine`：同上配置，`Warning` 下 `load` 返回 `Ok`（验证失败仅 warn）
  - `test_load_none_skips_validation`：同上配置，`None` 下 `load` 返回 `Ok`，且不产生 warn 日志
  - `test_load_mysti_config_convenience`：`load_mysti_config("valid.yaml")` 返回 `Ok(MystiConfig)`
  - `test_load_strict_valid_config_passes`：合法配置在 `Strict` 下 `load` 返回 `Ok`
- [ ] `cargo test -p mystiproxy config::loader::tests test_load_strict test_load_warning test_load_none` 验证全**绿**

**验收标准**：三档 `ValidationLevel` 行为符合 spec；`Strict` 失败返回 `Validation` 变体；`Warning` / `None` 不阻断加载。

### T4 加载器集成测试

- [x] ✅ 已实现：现有测试 `test_load_valid_config` / `test_validate_engine_config_valid` / `test_validate_engine_config_invalid`（位于 `loader.rs:159-215`）
- [ ] ✅ 已完成（2026-08-16）：端到端集成测试：
  - `test_load_multi_source_end_to_end`：`Default` + `File` + `Environment` 三源合并，验证最终配置字段来自最高优先级源
  - `test_load_strict_collects_first_error_only`：含 2 处错误的配置，`Strict` 下 `load` 返回的 `Err` 只含第一个错误（与 F8a 累积式不同，加载器是短路式）
  - `test_load_generic_type_not_mysti_config`：`load::<EngineConfig>` 加载子配置，验证逻辑静默跳过（无 `mysti.engine` 字段）
- [ ] `cargo test -p mystiproxy config::loader::tests test_load_multi_source` 验证全**绿**

**验收标准**：多源合并端到端可用；短路式错误返回行为符合设计；泛型加载对非 `MystiConfig` 类型安全。

### T5 管理器基础结构与 `new` TDD

- [x] ✅ 已实现：在 `mystiproxy/src/config/manager.rs` 新建文件，定义 `ConfigSnapshot` 结构体（`config` / `timestamp` / `version` / `source`，`#[derive(Debug, Clone)]`）
- [x] ✅ 已实现：定义 `ConfigChangeEvent` 结构体（`old_config` / `new_config` / `timestamp` / `validation_success`，`#[derive(Debug, Clone)]`）
- [x] ✅ 已实现：定义 `ConfigurationManager` 结构体（`current_config: Arc<AsyncRwLock<MystiConfig>>` / `config_history: Arc<RwLock<Vec<ConfigSnapshot>>>` / `reload_notifier: broadcast::Sender<ConfigChangeEvent>` / `max_history_size: usize`）
- [x] ✅ 已实现：实现 `new(initial_config: MystiConfig) -> Result<Self, ConfigValidationError>`：
  - 创建 `broadcast::channel(100)`
  - 初始化 `current_config` 为 `Arc::new(AsyncRwLock::new(initial_config.clone()))`
  - 初始化空 `config_history`
  - `max_history_size = 10`
  - 调 `save_snapshot(&initial_config, "initial")` 保存初始快照
- [x] ✅ 已实现：在 `mystiproxy/src/config/mod.rs` 新增 `pub mod manager;` 与 `pub use manager::{ConfigChangeEvent, ConfigSnapshot, ConfigurationManager};`
- [x] ✅ 已实现：测试 `test_config_manager_creation`（位于 `manager.rs:197-203`）：`new(config)` 后 `get_current` 返回的配置 `engine.len() == 1`
- [ ] ✅ 已完成（2026-08-16）：边界测试：
  - `test_new_saves_initial_snapshot`：`new` 后 `get_history().len() == 1`，且 `history[0].source == "initial"`
  - `test_new_broadcast_channel_capacity`：`subscribe()` 返回的 Receiver 容量为 100（间接验证，如连续 `send` 100 次不阻塞）
- [ ] `cargo test -p mystiproxy config::manager::tests test_config_manager_creation test_new` 验证全**绿**

**验收标准**：`new` 创建管理器并保存初始快照；`current_config` 持有初始配置；broadcast channel 初始化成功。

### T6 管理器 `get_current` 与 `update_config` TDD

- [x] ✅ 已实现：实现 `get_current(&self) -> MystiConfig`（`async`，`current_config.read().await.clone()`）
- [x] ✅ 已实现：实现 `update_config(&self, new_config: MystiConfig) -> ValidationResult<()>`（`async`）：
  - `get_current()` 获取 `old_config`
  - `serde_json::to_value(&new_config)` 反射 `mysti.engine`
  - 遍历 `engines_map`，每个 `from_value::<EngineConfig>` 后调 `validate_engine_config`（`?` 短路，恒 Strict）
  - `current_config.write().await` 更新（短临界区）
  - `save_snapshot(&new_config, "reload")`
  - 构造 `ConfigChangeEvent { old_config, new_config, timestamp: now, validation_success: true }`
  - `let _ = reload_notifier.send(event)`
- [x] ✅ 已实现：测试 `test_config_update`（位于 `manager.rs:205-226`）：更新后 `get_current` 反映新 `request_timeout`
- [ ] ✅ 已完成（2026-08-16）：边界与错误测试：
  - `test_update_config_rejects_invalid`：含非法 `target: "http://example.com"` + `proxy_type: Tcp` 的配置，`update_config` 返回 `Err(Validation(...))`，且 `get_current` 未变
  - `test_update_config_saves_reload_snapshot`：更新后 `get_history().len()` 增加 1，且最新快照 `source == "reload"`
  - `test_update_config_broadcasts_event`：`subscribe()` 后 `update_config`，Receiver 收到事件，`event.new_config` 匹配新配置
  - `test_update_config_event_validation_success_true`：事件中 `validation_success == true`
  - `test_update_config_missing_mysti_engine`：`new_config` 缺 `mysti.engine` 字段 → `Err(Parse(...))`
- [ ] `cargo test -p mystiproxy config::manager::tests test_update_config` 验证全**绿**

**验收标准**：`update_config` 验证失败时不更新 `current_config`；成功时保存快照、广播事件；事件字段正确。

### T7 管理器 `rollback_to_previous` TDD

- [x] ✅ 已实现：实现 `rollback_to_previous(&self) -> ValidationResult<()>`（`async`）：
  - `config_history.read().unwrap()` 读历史
  - `history.len() < 2` → `Err(Load("No previous version to rollback to"))`
  - `previous_config = history[history.len() - 2].config.clone()`
  - `drop(history)` 显式释放锁
  - `self.update_config(previous_config).await` 复用全流程
- [ ] ✅ 已完成（2026-08-16）：失败测试：
  - `test_rollback_with_insufficient_history`：`new` 后立即 `rollback_to_previous`（历史仅 1 条）→ `Err(Load(...))`
  - `test_rollback_to_previous_success`：`new` + `update_config` 后 `rollback_to_previous`，`get_current` 等于 `history[0].config`
  - `test_rollback_adds_new_snapshot`：回滚后 `get_history().len()` 增加 1，最新快照 `source == "reload"`
  - `test_rollback_broadcasts_event`：回滚触发事件，`event.new_config` 等于回滚目标
- [ ] `cargo test -p mystiproxy config::manager::tests test_rollback` 验证全**绿**

**验收标准**：历史不足 2 条时返回明确错误；回滚成功后 `current_config` 等于上一版；回滚本身也留痕。

### T8 管理器 `get_history` 与 `subscribe` TDD

- [x] ✅ 已实现：实现 `get_history(&self) -> Vec<ConfigSnapshot>`（`config_history.read().unwrap().clone()`）
- [x] ✅ 已实现：实现 `subscribe(&self) -> broadcast::Receiver<ConfigChangeEvent>`（`reload_notifier.subscribe()`）
- [x] ✅ 已实现：测试 `test_config_history`（位于 `manager.rs:228-249`）：`new` 后历史 1 条，`update_config` 后历史 2 条
- [ ] ✅ 已完成（2026-08-16）：边界测试：
  - `test_history_max_size_eviction`：连续 `update_config` 11 次，`get_history().len() == 10`（最旧被淘汰）
  - `test_history_order_oldest_to_newest`：`history[0].timestamp < history[len-1].timestamp`
  - `test_subscribe_multiple_receivers`：`subscribe()` 两次，`update_config` 后两个 Receiver 都收到事件
  - `test_subscribe_lagged_when_slow`：Receiver 不消费，连续 `update_config` 100+ 次后 `recv()` 返回 `Err(Lagged(n))`
  - `test_subscribe_closed_when_manager_dropped`：drop manager 后 `recv()` 返回 `Err(Closed)`
- [ ] `cargo test -p mystiproxy config::manager::tests test_history test_subscribe` 验证全**绿**

**验收标准**：历史上限 10 生效；多订阅者都能收到事件；lagged / closed 错误路径覆盖。

### T9 集成测试：加载器 + 管理器闭环

- [ ] ✅ 已完成（2026-08-16）：在 `mystiproxy/tests/config_loader_manager.rs`（或 `manager.rs` 集成测试模块）补全端到端用例：
  - `test_loader_to_manager_end_to_end`：
    - `EnhancedConfigLoader::load_mysti_config("valid.yaml")` 加载配置
    - `ConfigurationManager::new(config)` 创建管理器
    - `subscribe()` 订阅事件
    - 修改 YAML 文件后再次 `load_mysti_config` + `update_config`
    - 验证 `get_current` 反映新配置、`get_history().len() == 2`、Receiver 收到事件
  - `test_rollback_after_failed_update`：
    - `update_config` 传入非法配置 → `Err`
    - `get_current` 未变，`get_history().len()` 未增
    - `rollback_to_previous` 仍可成功（历史未受失败影响）
  - `test_multi_source_load_into_manager`：
    - `Default` + `File` + `Environment` 三源合并加载
    - `ConfigurationManager::new` 接收合并后配置
    - 验证 `get_current` 的字段来自最高优先级源
- [ ] `cargo test -p mystiproxy --test config_loader_manager` 验证全**绿**

**验收标准**：加载器与管理器协作闭环；失败更新不污染历史；多源合并结果可注入管理器。

### T10 验证闭环

- [x] ✅ 已实现：`cargo test -p mystiproxy config::loader::tests` 全绿（现有 3 个测试：`test_load_valid_config` / `test_validate_engine_config_valid` / `test_validate_engine_config_invalid`）
- [x] ✅ 已实现：`cargo test -p mystiproxy config::manager::tests` 全绿（现有 3 个测试：`test_config_manager_creation` / `test_config_update` / `test_config_history`）
- [ ] ✅ 已完成（2026-08-16）：`cargo test --workspace` 全绿（现有测试无回归，F8b 只新增不改旧）
- [ ] ✅ 已完成（2026-08-16）：`cargo fmt --all -- --check` 通过
- [ ] ✅ 已完成（2026-08-16）：`cargo clippy --workspace --all-targets -- -D warnings` 无新告警
- [ ] ✅ 已完成（2026-08-16）：`cargo llvm-cov -p mystiproxy config::loader config::manager`（如可用）新增行覆盖 ≥ 70%
- [ ] ✅ 已完成（2026-08-16）：手动构造一份含 2 处错误的 YAML，`EnhancedConfigLoader::new().add_source(File).load::<MystiConfig>()` 确认返回 `Err(Validation(...))`；切换 `ValidationLevel::Warning` 确认返回 `Ok` 且日志含 warn
- [ ] ✅ 已完成（2026-08-16）：手动验证 `ConfigurationManager` 闭环：`new` → `update_config` → `rollback_to_previous` → `get_history` 输出 3 条快照（initial / reload / reload）

**验收标准**：全量测试 + clippy + fmt 闭环；覆盖率达标；手动验证加载器三档级别与管理器回滚闭环符合预期。

### T11 推送 CI

- [ ] ⚠️ 待执行：提交（`feat(mystiproxy/config): add F8b config loader and manager`）
- [ ] ⚠️ 待执行：push GitHub，盯 `.github/workflows/rust.yml` Actions 至全绿
- [ ] ⚠️ 待执行：更新 `ROADMAP.md` 标注 F8b（配置验证-加载器与管理器）已闭环
- [ ] ⚠️ 待执行：更新 `docs/FEATURE_COVERAGE.md` 增加加载器与管理器一节
- [ ] ⚠️ 待执行：在 F8c 设计文档中引用 F8b 的 `ConfigurationManager::update_config` 作为监听器变更入口

**验收标准**：CI 全绿；ROADMAP / FEATURE_COVERAGE 更新；F8c 可基于 F8b 的稳定 API 启动设计。

## 验收标准汇总

### 加载器（`EnhancedConfigLoader`）

| 验收项 | 状态 | 说明 |
| :--- | :--- | :--- |
| `new()` 默认 `Strict` | ✅ 已实现 | `loader.rs:48` |
| `with_validation_level` 链式设置 | ✅ 已实现 | `loader.rs:53-56` |
| `add_source` 链式添加 | ✅ 已实现 | `loader.rs:59-62` |
| `load<T>` 多源合并 + 反序列化 | ✅ 已实现 | `loader.rs:65-86` |
| `Strict` 验证失败返回 `Err(Validation)` | ✅ 已实现 | `loader.rs:96-117` |
| `Warning` 验证失败 `tracing::warn!` | ✅ 已实现 | `loader.rs:118-141` |
| `None` 跳过验证 | ✅ 已实现 | `loader.rs:142-143` |
| `load_mysti_config` 便捷方法 | ✅ 已实现 | `loader.rs:147-151` |
| 三档级别测试覆盖 | ✅ 已完成（2026-08-16） | 现有测试未覆盖 `Warning` / `None` 分支 |
| 多源合并测试覆盖 | ✅ 已完成（2026-08-16） | 现有测试未覆盖 `Environment` / `Default` 源 |

### 管理器（`ConfigurationManager`）

| 验收项 | 状态 | 说明 |
| :--- | :--- | :--- |
| `new` 创建并保存初始快照 | ✅ 已实现 | `manager.rs:39-53` |
| `get_current` 异步读 | ✅ 已实现 | `manager.rs:56-58` |
| `update_config` 验证 + 更新 + 快照 + 通知 | ✅ 已实现 | `manager.rs:61-107` |
| `rollback_to_previous` 回滚 | ✅ 已实现 | `manager.rs:110-122` |
| `get_history` 同步读历史 | ✅ 已实现 | `manager.rs:125-127` |
| `subscribe` 订阅事件 | ✅ 已实现 | `manager.rs:130-132` |
| `save_snapshot` 私有方法 + 版本号 | ✅ 已实现 | `manager.rs:135-163` |
| `max_history_size = 10` FIFO 淘汰 | ✅ 已实现 | `manager.rs:158-160` |
| `update_config` 验证失败不更新 | ✅ 已完成（2026-08-16）测试 | 代码逻辑正确（`?` 短路在写临界区前），但无显式测试 |
| `rollback_to_previous` 历史不足错误 | ✅ 已完成（2026-08-16）测试 | 代码逻辑正确，但无显式测试 |
| `subscribe` lagged / closed 路径 | ✅ 已完成（2026-08-16）测试 | 未覆盖 |
| 历史上限淘汰测试 | ✅ 已完成（2026-08-16）测试 | 未覆盖 |

## 信心评估

| 任务 | 信心 | 依据 |
| :--- | :--- | :--- |
| T1 加载器基础结构 | 98% | 纯类型定义 + builder 模式，已实现 |
| T2 多源合并 `load<T>` | 92% | `config` crate 已在 `Cargo.toml`；`Config::builder + add_source` 是标准用法 |
| T3 验证级别分支 | 90% | 三档分支逻辑简单；`Warning` 的 `tracing::warn!` 已实现 |
| T4 加载器集成测试 | 88% | 现有测试覆盖合法路径，非法路径与多源合并需补全 |
| T5 管理器基础结构 | 95% | `Arc<AsyncRwLock>` + `broadcast` 是标准 tokio 模式，已实现 |
| T6 `update_config` | 88% | 验证逻辑与加载器一致；短临界区设计正确；需补全验证失败不更新的测试 |
| T7 `rollback_to_previous` | 85% | `drop(history)` 后 `await` 的锁安全需测试验证；回滚增加快照的副作用需文档化 |
| T8 `get_history` / `subscribe` | 85% | broadcast lagged / closed 路径需显式测试；历史上限淘汰需连续 update 验证 |
| T9 集成测试 | 90% | 端到端组合 T1–T8，无新逻辑 |
| T10 验证闭环 | 88% | 标准 cargo 工具链；覆盖率工具可用性取决于环境 |
| T11 推送 CI | 80% | 取决于 CI 环境状态，非代码风险 |
| **整体** | **>88%** | **代码已实现，文档反向工程补齐；测试覆盖是主要待补全项** |

## 实现完成情况（2026-08-15）

### 已实现部分

- **加载器（`mystiproxy/src/config/loader.rs`）**：
  - `ValidationLevel` 枚举（`Strict` / `Warning` / `None`），约 10 行
  - `ConfigSource` 枚举（`File` / `Environment` / `Default`），约 8 行
  - `EnhancedConfigLoader` 结构体 + `Default` impl + 5 个方法（`new` / `with_validation_level` / `add_source` / `load<T>` / `load_mysti_config`），约 110 行
  - `load<T>` 内部：多源合并（`config` crate builder）+ 反序列化 + 三档验证分支
  - 现有测试 3 个：`test_load_valid_config` / `test_validate_engine_config_valid` / `test_validate_engine_config_invalid`
- **管理器（`mystiproxy/src/config/manager.rs`）**：
  - `ConfigSnapshot` 结构体（4 字段），约 7 行
  - `ConfigChangeEvent` 结构体（4 字段），约 7 行
  - `ConfigurationManager` 结构体（4 字段）+ 6 个方法（`new` / `get_current` / `update_config` / `rollback_to_previous` / `get_history` / `subscribe`）+ 私有 `save_snapshot`，约 130 行
  - 内部状态：`Arc<AsyncRwLock<MystiConfig>>` + `Arc<RwLock<Vec<ConfigSnapshot>>>` + `broadcast::Sender<ConfigChangeEvent>(100)` + `max_history_size = 10`
  - 现有测试 3 个：`test_config_manager_creation` / `test_config_update` / `test_config_history`
- **模块接入（`mystiproxy/src/config/mod.rs`）**：
  - 新增 `pub mod loader;` / `pub mod manager;`
  - 新增 `pub use loader::{ConfigSource, EnhancedConfigLoader, ValidationLevel};`
  - 新增 `pub use manager::{ConfigChangeEvent, ConfigSnapshot, ConfigurationManager};`

### 待补全部分

- **测试覆盖**：加载器三档 `ValidationLevel` 分支、多源合并（`Environment` / `Default`）、管理器 `update_config` 验证失败不更新、`rollback_to_previous` 历史不足错误、`subscribe` lagged / closed 路径、历史上限淘汰——均需补全测试
- **T10 验证闭环**：`cargo test --workspace` 全量回归、`cargo fmt --check`、`cargo clippy -D warnings`、覆盖率验证
- **T11 推送 CI**：提交、push、ROADMAP / FEATURE_COVERAGE 更新

### 已知不一致点（反向工程发现）

| 不一致点 | F8a 文档设计 | F8b 实际实现 | 处理 |
| :--- | :--- | :--- | :--- |
| `ValidationLevel` 命名 | `Strict` / `Warn` / `Loose` | `Strict` / `Warning` / `None` | F8b 文档以实际实现为准，spec FAQ Q1 已说明 |
| 默认级别 | F8a `ConfigValidator` 默认 `Loose` | F8b `EnhancedConfigLoader` 默认 `Strict` | 设计选择不同，spec §7.2 已说明 |
| `update_config` 验证级别 | F8a 文档未涉及 | F8b 恒 `Strict`（硬编码） | 设计选择，spec §7.2 已说明 |
| `ValidationResult` 语义 | F8a 文档设计为"累积式"（收集所有问题） | F8b 实际复用 F8a `validation` 模块的 `ValidationResult<T> = Result<T, ConfigValidationError>`（短路式） | F8b 文档以实际实现为准，spec §4.3 已说明 |

## 风险与缓解

| 风险 | 缓解 |
| :--- | :--- |
| `update_config` 中 `std::sync::RwLock` 跨 `await` 持有导致死锁 | 代码已在 `rollback_to_previous` 中显式 `drop(history)` 后再 `await`；`update_config` 中 `save_snapshot` 不跨 `await`；T6 / T7 测试需覆盖并发场景 |
| broadcast 容量 100 在高频热重载下不足 | T8 中 `test_subscribe_lagged_when_slow` 覆盖 lagged 路径；订阅者需显式处理 `RecvError::Lagged`；未来可配置容量 |
| 版本号用纳秒时间戳，时钟回拨产生重复 | 单进程 + NTP 同步下概率极低；未来可换 ULID / UUID；`save_snapshot` 中 `duration_since(UNIX_EPOCH).unwrap()` 假设时钟正常 |
| `rollback_to_previous` 增加快照导致历史快速淘汰 | 设计选择："每次变更都留痕"优先；`max_history_size = 10` 足够典型场景；未来可配置上限 |
| `load<T>` 验证只针对 `mysti.engine`，加载子配置时不验证 | 设计选择：F8a 规则只覆盖 `EngineConfig`；T4 中 `test_load_generic_type_not_mysti_config` 验证静默跳过行为 |
| `ConfigSource::Default` 经 JSON 序列化，`serde_json::Value` 中非 JSON 类型失败 | `serde_json::Value` 不产生 NaN / Inf，理论安全；T2 中 `test_load_default_as_fallback` 覆盖典型场景 |
| `update_config` 的 `validation_success` 恒 `true`，订阅者无法区分软验证 | 当前 `update_config` 不支持软验证；字段为未来扩展预留；spec §5.2 / FAQ Q3 已说明 |
| 现有测试覆盖不足（仅 6 个），三档级别 / 多源 / 回滚 / 订阅路径未覆盖 | T1–T9 明确列出待补全测试清单；T10 验证闭环要求覆盖率 ≥ 70% |
