# F9 mystiproxy 本地管理 API 启用入口 — Proposal

## 需求深度理解

### 背景与问题

specs/001-mock-management 的 FR-068 明确要求 MystiProxy 提供本地 REST API（与中心同契约）。代码侧 `management` 模块（feature `local-management`）完整实现了 SQLite 仓储 + `/api/v1/health`、`/mocks`(+batch)、`/sync/status`、`/sync/trigger` 六组端点 + 同步客户端 + 离线队列——但审计确认：

- **main.rs 没有任何 management 引用**：feature 开了也只是编进了 lib，二进制永远不暴露该 API
- 无 CLI 参数、无 YAML 配置字段能打开它
- F3 做的中心→实例下发（`POST {endpoint}/api/v1/sync/trigger`）依赖实例端这个 API 存在——**当前没有任何真实 mystiproxy 进程能响应它**，整条下行链路在真实部署中是断的

### 深层需求

双层架构（spec 澄清第 2 条）的"本地层"必须可部署。F9 打通：
1. 引擎配置新增 `management` 段（listen 地址 + sync 中心地址 + db 路径）
2. main.rs 按配置启动 LocalManagement 的 axum router（独立监听端口）
3. 启用 sync 时实例自动向中心注册（心跳），中心的 push-all 从此有真实对端

### 成功标准（真实运行端到端实测）

1. 带 management 配置的 mystiproxy 启动后 `GET :mgmt/api/v1/health` → 200
2. 本地创建 mock（POST mgmt /mocks）→ SQLite 持久化，重启后仍在
3. 中心 push-all → 实例 `/sync/trigger` 被真实调用（实例日志记录）
4. 实例注册出现在中心 `/api/v1/instances` 列表
5. 无 management 配置时行为与现状零差异
6. 覆盖率新逻辑 ≥70%，Actions 全绿

### 范围

做：EngineConfig.management 字段、main.rs 接线（feature 门控）、CLI 快捷参数、e2e（真实实例 + 真实中心全链路）。
不做：本地 Web UI（FR-067，前端另有本地模式设计）、热重载管理配置、多 management 实例。

## 信心评估
- LocalManagement::create_router/start_sync API 现成（lib 单测覆盖 181 个）：**信心 93%**
- main.rs spawn axum server：F4 metrics 同模式：**信心 95%**
- 注册/心跳链路：SyncClient 内已实现（F1 时期读过）：**信心 90%**
- 全部 >85%，无需网络调研。
