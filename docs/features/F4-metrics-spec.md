# F4 Prometheus 指标导出 — Spec（说明文档）

## 概述

让 mystiproxy 的指标真正可被抓取：注册到 Registry、在 9090 端口提供 `/metrics` exposition 端点、为请求计数增加 method/status 维度。

## 端点规格

### GET http://127.0.0.1:9090/metrics

- 200 `text/plain; version=0.0.4`
- 响应为 Prometheus exposition 格式（`# HELP` / `# TYPE` + 样本行）
- 指标清单：

| 指标 | 类型 | 标签 | 说明 |
|------|------|------|------|
| `http_requests_total` | counter | method, status | HTTP 请求计数 |
| `http_request_duration_seconds` | histogram | - | 请求耗时（默认桶） |
| `tcp_connection_duration_seconds` | histogram | - | TCP 连接时长 |
| `errors_total` | counter | - | 错误计数 |
| `memory_usage_bytes` | gauge | - | 内存使用 |

- 其他路径 → 404

### 配置

端口沿用 main.rs 现有硬编码 `127.0.0.1:9090`（后续可提为配置项，不在本 feature 范围）。

## 行为语义

1. **聚合**：main 与各 engine handler 共享同一进程级 Registry（prometheus 全局 default registry），所有引擎的请求计入同一组指标
2. **标签**：`http_requests_total{method="GET",status="200"}` 按 method+status 组合分别计数；path 不做标签（高基数）
3. **幂等**：重复初始化不 panic（AlreadyReg 错误忽略）
4. **服务生命周期**：随进程存续；`stop_server` 保留 API（当前无优雅停机场景，仅日志）

## 验收标准（真实运行实测）

1. 启动 mystiproxy 后 `curl -s :9090/metrics` 返回 200 与 exposition 文本，含 5 个指标的 HELP/TYPE
2. `curl :8080/...` 发起 3 次代理请求后，`http_requests_total` 相应 label 行数值 ≥3
3. GET/POST 或 200/404 产生可区分的 label 行
4. `GET :9090/other` → 404
5. `cargo test` 全绿，metrics 模块覆盖率 ≥70%
6. Actions 全绿
