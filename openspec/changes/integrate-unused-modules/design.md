## 背景

MystiProxy 在迭代开发中实现了完整的七层 HTTP 代理能力，但 `main.rs` 只使用 `ProxyServer` 做四层盲转发。`proxy_type` 字段被存储但从未检查。所有 `proxy_type: Http` 的引擎实际上也在做 TCP 管道转发，导致 HttpRequestHandler、HttpServer、Router、MockService、HeaderTransformer、BodyTransformer、Authenticator、StaticFileService、TLS 模块全部从未被调用。

## 目标 / 非目标

**目标：**
- 让 `proxy_type: Http` 的引擎走七层 HTTP 代理链路
- 接入 Router 模块（参数提取）和 StaticFileService（静态文件服务）
- 修复 Host header 重写问题和 ForceDelete bug
- 拆分超时语义（请求级 vs 连接级）
- 清理明确冗余的代码（engine.rs、UnixProxy、Proxy、TcpProxy、遗留配置类型）

**非目标：**
- 替换 handler.rs 内联的 header/mock/body 处理（后续批次）
- 接入认证（后续批次）
- 接入 TLS（需重新设计配置，后续批次）
- HttpClient 连接复用优化（后续批次）

## 决策

### D1：main.rs 按 proxy_type 分派
**决策**：`proxy_type == Tcp` 创建 `ProxyServer`，`proxy_type == Http` 创建 `HttpServer<HttpRequestHandler>`。
**理由**：这是所有问题的根因。ProxyServer 做四层转发，HttpServer 做七层 HTTP 处理，各自适用于不同场景。
**影响**：`proxy_type: Http` 的引擎行为从四层盲转发变为七层代理，现有配置行为变化显著。

### D2：Host header 智能重写
**决策**：HttpClient 转发请求时，默认将 `Host` header 重写为 target 地址。用户可通过全局 `header` 配置覆盖。
**理由**：大多数后端服务器检查 Host header，保持客户端原始 Host 会导致 404。这是 nginx、envoy 等代理的默认行为。
**实现**：在 `HttpClient::send_request()` 中，构建新请求后，如果 headers 中没有用户配置的 `Host`，则自动设为 target 地址。

### D3：Router 替代内联 match_route
**决策**：`HttpRequestHandler` 使用 `Router` 模块进行路由匹配，替代内联的 `match_route()` 方法。
**理由**：Router 支持命名参数提取（`{param}`），是内联版本不具备的能力。Router 已有完整的测试覆盖。
**实现**：`HttpRequestHandler::new()` 中创建 `Router` 实例，`call()` 中调用 `router.match_uri()`。`RouteMatch` 的结构需要适配 `MatchResult`。

### D4：StaticFileService 接入
**决策**：`ProviderType::Static` 分支调用 `StaticFileService::serve()`。`LocationConfig` 增加 `root: Option<String>` 字段指定文件根目录。
**理由**：当前 Static 分支错误地走 Mock 路径，返回空响应。StaticFileService 已实现完整的文件服务能力（MIME 检测、Range 请求、目录列表）。
**实现**：`RouteMatch` 增加 `Static` 变体，`call()` 中读取文件并返回。

### D5：超时拆分
**决策**：`EngineConfig.timeout` 拆分为 `request_timeout: Option<Duration>` 和 `connection_timeout: Option<Duration>`。
**理由**：HttpServer 的超时应作用于单次请求而非整个连接。ProxyServer 可复用 `connection_timeout`。
**兼容性**：旧 `timeout` 字段通过 `#[serde(alias)]` 保留读取能力，内部映射到 `request_timeout`。

### D6：删除 engine.rs
**决策**：删除 `src/engine.rs`。
**理由**：孤儿文件，未声明为模块，引用不存在的 `MystiEngine`，被 `HttpRequestHandler` 完全替代。

### D7：删除 UnixProxy、Proxy、TcpProxy
**决策**：从 `proxy/mod.rs` 中移除 `UnixProxy`、`Proxy`、`TcpProxy` 及相关代码。
**理由**：`ProxyServer` + `SocketStream` 已覆盖所有功能。`Proxy` 是空壳，`TcpProxy` 与 ProxyServer 重叠，`UnixProxy` 与 SocketStream 重叠。

### D8：删除遗留配置类型
**决策**：从 `config/mod.rs` 底部删除 `Config`、`TlsConfig`（旧版）、`RouteConfig`、`MockConfig` 及其 `from_yaml` 方法。
**理由**：标记为"向后兼容"但没有需要兼容的消费者，应用只使用 `MystiConfig`。

### D9：删除 LocationConfig.alias 和 condition
**决策**：从 `LocationConfig` 中移除 `alias` 和 `condition` 字段。
**理由**：从未使用。条件匹配能力将在后续批次通过 MockService 接入时重新设计。

### D10：删除 utils.rs
**决策**：删除 `src/utils.rs`。
**理由**：孤儿文件，未声明为模块，`fix_length` 从未被调用。

## 风险 / 权衡

- **[HttpServer + HttpRequestHandler 从未运行过]** → 理论上泛型约束完全匹配，但需要端到端测试验证。缓解措施：首批完成后立即进行功能测试。
- **[配置兼容性破坏]** → `timeout` 拆分为两个字段。缓解措施：使用 `#[serde(alias)]` 保持旧字段名的读取能力。
- **[行为变化]** → `proxy_type: Http` 的引擎从四层变为七层。缓解措施：这是正确的修复，不是退化。现有行为本就是错误的。
- **[Router 接入增加首批复杂度]** → 需要适配 `RouteMatch` 和 `MatchResult` 的差异。缓解措施：Router 已有完整测试，适配层简单。
