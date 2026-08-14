# F2 冲突管理 API 对齐 — Task 规划

## 任务分解（TDD 顺序）

### T1 迁移
- [ ] `20260815000000_sync_conflicts.sql`（config_id PK + 两个 JSONB + detected_at）

### T2 ConflictRepository（先测后写）
- [ ] 单元：row↔record 转换、坏 JSON 容错、序列化形状
- [ ] 实现 upsert/find_by_config/find_all/delete
- [ ] 集成：真实 PG 全套 CRUD + upsert 覆盖语义

### T3 sync/push 登记冲突
- [ ] 修改 sync_push 并发分支，upsert 冲突
- [ ] 集成：push 并发版本后列表可见

### T4 handlers/conflicts.rs
- [ ] 单元：resolution 枚举、请求校验
- [ ] 实现 4 端点（list/get/resolve(PUT)/ignore(DELETE)）
- [ ] 挂到 create_protected_routes

### T5 旧端点兼容
- [ ] GET /sync/conflicts 改读新表
- [ ] POST /sync/conflicts/:id/resolve 改走同一解决逻辑

### T6 验证闭环
- [ ] 集成测试覆盖 spec 6 条验收
- [ ] `cargo test` 全绿、fmt/clippy 干净
- [ ] llvm-cov 新模块 ≥70%
- [ ] 真实运行逐条 curl 验收
- [ ] 前端 npm build 仍绿

### T7 推送与 CI
- [ ] 提交（feat(mysticentral): conflicts API）
- [ ] push、盯 Actions 至全绿
- [ ] 更新 OVERVIEW.md 接口清单、勾选 DEVELOPMENT.md

## 信心评估
全部组件为 F1 刚验证过的模式（迁移/仓储/JWT 路由/集成测试框架），整体信心 >92%，无需网络调研。
