# F3 设置 / 同步状态 / 实例推送 — Proposal

## 需求深度理解

### 背景与问题

前端还有三组调用无后端支撑（实测 404/405）：

1. **设置页**（`SettingsPage.tsx`）：`GET/PUT /settings`，字段 `central_url/sync_interval/log_level/max_request_history/default_environment`
2. **同步状态徽章**（`SyncStatus.tsx`，30s 轮询）：`GET /sync/status`（connected/last_sync_at/sync_in_progress/pending_changes/central_url）+ `POST /sync`（手动触发全量同步）
3. **实例配置推送**：`POST /instances/:id/push`（单实例）、`POST /instances/push-all`（全部）

### 深层需求分析

这三个端点共享一个核心问题：**mysticentral 是中心端，但这些设置描述的是"本 mysticentral 服务"的行为**。逐一分析语义：

- `SystemSettings.central_url`：前端把它当作"中心对外可达地址"。中心自己是知道 `server.addr` 的，但 settings 是**可编辑的运行时配置**——存在数据库比环境变量更符合 UI 语义（改了立即生效、无需重启）。
- `/sync/status`：中心视角 = 数据库统计（mock 总数、活动实例数、最近同步记录）+ 是否有实例在线（心跳新鲜度）。`pending_changes` 可映射为未解决冲突数（F2 刚建的表）。
- `POST /sync`：中心端"触发同步"的合理语义 = 触发**对所有已注册实例的配置下发尝试**。中心无法反向连接 NAT 后的实例，但 `endpoint_url` 是实例自报的可达地址（F1 验证过注册必填）——中心可以 HTTP POST 到实例的本地管理 API（`/api/v1/sync/trigger`，mystiproxy local-management 已实现）。这就是 `instances/:id/push` 的实现路径，`push-all` 是其循环 + `POST /sync` 的聚合版本。

### 用户价值

- 设置页可保存生效（当前是摆设）
- 顶栏同步状态实时可见（connected、待处理冲突数）
- 管理员可一键向实例推送最新配置（打通中心→实例的下行链路，此前只有实例→中心上行）

### 成功标准

1. `GET /settings` 返回完整字段；`PUT` 校验并持久化，非法值 400
2. `GET /sync/status` 返回五字段契约（pending_changes=未解决冲突数）
3. `POST /instances/:id/push` 对可达实例返回 200 并记录推送；不可达实例返回 502
4. `POST /instances/push-all` 返回每实例结果汇总
5. `POST /sync` 语义等同 push-all，返回 `{success, synced_count, conflicts, synced_at}`
6. 覆盖率 ≥70%，Actions 全绿

### 范围

做：`system_settings` 表（单行）、`SettingsRepository`、3 组 handlers、HTTP 下发客户端（reqwest 已在依赖中）。
不做：设置热更新触发实际行为变化（如 log_level 动态调级）、实例认证握手（下发请求暂不带实例 api_key，后续立项）。

## 信心评估

- 单行 settings 表 + 仓储：F1/F2 已两次验证该模式，信心 95%
- 下发到实例：`endpoint_url` 在 F1 实测中确认必填且格式为 URL；reqwest POST JSON 是标准操作，信心 88%
- push 不可达返回 502：HTTP 语义标准（Bad Gateway 表示上游不可达），信心 90%
- 全部 >85%，无需网络调研。
