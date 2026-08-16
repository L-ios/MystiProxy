# F8c 配置验证框架-热重载 Watcher — Task 规划

## 任务分解（TDD 顺序）

> 总体策略：代码已实现但测试覆盖不足（`watcher.rs` 仅 1 个空测试 `test_should_trigger_reload`）。F8c 按 TDD 视角重新规划任务结构，已实现部分标注"✅ 已实现"，未实现部分（主要是测试与 bug 修复）标注"⬜ 待补全"。每条任务独立 TDD，便于定位回归。

### T1 watcher 基础结构 TDD

- [x] ✅ 已实现：在 `mystiproxy/src/config/watcher.rs` 新建文件，定义 `ConfigFileWatcher` 结构体（字段：`watcher` / `config_path` / `debounce_interval` / `reload_tx`）
- [x] ✅ 已实现：实现 `ConfigFileWatcher::new`，接收 `config_path` / `debounce_ms` / `reload_callback`
- [x] ✅ 已实现：内部创建 `mpsc::channel(1)`
- [x] ✅ 已实现：内部 `Arc::new(reload_callback)` 共享回调
- [x] ✅ 已实现：`tokio::spawn` 后台任务处理重载
- [ ] ⬜ 待补全：先写失败测试：
  - `test_watcher_creation_with_valid_path`：用 `tempfile::NamedTempFile` 创建临时配置文件，`ConfigFileWatcher::new` 返回 `Ok`
  - `test_watcher_creation_with_invalid_path`：传不存在的路径，`ConfigFileWatcher::new` 返回 `Err(ConfigValidationError::Watch(_))`
  - `test_watcher_creation_with_invalid_yaml_content`：临时文件含非法 YAML，`new` 仍返回 `Ok`（加载在后台任务，`new` 不预加载）
- [ ] ⬜ 待补全：确认 `notify` crate 已在 `mystiproxy/Cargo.toml`（若未在则先添加依赖）
- [ ] ⬜ 待补全：`cargo test -p mystiproxy config::watcher::tests test_watcher_creation` 验证全**绿**
- [ ] ⬜ 待补全：`cargo clippy -p mystiproxy --all-targets -- -D warnings` 无新告警

**验收标准**：3 个测试全绿；`new` 对合法路径返回 `Ok`，对不存在的路径返回 `Err`。

**已实现状态说明**：结构体与 `new` 方法已实现，但缺少测试覆盖。`callback_clone` 变量为死代码（创建后未使用，实际用 `callback_bg`），需在 T5 修复时一并清理。

### T2 事件过滤 TDD

- [x] ✅ 已实现：在 watcher 闭包中过滤 `EventKind::Modify(_)` / `EventKind::Create(_)` / `EventKind::Remove(_)`
- [x] ✅ 已实现：过滤后通过 `tx.blocking_send(())` 发信号
- [ ] ⬜ 待补全：先写失败测试：
  - `test_modify_triggers_reload`：写入初始配置 → 创建 watcher → `std::fs::write` 修改文件 → mock 回调在 `timeout(2s)` 内被调用至少 1 次
  - `test_create_event_triggers_reload`：删除文件后重新创建 → 回调被调用（覆盖编辑器原子 rename 场景）
  - `test_remove_event_triggers_reload`：删除文件 → 回调被调用（加载会失败，但回调的"被调用"指 watcher 收到事件，而非 callback 执行成功；此处需用 `reload_tx` 的旁路观测或日志断言）
  - `test_access_event_does_not_trigger`：用 `std::fs::read` 读取文件（触发 atime）→ 回调在 `timeout(500ms)` 内**不**被调用（排除 Access 事件）
- [ ] ⬜ 待补全：`cargo test -p mystiproxy config::watcher::tests test_modify_triggers_reload test_create_event test_remove_event test_access_event` 验证全**绿**

**验收标准**：4 个测试全绿；`Modify` / `Create` / `Remove` 触发，`Access` 不触发。

**测试策略说明**：文件系统事件在不同平台延迟差异大，测试用 `tokio::time::timeout(Duration::from_secs(5), ...)` 而非固定 sleep。`test_access_event_does_not_trigger` 需用较短 timeout（如 500ms）+ 多次读取确认稳定不触发。

### T3 debounce 策略 TDD

- [x] ✅ 已实现：`mpsc::channel(1)` 容量选择（背压式）
- [x] ✅ 已实现：后台任务 `rx.recv().await` 后 `sleep`
- [ ] ⬜ 待补全（bug 修复）：当前 `sleep(Duration::from_millis(500))` 写死，需改为 `sleep(Duration::from_millis(debounce_ms))`
- [ ] ⬜ 待补全：先写失败测试：
  - `test_debounce_merges_rapid_writes`：`debounce_ms=200`，100ms 内连续 `std::fs::write` 3 次 → 回调在 `timeout(2s)` 内被调用次数 ≤ 2（debounce 合并；允许 2 次是因为第三次写入可能在 debounce 后触发）
  - `test_debounce_uses_configured_interval`：`debounce_ms=1000`，修改文件后测量回调触发时间 ≥ 1000ms（用 `Instant::now()` 记录修改时间，回调内记录触发时间，断言差值 ≥ 950ms 容忍误差）
  - `test_debounce_zero_ms_still_works`：`debounce_ms=0`，watcher 不崩溃，回调仍被调用（边缘用例）
- [ ] ⬜ 待补全：实现 bug 修复（`sleep(Duration::from_millis(debounce_ms))`）
- [ ] ⬜ 待补全：`cargo test -p mystiproxy config::watcher::tests test_debounce` 验证全**绿**

**验收标准**：3 个测试全绿；`debounce_ms` 参数真正驱动 sleep 时长；多次写入被合并。

**已实现状态说明**：debounce 机制已实现（`mpsc(1) + sleep`），但 `debounce_ms` 参数被忽略，写死 500ms。这是 F8c 必须修复的 bug。

### T4 回调机制 TDD

- [x] ✅ 已实现：回调签名 `F: Fn(MystiConfig) -> Result<(), ConfigValidationError> + Send + Sync + 'static`
- [x] ✅ 已实现：`Arc::new(reload_callback)` 共享
- [x] ✅ 已实现：后台任务中 `callback_bg(new_config)` 调用
- [x] ✅ 已实现：回调返回 `Err` 时 `error!` 记录不重试
- [ ] ⬜ 待补全：先写失败测试：
  - `test_callback_receives_new_config`：初始配置 1 个引擎 → 修改为 2 个引擎 → mock 回调收到的 `MystiConfig` 含 2 个引擎（用 `Arc<Mutex<Vec<MystiConfig>>>` 收集）
  - `test_callback_error_does_not_crash_watcher`：回调第一次返回 `Err`，第二次返回 `Ok` → 两次修改后回调都被调用（watcher 仍存活）
  - `test_callback_can_be_called_multiple_times`：连续 3 次修改（间隔 > debounce）→ 回调被调用 3 次
- [ ] ⬜ 待补全：`cargo test -p mystiproxy config::watcher::tests test_callback` 验证全**绿**

**验收标准**：3 个测试全绿；回调收到正确的 `MystiConfig`；回调错误不击穿 watcher。

### T5 `start_config_watcher` 便捷函数 TDD

- [x] ✅ 已实现：`start_config_watcher` 函数签名 `async fn(config_path, debounce_ms, Arc<ConfigurationManager>) -> Result<JoinHandle<()>, ConfigValidationError>`
- [x] ✅ 已实现：内部 `ConfigFileWatcher::new` + `manager.update_config` 回调包装
- [x] ✅ 已实现：回调内 `tokio::spawn` 异步执行 `update_config`
- [x] ✅ 已实现：返回 `JoinHandle`（`loop { sleep(60s) }` 占位保持存活）
- [ ] ⬜ 待补全（代码清理）：删除 `callback_clone` 死代码（实际用 `callback_bg`）
- [ ] ⬜ 待补全（代码清理）：`let mut watcher = ...` 改为 `let _watcher = ...` 避免 unused 告警
- [ ] ⬜ 待补全：先写失败测试：
  - `test_start_config_watcher_updates_manager`：创建 `ConfigurationManager`（初始配置 1 引擎）→ `start_config_watcher` → 修改文件为 2 引擎 → `timeout(3s)` 内 `manager.get_current().await.mysti.engine.len() == 2`
  - `test_start_config_watcher_returns_handle`：`start_config_watcher` 返回 `Ok(JoinHandle)`
  - `test_start_config_watcher_invalid_path_returns_err`：不存在的路径 → 返回 `Err(ConfigValidationError::Watch(_))`
  - `test_start_config_watcher_invalid_yaml_does_not_update_manager`：写入非法 YAML → `timeout(1s)` 后 `manager.get_current()` 仍为初始配置（加载失败不更新）
- [ ] ⬜ 待补全：`cargo test -p mystiproxy config::watcher::tests test_start_config_watcher` 验证全**绿**

**验收标准**：4 个测试全绿；`manager.update_config` 被正确调用；加载失败不更新 manager。

**已实现状态说明**：`start_config_watcher` 已实现，但有死代码（`callback_clone`）和潜在 unused 告警（`mut watcher`）。F8c 需清理。

### T6 集成测试

- [ ] ⬜ 待补全：先写失败测试：
  - `test_watcher_full_lifecycle`：创建临时配置 → 启动 watcher → 修改文件 → 回调被调用 → 修改为非法 YAML → 回调不被调用（加载失败）→ 修改回合法 → 回调被调用 → watcher 全程存活
  - `test_watcher_with_manager_subscribe`：`start_config_watcher` + `manager.subscribe()` → 修改文件 → `rx.recv()` 收到 `ConfigChangeEvent`，`validation_success == true`
  - `test_watcher_invalid_config_no_event`：修改为非法配置 → `subscribe()` 不收到事件（`update_config` 失败时不广播；或收到 `validation_success == false` 的事件，需确认 `manager.rs` 行为）
  - `test_watcher_concurrent_modifications`：并发 `tokio::spawn` 多个 `std::fs::write` → watcher 不崩溃，最终 `manager.get_current()` 反映最后一次写入
- [ ] ⬜ 待补全：提供 `create_temp_config()` / `write_temp_config(content)` 测试辅助函数
- [ ] ⬜ 待补全：`cargo test -p mystiproxy config::watcher::tests test_watcher_full_lifecycle test_watcher_with_manager test_watcher_concurrent` 验证全**绿**

**验收标准**：4 个集成测试全绿；watcher 全生命周期不崩溃；`manager.subscribe()` 事件流正确。

**测试策略说明**：`test_watcher_concurrent_modifications` 在 CI 上可能 flaky，建议加 `#[ignore]` 标记供手动执行。

### T7 验证闭环

- [ ] ⬜ 待补全：`cargo test -p mystiproxy config::watcher::tests` 全绿（基础 + 事件过滤 + debounce + 回调 + start_config_watcher + 集成，约 20+ 测试）
- [ ] ⬜ 待补全：`cargo test --workspace` 全绿（现有测试无回归）
- [ ] ⬜ 待补全：`cargo fmt --all -- --check` 通过
- [ ] ⬜ 待补全：`cargo clippy --workspace --all-targets -- -D warnings` 无新告警（修复 `callback_clone` 死代码 + `mut watcher` unused）
- [ ] ⬜ 待补全：`cargo llvm-cov -p mystiproxy config::watcher`（如可用）新增行覆盖 ≥ 60%（文件系统测试天然覆盖率低于纯函数）
- [ ] ⬜ 待补全：手动验证：
  1. 启动带 watcher 的 mystiproxy
  2. 修改配置文件
  3. 观察日志 `INFO Configuration loaded successfully`
  4. 修改为非法 YAML，观察日志 `ERROR Failed to reload configuration: ...`
  5. 修改回合法，观察日志恢复 `INFO`
  6. 全程不重启进程

**验收标准**：全量测试 + clippy + fmt 闭环；覆盖率达标；手动验证输出符合预期。

### T8 推送 CI

- [ ] ⬜ 待补全：提交（`feat(mystiproxy/config): add F8c config hot-reload watcher with tests`）
- [ ] ⬜ 待补全：push GitHub，盯 `.github/workflows/rust.yml` Actions 至全绿
- [ ] ⬜ 待补全：更新 `ROADMAP.md` 标注 F8c（配置验证-热重载 Watcher）已闭环
- [ ] ⬜ 待补全：更新 `docs/FEATURE_COVERAGE.md` 增加热重载一节
- [ ] ⬜ 待补全：在 F8d 设计文档中引用 F8c 的 `start_config_watcher` 作为编排基础

**验收标准**：CI 全绿；ROADMAP / FEATURE_COVERAGE 更新；F8d 可基于 F8c 的稳定 API 启动设计。

## 验收标准汇总

| 任务 | 验收标准 | 状态 |
| :--- | :--- | :--- |
| T1 watcher 基础结构 | 3 个测试全绿；`new` 对合法/非法路径行为正确 | ✅ 代码已实现，⬜ 测试待补 |
| T2 事件过滤 | 4 个测试全绿；`Modify`/`Create`/`Remove` 触发，`Access` 不触发 | ✅ 代码已实现，⬜ 测试待补 |
| T3 debounce 策略 | 3 个测试全绿；`debounce_ms` 真正生效；多次写入合并 | ⚠️ debounce bug 待修，⬜ 测试待补 |
| T4 回调机制 | 3 个测试全绿；回调收到正确 config；错误不击穿 | ✅ 代码已实现，⬜ 测试待补 |
| T5 start_config_watcher | 4 个测试全绿；manager 正确更新；死代码清理 | ✅ 代码已实现，⚠️ 死代码待清，⬜ 测试待补 |
| T6 集成测试 | 4 个测试全绿；全生命周期不崩溃 | ⬜ 待补 |
| T7 验证闭环 | 全量 test + clippy + fmt 通过；覆盖率 ≥ 60% | ⬜ 待补 |
| T8 推送 CI | CI 全绿；ROADMAP / FEATURE_COVERAGE 更新 | ⬜ 待补 |

## 信心评估

| 任务 | 信心 | 依据 |
| :--- | :--- | :--- |
| T1 watcher 基础结构 | 95% | 代码已实现，测试只需覆盖创建路径 |
| T2 事件过滤 | 85% | 文件系统事件在 CI 上可能 flaky，需 generous timeout |
| T3 debounce 策略 | 90% | bug 修复简单（一行改动），测试需测量时间延迟 |
| T4 回调机制 | 92% | mock 回调 + `Arc<Mutex<Vec>>` 收集是标准模式 |
| T5 start_config_watcher | 88% | 集成 `ConfigurationManager` 增加复杂度，死代码清理需谨慎 |
| T6 集成测试 | 80% | 全生命周期测试涉及多次文件操作，flaky 风险最高 |
| T7 验证闭环 | 90% | 标准 cargo 工具链；覆盖率工具可用性取决于环境 |
| T8 推送 CI | 85% | 取决于 CI 环境状态，非代码风险 |
| **整体** | **>87%** | **代码已实现，主要风险在测试 flaky 与 bug 修复** |

## 实现完成情况（2026-08-15）

### 已实现部分

- `mystiproxy/src/config/watcher.rs` 新建，约 135 行（含 1 个空测试）
- `ConfigFileWatcher` 结构体定义完整（4 个字段）
- `ConfigFileWatcher::new` 方法实现完整（创建 watcher / 注册路径 / 启动后台任务）
- `start_config_watcher` 便捷函数实现完整（封装 manager 集成）
- notify 集成：`RecommendedWatcher` + `NonRecursive` + 事件过滤（`Modify`/`Create`/`Remove`）
- debounce 机制：`mpsc(1) + sleep`（但 `debounce_ms` 参数未生效，写死 500ms）
- 错误隔离：三阶段 `match + error!`
- 回调机制：`Arc<dyn Fn>` + 后台任务调用

### 测试覆盖不足（需补全）

**当前状态**：`watcher.rs` 的 `#[cfg(test)] mod tests` 仅含 1 个空测试：

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_should_trigger_reload() {
        // 这个测试需要实际的文件系统事件，这里仅验证编译通过
    }
}
```

**需补全的测试矩阵**（共约 20+ 测试）：

| 类别 | 测试名 | 任务归属 |
| :--- | :--- | :--- |
| 创建 | `test_watcher_creation_with_valid_path` | T1 |
| 创建 | `test_watcher_creation_with_invalid_path` | T1 |
| 创建 | `test_watcher_creation_with_invalid_yaml_content` | T1 |
| 事件 | `test_modify_triggers_reload` | T2 |
| 事件 | `test_create_event_triggers_reload` | T2 |
| 事件 | `test_remove_event_triggers_reload` | T2 |
| 事件 | `test_access_event_does_not_trigger` | T2 |
| debounce | `test_debounce_merges_rapid_writes` | T3 |
| debounce | `test_debounce_uses_configured_interval` | T3 |
| debounce | `test_debounce_zero_ms_still_works` | T3 |
| 回调 | `test_callback_receives_new_config` | T4 |
| 回调 | `test_callback_error_does_not_crash_watcher` | T4 |
| 回调 | `test_callback_can_be_called_multiple_times` | T4 |
| 集成 | `test_start_config_watcher_updates_manager` | T5 |
| 集成 | `test_start_config_watcher_returns_handle` | T5 |
| 集成 | `test_start_config_watcher_invalid_path_returns_err` | T5 |
| 集成 | `test_start_config_watcher_invalid_yaml_does_not_update_manager` | T5 |
| 生命周期 | `test_watcher_full_lifecycle` | T6 |
| 生命周期 | `test_watcher_with_manager_subscribe` | T6 |
| 生命周期 | `test_watcher_invalid_config_no_event` | T6 |
| 生命周期 | `test_watcher_concurrent_modifications` | T6（`#[ignore]` 标记） |

### 待修复的 bug 与代码清理

| 项 | 位置 | 修复方案 |
| :--- | :--- | :--- |
| debounce 写死 500ms | `watcher.rs` 第 66 行 | 改为 `sleep(Duration::from_millis(debounce_ms))` |
| `callback_clone` 死代码 | `watcher.rs` 第 37 行 | 删除，仅保留 `callback`，后台任务用 `callback` 的 clone |
| `let mut watcher` 潜在 unused 告警 | `watcher.rs` `start_config_watcher` 第 111 行 | 改为 `let _watcher = ...` 或在闭包内显式 hold |

### 全量验证状态

- `cargo test --workspace`：✅ 通过（但 watcher 模块仅 1 个空测试）
- `cargo fmt --check`：✅ 通过
- `cargo clippy --all-targets -- -D warnings`：⚠️ 可能存在 `callback_clone` / `mut watcher` 告警（需确认）
- `cargo llvm-cov -p mystiproxy config::watcher`：❌ 覆盖率接近 0%（仅空测试）

## 风险与缓解

| 风险 | 缓解 |
| :--- | :--- |
| 文件系统事件在 CI 上 flaky | 测试用 `tokio::time::timeout(Duration::from_secs(5), ...)` generous timeout；极 flaky 的用 `#[ignore]` 标记 |
| macOS FSEvents 事件延迟高于 Linux inotify | timeout 设为 5s 覆盖最慢平台；debounce_ms 测试用 1000ms 而非 100ms 减少误差 |
| `notify` crate 版本升级破坏 API | 锁定 `notify` 版本（`Cargo.toml` 已锁定）；升级前跑全量测试 |
| debounce bug 修复后行为变化 | 修复前后的测试用例分别覆盖 500ms 默认与自定义 `debounce_ms`，确保兼容 |
| `start_config_watcher` 的 `loop { sleep }` 占位被误改为退出 | 文档化"占位"意图；F8d 引入 shutdown 信号时再替换 |
| 并发修改测试 flaky | `#[ignore]` 标记，手动执行；或用文件锁串行化写入 |
| `ConfigurationManager::update_config` 内部验证逻辑与 loader 重复 | 文档化此重复；F8d 可重构为单一验证入口 |
| 测试中临时文件被提前 drop 导致路径失效 | 用 `tempfile::NamedTempFile`（drop 时自动删除）+ 在 watcher 存活期间持有句柄 |
