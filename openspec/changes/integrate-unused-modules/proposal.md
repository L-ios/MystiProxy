## 动机

MystiProxy 实现了完整的七层 HTTP 代理能力（路由、Mock、Header 变换、Body 变换、静态文件、认证、TLS），但 `main.rs` 对所有引擎配置都使用 `ProxyServer` 做四层盲转发，`proxy_type` 字段被存储但从未检查。导致约 60-70% 的已实现代码从未被调用。

本变更将这些已实现的模块接入核心业务流程，使 HTTP 代理功能真正工作。

## 变更内容

### 首批（本次变更）

- **核心链路打通**：`main.rs` 根据 `proxy_type` 分派——`Tcp` 用 `ProxyServer`（四层），`Http` 用 `HttpServer<HttpRequestHandler>`（七层）
- **Host header 智能重写**：默认将 `Host` 重写为 target 地址，用户可通过全局 `header` 配置覆盖
- **Router 接入**：用 `Router` 模块替代 handler.rs 内联的 `match_route()`，启用参数提取能力
- **StaticFileService 接入**：`ProviderType::Static` 分支调用 `StaticFileService::serve()`，`LocationConfig` 增加 `root` 字段
- **超时语义修正**：`EngineConfig.timeout` 拆分为 `request_timeout`（单次请求）和 `connection_timeout`（连接存活）
- **清理冗余代码**：删除 `engine.rs`、`UnixProxy`、`Proxy`、`TcpProxy`、遗留配置类型、`LocationConfig.alias`/`condition`

### 延后（后续批次）

- HeaderTransformer 替代内联 header 处理
- BodyTransformer 接入（JSON body 变换）
- MockService 替代内联 `build_mock_response()`
- Authenticator 认证接入
- TLS 支持接入（需重新设计 CertConfig）
- HttpClient 连接复用

## 能力变更

### 新增能力

- **HTTP 七层代理**：`proxy_type: Http` 的引擎将解析 HTTP 协议，支持路由匹配、请求修改、Mock 响应
- **路由参数提取**：通过 `Router` 模块支持 `{param}` 命名捕获组
- **静态文件服务**：`ProviderType::Static` 可正确返回文件内容

### 修改的能力

- **超时行为**：从单一 `timeout` 拆分为 `request_timeout` + `connection_timeout`，语义更精确

## 影响

- **源文件**：`main.rs`、`proxy/mod.rs`、`http/handler.rs`、`http/server.rs`、`config/mod.rs`、`lib.rs` 等约 10+ 个文件
- **配置兼容性**：`timeout` 字段拆分为两个字段，现有 YAML 配置需更新。使用 `#[serde(default)]` 保持向后兼容
- **公共 API**：删除冗余的 `pub` 类型（Proxy、TcpProxy、UnixProxy、旧 Config 系列等）
- **二进制行为**：`proxy_type: Http` 的引擎从四层盲转发变为七层 HTTP 代理，行为变化显著
