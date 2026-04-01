## 移除的需求

### 需求：孤儿源文件不得存在
**原因**：`src/engine.rs`、`src/utils.rs` 和 `src/http/header.rs` 未被声明为模块，对构建没有任何影响。
**迁移方案**：不适用 — 这些文件从未被编译过。

### 需求：遗留配置类型向后兼容
**原因**：`Config`、`TlsConfig`（config/mod.rs 中的版本）、`RouteConfig`、`MockConfig` 结构体及其 `from_yaml` 方法是为不存在的向后兼容而保留的死代码。
**迁移方案**：直接使用 `MystiConfig` 及其关联类型。

### 需求：未使用的错误变体必须可构造
**原因**：`MystiProxyError::ConfigFileRead`、`MystiProxyError::InvalidRegex`、`MystiProxyError::Auth`、`MystiProxyError::Jwt` 在生产代码中从未被构造。
**迁移方案**：这些场景的错误处理已使用其他变体（`Config`、`Router`、`Proxy`）。

### 需求：未使用的 crate 依赖
**原因**：`jsonpath-rust` 和 `anyhow` 在源代码中零引用。
**迁移方案**：`body.rs` 中的 JSON path 操作已手动实现。应用错误使用 `MystiProxyError`。

### 需求：未使用的配置字段必须可解析
**原因**：`LocationConfig::alias`、`LocationConfig::condition`、`CertConfig::root_key`、`MystiConfig::cert` 被解析但运行时从未读取。
**迁移方案**：从配置结构体中移除。包含这些字段的现有 YAML 配置将反序列化失败 — 可接受，因为这些字段从未生效过。

## 新增需求

### 需求：所有保留代码必须在生产或测试路径中被使用
清理后，crate 中的每个 `pub` 函数、结构体、枚举、trait 和类型别名都必须可以从 `main.rs`（生产代码）或 `#[cfg(test)]` 模块中可达。

#### 场景：Cargo check 零警告通过
- **当** 运行 `cargo clippy --all-targets -- -W dead_code -W unused_imports` 时
- **则** 不产生任何 dead_code 或 unused_imports 警告

#### 场景：清理后所有测试通过
- **当** 运行 `cargo test --all` 时
- **则** 所有测试通过，退出码为 0

#### 场景：构建成功
- **当** 运行 `cargo build` 时
- **则** 构建成功完成，退出码为 0
