# F8b 验证加载器接线 main.rs — Proposal

## 需求深度理解

### 背景与问题

F8a 交付了 `EnhancedConfigLoader` + `validate_engine_config`（8 类规则），`MystiConfig::load_validated()` 也已封装。但审计发现：

- **main.rs:287 仍调用 `from_yaml_file`**（纯 serde 解析，零验证）
- `load_validated` / `EnhancedConfigLoader` / `ConfigurationManager` 全部无生产调用方——又一座孤岛
- F8a 提交说明宣称"validation framework"，但验证从未在真实启动路径执行过

### 深层需求

F8a proposal 的原始动机："把语义错误从运行时 panic/静默失败前移到加载阶段"。不接线 = 动机未达成：
- 非法 CIDR 仍要到首条连接才暴露
- 坏正则仍会在 `Regex::new` panic
- 运维改坏 YAML 重启，"没崩就是对了"

### 设计决策

1. **main.rs 的 `load_config` 改走 `load_validated`**，`Strict` 级别：Error 即拒启动（基础设施立场）
2. **CLI 逃生门**：新增 `--validation-level loose|warn|strict`（默认 strict），兼容存量脏配置灰度迁移
3. **错误输出走 F8a 的 user_interface**（友好提示 + 修复建议），而非裸 Debug 串

### 成功标准（真实运行实测）

1. 坏 CIDR 配置启动 → 立即退出 + 明确指出条目（不再进入运行态）
2. 坏正则 location → 启动拒绝
3. 合法配置 → 行为与现状完全一致（回归）
4. `--validation-level loose` 时坏配置放行（逃生门生效）
5. 覆盖率新路径 ≥70%，Actions 全绿

### 范围

做：main.rs 接线 + CLI 参数 + 错误美化输出 + e2e。
不做：热重载（F8d）、管理 API 暴露验证结果（F8d）、配置合并策略。

## 信心评估
load_validated 现成（94%）；clap 参数模式同 args.rs 既有（96%）；user_interface 格式化已实现（92%）。全部达标。
