# 功能列表

## 已实现功能

### TCP 代理 (4 层)

TCP 代理工作在传输层，将客户端的 TCP 连接直接转发到目标服务器。支持 Unix 域套接字作为源或目标地址。超时机制同时应用于连接建立和数据传输阶段，并在日志中输出双向流量统计（字节数和包数）。

### HTTP 代理 (7 层)

HTTP 代理工作在应用层，处理完整的请求和响应生命周期。请求首先经过 HttpRequestHandler 进行预处理，然后通过 HttpClient 建立的连接池发送到目标服务器。代理具备智能 Host 头部重写功能，确保转发请求时 Host 头与目标地址一致。

### 路由匹配

支持四种路由模式以满足不同场景需求。Full 模式要求请求路径与配置路径完全一致；Prefix 模式匹配路径前缀；Regex 模式使用正则表达式进行灵活匹配；PrefixRegex 则结合前缀匹配和正则替换，可对匹配到的路径部分进行转换。

### 提供者类型

每个路由可以配置不同的提供者类型。proxy 类型将请求转发到指定的目标服务器；mock 类型直接返回预设的响应内容，用于测试或模拟；static 类型从本地文件系统读取文件并返回给客户端。

### 请求改写

代理支持对转发的请求进行多方面修改。可调整 HTTP 方法（如将 GET 改为 POST）；可重写 URI 的路径和查询参数；可新增、覆盖或删除请求头。对于配置中未指定的请求头，默认会转发到目标服务器。

### Mock 响应

mock 提供者允许为每个 location 定义独立的响应状态码和响应头。状态码可以是非标准的 4xx 或 5xx 系列；响应头可以包含自定义的 Content-Type、Cache-Control 等任意字段。

### 静态文件服务

static 提供者从配置的根目录读取文件并返回给客户端。根据文件扩展名自动检测 MIME 类型，同时支持 HTTP Range 请求，允许客户端进行断点续传和多段下载。

### 超时配置

支持为每个引擎独立配置超时时间。connection_timeout 控制与目标服务器建立连接的时间上限；request_timeout 控制单个请求的总耗时。配置中的 timeout 字段作为 connection_timeout 的向后兼容别名仍然可用。

### 多引擎支持

可以在同一个配置文件中定义多个引擎，每个引擎拥有独立的监听地址、目标地址和代理类型。各引擎独立运行，互不干扰，便于在一台机器上同时提供多种代理服务。

## 额外扩展 / 实验性能力

### TLS 认证

TLS 相关代码位于 src/tls/ 目录，支持单向和双向 TLS 认证。TLS 可通过引擎配置中的 `tls` 字段启用（HTTP 引擎已支持），证书热重载目前仅在 mysticentral 的 legacy-tls 模块中实现。

### HTTP 鉴权

HTTP 认证功能实现于 src/http/auth.rs，支持基于请求头的认证方式和 JWT Token 验证。认证逻辑已接入 HTTP Handler 的处理流程（见 handler.rs 中的 authenticate 调用），可通过引擎配置的 `auth` 字段启用。

### 请求体 JSON 转换

BodyTransformer 实现了基于 JSONPath 的请求体转换功能，可以对 JSON 请求体进行部分字段的提取、修改或删除。已接入代理请求链路（配置 location.request.body 后生效），实测可用。

### 连接池

HttpClientPool 提供了 HTTP 连接池管理能力，可以复用连接以提升性能。该模块已接入主请求处理流程（运行日志可见 "Created new HTTP client"）。

### 性能监控（部分实现）

metrics.rs 中的 MetricsManager 在进程内统计请求数/耗时/错误数，但 Prometheus 指标导出端点（127.0.0.1:9090/metrics）目前为占位实现，`start_server()` 仅记录日志，实际未暴露任何指标数据。