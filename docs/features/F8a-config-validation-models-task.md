# F8a 配置验证框架-模型与验证规则 — Task 规划

## 任务分解（TDD 顺序）

> 总体策略：先写失败测试（红），再实现模型与规则使测试转绿，最后做集成与闭环验证。每条规则独立 TDD，便于定位回归。

### T1 模型定义 TDD

- [ ] 在 `mystiproxy/src/config/validation.rs` 新建文件，先写 `#[cfg(test)] mod tests` 内的**模型层失败测试**：
  - `test_validation_level_default_is_loose`：`assert_eq!(ValidationLevel::default(), ValidationLevel::Loose)`
  - `test_is_valid_strict_with_error_is_false`：`Strict` 级别 push 一条 `Error` → `is_valid() == false`
  - `test_is_valid_warn_with_error_is_true`：`Warn` 级别 push 一条 `Error` → `is_valid() == true`
  - `test_is_valid_loose_with_error_is_true`：`Loose` 级别 push 一条 `Error` → `is_valid() == true`
  - `test_loose_drops_warnings`：`Loose` 级别 push 一条 `Warning` → `issues().is_empty()`
  - `test_warn_keeps_warnings`：`Warn` 级别 push 一条 `Warning` → `issues().len() == 1`
  - `test_strict_keeps_warnings`：`Strict` 级别 push 一条 `Warning` → `issues().len() == 1`
  - `test_merge_combines_issues`：两个 `Strict` 结果各 push 一条 → merge 后 `len() == 2`
  - `test_merge_level_uses_self`：`Loose` 结果 merge 一个 `Strict` 结果（含 Warning）→ Warning 被丢弃
  - `test_from_issues_constructs_result`：`ValidationResult::from_issues(level, vec)` 后 `issues().len()` 匹配
  - `test_errors_filter_only_errors`：混合 push Error/Warning → `errors().count()` 只算 Error
  - `test_warnings_filter_only_warnings`：混合 push → `warnings().count()` 只算 Warning
- [ ] 在 `mystiproxy/src/config/mod.rs` 顶部新增 `pub mod validation;`
- [ ] `cargo test -p mystiproxy config::validation::tests` 验证测试**红**（编译失败：类型未定义）
- [ ] 实现 `ValidationLevel`（含 `#[derive(Default)]` 让 `Loose` 为默认）、`ValidationSeverity`、`ValidationIssue`（含私有 `error()` / `warning()` 构造器）、`ValidationResult`（含 `new` / `is_valid` / `issues` / `errors` / `warnings` / `push` / `merge` / `from_issues` / `len` / `is_empty`）、`ConfigValidator`（含 `new` / `with_level`，`validate_*` 先返回空结果）
- [ ] `cargo test -p mystiproxy config::validation::tests` 验证模型层测试全**绿**
- [ ] `cargo clippy -p mystiproxy --all-targets -- -D warnings` 无新告警

**验收标准**：模型层 12 个测试全绿；`ConfigValidator::new()` 默认 `Loose`；`ValidationResult` 在三种级别下 `is_valid()` 行为正确。

### T2 规则 1 — listen 地址

- [ ] 先写失败测试：
  - `test_listen_valid_tcp_scheme`：`listen: "tcp://0.0.0.0:3128"` → 无 `listen_scheme` 错误
  - `test_listen_valid_unix_scheme`：`listen: "unix:///var/run/docker.sock"` → 无 `listen_scheme` 错误
  - `test_listen_invalid_scheme`：`listen: "0.0.0.0:3128"` → 有 `listen_scheme` 错误
  - `test_listen_empty`：`listen: ""` → 有 `listen_scheme` 错误
- [ ] `cargo test -p mystiproxy config::validation::tests test_listen` 验证**红**（规则未实现，断言失败）
- [ ] 实现私有函数 `validate_listen(result, engine, cfg)`：检查 `cfg.listen` 非空且以 `tcp://` 或 `unix://` 开头，否则 push `ValidationIssue::error(engine, "listen", "listen_scheme", ...)`
- [ ] 实现私有辅助 `has_valid_scheme(addr: &str) -> bool`
- [ ] 在 `ConfigValidator::validate_engine` 中调用 `validate_listen`
- [ ] `cargo test -p mystiproxy config::validation::tests test_listen` 验证全**绿**

**验收标准**：4 个测试全绿；错误消息含当前 listen 值。

### T3 规则 2 — target 地址

- [ ] 先写失败测试：
  - `test_target_valid_tcp_scheme`：`target: "tcp://127.0.0.1:8080"` → 无 `target_scheme` 错误
  - `test_target_valid_unix_scheme`：`target: "unix:///tmp/upstream.sock"` → 无 `target_scheme` 错误
  - `test_target_invalid_scheme`：`target: "127.0.0.1:8080"` → 有 `target_scheme` 错误
  - `test_target_empty`：`target: ""` → 有 `target_scheme` 错误
- [ ] `cargo test -p mystiproxy config::validation::tests test_target` 验证**红**
- [ ] 实现私有函数 `validate_target(result, engine, cfg)`：复用 `has_valid_scheme`，检查 `cfg.target`
- [ ] 在 `ConfigValidator::validate_engine` 中调用 `validate_target`
- [ ] `cargo test -p mystiproxy config::validation::tests test_target` 验证全**绿**

**验收标准**：4 个测试全绿；与 `validate_listen` 共用 `has_valid_scheme` 辅助。

### T4 规则 3 — CIDR

- [ ] 先写失败测试：
  - `test_cidr_valid_ipv4`：`allow: ["192.168.1.0/24"]` → 无 `cidr_valid` 错误
  - `test_cidr_valid_ipv6`：`allow: ["2001:db8::/32"]` → 无 `cidr_valid` 错误
  - `test_cidr_valid_no_prefix`：`allow: ["10.0.0.1"]` → 无 `cidr_valid` 错误（按 IP 解析）
  - `test_cidr_invalid_prefix_ipv4`：`allow: ["192.168.1.0/33"]` → 有 `cidr_valid` 错误
  - `test_cidr_invalid_prefix_ipv6`：`allow: ["2001:db8::/129"]` → 有 `cidr_valid` 错误
  - `test_cidr_invalid_ip`：`deny: ["not-an-ip/24"]` → 有 `cidr_valid` 错误
  - `test_cidr_deny_also_validated`：`deny: ["10.0.0.0/8"]` → 无 `cidr_valid` 错误
  - `test_cidr_none_skipped`：`allow: None, deny: None` → 无 `cidr_valid` 错误
- [ ] `cargo test -p mystiproxy config::validation::tests test_cidr` 验证**红**
- [ ] 实现私有辅助 `is_valid_cidr(s: &str) -> Result<(), String>`：
  - `split('/')` 得到 IP 与前缀；
  - 无 `/` 时按 `IpAddr::from_str` 校验 IP 即可；
  - 有 `/` 时 `IpAddr::from_str` 校验 IP，`u8::from_str` 校验前缀，前缀范围按 IPv4 ≤ 32 / IPv6 ≤ 128；
  - 失败返回描述性 `String` 错误
- [ ] 实现私有函数 `validate_cidr(result, engine, cfg)`：遍历 `cfg.allow` 与 `cfg.deny`，每条调 `is_valid_cidr`，失败 push `ValidationIssue::error(engine, "allow[i]" / "deny[i]", "cidr_valid", ...)`
- [ ] 在 `ConfigValidator::validate_engine` 中调用 `validate_cidr`
- [ ] `cargo test -p mystiproxy config::validation::tests test_cidr` 验证全**绿**

**验收标准**：8 个测试全绿；不引入 `cidr` crate；错误消息含索引、原值、解析错误。

### T5 规则 4 — TLS 路径

- [ ] 先写失败测试：
  - `test_tls_paths_valid`：`tls: Some(TlsConfig { cert_path: "/etc/cert.pem", key_path: "/etc/key.pem", ... })` → 无 `tls_paths_nonempty` 错误
  - `test_tls_cert_path_empty`：`cert_path: ""` → 有 `tls_paths_nonempty` 错误，`field` 含 `cert_path`
  - `test_tls_key_path_empty`：`key_path: ""` → 有 `tls_paths_nonempty` 错误，`field` 含 `key_path`
  - `test_tls_none_skipped`：`tls: None` → 无 `tls_paths_nonempty` 错误
- [ ] `cargo test -p mystiproxy config::validation::tests test_tls` 验证**红**
- [ ] 实现私有函数 `validate_tls(result, engine, cfg)`：若 `cfg.tls` 为 `Some`，检查 `cert_path` / `key_path` 非空，空则 push `ValidationIssue::error(engine, "tls.cert_path" / "tls.key_path", "tls_paths_nonempty", ...)`
- [ ] 在 `ConfigValidator::validate_engine` 中调用 `validate_tls`
- [ ] `cargo test -p mystiproxy config::validation::tests test_tls` 验证全**绿**

**验收标准**：4 个测试全绿；不校验文件存在性（属 I/O，留给 F8b）。

### T6 规则 5 — auth

- [ ] 先写失败测试：
  - `test_auth_type_valid_when_enabled`：`auth: Some(AuthConfig { auth_type: "header", enabled: true, ... })` → 无 `auth_type_nonempty` 错误
  - `test_auth_type_empty_when_enabled`：`auth_type: "", enabled: true` → 有 `auth_type_nonempty` 错误
  - `test_auth_type_empty_when_disabled`：`auth_type: "", enabled: false` → 无 `auth_type_nonempty` 错误
  - `test_auth_none_skipped`：`auth: None` → 无 `auth_type_nonempty` 错误
- [ ] `cargo test -p mystiproxy config::validation::tests test_auth` 验证**红**
- [ ] 实现私有函数 `validate_auth(result, engine, cfg)`：若 `cfg.auth` 为 `Some` 且 `enabled == true`，检查 `auth_type` 非空
- [ ] 在 `ConfigValidator::validate_engine` 中调用 `validate_auth`
- [ ] `cargo test -p mystiproxy config::validation::tests test_auth` 验证全**绿**

**验收标准**：4 个测试全绿；`enabled: false` 时跳过校验。

### T7 规则 6 — upstream

- [ ] 先写失败测试：
  - `test_upstream_valid_http`：`upstream: Some("http://proxy:8080")` → 无 `upstream_url_valid` 错误
  - `test_upstream_valid_https`：`upstream: Some("https://proxy:8443")` → 无 `upstream_url_valid` 错误
  - `test_upstream_invalid_scheme`：`upstream: Some("tcp://proxy:8080")` → 有 `upstream_url_valid` 错误
  - `test_upstream_missing_scheme`：`upstream: Some("proxy:8080")` → 有 `upstream_url_valid` 错误
  - `test_upstream_none_skipped`：`upstream: None` → 无 `upstream_url_valid` 错误
- [ ] 确认 `url` crate 已在 `mystiproxy/Cargo.toml`（若未在则先添加依赖；F8a 设计声明已存在，需核对）
- [ ] `cargo test -p mystiproxy config::validation::tests test_upstream` 验证**红**
- [ ] 实现私有函数 `validate_upstream(result, engine, cfg)`：若 `cfg.upstream` 为 `Some`，`url::Url::parse(upstream)` 成功后检查 `url.scheme()` 是否为 `http` / `https`；解析失败或 scheme 错误均 push `ValidationIssue::error(engine, "upstream", "upstream_url_valid", ...)`
- [ ] 在 `ConfigValidator::validate_engine` 中调用 `validate_upstream`
- [ ] `cargo test -p mystiproxy config::validation::tests test_upstream` 验证全**绿**

**验收标准**：5 个测试全绿；显式校验 scheme，避免 `tcp://` 被误判为合法。

### T8 规则 7 — regex location

- [ ] 先写失败测试：
  - `test_regex_location_valid`：`locations: [{ location: "^/api/.*$", mode: Regex }]` → 无 `regex_pattern_valid` 错误
  - `test_prefix_regex_location_valid`：`locations: [{ location: "/api/.*", mode: PrefixRegex }]` → 无 `regex_pattern_valid` 错误
  - `test_regex_location_invalid`：`locations: [{ location: "[invalid", mode: Regex }]` → 有 `regex_pattern_valid` 错误
  - `test_full_mode_not_validated`：`locations: [{ location: "[invalid", mode: Full }]` → 无 `regex_pattern_valid` 错误（非正则模式不校验）
  - `test_prefix_mode_not_validated`：`locations: [{ location: "[invalid", mode: Prefix }]` → 无 `regex_pattern_valid` 错误
  - `test_regex_location_index_in_field`：`locations: [{ location: "^/ok$", mode: Regex }, { location: "[bad", mode: Regex }]` → 错误 `field` 含 `locations[1].location`
- [ ] `cargo test -p mystiproxy config::validation::tests test_regex_location` 验证**红**
- [ ] 实现私有函数 `validate_regex_locations(result, engine, cfg)`：遍历 `cfg.locations`，对 `mode` 为 `Regex` / `PrefixRegex` 的项调 `regex::Regex::new(location)`，失败 push `ValidationIssue::error(engine, "locations[i].location", "regex_pattern_valid", ...)`
- [ ] 在 `ConfigValidator::validate_engine` 中调用 `validate_regex_locations`
- [ ] `cargo test -p mystiproxy config::validation::tests test_regex_location` 验证全**绿**

**验收标准**：6 个测试全绿；`Full` / `Prefix` 模式不校验；错误 `field` 含索引。

### T9 规则 8 — timeout

- [ ] 先写失败测试：
  - `test_request_timeout_positive_ok`：`request_timeout: Some(Duration::from_secs(10))` → 无 `timeout_positive` 错误
  - `test_request_timeout_zero_is_error`：`request_timeout: Some(Duration::ZERO)` → 有 `timeout_positive` 错误
  - `test_connection_timeout_positive_ok`：`connection_timeout: Some(Duration::from_secs(5))` → 无 `timeout_positive` 错误
  - `test_connection_timeout_zero_is_error`：`connection_timeout: Some(Duration::ZERO)` → 有 `timeout_positive` 错误
  - `test_timeouts_none_skipped`：`request_timeout: None, connection_timeout: None` → 无 `timeout_positive` 错误
- [ ] `cargo test -p mystiproxy config::validation::tests test_timeout` 验证**红**
- [ ] 实现私有函数 `validate_timeouts(result, engine, cfg)`：`request_timeout` / `connection_timeout` 为 `Some` 时检查 `> Duration::ZERO`，否则 push `ValidationIssue::error(engine, "request_timeout" / "connection_timeout", "timeout_positive", ...)`
- [ ] 在 `ConfigValidator::validate_engine` 中调用 `validate_timeouts`
- [ ] `cargo test -p mystiproxy config::validation::tests test_timeout` 验证全**绿**

**验收标准**：5 个测试全绿；`None` 跳过；`Duration::ZERO` 判为非法。

### T10 集成测试

- [ ] 先写失败测试：
  - `test_validate_engine_collects_all_errors`：构造一个 `EngineConfig` 同时违反 listen / target / cidr / timeout 4 条规则，`validate_engine` 返回的结果应含 4 条 `Error`，`is_valid()` 在 `Strict` 下为 `false`
  - `test_validate_config_merges_engines`：构造 `MystiConfig` 含 2 个引擎（一好一坏），`validate_config` 结果应仅含坏引擎的问题，`errors()` 中每条 `issue.engine` 匹配坏引擎名
  - `test_validate_config_all_good_engines`：全部引擎合法 → `is_valid()` 为 `true`，`issues().is_empty()`
  - `test_validate_config_loose_is_valid_even_with_errors`：`Loose` 级别下含错误的配置 → `is_valid() == true`，但 `errors().count() > 0`
  - `test_validate_config_warn_keeps_warnings`：`Warn` 级别下含 Warning 的配置 → `warnings().count() > 0`，`is_valid() == true`
- [ ] `cargo test -p mystiproxy config::validation::tests test_validate` 验证**红**（若 T2–T9 已实现，集成测试应直接转绿；此处仅作回归保护）
- [ ] 提供 `base_engine()` / `engine_with_listen(...)` 等测试辅助构造器
- [ ] `cargo test -p mystiproxy config::validation::tests test_validate` 验证全**绿**

**验收标准**：5 个集成测试全绿；`validate_config` 正确合并多引擎结果；级别行为在端到端路径上保持一致。

### T11 验证闭环

- [ ] `cargo test -p mystiproxy config::validation::tests` 全绿（模型 + 8 规则 + 集成，约 40+ 测试）
- [ ] `cargo test --workspace` 全绿（现有测试无回归，F8a 只新增不改旧）
- [ ] `cargo fmt --all -- --check` 通过
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` 无新告警
- [ ] `cargo llvm-cov -p mystiproxy config::validation`（如可用）新增行覆盖 ≥ 70%
- [ ] 手动构造一份含 3 处错误的 YAML，`from_yaml` 后调 `ConfigValidator::with_level(Strict).validate_config(&cfg)`，确认 `errors()` 输出 3 条且 `is_valid() == false`

**验收标准**：全量测试 + clippy + fmt 闭环；覆盖率达标；手动验证输出符合预期。

### T12 推送 CI

- [ ] 提交（`feat(mystiproxy/config): add F8a validation models and rules`）
- [ ] push GitHub，盯 `.github/workflows/rust.yml` Actions 至全绿
- [ ] 更新 `ROADMAP.md` 标注 F8a（配置验证-模型与规则）已闭环
- [ ] 更新 `docs/FEATURE_COVERAGE.md` 增加配置校验一节
- [ ] 在 F8b 设计文档中引用 F8a 的公共 API 作为加载器依赖

**验收标准**：CI 全绿；ROADMAP / FEATURE_COVERAGE 更新；F8b 可基于 F8a 的稳定 API 启动设计。

## 信心评估

| 任务 | 信心 | 依据 |
| :--- | :--- | :--- |
| T1 模型定义 | 98% | 纯类型定义 + 标准模式，无外部依赖 |
| T2–T9 八条规则 | 92% | `regex` / `url` / `std::net` 均已在 `Cargo.toml`；每条规则都是"解析 → 失败转 Issue"的简单模式 |
| T4 CIDR 手动解析 | 88% | 不引入 `cidr` crate，手动 `split('/')` + `IpAddr::from_str` + 前缀范围校验，需覆盖 IPv4/IPv6/无前缀三种情况 |
| T10 集成测试 | 95% | 端到端组合 T1–T9，无新逻辑 |
| T11 验证闭环 | 90% | 标准 cargo 工具链；覆盖率工具可用性取决于环境 |
| T12 推送 CI | 85% | 取决于 CI 环境状态，非代码风险 |
| **整体** | **>90%** | **无需网络调研，均为标准 Rust 模式 + 项目既有依赖** |

## 实现完成情况（2026-08-15）

- T1–T10 全部实现完成：
  - `mystiproxy/src/config/validation.rs` 新建，约 700 行（含 62 个测试）
  - `config/mod.rs` 新增 `pub mod validation;` 一行
  - 8 条验证规则全部实现：listen/target scheme、CIDR、TLS 路径、auth_type、upstream URL、regex location、timeout 正数
  - 数据模型：`ValidationLevel`(Strict/Warn/Loose)、`ValidationSeverity`、`ValidationIssue`、`ValidationResult`、`ConfigValidator`
  - `is_valid()` 三档语义修正：Strict 任何问题→false，Warn 仅 Error→false，Loose 恒 true
- 全量 `cargo test --workspace` 全绿（62 新增验证测试 + 171 原有测试）
- fmt 通过；clippy 无新告警
- T11 验证闭环：fmt ✅ clippy ✅ test ✅
- T12 推送 CI：待执行

## 风险与缓解

| 风险 | 缓解 |
| :--- | :--- |
| `url` crate 未在 `mystiproxy/Cargo.toml` | T7 第 2 步先核对 `Cargo.toml`；若缺失则需先添加依赖（与 F8a 设计声明"已存在"冲突时以实际为准） |
| `LocationConfig` 字段较多，测试构造冗长 | 在 `tests` 内提供 `base_location(mode)` 辅助构造器，填默认 `None` |
| `merge` 在不同级别间组合的行为易错 | T1 中 `test_merge_level_uses_self` 显式覆盖 `Loose` merge `Strict` 的边界 |
| 手动 CIDR 解析遗漏边界（如 `/0` 全网段） | T4 中 `test_cidr_valid_no_prefix` 覆盖无前缀；`/0` 应视为合法（前缀为 0） |
