# F4 Prometheus 指标导出 — Task 规划

## 任务分解（TDD 顺序）

### T1 指标重构（先测后写）
- [ ] 单元测试：gather 含 5 指标与 HELP/TYPE；label 计数；累加；重复 new 幂等；histogram bucket；gauge
- [ ] 实现：Registry 注册、IntCounterVec(method,status)、gather()

### T2 导出服务
- [ ] 实现 start_server（hyper，GET /metrics → text exposition，404 其他）
- [ ] 集成测试：真实端口 200/内容/404、计数增长

### T3 接线
- [ ] main.rs：Arc 共享 + spawn 服务
- [ ] handler.rs：record_http_request 传真实 method/status

### T4 验证闭环
- [ ] 真实运行 curl 验收（含代理流量后计数增长）
- [ ] llvm-cov metrics 模块 ≥70%
- [ ] workspace 全绿、fmt/clippy

### T5 推送 CI
- [ ] 提交 push、盯 Actions 全绿
- [ ] 更新 OVERVIEW.md 成熟度说明（移除"未实现"标注）

## 信心评估
官方 prometheus crate 标准用法（Registry/TextEncoder/service_fn 均为库文档示例），crate 已在依赖中。整体 92-96%，无需网络调研。
