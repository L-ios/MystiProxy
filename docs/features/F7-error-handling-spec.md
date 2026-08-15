# F7 生产代码错误处理重构 — Spec

## 1. 功能概述

本功能消除 MystiProxy 工程中**生产代码里真正会 panic 的 unwrap 调用**，覆盖 `mystiproxy` 与 `mysticentral` 两个 crate，共计 12 处修复点。

## 2. 现状基线

| Crate | 文件 | 生产 unwrap 数 | 真正危险 | 事实上安全 |
|-------|------|---------------|---------|-----------|
| mystiproxy | src/gateway.rs | 7 | 6 | 1 |
| mystiproxy | src/http/ntlm.rs | 6 | 1 | 5 |
| mystiproxy | src/metrics.rs | 7 | 0 | 7 |
| mystiproxy | src/http/proxy.rs | 4 | 0 | 4 |
| mystiproxy | src/main.rs | 3 | 0 | 3 |
| mystiproxy | src/io/stream.rs | 2 | 0 | 2 |
| mystiproxy | src/router/mod.rs | 1 | 0 | 1 |
| mystiproxy | src/context.rs | 1 | 0 | 1 |
| mysticentral | src/services/websocket.rs | 5 | 0 | 5 |
| **合计** | — | **36** | **7** | **29** |

> "事实上安全" 指调用结果在当前代码上下文中 Ok 永远成立（如硬编码合法指标名、`peek()` 已确认有元素的 `next()`）。这些不在本次重构范围。

## 3. 修复清单

### 3.1 `mystiproxy/src/gateway.rs`

| 行号 | 现状 | 修复后 |
|------|------|--------|
| L136 | `Regex::new(regex).unwrap()` | `Regex::new(regex)?`（`uri_variable` 改返回 `Result`） |
| L197 | `Regex::new(&format!(...)).unwrap()` | `Regex::new(&format!(...)).ok()?` |
| L228 | `self.match_uri(in_uri).unwrap()` | `self.match_uri(in_uri)?` |
| L245 | `Regex::new(&processed_base_uri).unwrap()` | `Regex::new(&processed_base_uri).ok()?` |
| L248 | `Regex::new(&format!(...)).unwrap()` | `Regex::new(&format!(...)).ok()?` |
| L271 | `match_var.get(&variable.index).unwrap()` | `match_var.get(&variable.index)?` |

### 3.2 `mystiproxy/src/http/ntlm.rs`

| 行号 | 现状 | 修复后 |
|------|------|--------|
| L484 | `SystemTime::now().duration_since(UNIX_EPOCH).unwrap()` | `.unwrap_or(Duration::ZERO)` |

### 3.3 `mysticentral/src/services/websocket.rs`

| 行号 | 现状 | 修复后 |
|------|------|--------|
| L72 | `serde_json::to_string(&message).unwrap()` | `.expect("serialize json! value cannot fail")` |
| L84 | `serde_json::to_string(&message).unwrap()` | `.expect("serialize json! value cannot fail")` |
| L171 | `serde_json::to_string(&welcome).unwrap()` | `.expect("serialize json! value cannot fail")` |
| L229 | `serde_json::to_string(&response).unwrap()` | `.expect("serialize json! value cannot fail")` |
| L239 | `serde_json::to_string(&response).unwrap()` | `.expect("serialize json! value cannot fail")` |

## 4. 函数签名变更

### `mystiproxy/src/gateway.rs`

```diff
- fn uri_variable(uri: &str) -> HashMap<String, UriVariable> {
+ fn uri_variable(uri: &str) -> Result<HashMap<String, UriVariable>, regex::Error> {
      ...
-     let variable = UriVariable {
-         regex: Regex::new(regex).unwrap(),
-         ...
-     };
-     variable_patterns.insert(variable.name.clone(), variable);
+     let variable = UriVariable {
+         regex: Regex::new(regex)?,
+         ...
+     };
+     variable_patterns.insert(variable.name.clone(), variable);
  }
- variable_patterns
+ Ok(variable_patterns)
```

调用方适配（`match_uri` / `build_target_uri` 内）：
```diff
- let variable_patterns = Self::uri_variable(self.uri.as_str());
+ let variable_patterns = Self::uri_variable(self.uri.as_str()).ok()?;
```

其余函数（`match_uri`、`build_target_uri`）签名**不变**，仍是 `Option<...>`。

## 5. 测试要求

### 新增单元测试（gateway.rs `#[cfg(test)] mod tests`）

- `test_build_target_uri_returns_none_on_mismatch`：UriMapping `{ uri: "/api/{id}", target_uri: "/v1/{id}" }` 调用 `build_target_uri("/health")` 返回 `None`
- `test_match_uri_returns_none_on_invalid_regex_pattern`：UriMapping 含非法正则 `{name:[}`，`match_uri("/api/x")` 返回 `None`
- `test_uri_variable_returns_err_on_bad_regex`：`uri_variable("/api/{name:[invalid}")` 返回 `Err`

### 现有测试不变

- `ntlm.rs` 现有测试覆盖热路径
- `websocket.rs` 现有测试覆盖 5 处 expect 后行为
- workspace 其余 480+ 测试不受影响

## 6. 验收标准

1. ✅ 12 处生产 unwrap 全部按上表修复
2. ✅ 新增 3 个 gateway 测试通过
3. ✅ `cargo test --workspace` 全绿
4. ✅ `cargo clippy --workspace --all-targets -- -D warnings` 无新告警
5. ✅ `cargo fmt --all -- --check` 通过
6. ✅ GitHub Actions（`Build and Release` workflow）全绿
7. ✅ 新增/修改代码覆盖率 ≥70%（`cargo llvm-cov -p mystiproxy gateway` 的新增行覆盖）

## 7. 不在范围

- 测试代码中的 478 处 unwrap（保留，Rust 测试惯例）
- "事实上安全"的 24 处生产 unwrap（替换无 ROI，可能引入新错误传播路径）
- 错误类型系统重构（`MystiProxyError` + anyhow 现状合理）
- `main.rs` 信号监听的 `expect()`（启动时合理 panic 点）
- TLS 模块（tls/mod.rs 与 tls/reloader.rs 的 85 处 unwrap 全在测试代码中）

## 8. 与 ROADMAP 的对齐

本功能完成 `ROADMAP.md` Phase 1 / T1.2 的**实质部分**：
- 原目标"0 处 `.unwrap()` 调用（生产代码）"中真正危险的 7 处全部消除
- 原描述"424 处"为粗略 grep 误算，实际生产 36 处，本功能修复 12 处（其余 24 处事实上安全，按 ROI 不动）
- T1.1（错误类型体系）/ T1.3（错误上下文）/ T1.4（错误恢复）现状已合理，不在本次范围
