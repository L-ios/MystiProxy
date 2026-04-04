## 背景

MystiProxy 在迭代开发过程中积累了约 60-70% 的死代码。存在多套并行实现：路由（`handler.rs` 内联 vs `router/mod.rs`）、Mock 处理（`handler.rs::build_mock_response()` vs `mock/MockService`）、服务器启动（`ProxyServer` vs `HttpServer`）。被替代的实现从未被清理。此外，`engine.rs` 和 `utils.rs` 是未被构建系统编译的孤儿文件。

## 目标 / 非目标

**目标：**
- 移除所有对编译或运行时没有影响的代码
- 移除未使用的 crate 依赖，以减少编译时间和攻击面
- 清理无效重导出和遗留向后兼容类型
- 确保清理后所有保留的测试通过
- 修复审计过程中发现的 clippy 警告

**非目标：**
- 重构保留的活代码架构
- 从一套实现迁移到另一套（如内联路由 → Router 模块）
- 添加新功能或改变现有行为
- 决定是否为将来使用保留 auth/TLS/static-files 模块 — 我们移除它们，如需要可从 git 历史恢复

## 决策

### D1：完全移除孤儿文件
**决策**：彻底删除 `src/engine.rs`、`src/utils.rs`、`src/http/header.rs`。
**理由**：这些文件未在任何地方声明为模块，对构建没有影响。`engine.rs` 甚至引用了不存在的 `crate::arg::MystiEngine`。
**备选方案**：声明它们为模块并接入 — 否决，因为它们包含已被替代的原型代码。

### D2：移除仅被自身测试使用的未使用模块
**决策**：移除 `router/` 模块、`http/auth.rs`、`http/static_files.rs`、`http/server.rs`（HttpServer/HttpProxyService/create_simple_server）以及 `http/mod.rs` 中的 `HttpHandler` 结构体。保留 `mock/MockResponse`（被 handler 使用），移除 `MockService`/`MockBuilder`/`MockLocation`。
**理由**：这些模块在生产中零使用。它们被 `handler.rs` 或 `ProxyServer` 中的内联实现所替代。死模块内的测试随模块一起移除。
**备选方案**：保留为公共库 API — 否决，没有外部消费者的证据，代码可从 git 恢复。

### D3：移除遗留配置类型
**决策**：删除 `Config`、`TlsConfig`（config/mod.rs 版本）、`RouteConfig`、`MockConfig` 结构体及其 `from_yaml` 方法。
**理由**：这些被标记为"向后兼容"，但没有需要兼容的东西 — 应用只使用 `MystiConfig`。

### D4：移除未使用的错误变体
**决策**：从 `MystiProxyError` 枚举中移除 `ConfigFileRead`、`InvalidRegex`、`Auth`、`Jwt`。
**理由**：这些在生产代码路径中从未被构造。`InvalidRegex` 仅在已删除的 `router` 模块中使用。`Auth`/`Jwt` 来自已删除的 auth 模块。`ConfigFileRead` 从未被构造（IO 错误通过 `From<std::io::Error>` 转换）。

### D5：移除未使用的 Cargo 依赖
**决策**：从 `Cargo.toml` 中移除 `jsonpath-rust` 和 `anyhow`。
**理由**：源代码中零引用。`jsonpath-rust` 可能是为 body.rs 的 JSON path 操作准备的，但已手动实现。`anyhow` 被自定义 `MystiProxyError` 类型所替代。

### D6：移除未使用的配置字段
**决策**：移除 `LocationConfig::alias`、`LocationConfig::condition`、`CertConfig::root_key` 以及 `MystiConfig::cert` 字段（连同 `CertConfig` 结构体）。
**理由**：这些字段从 YAML 解析但运行时从未读取。`cert` 配置在实践中始终为空 `vec![]`。

### D7：修复保留代码的 clippy 警告
**决策**：修复清理后保留文件中的冗余闭包、可派生 impl、手动 strip 模式、不必要的惰性求值和手动 flatten 模式。
**理由**：这些是低风险的机械修复，可提高代码质量。

### D8：清理 lib.rs 和 http/mod.rs 中的重导出
**决策**：移除无效的 `pub use` 重导出。如果模块变空则移除 `pub mod router` 和 `pub mod tls`（或移除整个模块）。更新 `pub use mock::{...}` 只重导出 `MockResponse`。
**理由**：无效重导出会产生误导性的公共 API 接口。

## 风险 / 权衡

- **[对库消费者的破坏性变更]** → 如果有人将 MystiProxy 作为库 crate 使用，移除 `pub` 类型会破坏他们的代码。缓解措施：没有库消费者的证据；这主要是一个二进制应用。所有移除的代码可从 git 恢复。
- **[移除潜在有用的模块（auth、TLS、static-files）]** → 这些代表了大量实现工作，可能以后需要。缓解措施：它们保留在 git 历史中，可以恢复。无限期保留死代码更糟糕。
- **[测试覆盖率降低]** → 移除死模块会移除其测试。缓解措施：死代码的测试不提供真正的覆盖率。活代码路径的测试保持完整。
- **[大型 diff]** → 一次变更中修改 20+ 个文件。缓解措施：所有变更纯粹是减法（删除），使审查简单明了。无行为变更。
