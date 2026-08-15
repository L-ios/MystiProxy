# F7 生产代码错误处理重构 — Design

## 总体设计原则

1. **精准修复**：只动 12 处真正暴露 panic 风险的生产 unwrap，不做大规模重构
2. **TDD 先行**：先写测试覆盖现有 panic 路径，再修复代码使测试通过
3. **保持向后兼容**：函数签名变更仅扩大返回类型（`T` → `Option<T>` 或 `Result<T, E>`），现有调用方全部兼容
4. **测试代码不动**：cfg(test) 内的 unwrap 是 Rust 惯例，保留

## 代码设计

### 1. `mystiproxy/src/gateway.rs` — 6 处修复

#### 1.1 L228 `build_target_uri` 中 `match_uri().unwrap()` （**最严重**）

**现状**：
```rust
pub fn build_target_uri(&self, in_uri: &str) -> Option<String> {
    match self.match_uri(in_uri).unwrap() {   // ← None 时 panic
        UriMatch::Exact => Some(self.target_uri.clone()),
        ...
    }
}
```

**修复**：
```rust
pub fn build_target_uri(&self, in_uri: &str) -> Option<String> {
    let matched = self.match_uri(in_uri)?;   // ← ? 传播 None
    match matched {
        UriMatch::Exact => Some(self.target_uri.clone()),
        ...
    }
}
```

**测试**：原行为 `build_target_uri("/nonexistent")` 在某些 UriMapping 配置下会 panic；新行为返回 `None`。写一个回归测试：构造一个 UriMapping，调用 `build_target_uri` 传入不匹配的 URI，断言返回 `None`。

#### 1.2 L136 `uri_variable` 中 `Regex::new(regex).unwrap()`

**现状**：
```rust
fn uri_variable(uri: &str) -> HashMap<String, UriVariable> {
    ...
    let variable = UriVariable {
        regex: Regex::new(regex).unwrap(),   // ← 用户配置非法正则时 panic
        ...
    };
}
```

**修复**：将返回类型改为 `Result`，让上层决定如何处理（启动时 fail-fast，运行时返回 None）：
```rust
fn uri_variable(uri: &str) -> Result<HashMap<String, UriVariable>, regex::Error> {
    ...
    let variable = UriVariable {
        regex: Regex::new(regex)?,   // ← 配置错误时返回 Err
        ...
    };
    Ok(variable_patterns)
}
```

**调用方适配**：
- L172 `let variable_patterns = Self::uri_variable(self.uri.as_str());` → `Self::uri_variable(self.uri.as_str()).ok()?;`（match_uri 内）
- L233 `let in_map = Self::uri_variable(self.uri.as_str());` → `let in_map = Self::uri_variable(self.uri.as_str()).ok()?;`（build_target_uri 内，签名已是 Option）
- L266 `let out_map = Self::uri_variable(self.target_uri.as_str());` → `let out_map = Self::uri_variable(self.target_uri.as_str()).ok()?;`

**测试**：构造 UriMapping 的 uri 含 `{name:[invalid regex}`，调用 `uri_variable` 断言返回 `Err`。`match_uri` / `build_target_uri` 调用时返回 `None`。

#### 1.3 L197 / L245 / L248 `Regex::new(&format!(...)).unwrap()`

**现状**：基于 `processed_base_uri`（由配置模板派生）构造正则，配置含特殊字符时 panic。

**修复**：在 `match_uri` 与 `build_target_uri` 中改为：
```rust
let regex = Regex::new(&format!("^{processed_base_uri}\\/?.*$")).ok()?;  // ← ok() 转 Option
```

`match_uri` 返回类型本就是 `Option<UriMatch>`，`?` 传播 None 即可。`build_target_uri` 同理。

#### 1.4 L271 `match_var.get(&variable.index).unwrap()`

**现状**：
```rust
let path = match_var.get(&variable.index).unwrap();   // ← index 缺失时 panic
```

**修复**：
```rust
let path = match_var.get(&variable.index)?;   // ← ? 传播 None
```

### 2. `mystiproxy/src/http/ntlm.rs` — 1 处修复（L484）

**现状**：
```rust
let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()   // ← 系统时钟早于 1970 时 panic（NTP 跳变/虚拟机迁移）
    .as_nanos() as u64;
```

**修复**：
```rust
let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or(Duration::ZERO)   // ← 时钟回拨时退化为 0，下游 NTLM 时间戳为 0
    .as_nanos() as u64;
```

**测试**：单元测试无法直接模拟时钟回拨（SystemTime 不可 mock），但可以加 doc 测试说明语义。覆盖率通过其他 ntlm 测试覆盖。

### 3. `mysticentral/src/services/websocket.rs` — 5 处修复

**现状**：5 处都是 `serde_json::to_string(&json!({...})).unwrap()`，被序列化的对象是 `serde_json::Value`（已解析的 JSON 树），事实安全。

**修复**：统一改为 `.expect("serialize json! value cannot fail")`，使意图显式且与团队"零 unwrap"风格一致：
```rust
let msg_str = serde_json::to_string(&message)
    .expect("serialize json! value cannot fail");
```

**测试**：现有测试已覆盖 broadcast_config_update / handle_connection 路径，无需新测试。

## 测试策略

### 单元测试（TDD）

1. **`gateway.rs` 回归测试**（新增到 `#[cfg(test)] mod tests`）
   - `test_build_target_uri_returns_none_on_mismatch`：构造 UriMapping `{ uri: "/api/{id}", target_uri: "/v1/{id}" }`，调用 `build_target_uri("/health")` 断言返回 `None`（原行为 panic）
   - `test_match_uri_returns_none_on_invalid_regex_pattern`：UriMapping uri 含非法正则 `{name:[}`，`match_uri("/api/x")` 返回 `None`
   - `test_uri_variable_returns_err_on_bad_regex`：直接调用 `UriMapping::uri_variable("/api/{name:[invalid}")` 断言返回 `Err(regex::Error)`

2. **`ntlm.rs` 时钟回拨测试**：通过 doc test 说明 `unwrap_or(Duration::ZERO)` 语义；现有 NTLM 单元测试覆盖热路径

3. **`websocket.rs`**：现有测试覆盖（5 处均在 `expect()` 后行为不变）

### 集成验证

- `cargo test -p mystiproxy gateway::tests` — gateway 新增测试通过
- `cargo test --workspace` — 全量绿
- `cargo clippy --workspace --all-targets -- -D warnings` — 无新告警

### 覆盖率

- `cargo llvm-cov -p mystiproxy gateway` 新增代码 ≥70%（gateway.rs 中新增的 `?` 路径都有 None 输入测试）
- `cargo llvm-cov -p mysticentral services::websocket` 现有覆盖率不降

## 风险与回退

- **风险 1**：`uri_variable` 返回类型从 `HashMap` 改为 `Result<HashMap, regex::Error>` 是 breaking change（库 API）。但 `uri_variable` 是 `fn`（非 `pub fn`），私有方法，无外部调用方，**无影响**。
- **风险 2**：`match_uri` 行为对非法正则从 panic 变为返回 `None`，可能影响现有调用方的预期。但调用方 `build_target_uri` 本就处理 None，handler.rs 的 `match_route` 也处理 None，**无影响**。
- **回退**：所有变更在 12 个文件（实际改 3 个源文件 + 测试），单 commit 可 revert。
