## 1. 删除孤儿文件

- [ ] 1.1 删除 `src/engine.rs`（未声明为模块，引用了不存在的 `MystiEngine`）
- [ ] 1.2 删除 `src/utils.rs`（未声明为模块，`fix_length` 从未被调用）
- [ ] 1.3 删除 `src/http/header.rs`（未在 `http/mod.rs` 中声明为模块，`HeaderTransformer` 从未使用）
- [ ] 1.4 验证 `cargo build` 删除后编译成功

## 2. 移除死模块

- [ ] 2.1 删除整个 `src/tls/` 目录（无模块导入 `crate::tls`）
- [ ] 2.2 从 `src/lib.rs` 中移除 `pub mod tls;`
- [ ] 2.3 删除整个 `src/router/` 目录（无模块导入 `crate::router`，handler 使用内联路由）
- [ ] 2.4 从 `src/lib.rs` 中移除 `pub mod router;`
- [ ] 2.5 删除 `src/http/auth.rs`（仅在其自身测试中使用，生产中未使用）
- [ ] 2.6 从 `src/http/mod.rs` 中移除 `mod auth;` 和 `pub use auth::{AuthConfig, AuthResult, AuthType, Authenticator, Claims};`
- [ ] 2.7 删除 `src/http/static_files.rs`（仅在其自身测试中使用，生产中未使用）
- [ ] 2.8 从 `src/http/mod.rs` 中移除 `mod static_files;` 和 `pub use static_files::{StaticFileConfig, StaticFileService};`
- [ ] 2.9 验证 `cargo build` 模块移除后编译成功

## 3. 移除已使用模块中的死结构体和函数

- [ ] 3.1 从 `src/http/mod.rs` 中移除 `HttpHandler` 结构体和 `HttpHandler::read_body`（从未调用，仅测试使用）
- [ ] 3.2 从 `src/http/mod.rs` 中移除 `test_http_handler_creation` 测试
- [ ] 3.3 从 `src/http/body.rs` 中移除 `BodyTransformer` 结构体及其 `transform` 方法（仅测试使用）
- [ ] 3.4 从 `src/http/body.rs` 中移除 `read_json_body` 和 `write_json_body`（重导出但从未被导入）
- [ ] 3.5 从 `src/http/mod.rs` 中移除对应重导出：`pub use body::{read_json_body, write_json_body, BodyTransformer};`
- [ ] 3.6 从 `src/http/client.rs` 中移除 `HttpClientPool` 结构体及所有方法（仅在其自身测试中使用）
- [ ] 3.7 从 `src/http/client.rs` 中移除 `test_http_client_pool_creation` 测试
- [ ] 3.8 从 `src/http/mod.rs` 中移除 `HttpClientPool` 重导出
- [ ] 3.9 从 `src/http/handler.rs` 中移除 `create_handler` 函数（重导出但从未被导入）
- [ ] 3.10 从 `src/http/mod.rs` 中移除 `create_handler` 重导出
- [ ] 3.11 从 `src/proxy/mod.rs` 中移除 `Proxy` 结构体、`Proxy::new()` 和 `Default` impl（向后兼容桩）
- [ ] 3.12 从 `src/proxy/mod.rs` 中移除 `TcpProxy` 结构体及其方法（从未实例化）
- [ ] 3.13 从 `src/proxy/forward.rs` 中移除 `forward_tcp_to_tcp` 和 `forward_tcp_to_uds`（从未被外部调用）
- [ ] 3.14 从 `src/proxy/mod.rs` 中移除对应重导出
- [ ] 3.15 从 `src/proxy/unix.rs` 中移除 `UnixProxy`、`ProxyTarget`（仅测试使用）
- [ ] 3.16 从 `src/proxy/tcp.rs` 中移除 `TcpProxyListener::bind_addr`、`local_addr`、`inner`（从未被外部调用）
- [ ] 3.17 从 `src/mock/mod.rs` 中移除死类型：`MockBuilder`、`MockLocation`、`MockService`（仅测试使用）
- [ ] 3.18 更新 `src/lib.rs` 中的重导出：将 `pub use mock::{MockBuilder, MockLocation, MockResponse, MockService}` 改为 `pub use mock::MockResponse;`
- [ ] 3.19 从 `src/http/server.rs` 中移除 `HttpServer`、`HttpServerConfig`、`HttpProxyService`、`create_simple_server` 及其在 `src/http/mod.rs` 中的重导出（仅 server.rs 测试使用）
- [ ] 3.20 验证 `cargo build` 所有移除后编译成功

## 4. 移除遗留配置类型

- [ ] 4.1 从 `src/config/mod.rs` 中移除 `Config` 结构体、`Config::from_yaml_file`、`Config::from_yaml`（第 310-365 行）
- [ ] 4.2 从 `src/config/mod.rs` 中移除 `config::TlsConfig` 结构体（第 321 行，与 tls::TlsConfig 不同）
- [ ] 4.3 从 `src/config/mod.rs` 中移除 `RouteConfig` 结构体（第 330 行）
- [ ] 4.4 从 `src/config/mod.rs` 中移除 `MockConfig` 结构体（第 341 行）
- [ ] 4.5 移除未使用的配置字段：`LocationConfig::alias`、`LocationConfig::condition`、`CertConfig::root_key`、`MystiConfig::cert` 字段及 `CertConfig` 结构体
- [ ] 4.6 验证 `cargo build` 配置清理后编译成功

## 5. 移除未使用的错误变体

- [ ] 5.1 从 `src/error.rs` 中移除 `MystiProxyError::ConfigFileRead`（从未构造）
- [ ] 5.2 从 `src/error.rs` 中移除 `MystiProxyError::Auth`（从未构造）
- [ ] 5.3 从 `src/error.rs` 中移除 `MystiProxyError::Jwt`（从未构造）
- [ ] 5.4 验证 `cargo build` 错误变体移除后编译成功

## 6. 移除未使用的依赖

- [ ] 6.1 从 `Cargo.toml` 中移除 `jsonpath-rust = "0.5"`（源代码中零引用）
- [ ] 6.2 从 `Cargo.toml` 中移除 `anyhow = "1"`（源代码中零引用）
- [ ] 6.3 验证 `cargo build` 依赖移除后编译成功

## 7. 清理无效重导出和导入

- [ ] 7.1 从 `src/http/mod.rs` 中移除所有无效的 `pub use` 语句
- [ ] 7.2 移除仅因已删除代码而需要的无效 `use` 导入
- [ ] 7.3 从 `src/lib.rs` 中移除 `pub mod router;` 和 `pub mod tls;`（如模块已删除）
- [ ] 7.4 验证 `cargo build` 和 `cargo clippy --all-targets` 成功

## 8. 修复保留代码的 clippy 警告

- [ ] 8.1 在 `src/http/handler.rs` 中将冗余闭包 `|e| MystiProxyError::Http(e)` 替换为 `MystiProxyError::Http`（6 处）
- [ ] 8.2 在 `src/http/client.rs` 中替换冗余闭包（2 处）
- [ ] 8.3 在 `src/http/server.rs` 中替换冗余闭包（2 处）
- [ ] 8.4 在 `src/http/auth.rs` 中将手动 `impl Default for AuthResult` 替换为 `#[derive(Default)]` — 或如 auth 模块已删除则跳过
- [ ] 8.5 在 `src/http/auth.rs` 中将 `starts_with` + 切片替换为 `strip_prefix` — 或如 auth 模块已删除则跳过
- [ ] 8.6 在 `src/router/mod.rs` 中替换手动 strip 模式 — 或如 router 模块已删除则跳过
- [ ] 8.7 在 `src/router/mod.rs` 中替换手动 flatten 模式 — 或如 router 模块已删除则跳过
- [ ] 8.8 在 `src/mock/mod.rs` 第 469 行将 `unwrap_or_else` 替换为 `unwrap_or`
- [ ] 8.9 验证 `cargo clippy --all-targets` 零警告

## 9. 最终验证

- [ ] 9.1 运行 `cargo build --release` 并验证成功
- [ ] 9.2 运行 `cargo test --all` 并验证所有保留测试通过
- [ ] 9.3 运行 `cargo clippy --all-targets -- -W dead_code -W unused_imports` 并验证零警告
- [ ] 9.4 运行 `cargo fmt -- --check` 并验证格式正确
