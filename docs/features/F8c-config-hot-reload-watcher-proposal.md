# F8c 配置验证框架-热重载 Watcher — Proposal

## 背景与动机

### 现状：F8a/F8b 之后，缺一座"桥梁"

MystiProxy 配置验证框架按 4 阶段推进：

| 阶段 | 交付物 | 状态 |
| :--- | :--- | :--- |
| F8a | 验证模型与规则（`ConfigValidator` / `ValidationResult` / 8 条规则） | ✅ 已交付 |
| F8b | 增强加载器（`EnhancedConfigLoader` / `ConfigSource` / `ValidationLevel`） | ✅ 已交付 |
| **F8c** | **配置文件监听 + 触发热重载** | **本阶段** |
| F8d | 热重载编排 + 管理 UI（`ConfigurationManager` 事件流对外暴露） | 后续 |

F8a 把"语义错误"前移到加载后显式校验，F8b 把"加载"做成可组合多源的能力。但二者都还是**一次性**：配置加载完就定型，运维改 YAML 必须重启进程才能生效。

F8c 的核心动机：**让配置在运行时可被监控、可被重载，把"改了配置就要重启"的成本砍掉**。它处在 F8a/F8b（拿到 `MystiConfig`）与 F8d（编排 + UI）之间的桥梁位置，依赖 F8b 的 `EnhancedConfigLoader` 重新加载，依赖 `ConfigurationManager`（已在 `manager.rs` 中存在）落盘新配置并发广播事件。

### 当前实现的"半成品"状态

`mystiproxy/src/config/watcher.rs` 已实现 `ConfigFileWatcher` 与 `start_config_watcher`，但存在以下不足，需在 F8c 阶段一并补齐：

| 不足 | 影响 | F8c 处置 |
| :--- | :--- | :--- |
| `debounce_interval` 字段被存储但未实际使用，debounce 写死 `500ms` | 调用方传 `debounce_ms=2000` 不生效，行为不可预期 | 让 debounce 实际使用传入参数 |
| `callback_clone` 创建后未使用（实际用 `callback_bg`） | 死代码，clippy 会告警 | 删除冗余 clone |
| `watcher.rs` 仅 1 个空测试 `test_should_trigger_reload` | 热重载是高风险路径，零覆盖等于裸奔 | 补全 watcher 事件过滤 / debounce / 回调 / 错误路径单测 + 集成测试 |
| 后台任务无 shutdown 机制 | 进程退出时 watcher 句柄被 drop，但 `tokio::spawn` 的接收循环无优雅退出 | 文档化生命周期，标注限制 |
| 事件过滤粒度粗（任何 `Modify` / `Create` / `Remove` 都触发） | 编辑器临时文件、`mv` 等会触发多次重载 | debounce 已部分缓解；明确过滤策略 |
| `start_config_watcher` 中的 `loop { sleep(60s) }` 仅占位 | 资源浪费但不致命 | 文档化"保持 watcher 存活"的设计意图 |

### 框架定位

F8c 是配置验证框架的**第三阶段**，只做"监听 + 触发"，不做"编排 + UI"。具体边界：

- ✅ 监听配置文件变化（基于 `notify` crate）
- ✅ debounce 事件，避免编辑器抖动导致的多次重载
- ✅ 通过回调把"新 `MystiConfig`"交给调用方（默认是 `ConfigurationManager::update_config`）
- ✅ 错误隔离：加载失败 / 校验失败 / 回调失败都不会击穿 watcher 主循环
- ❌ 不做配置回滚 UI（`ConfigurationManager::rollback_to_previous` 已存在，UI 留给 F8d）
- ❌ 不做配置 diff 展示（留给 F8d）
- ❌ 不做多文件监听（F8c 仅监听单个 `config_path`）

## 需求深度理解

### 为什么必须做（不是可选锦上添花）

1. **代理是长驻进程，重启代价高**
   - 重启意味着断开所有现有连接（TCP/HTTP/WebSocket），生产环境不可接受
   - 灰度发布、限流策略、IP 黑白名单的调整都需要"热"生效
   - F8a/F8b 把校验做对了，但若仍需重启才能用上新配置，校验的价值大打折扣

2. **"静默失败"比 panic 更危险**
   - 运维改完 YAML 重启进程，靠"没崩就是对了"判断；不重启则配置完全不变，运维误以为已生效
   - 没有 watcher，就没有"配置是否真的被加载"的反馈闭环
   - F8c 通过 `info!` / `error!` 日志 + `ConfigurationManager` 的 `ConfigChangeEvent` 广播，让"重载是否成功"可观测

3. **编辑器抖动是真实场景**
   - Vim / VSCode 保存时常产生 `4913` 临时文件、`config.yaml~` 备份、原子 rename 等多个事件
   - 不 debounce 会导致 1 次保存触发 3-5 次重载，每次都重新解析 + 校验 + 应用，浪费 CPU
   - debounce 是"非功能性需求"中的硬需求

### 深层需求

- **watcher 必须与运行时解耦**：`ConfigFileWatcher::new` 接收一个 `Fn(MystiConfig) -> Result<()>` 回调，watcher 不直接知道 `ConfigurationManager` 的存在。这保证 watcher 可独立单测（用 mock 回调验证"是否被调用、调用时的 config 是什么"）。
- **debounce 必须可配**：不同编辑器 / 文件系统（NFS / Docker volume）的事件抖动差异大，硬编码 `500ms` 不够灵活。`debounce_ms` 参数必须真正生效。
- **错误必须隔离**：加载失败（YAML 语法错）、校验失败（F8a 规则不过）、回调失败（`update_config` 内部错误）三类错误都不能让 watcher 退出。每次错误都要 `error!` 记录，下一轮事件继续响应。
- **事件过滤必须明确**：只对 `Modify` / `Create` / `Remove` 三类事件触发，忽略 `Access`（atime 读取）等噪声事件。

## 目标与非目标

### 目标（F8c 仅做这些）

1. **`ConfigFileWatcher` 结构体**：持有 `notify::RecommendedWatcher`、`config_path`、`debounce_interval`、`reload_tx`（mpsc 发送端）
2. **`ConfigFileWatcher::new`**：接收 `config_path` / `debounce_ms` / `reload_callback`，创建 watcher、注册路径、启动后台任务
3. **`start_config_watcher` 便捷函数**：封装 `ConfigFileWatcher` + `ConfigurationManager` 的常见组合，返回 `JoinHandle` 供调用方管理生命周期
4. **notify 集成**：使用 `notify::RecommendedWatcher`（跨平台后端），`RecursiveMode::NonRecursive`，事件过滤 `Modify` / `Create` / `Remove`
5. **debounce 策略**：通过 `tokio::sync::mpsc` + `tokio::time::sleep` 实现"收到事件后等 N ms，期间新事件被合并"
6. **回调机制**：通过 `Arc<dyn Fn(MystiConfig) -> Result<()>>` 在后台任务中调用，错误用 `error!` 记录不传播
7. **错误隔离**：加载 / 校验 / 回调三阶段错误分别 `error!` 记录，不影响 watcher 存活
8. **补全测试覆盖**：从 1 个空测试补到覆盖事件过滤、debounce、回调、错误路径、`start_config_watcher` 集成等场景

### 非目标（明确推迟到 F8d 或后续）

| 推迟项 | 归属阶段 | 原因 |
| :--- | :--- | :--- |
| 配置回滚 UI / 管理 API | F8d | `ConfigurationManager::rollback_to_previous` 已存在，UI 是独立复杂度 |
| 配置 diff 可视化 | F8d | 需要前端 + diff 算法，F8c 只保证"新 config 到达回调" |
| 多文件 / 目录监听 | 后续 | F8c 仅监听单个 `config_path`，多文件需要 include / exclude 规则 |
| Watcher 优雅 shutdown | 后续 | 当前靠进程退出 + `JoinHandle` drop，显式 shutdown 信号留给 F8d |
| 跨平台行为差异测试 | 后续 | macOS（FSEvents）/ Linux（inotify）/ Windows（ReadDirectoryChangesW）事件语义不同，F8c 用 debounce 统一缓解 |
| 配置 schema 变更检测 | 后续 | 锦上添花，不影响运行时正确性 |
| Watcher 自身指标（重载次数、失败次数） | 后续 | 与 `metrics.rs` 集成是独立工作 |

## 利益相关方

| 角色 | 关注点 | F8c 如何满足 |
| :--- | :--- | :--- |
| 运维 / SRE | 改完配置能否不重启就生效 | watcher 自动监听 + debounce，保存即触发重载；日志可观测成功 / 失败 |
| 平台开发者 | 重载流程能否被程序化消费 | `ConfigurationManager::subscribe()` 已返回 `broadcast::Receiver<ConfigChangeEvent>`，F8c 通过 `update_config` 触发该事件 |
| F8d 实现者 | watcher 是否提供稳定回调契约 | `ConfigFileWatcher::new` 的回调签名 `Fn(MystiConfig) -> Result<()>` 稳定，F8d 可包装为事件流 |
| 安全审计 | 重载失败时是否会"半应用"配置 | `ConfigurationManager::update_config` 先验证再写入，失败时不修改 `current_config`，F8c 回调返回 `Err` 仅记录不重试 |
| 现有用户 | 升级后是否强制启用热重载 | `start_config_watcher` 是显式调用，不调用即无 watcher，零行为变化 |
| CI / 测试 | watcher 是否可被自动化测试 | F8c 通过临时文件 + mock 回调实现可单测，不依赖真实编辑器事件 |

## 信心评估

| 决策点 | 信心 | 依据 |
| :--- | :--- | :--- |
| `notify::RecommendedWatcher` 跨平台可用 | 95% | `notify` 是 Rust 生态事实标准，`RecommendedWatcher` 自动选择 FSEvents / inotify / ReadDirectoryChangesW |
| `mpsc + sleep` 的 debounce 模式 | 90% | 标准 tokio 模式，但当前实现 debounce 写死 500ms，需修复为使用 `debounce_ms` 参数 |
| 回调 `Arc<dyn Fn>` 在后台任务中的安全调用 | 92% | `Send + Sync + 'static` 约束已在签名中体现，`Arc` 共享无锁读取 |
| 错误隔离三阶段不击穿主循环 | 95% | `match` + `error!` 模式，每阶段独立捕获，不向上传播 |
| 补全测试覆盖的可行性 | 85% | 临时文件 + `tokio::time::timeout` 等待事件 + mock 回调可覆盖核心路径；但文件系统事件延迟在 CI 上可能 flaky |
| **整体** | **>88%** | **代码已实现，主要工作是修 debounce bug + 补测试，技术风险低** |
