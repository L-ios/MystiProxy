# F8d 配置验证框架-安全验证与用户界面 — Task 规划

## 任务分解（TDD 顺序）

> 总体策略：先写失败测试（红），再实现模型与规则使测试转绿，最后做集成与闭环验证。每条安全规则独立 TDD，便于定位回归。
>
> **⚠️ 实现状态说明**：F8d 代码已实现（`security.rs` 176 行 + `user_interface.rs` 248 行），但测试覆盖不足。本 Task 文档保留 TDD 视角的规划结构，并在每节标注"已实现"状态与待补测试。当前 `security.rs` 有 3 个测试（`test_validate_headers` / `test_validate_target_url` / `test_validate_cidr_list`），`user_interface.rs` 有 2 个测试（`test_ui_creation` / `test_extract_suggestions`，后者近乎空壳仅验证编译通过）。需按本规划补全测试矩阵。

### T1 SecurityValidator 基础 + 常量完整性

- [x] **已实现**：在 `mystiproxy/src/config/security.rs` 新建文件，定义 `DANGEROUS_HEADERS`（10 项）、`INTERNAL_NETWORKS`（8 项含 IPv6）、`URL_BLACKLIST_PATTERNS`（6 项）三个常量
- [x] **已实现**：`SecurityValidator` 结构体（字段 `dangerous_headers: Vec<String>` / `internal_networks: Vec<IpNetwork>` / `url_blacklist_patterns: Vec<Regex>`）
- [x] **已实现**：`impl Default for SecurityValidator` 委托 `new()`
- [x] **已实现**：`new()` 构造器，使用 `filter_map(|s| s.parse().ok())` / `filter_map(|s| Regex::new(s).ok())` 把常量转存为类型化集合
- [x] **已实现**：在 `mystiproxy/src/config/mod.rs` 新增 `pub mod security;`
- [ ] **待补测试**：先写失败测试：
  - `test_dangerous_headers_count`：`assert_eq!(DANGEROUS_HEADERS.len(), 10)`
  - `test_internal_networks_count`：`assert_eq!(INTERNAL_NETWORKS.len(), 8)`
  - `test_url_blacklist_patterns_count`：`assert_eq!(URL_BLACKLIST_PATTERNS.len(), 6)`
  - `test_security_validator_new_no_panic`：`SecurityValidator::new()` 不 panic
  - `test_security_validator_default_eq_new`：`SecurityValidator::default()` 与 `new()` 行为一致
  - `test_internal_networks_all_parse`：所有 `INTERNAL_NETWORKS` 条目 `IpNetwork::parse` 成功（验证 `filter_map` 未静默跳过）
  - `test_url_blacklist_patterns_all_compile`：所有 `URL_BLACKLIST_PATTERNS` 条目 `Regex::new` 成功
- [ ] `cargo test -p mystiproxy config::security::tests` 验证测试全**绿**
- [ ] `cargo clippy -p mystiproxy --all-targets -- -D warnings` 无新告警

**验收标准**：常量完整；构造器不 panic；`Default` 与 `new()` 一致；`filter_map` 未静默跳过任何常量。

### T2 规则 1 — 危险头部（`validate_headers`）

- [x] **已实现**：`validate_headers(&self, headers: &HashMap<String, String>) -> ValidationResult<()>`，逻辑为遍历头部、`to_lowercase()` 归一化、命中 `dangerous_headers` 即 `Err(Security)`
- [x] **已实现**：测试 `test_validate_headers`（合法头部 + `Content-Length` 拦截，2 个断言）
- [ ] **待补测试**（当前仅 1 个测试，需扩展为矩阵）：
  - `test_validate_headers_valid_custom`：`X-Custom-Header` → `Ok`
  - `test_validate_headers_multiple_valid`：多个合法头部 → `Ok`
  - `test_validate_headers_content_length`：`Content-Length` → `Err`，消息含 `dangerous header 'Content-Length'`
  - `test_validate_headers_authorization`：`authorization` → `Err`
  - `test_validate_headers_case_insensitive`：`CONTENT-LENGTH` / `Content-Length` / `content-length` 均拦截
  - `test_validate_headers_host`：`Host` → `Err`
  - `test_validate_headers_proxy_authorization`：`Proxy-Authorization` → `Err`
  - `test_validate_headers_empty_map`：空 `HashMap` → `Ok`
  - `test_validate_headers_all_dangerous`：遍历 `DANGEROUS_HEADERS` 每项构造头部，均 `Err`
- [ ] `cargo test -p mystiproxy config::security::tests test_validate_headers` 验证全**绿**

**验收标准**：9 个测试全绿；大小写不敏感；错误消息含原始头部名（未归一化）；`DANGEROUS_HEADERS` 全量覆盖。

### T3 规则 2 — SSRF 防护（`validate_target_url`）

- [x] **已实现**：`validate_target_url(&self, url: &str) -> ValidationResult<()>`，四层检查：黑名单正则 → `url::Url::parse` → 内网 IP（`IpAddr::from_str` 成功才查，失败视为域名放行）→ scheme 白名单（`http`/`https`）
- [x] **已实现**：测试 `test_validate_target_url`（合法外部 URL + 内网 IP 拦截 + 黑名单协议，约 6 个断言）
- [ ] **待补测试**（当前 1 个测试，需扩展为矩阵）：
  - `test_validate_target_url_valid_http`：`http://example.com/path` → `Ok`
  - `test_validate_target_url_valid_https`：`https://api.example.com` → `Ok`
  - `test_validate_target_url_valid_domain_only`：`http://localhost` → `Ok`（域名跳过内网检查）
  - `test_validate_target_url_ipv4_loopback`：`http://127.0.0.1/admin` → `Err`，消息含 `internal network`
  - `test_validate_target_url_ipv4_private_10`：`http://10.0.0.1/x` → `Err`
  - `test_validate_target_url_ipv4_private_172`：`http://172.16.0.1/x` → `Err`
  - `test_validate_target_url_ipv4_private_192`：`http://192.168.1.1/api` → `Err`
  - `test_validate_target_url_ipv4_link_local`：`http://169.254.169.254/latest/meta-data/` → `Err`（云元数据）
  - `test_validate_target_url_ipv6_loopback`：`http://[::1]/admin` → `Err`
  - `test_validate_target_url_file_scheme`：`file:///etc/passwd` → `Err`，消息含 `blacklisted pattern`
  - `test_validate_target_url_data_scheme`：`data:text/html,<script>` → `Err`
  - `test_validate_target_url_ftp_scheme`：`ftp://server/file` → `Err`
  - `test_validate_target_url_gopher_scheme`：`gopher://host/x` → `Err`
  - `test_validate_target_url_invalid_url`：`http://[invalid` → `Err`，消息含 `invalid URL`
  - `test_validate_target_url_javascript_scheme`：`javascript:alert(1)` → `Err`，消息含 `only HTTP and HTTPS`
  - `test_validate_target_url_tcp_scheme_blocked`：`tcp://host:8080` → `Err`（非 http/https）
- [ ] `cargo test -p mystiproxy config::security::tests test_validate_target_url` 验证全**绿**

**验收标准**：16 个测试全绿；四层检查顺序正确（黑名单短路）；IPv4/IPv6 内网均拦截；云元数据段覆盖；域名放行。

### T4 规则 3 — CIDR 宽泛性（`validate_cidr_list`）

- [x] **已实现**：`validate_cidr_list(&self, cidrs: &[String]) -> ValidationResult<()>`，逻辑为每条 `IpNetwork::parse` + 检查 `prefix == 0`
- [x] **已实现**：测试 `test_validate_cidr_list`（正常 CIDR + `0.0.0.0/0` + `::/0`，4 个断言）
- [ ] **待补测试**（当前 1 个测试，需扩展）：
  - `test_validate_cidr_valid_ipv4`：`192.168.1.0/24` → `Ok`
  - `test_validate_cidr_valid_ipv6`：`2001:db8::/32` → `Ok`
  - `test_validate_cidr_valid_private_8`：`10.0.0.0/8` → `Ok`（宽泛但不全网段）
  - `test_validate_cidr_zero_ipv4`：`0.0.0.0/0` → `Err`，消息含 `overly permissive`
  - `test_validate_cidr_zero_ipv6`：`::/0` → `Err`
  - `test_validate_cidr_invalid_syntax`：`not-an-ip/24` → `Err`，消息含 `invalid CIDR`
  - `test_validate_cidr_prefix_out_of_range_ipv4`：`192.168.1.0/33` → `Err`（`IpNetwork::parse` 失败）
  - `test_validate_cidr_prefix_out_of_range_ipv6`：`2001:db8::/129` → `Err`
  - `test_validate_cidr_empty_list`：`&[]` → `Ok`
  - `test_validate_cidr_multiple_one_bad`：`["10.0.0.0/8", "0.0.0.0/0"]` → `Err`（遇第一个即拒）
  - `test_validate_cidr_error_message_contains_cidr`：错误消息含原始 CIDR 字符串
- [ ] `cargo test -p mystiproxy config::security::tests test_validate_cidr` 验证全**绿**

**验收标准**：11 个测试全绿；`prefix == 0` 拦截；IPv4/IPv6 均覆盖；语法错误也 `Err`；遇第一个即拒。

### T5 规则 4 — 敏感信息（`check_sensitive_config`）

- [x] **已实现**：`check_sensitive_config(&self, config: &str) -> ValidationResult<()>`，5 类正则模式（`password` / `secret` / `api_key` / `token` / `private_key`），命中即 `warn!`，始终返回 `Ok`
- [ ] **待补测试**（当前**无测试**，需从零补全）：
  - `test_check_sensitive_config_password`：`"password: secret123"` → `Ok`（验证不 panic）
  - `test_check_sensitive_config_secret`：`"secret: abc"` → `Ok`
  - `test_check_sensitive_config_api_key_underscore`：`"api_key: xxx"` → `Ok`
  - `test_check_sensitive_config_api_key_hyphen`：`"api-key: xxx"` → `Ok`
  - `test_check_sensitive_config_token`：`"token: bearer xxx"` → `Ok`
  - `test_check_sensitive_config_private_key`：`"private_key: ..."` → `Ok`
  - `test_check_sensitive_config_case_insensitive`：`"PASSWORD: secret"` → `Ok`（`(?i)` 大小写不敏感）
  - `test_check_sensitive_config_no_match`：`"name: my-service"` → `Ok`
  - `test_check_sensitive_config_empty`：`""` → `Ok`
  - `test_check_sensitive_config_always_ok`：含敏感模式时仍 `Ok`（验证"只 warn 不 Err"语义）
  - `test_check_sensitive_config_multiple_patterns`：同时含 `password` + `token` → `Ok`，应触发多次 `warn!`
- [ ] `cargo test -p mystiproxy config::security::tests test_check_sensitive_config` 验证全**绿**

> **⚠️ 已知问题（必须修复）**：`security.rs` 第 163 行 `Regex::new(pattern).unwrap()` 是 panic 风险点。常量正则编译期固定，理论上不会失败，但 `unwrap()` 违反"不 panic"原则。建议修复方案（二选一）：
> 1. 改为 `Regex::new(pattern).expect("sensitive pattern must compile")`——保留语义，panic 消息更明确
> 2. 在 `SecurityValidator::new()` 构造期预编译所有敏感正则并缓存为 `Vec<Regex>` 字段，`check_sensitive_config` 直接复用——彻底消除运行时 `Regex::new`
>
> 推荐方案 2，与 `url_blacklist_patterns` 的处理方式一致。

**验收标准**：11 个测试全绿；5 类敏感模式全覆盖；大小写不敏感；始终 `Ok`；`Regex::new` 的 `unwrap()` 风险已修复。

### T6 ConfigUserInterface 基础 + 着色打印

- [x] **已实现**：`ConfigUserInterface` 结构体（pub 字段 `verbose: bool` / `color_output: bool`）
- [x] **已实现**：`impl Default`（`verbose: false, color_output: true`）
- [x] **已实现**：`new(verbose, color_output)` 构造器
- [x] **已实现**：私有 `print_success`（绿色 ✓）/ `print_error`（红色 ✗）/ `print_fix_suggestions`（黄色标题 + 蓝色 • 项）
- [x] **已实现**：测试 `test_ui_creation`（构造 + 字段断言）
- [ ] **待补测试**（当前 1 个测试，需扩展）：
  - `test_ui_default`：`ConfigUserInterface::default()` → `verbose == false, color_output == true`
  - `test_ui_new_verbose_color`：`new(true, false)` → 字段正确
  - `test_ui_new_silent_color`：`new(false, true)` → 字段正确
  - `test_ui_print_success_no_panic_color`：`print_success("ok")` 不 panic（`color_output: true`）
  - `test_ui_print_success_no_panic_plain`：`print_success("ok")` 不 panic（`color_output: false`）
  - `test_ui_print_error_no_panic_color` / `test_ui_print_error_no_panic_plain`
  - `test_ui_print_validation_result_ok_verbose`：`Ok` + `verbose: true` → 不 panic（会打印 ✓）
  - `test_ui_print_validation_result_ok_silent`：`Ok` + `verbose: false` → 不 panic（不打印）
  - `test_ui_print_validation_result_err`：`Err(Security("x"))` → 不 panic（打印 ✗ + 建议）
  - `test_ui_print_config_summary_no_panic`：传入空 `MystiConfig` → 不 panic
  - `test_ui_print_config_summary_with_engines`：传入含引擎的 `MystiConfig` → 不 panic
- [ ] `cargo test -p mystiproxy config::user_interface::tests test_ui` 验证全**绿**

> **注**：UI 方法涉及 `println!`，测试以"不 panic"为主，不验证 stdout 内容（stdout 捕获需 `portable-pty` 或类似 crate，超出 F8d 范围）。

**验收标准**：11 个测试全绿；`Default` / `new` 字段正确；着色/纯文本两路径均不 panic；各变体错误渲染不 panic。

### T7 建议提取（`extract_validation_suggestions`）

- [x] **已实现**：`extract_validation_suggestions(&self, errors: &ValidationErrors) -> Vec<String>`，按 `(field, code)` 元组 match，覆盖 `listen` / `target` / `proxy_type` / `locations` / `tls` / `auth` / `upstream` 7 字段，含兜底分支
- [x] **已实现**：测试 `test_extract_suggestions`（**近乎空壳**，仅 `let _ui = ConfigUserInterface::default();`，注释称"测试需要实际的 ValidationErrors，这里仅验证编译通过"）
- [ ] **待补测试**（当前空壳，需从零补全）：
  - `test_extract_suggestions_listen_empty`：构造 `ValidationErrors` 含 `("listen", "listen_empty")` → 建议含 `Listen address cannot be empty`
  - `test_extract_suggestions_listen_invalid_tcp`：`("listen", "invalid_tcp_address")` → 建议含 `Invalid TCP address format`
  - `test_extract_suggestions_listen_empty_unix`：`("listen", "empty_unix_socket_path")` → 建议含 `Unix socket path cannot be empty`
  - `test_extract_suggestions_listen_unsupported_protocol`：`("listen", "unsupported_protocol")` → 建议含 `Supported protocols`
  - `test_extract_suggestions_target_empty` / `target_invalid_tcp` / `target_empty_unix` / `target_unsupported_protocol`
  - `test_extract_suggestions_proxy_type_tcp_requires_tcp`：`("proxy_type", "tcp_proxy_requires_tcp_addresses")` → 建议含 `TCP proxy requires`
  - `test_extract_suggestions_proxy_type_http_listen` / `http_target` / `forward_tcp_listen`
  - `test_extract_suggestions_tls_cert_empty` / `tls_key_empty` / `tls_cert_not_found` / `tls_key_not_found` / `tls_client_ca_not_found`
  - `test_extract_suggestions_auth_header_requires_value` / `auth_jwt_requires_secret` / `auth_unsupported_type`
  - `test_extract_suggestions_upstream_invalid_url`
  - `test_extract_suggestions_locations_any_code`：`("locations", "any")` → 建议含 `Location configuration error`
  - `test_extract_suggestions_unknown_field_fallback`：`("unknown_field", "unknown_code")` → 建议含 `Validation error in 'unknown_field'`
  - `test_extract_suggestions_multiple_errors`：多条错误 → `Vec` 长度匹配
  - `test_extract_suggestions_empty_errors`：空 `ValidationErrors` → `Vec::new()`
- [ ] `cargo test -p mystiproxy config::user_interface::tests test_extract_suggestions` 验证全**绿**

> **注**：构造 `ValidationErrors` 需使用 `validator` crate 的 API 或手工构造。`ValidationErrors` 可通过 `ValidationErrors::new()` + `add(field, ValidationError::new(code))` 构建。若 `validator` crate 的 API 不支持精细构造，可考虑测试以集成方式跑——构造会触发 `ValidationErrors` 的 `EngineConfig`，验证 `extract_validation_suggestions` 对真实错误的映射。

**验收标准**：22 个测试全绿；7 字段映射全覆盖；兜底分支覆盖；多条错误聚合正确。

### T8 集成测试

- [ ] 先写失败测试：
  - `test_security_validator_full_workflow`：构造 `SecurityValidator`，依次跑 `validate_headers`（合法）→ `validate_target_url`（合法）→ `validate_cidr_list`（合法）→ `check_sensitive_config`（含敏感信息但 `Ok`），全流程不 panic
  - `test_security_validator_blocks_ssrf_chain`：`validate_target_url("http://127.0.0.1")` `Err`，后续方法不调用
  - `test_ui_render_security_error`：`SecurityValidator.validate_target_url("file:///etc/passwd")` 的 `Err` 交给 `ConfigUserInterface::print_validation_result`，不 panic
  - `test_ui_render_validation_error`：构造 `ConfigValidationError::Validation(ValidationErrors)` 交给 UI，不 panic
  - `test_ui_render_config_summary_full`：构造含 2 引擎 + locations + 证书的 `MystiConfig`，`print_config_summary` 不 panic
  - `test_security_with_f8a_validator`：F8a `ConfigValidator` 校验通过后，F8d `SecurityValidator` 接着校验，两者不冲突
- [ ] `cargo test -p mystiproxy config::security::tests test_integration` + `config::user_interface::tests test_integration` 验证全**绿**

**验收标准**：6 个集成测试全绿；安全 + UI 端到端不 panic；F8a 与 F8d 可组合调用。

### T9 验证闭环

- [ ] `cargo test -p mystiproxy config::security::tests` 全绿（T1–T5 + 集成，约 50+ 测试）
- [ ] `cargo test -p mystiproxy config::user_interface::tests` 全绿（T6–T7 + 集成，约 35+ 测试）
- [ ] `cargo test --workspace` 全绿（现有测试无回归，F8d 只新增不改旧）
- [ ] `cargo fmt --all -- --check` 通过
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` 无新告警
- [ ] `cargo llvm-cov -p mystiproxy config::security`（如可用）新增行覆盖 ≥ 70%
- [ ] `cargo llvm-cov -p mystiproxy config::user_interface`（如可用）新增行覆盖 ≥ 60%
- [ ] 手动构造一份含 SSRF 风险的 YAML（`target: http://127.0.0.1`），`SecurityValidator.validate_target_url` 返回 `Err`，`ConfigUserInterface.print_validation_result` 输出红色 ✗ + 建议
- [ ] 修复 `security.rs` 第 163 行 `Regex::new(pattern).unwrap()` 的 panic 风险（改为 `expect` 或构造期预编译）

**验收标准**：全量测试 + clippy + fmt 闭环；覆盖率达标；手动验证输出符合预期；`unwrap()` 风险已修复。

### T10 推送 CI

- [ ] 提交（`feat(mystiproxy/config): add F8d security validator and user interface`）
- [ ] push GitHub，盯 `.github/workflows/rust.yml` Actions 至全绿
- [ ] 更新 `ROADMAP.md` 标注 F8d（配置验证-安全验证与用户界面）已闭环
- [ ] 更新 `docs/FEATURE_COVERAGE.md` 增加安全验证与 UI 一节
- [ ] 在 F9 设计文档中引用 F8d 的 `SecurityValidator` / `ConfigUserInterface` 作为本地管理模块依赖

**验收标准**：CI 全绿；ROADMAP / FEATURE_COVERAGE 更新；F9 可基于 F8d 的稳定 API 启动设计。

## 信心评估

| 任务 | 信心 | 依据 |
| :--- | :--- | :--- |
| T1 基础 + 常量 | 98% | 纯类型定义 + 常量，`ipnetwork` / `regex` 均已在 `Cargo.toml` |
| T2 危险头部 | 95% | `HashMap` 遍历 + `to_lowercase` + `contains`，标准模式 |
| T3 SSRF 防护 | 88% | 四层检查顺序、IPv6 内网、域名跳过逻辑需仔细覆盖；`url::Url::parse` 对 `file:///etc/passwd` 的解析行为需验证 |
| T4 CIDR 宽泛性 | 92% | `IpNetwork::parse` + `prefix == 0`，简单模式 |
| T5 敏感信息 | 85% | **含 `unwrap()` panic 风险修复**；正则模式覆盖 `api_key` / `apiKey` 需验证；"只 warn 不 Err"语义测试需 `tracing` mock |
| T6 UI 基础 | 90% | `colored` crate 标准用法，测试以"不 panic"为主 |
| T7 建议提取 | 82% | **当前测试近乎空壳，需从零补全 22 个**；`ValidationErrors` 构造依赖 `validator` crate API，可能需集成方式测试 |
| T8 集成测试 | 90% | 端到端组合 T1–T7，无新逻辑 |
| T9 验证闭环 | 88% | 标准 cargo 工具链；覆盖率工具可用性取决于环境；`unwrap()` 修复需回归 |
| T10 推送 CI | 85% | 取决于 CI 环境状态，非代码风险 |
| **整体** | **>85%** | **代码已实现，主要工作在测试补全与 `unwrap()` 修复** |

## 实现完成情况（2026-08-25）

- **代码已实现**：
  - `mystiproxy/src/config/security.rs` 新建，176 行（含 3 个测试）
  - `mystiproxy/src/config/user_interface.rs` 新建，248 行（含 2 个测试）
  - `config/mod.rs` 新增 `pub mod security;` `pub mod user_interface;` 两行
  - 4 个安全验证方法全部实现：`validate_headers` / `validate_target_url` / `validate_cidr_list` / `check_sensitive_config`
  - 3 个 UI 方法全部实现：`print_validation_result` / `print_config_summary` / `extract_validation_suggestions`
  - 3 类常量全部定义：`DANGEROUS_HEADERS`（10 项）/ `INTERNAL_NETWORKS`（8 项）/ `URL_BLACKLIST_PATTERNS`（6 项）
- **测试覆盖不足（待补）**：
  - `security.rs` 仅 3 个测试（`test_validate_headers` / `test_validate_target_url` / `test_validate_cidr_list`），`check_sensitive_config` **零测试**
  - `user_interface.rs` 仅 2 个测试（`test_ui_creation` + `test_extract_suggestions` 空壳），UI 渲染方法**零测试**
  - 待补测试矩阵约 80+ 个（见 T1–T8）
- **已知问题**：
  - `security.rs` 第 163 行 `Regex::new(pattern).unwrap()` 是 panic 风险点，待修复（T9）
  - `user_interface.rs` 的 `extract_validation_suggestions` 是私有方法，测试需通过 `print_fix_suggestions` 间接调用或改为 `pub`
- **fmt / clippy / test 状态**：待 T9 闭环时验证

## 风险与缓解

| 风险 | 缓解 |
| :--- | :--- |
| `security.rs` 第 163 行 `Regex::new(pattern).unwrap()` 可能 panic | T5/T9 中修复：改为 `expect("sensitive pattern must compile")` 或在 `new()` 构造期预编译所有敏感正则为 `Vec<Regex>` 字段，`check_sensitive_config` 直接复用（推荐后者，与 `url_blacklist_patterns` 处理一致） |
| `validate_target_url` 对 `file:///etc/passwd` 的解析行为不确定 | T3 中 `test_validate_target_url_file_scheme` 显式覆盖；黑名单正则在第 1 层短路，应在 `Url::parse` 之前拦截 |
| `validate_target_url` 对 `http://[::1]/admin`（IPv6 字面量）的 host 提取 | T3 中 `test_validate_target_url_ipv6_loopback` 显式覆盖；`url::Url::host_str()` 对 IPv6 返回 `[::1]`，`IpAddr::from_str` 需去方括号（实际 `Url::host_str()` 返回不含方括号的 `::1`，需验证） |
| `check_sensitive_config` 的 `warn!` 难以测试 | T5 中以"不 panic + 返回 `Ok`"为验收点；若需验证 `warn!` 触发，可用 `tracing_subscriber` 的 ` alloc::fmt::Subscriber` 或 `tracing-test` crate 捕获日志 |
| `extract_validation_suggestions` 测试需构造 `ValidationErrors` | T7 中优先用 `validator` crate 的 `ValidationErrors::new()` + `add(field, ValidationError::new(code))` 构造；若 API 不支持精细构造，改用集成方式（构造会触发 `ValidationErrors` 的 `EngineConfig`） |
| `user_interface.rs` 的 `println!` 输出难以测试 | T6 中以"不 panic"为验收点；stdout 捕获留给后续（需 `portable-pty` 或类似 crate） |
| `extract_validation_suggestions` 是私有方法，测试无法直接调用 | T7 中或改 `pub`（设计上可接受，便于管理 API 消费），或通过 `print_validation_result` 间接测试（但 stdout 捕获同上） |
| `LocationConfig` 字段较多，集成测试构造冗长 | 在 `tests` 内提供 `base_location(mode)` / `base_engine()` 辅助构造器，填默认 `None` |
