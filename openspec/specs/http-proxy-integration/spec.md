## 修改的需求

### 需求：proxy_type 字段仅用于存储
**原因**：`ProxyServer` 对所有引擎都执行四层盲转发，`proxy_type` 从未被检查。
**迁移方案**：`main.rs` 根据 `proxy_type` 分派到 `ProxyServer`（Tcp）或 `HttpServer<HttpRequestHandler>`（Http）。

### 需求：ProviderType::Static 等同于 Mock
**原因**：handler.rs 中 Static 分支调用 `build_mock_response()`，返回空响应。
**迁移方案**：调用 `StaticFileService::serve()`，返回真实文件内容。

### 需求：单一 timeout 字段
**原因**：`EngineConfig.timeout` 语义模糊，在 HttpServer 中作用于整个连接而非单次请求。
**迁移方案**：拆分为 `request_timeout`（单次请求处理时间）和 `connection_timeout`（连接最大存活时间）。旧字段通过 `#[serde(alias)]` 保持读取兼容。

### 需求：Host header 保持客户端原始值
**原因**：HttpClient 转发请求时不重写 Host，上游服务器可能 404。
**迁移方案**：默认将 Host 重写为 target 地址，用户可通过全局 header 配置覆盖。

## 新增需求

### 需求：proxy_type 为 Http 时必须走七层代理
当 `EngineConfig.proxy_type == ProxyType::Http` 时，`main.rs` 必须创建 `HttpServer<HttpRequestHandler>` 而非 `ProxyServer`。

#### 场景：HTTP 代理引擎正确解析请求
- **当** 配置 `proxy_type: Http` 的引擎收到 HTTP 请求时
- **则** 请求经过路由匹配、header 修改、代理转发/Mock 响应

#### 场景：TCP 代理引擎保持四层转发
- **当** 配置 `proxy_type: Tcp` 的引擎收到连接时
- **则** 执行双向字节流转发，不解析 HTTP 协议

### 需求：路由支持参数提取
`Router` 模块替代内联 `match_route()`，支持 `{param}` 命名捕获组。

#### 场景：路径参数正确提取
- **当** 路由模式为 `/users/{id}`，请求路径为 `/users/123` 时
- **则** 参数 `id = "123"` 被正确提取

### 需求：静态文件服务正确工作
`ProviderType::Static` 的 location 必须返回指定根目录下的文件内容。

#### 场景：静态文件请求返回文件内容
- **当** 请求路径匹配 Static 类型的 location，且 `root` 目录下存在对应文件时
- **则** 返回文件内容，Content-Type 根据 MIME 类型自动设置

#### 场景：文件不存在返回 404
- **当** 请求路径对应的文件不存在时
- **则** 返回 404 Not Found

### 需求：超时语义精确
- `request_timeout`：限制单次 HTTP 请求的处理时间
- `connection_timeout`：限制 TCP/HTTP 连接的最大存活时间

#### 场景：请求超时
- **当** 单次 HTTP 请求处理超过 `request_timeout` 时
- **则** 返回 504 Gateway Timeout

#### 场景：连接超时
- **当** TCP 连接存活超过 `connection_timeout` 时
- **则** 关闭连接

### 需求：构建和测试通过
- `cargo build` 成功
- `cargo test --all` 所有测试通过
- `cargo clippy --all-targets` 零警告
