# F3 设置 / 同步状态 / 实例推送 — Spec（说明文档）

## 概述

补齐前端剩余三组后端缺口：系统设置 CRUD、同步状态查询与手动触发、中心到实例的配置下发。

## 端点规格（JWT 保护）

### GET /api/v1/settings

```json
{
  "central_url": "",
  "sync_interval": 30,
  "log_level": "info",
  "max_request_history": 1000,
  "default_environment": null
}
```

### PUT /api/v1/settings（部分更新，全字段可选）

校验规则（违反 → 400 + 明确 message）：
- `central_url`：空串或必须 `http://` / `https://` 开头
- `sync_interval`：整数，5–3600（秒）
- `log_level`：`debug|info|warn|error`
- `max_request_history`：≥0
- `default_environment`：任意字符串或 null

成功返回更新后的完整对象。

### GET /api/v1/sync/status

```json
{
  "connected": true,              // 是否存在 90 秒内心跳的实例
  "last_sync_at": "…",            // 最近一次实例同步时间或 null
  "sync_in_progress": false,      // v1 恒 false（无长任务队列）
  "pending_changes": 2,           // 未解决冲突数
  "central_url": "http://…"       // 来自 settings
}
```

### POST /api/v1/sync（body 可选 `{force: bool}`）

对全部注册实例执行下发，返回：
```json
{
  "success": true,                // pushed > 0 且 failed == 0
  "synced_count": 2,
  "conflicts": [ …F2 冲突对象… ],
  "synced_at": "…"
}
```

### POST /api/v1/instances/:id/push

- 实例不存在 → 404
- 实例可达且其管理 API 返回 2xx → 200 `{"ok": true, "detail": "…"}`，实例 `last_sync_at` 更新、`sync_status=connected`
- 不可达 / 非 2xx → **502** `{"ok": false, "detail": "<错误原因>"}`，`sync_status=error`

### POST /api/v1/instances/push-all

逐实例并发执行上述逻辑，返回：
```json
{
  "results": [{"instance_id": "…", "ok": true, "detail": "…"}],
  "pushed": 1,
  "failed": 1
}
```

## 下发协议

中心 → `POST {instance.endpoint_url}/api/v1/sync/trigger`，超时 3 秒。该端点是 mystiproxy `local-management` feature 已实现的本地管理 API（`management/handlers.rs`）。

## 数据模型

`system_settings` 单行表（BOOLEAN PK + CHECK 恒真），迁移时插入默认行。幂等（ON CONFLICT DO NOTHING）。

## 验收标准（真实运行实测）

1. settings 默认值可读；PUT 全字段更新后回读一致；三种非法输入均 400
2. /sync/status 空库 connected=false；注册实例+心跳后 true；pending_changes 随冲突数变化
3. 对监听中的假实例 push → 200 且实例行更新
4. 对死端口 push → 502 且 sync_status=error
5. push-all 混合结果汇总正确；POST /sync 返回四字段契约
6. cargo test 全绿 + 新模块覆盖率 ≥70% + Actions 全绿
