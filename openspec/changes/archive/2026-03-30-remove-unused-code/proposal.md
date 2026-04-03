## 动机

MystiProxy 代码库中包含约 60-70% 的死代码——孤儿文件、未使用模块、遗留向后兼容类型以及被替代但从未清理的实现。这些代码增加了编译时间、维护负担，同时通过未使用的依赖（jsonpath-rust、anyhow）扩大了攻击面。

## 变更内容

- **移除 3 个孤儿文件**：`engine.rs`、`utils.rs`、`http/header.rs` 未被声明为模块，对编译和运行时没有任何影响
- **移除生产代码中未使用的模块/函数**：`HttpHandler`、`HttpClientPool`、`HeaderTransformer`、`BodyTransformer`、`StaticFileService`、`HttpServer`/`HttpProxyService`/`create_simple_server`、`Router`/`Route`/`MatchResult`/`pattern_to_regex`、`MockService`/`MockBuilder`/`MockLocation`、`Authenticator`/`AuthConfig`/`AuthType`/`AuthResult`/`Claims`、`TlsLoader`、`TlsServer`、`create_tls_connector`/`create_tls_connector_with_client_cert`、`TcpProxy`、`Proxy`（向后兼容桩）、`UnixProxy`/`ProxyTarget`
- **移除遗留配置类型**：`Config`、`TlsConfig`（config/mod.rs）、`RouteConfig`、`MockConfig` — 为不存在的向后兼容而保留
- **移除未使用的错误变体**：`ConfigFileRead`、`InvalidRegex`、`Auth`、`Jwt`
- **移除未使用的配置字段**：`LocationConfig::alias`、`LocationConfig::condition`、`MystiConfig::cert`、`CertConfig::root_key`
- **移除未使用的依赖**：`jsonpath-rust`、`anyhow`
- **移除无效重导出**：`lib.rs` 中的 `pub mod router`、`pub mod tls`（模块变空后），无效的 `pub use` 项
- **移除死代码路径**：`RouteMatch::None` 不可达的 match 分支，`HeaderActionType::ForceDelete` 空操作分支
- **修复 clippy 警告**：冗余闭包、可派生的 impl、手动 strip 模式、不必要的惰性求值、手动 flatten

## 能力变更

### 新增能力

_（无 — 这是一个清理/移除变更）_

### 修改的能力

_（无规格级行为变更 — 所有移除均为内部死代码）_

## 影响

- **源文件**：`src/` 下约 20+ 个文件被修改或删除
- **依赖**：`Cargo.toml` — 移除 `jsonpath-rust`、`anyhow`
- **公共 API**：对库消费者为破坏性变更 — `pub` 类型和模块被移除。二进制行为不变。
- **编译时间**：因移除未使用代码和依赖而减少
- **测试覆盖**：死模块内的测试将被移除；所有保留的测试必须通过
