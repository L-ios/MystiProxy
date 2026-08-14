# F2 冲突管理 API 对齐 — Proposal

## 需求深度理解

### 背景与问题

前端冲突解决页（`ConflictsPage.tsx` + `api/conflicts.ts`）调用 4 个端点，全部 404：

- `GET /conflicts`（列表）
- `GET /conflicts/:configId`（详情）
- `PUT /conflicts/:configId/resolve`（解决）
- `DELETE /conflicts/:configId`（忽略）

后端现有的冲突能力是 `/api/v1/sync/conflicts`（GET，返回**硬编码空列表**）和 `/api/v1/sync/conflicts/:id/resolve`（POST，strategy 参数）。**路径不一致 + 方法不一致 + 响应结构不一致**，三重错位。

### 深层需求分析

这不是简单的"加路由"，而是要回答：**冲突状态应该存在哪里、活多久？**

当前实现中冲突是**瞬态**的——只在 `sync/push` 的响应体里出现过一次，没有持久化。前端却有完整的"冲突列表页 + 解决流程 UI"，说明产品预期是**持久化冲突队列**：push 检测到并发修改时不直接丢弃，而是登记到 `sync_conflicts` 表，等人工在 UI 上解决。

数据库迁移文件里已经有 `sync_records` 表但没有冲突专用表。因此本功能包含：**冲突登记（写入）+ 冲突查询 + 冲突解决（含既有 resolve 逻辑迁移）+ 冲突忽略（删除）**。

### 用户价值

- 同步产生的并发修改不再静默冲突，可在 UI 里看见两版差异并选择
- 团队协作时多实例编辑同一 Mock 的场景得到闭环处理

### 前端契约（已实测确认的调用形态）

| 前端调用 | 期望响应 |
|---------|---------|
| `GET /conflicts` | `{data: ConflictResponse[], total: N}`，ConflictResponse = `{config_id, local_version, central_version, detected_at}` |
| `GET /conflicts/:id` | 单个 ConflictResponse |
| `PUT /conflicts/:id/resolve` | body `{resolution: 'keep_local'\|'keep_central'\|'merge', merged_config?}` |
| `DELETE /conflicts/:id` | 204（忽略，保留两版不合并） |

注意前端 resolve 用 **PUT** 且字段名是 `resolution`（后端旧端点是 POST + `strategy`）。

### 成功标准

1. sync/push 检测到并发修改时写入冲突表，不再只返回瞬态响应
2. 4 个前端端点按上述契约全部可用（真实运行实测）
3. 解决后冲突从列表消失；keep_local/keep_central/merge 三策略正确生效（复用既有向量时钟逻辑）
4. DELETE（忽略）只删冲突记录不动配置
5. 新增代码覆盖率 ≥70%
6. Actions 全绿

### 范围

做：`sync_conflicts` 表迁移、`ConflictRepository`、`sync/push` 登记冲突、4 个新 handler、既有 `/sync/conflicts` 旧端点保持兼容。
不做：WebSocket 实时推送冲突（代码库有 websocket.rs 骨架，另行立项）、自动合并算法（merge 策略沿用现有字段覆盖式合并）。

## 技术信心评估

- 冲突登记进 `sync/push`：改动点集中（routes.rs sync_push 函数），信心 92%
- 表结构与 repository：沿用 `UserRepository` 模式（刚在 F1 验证过），信心 95%
- 三策略解决：既有 `resolve_conflict` 逻辑已实测正确（keep_local 向量合并），只需搬迁+参数适配，信心 93%
- 无需网络调研，全部为库内已验证模式。
