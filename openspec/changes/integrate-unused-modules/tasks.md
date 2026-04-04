## 1. 清理冗余代码

- [ ] 1.1 删除 `src/engine.rs`（孤儿文件，引用不存在的 MystiEngine）
- [ ] 1.2 删除 `src/utils.rs`（孤儿文件，fix_length 从未调用）
- [ ] 1.3 从 `src/proxy/mod.rs` 中移除 `Proxy` 结构体、`TcpProxy` 结构体及其所有方法和测试
- [ ] 1.4 从 `src/proxy/mod.rs` 中移除对 `forward_tcp_to_tcp`、`forward_tcp_to_uds` 的重导出（仅保留被 ProxyServer 使用的函数）
- [ ] 1.5 从 `src/config/mod.rs` 底部删除 `Config`、`TlsConfig`（旧版）、`RouteConfig`、`MockConfig` 及其 `from_yaml` 方法
- [ ] 1.6 从 `LocationConfig` 中移除 `alias` 和 `condition` 字段
- [ ] 1.7 验证 `cargo build` 编译通过

## 2. 配置结构变更

- [ ] 2.1 将 `EngineConfig.timeout` 拆分为 `request_timeout` 和 `connection_timeout`，均使用 `#[serde(default)]` + `#[serde(alias = "timeout")]` 保持向后兼容
- [ ] 2.2 在 `LocationConfig` 上添加 `root: Option<String>` 字段，用于静态文件服务指定根目录
- [ ] 2.3 更新 `main.rs` 中命令行构造 `EngineConfig` 的代码，适配新字段名
- [ ] 2.4 更新所有测试中的 `EngineConfig` 构造，适配新字段名
- [ ] 2.5 验证 `cargo test` 全部通过

## 3. 核心链路打通——main.rs 分派

- [ ] 3.1 在 `src/lib.rs` 中添加 `pub use http::server::{HttpServer, HttpServerConfig};`（确保外部可用）
- [ ] 3.2 修改 `main.rs` 的引擎启动循环：根据 `engine_config.proxy_type` 分派
  - `ProxyType::Tcp` → 创建 `ProxyServer`（保持现有行为）
  - `ProxyType::Http` → 创建 `HttpServer<HttpRequestHandler>`
- [ ] 3.3 为 `HttpServer` 实现与 `ProxyServer` 一致的接口模式：`from_engine_config()` + `start()` + `run()`
- [ ] 3.4 验证 `cargo build` 编译通过

## 4. Router 接入

- [ ] 4.1 在 `HttpRequestHandler::new()` 中创建 `Router` 实例，将 `Vec<RouteRule>` 替换为 `Router`
- [ ] 4.2 在 `Service::call()` 中用 `router.match_uri()` 替代 `self.match_route()`
- [ ] 4.3 适配 `MatchResult` → `RouteMatch` 的转换逻辑
- [ ] 4.4 移除内联的 `RouteRule` 结构体和 `match_route()` 方法
- [ ] 4.5 验证 `cargo test` 路由相关测试通过

## 5. StaticFileService 接入

- [ ] 5.1 在 `RouteMatch` 枚举中添加 `Static { root: String, path: String }` 变体
- [ ] 5.2 在 Router 匹配逻辑中，当 `ProviderType::Static` 时返回 `RouteMatch::Static`
- [ ] 5.3 在 `Service::call()` 中添加 `RouteMatch::Static` 分支，调用 `StaticFileService::serve()`
- [ ] 5.4 验证 `cargo build` 编译通过

## 6. Host header 智能重写

- [ ] 6.1 在 `HttpClient::send_request()` 中，构建新请求后检查是否已有用户配置的 `Host` header
- [ ] 6.2 如果没有用户配置的 `Host`，自动将 `Host` 设为 target 地址（从 `self.target` 提取 host:port）
- [ ] 6.3 验证 `cargo build` 编译通过

## 7. 超时语义修正

- [ ] 7.1 修改 `HttpServer::handle_connection()`：将 `connection_timeout` 用于整个连接的生命周期
- [ ] 7.2 修改 `HttpRequestHandler::call()`：将 `request_timeout` 用于单次请求处理
- [ ] 7.3 修改 `ProxyServer::handle_connection()`：使用 `connection_timeout`（保持现有行为）
- [ ] 7.4 验证 `cargo build` 编译通过

## 8. 最终验证

- [ ] 8.1 运行 `cargo build --release` 并验证成功
- [ ] 8.2 运行 `cargo test --all` 并验证所有测试通过
- [ ] 8.3 运行 `cargo clippy --all-targets` 并验证零警告
- [ ] 8.4 运行 `cargo fmt -- --check` 并验证格式正确
