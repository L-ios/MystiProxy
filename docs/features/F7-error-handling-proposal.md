# F7 生产代码错误处理重构 — Proposal

## 需求深度理解

### 背景与问题

`ROADMAP.md` Phase 1 / T1.2 列出"424 处 `.unwrap()` 调用需要替换"，且将 `mystiproxy/src` 划分为多个高计数文件（gateway.rs 15、http/proxy.rs 42、http/handler.rs 38 等）。

本次对 `mystiproxy/src` 与 `mysticentral/src` 全量审计后，**事实与原描述有重大出入**：

1. **粗略 grep 的 424→266→248 计数严重高估了风险**
   - `mystiproxy/src` grep 得到 266 处 `unwrap|expect|panic`，**只有 31 处位于生产代码**（其余 235 处全在 `#[cfg(test)] mod tests` 内，属于 Rust 测试惯例，**无需修改**）
   - `mysticentral/src` grep 得到 248 处 `unwrap`，**只有 5 处位于生产代码**（其余 243 处全在测试代码）

2. **真正的生产风险高度集中**
   - `mystiproxy/src`：31 处生产调用中，**7 处真正危险**（全在 `gateway.rs` 6 处 + `http/ntlm.rs` 1 处），24 处"事实上安全"（Ok 永远成立）
   - `mysticentral/src`：5 处生产调用全部位于 `services/websocket.rs`，全部为 `serde_json::to_string(&json!({...})).unwrap()`，事实上安全但热路径暴露

3. **`gateway.rs` 是唯一在正常请求流量中会 panic 的模块**
   - `match_uri()` 返回 `Option<UriMatch>`，但 `build_target_uri()` 用 `.unwrap()` 调用它——任何不匹配的 URI 直接 panic，而 `build_target_uri` 自身签名是 `Option<String>`，逻辑本应优雅返回 `None`
   - 配置驱动的 `Regex::new(user_pattern).unwrap()` 让用户配置非法正则时进程崩溃
   - 这些都在每个网关请求的热路径上

### 深层需求分析

错误处理重构的真正价值是**消除生产环境 panic 风险**，而不是数字游戏。

- **panic 在代理服务器中是不可接受的**：代理是基础设施层，单个请求的 URI 不匹配就让进程崩溃，会影响所有并发连接的可用性
- **测试代码中的 unwrap 不在重构范围**：Rust 社区共识是测试代码用 `unwrap` 是合理的（失败时显式失败），强行替换会降低测试可读性
- **"事实上安全"的 unwrap 也不应全部替换**：例如 `IntCounter::new("errors_total", ...).unwrap()` 中硬编码的合法指标名永远不会失败，替换为 `?` 反而扩散错误传播路径

### 用户价值

- 代理服务器在配置错误或非匹配 URI 时**不再 panic**，而是返回 `None`/`Err`，由上层决定 404/502 响应
- `ROADMAP.md` Phase 1 / T1.2 验收标准"0 处 `.unwrap()` 调用（生产代码）"中**真正危险的 7 处**被消除
- 配置非法正则时启动失败并明确报错，而非运行时 panic

### 成功标准

1. `gateway.rs` 中 6 处生产 unwrap 全部消除（`match_uri().unwrap()` / `Regex::new(user_pattern).unwrap()` × 4 / `match_var.get(...).unwrap()`）
2. `http/ntlm.rs` L484 时钟回拨 unwrap 改为 `unwrap_or(Duration::ZERO)`
3. `mysticentral/src/services/websocket.rs` 5 处 unwrap 改为 `expect("...")` 显式标注"事实上不会失败"
4. 新增/修改代码测试覆盖率 ≥70%
5. 全量 `cargo test --workspace` 通过；`cargo clippy --workspace --all-targets` 无新告警；`cargo fmt --check` 通过
6. GitHub Actions 全绿

### 范围边界

**做**：
- 上述 12 处生产 unwrap 的精准修复（gateway.rs 6 + ntlm.rs 1 + websocket.rs 5）
- 为修复点编写 TDD 测试（含原 panic 路径的回归测试）
- 顺手把 `gateway.rs` 中 `match_uri` / `build_target_uri` 错误传播路径补全为 `Option` 链式

**不做**：
- 重构错误类型体系（`MystiProxyError` + anyhow 已经合理，无需重写）
- 替换测试代码中的 unwrap（235+243=478 处测试 unwrap 保留）
- 替换"事实上安全"的 24 处生产 unwrap（如 `IntCounter::new` / `Response::builder().body(Full<Bytes>)` 等，替换无 ROI）
- 改 `main.rs` 信号监听的 `expect()`（启动时合理 panic 点）

## 技术方案选型（信心评估）

- `gateway.rs` L228 改 `?` 传播：**信心 96%**，一行修复，语义清晰
- `gateway.rs` L136/L197/L245/L248 配置驱动正则改 `map_err` + `?`：**信心 92%**，需要把 `uri_variable` 返回类型从 `HashMap` 改为 `Result<HashMap, MystiProxyError>`
- `gateway.rs` L271 `match_var.get(...).unwrap()` 改 `?`：**信心 95%**
- `http/ntlm.rs` L484 `unwrap_or(Duration::ZERO)`：**信心 98%**
- `websocket.rs` `.unwrap()` → `.expect("...")`：**信心 99%**
- 全部 >85%，**无需网络调研**
