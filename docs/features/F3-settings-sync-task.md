# F3 设置 / 同步状态 / 实例推送 — Task 规划

## 任务分解（TDD 顺序）

### T1 迁移 + SettingsRepository
- [ ] `20260816000000_system_settings.sql`（单行表 + 默认行）
- [ ] 单元：SettingsPatch 校验矩阵（先写测试）
- [ ] 实现 get/update

### T2 settings handlers
- [ ] GET/PUT /api/v1/settings，挂 protected 路由
- [ ] 集成：默认值→PUT→回读；三种非法 400

### T3 push_service
- [ ] `push_to_instance`（reqwest 3s 超时 + 状态回写）
- [ ] `push_to_all`（并发循环）
- [ ] 单元：PushOutcome 形状

### T4 instances push handlers
- [ ] POST /instances/:id/push（404/200/502）
- [ ] POST /instances/push-all（注意路由注册顺序在 :id 之前）
- [ ] 集成：真 listener 成功 / 死端口 502 / 混合汇总

### T5 sync/status + POST /sync
- [ ] 聚合查询（心跳新鲜度、冲突计数、settings.central_url）
- [ ] POST /sync 复用 push_to_all，包装四字段契约
- [ ] 集成：空库/有心跳/冲突数变化

### T6 验证闭环
- [ ] 全部集成过、workspace 全绿、fmt/clippy 干净
- [ ] llvm-cov 新模块 ≥70%
- [ ] 真实运行 curl 逐条验收

### T7 推送 CI
- [ ] 提交 push、盯 Actions 全绿
- [ ] 更新 OVERVIEW.md 清单与 DEVELOPMENT.md

## 信心评估
settings/仓储/路由 = F1/F2 已验证模式（95%）；reqwest 下发 + 假实例 listener 集成测试 = 标准做法（88%）；全部达标无需调研。
