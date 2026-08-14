# F4 Prometheus 指标导出 — Proposal

## 需求深度理解

### 背景与问题

`mystiproxy/src/metrics.rs` 的 `MetricsManager` 已经定义了 5 个指标（http_requests_total、http_request_duration_seconds、tcp_connection_duration_seconds、errors_total、memory_usage_bytes）并在 `handler.rs`/`main.rs` 中被调用记录，但：

1. **指标从未注册**到任何 Registry（`Counter::with_opts` 只创建对象，没有 `registry.register()`），TextEncoder 无从编码它们
2. **`start_server()` 是空壳**——只打一行日志，9090 端口上没有 HTTP 服务（实测 `GET :9090/metrics` 无响应）
3. **每个 `HttpRequestHandler` 各自 new 一个 MetricsManager**（`handler.rs:102`），指标分散在多个未注册的实例里，即使有端点也聚不起来

docs/OVERVIEW.md 已将此记录为"成熟度缺口"：指标在进程内计数但无 HTTP 暴露。

### 深层需求分析

这是 Prometheus 官方 Rust 客户端（`prometheus` crate 0.13，项目已有依赖）最标准的用法补全：

- **Registry 模式**：指标需注册进 `Registry`，`TextEncoder` 才能产出 `# HELP/# TYPE` 格式的 exposition 文本
- **单例聚合**：进程级指标必须共享一个 Registry。main.rs 已有一个 MetricsManager，handler 应复用而非自建
- **指标维度**：现在 `record_http_request` 收了 method/path/status 参数却全丢弃（`_method`/`_path`/`_status`）。Prometheus 的正确姿势是给 Counter 加 label（method/status），path 高基数不适合做 label（会撑爆时间序列），只用于日志

### 用户价值

- `curl :9090/metrics` 输出标准 exposition 格式，Grafana/Prometheus 可直接抓取
- K8s 部署（chart/ 已有 ServiceMonitor 场景）的监控闭环
- Helm values 中的 metrics 端口配置从此有真实后端

### 成功标准

1. `GET :9090/metrics` 返回 200 + `text/plain`，包含全部 5 个指标（带 HELP/TYPE 头）
2. 经过若干代理请求后 `http_requests_total` 数值增长，且按 method/status 维度可区分
3. 指标注册幂等（重复启动不 panic）
4. 新代码覆盖率 ≥70%，workspace 测试全绿
5. Actions 全绿

### 范围

做：Registry 注册、`/metrics` HTTP 导出服务、指标 label（method/status）、handler 共享进程级实例、单元测试。
不做：histogram bucket 自定义（默认桶够用）、process 指标（procfs collector 是另一个 feature）、TLS on metrics 端口。

## 信心评估

- `prometheus` crate 的 Registry + TextEncoder + `encode_metrics` 是官方文档第一示例，且 crate 已在依赖中，**信心 96%**
- tiny HTTP server 用 hyper（已有）起独立 listener，与 main.rs Forward 引擎同模式，**信心 92%**
- CounterVec label 用法为库的一级 API，**信心 94%**
- 全部 >85%，无需网络调研。
