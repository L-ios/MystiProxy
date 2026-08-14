# F5 入站 IP 过滤 — Task 规划

## 任务分解（TDD 顺序）

### T1 IpFilter 核心（先测后写）
- [ ] 单元：CIDR 解析/匹配/语义矩阵/from_config
- [ ] 实现 ip_filter.rs

### T2 配置字段
- [ ] EngineConfig::allow/deny（serde default None）
- [ ] 配置解析测试含新字段

### T3 引擎接入
- [ ] proxy/mod.rs accept 判定 + 构造时解析
- [ ] http/server.rs 同模式
- [ ] main.rs 启动失败传播

### T4 验证闭环
- [ ] 集成：allow 命中/未命中、deny 精确、无配置回归、坏 CIDR
- [ ] llvm-cov ip_filter ≥70%
- [ ] workspace 全绿、fmt/clippy

### T5 推送 CI
- [ ] 提交 push、盯 Actions 全绿
- [ ] 勾选 DEVELOPMENT.md "根据请求的 IP 过滤请求"、更新 OVERVIEW 功能清单

## 信心评估
CIDR 位运算是教科书算法；accept 层接入两处各数行；配置模式与现有字段一致。整体 90-95%，无需网络调研。
