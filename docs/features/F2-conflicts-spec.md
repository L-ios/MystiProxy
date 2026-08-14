# F2 冲突管理 API 对齐 — Spec（说明文档）

## 概述

为 mysticentral 建立持久化冲突队列：同步推送产生的并发修改登记入库，前端冲突解决页可列表、查看、解决或忽略。4 个新端点按前端契约提供。

## 数据模型

`sync_conflicts` 表（每配置至多一条，upsert 覆盖）：

| 列 | 类型 | 说明 |
|----|------|------|
| config_id | UUID PK | 冲突的 Mock 配置 id |
| local_version | JSONB | 实例推送的版本（完整 MockConfiguration） |
| central_version | JSONB | 中心当前版本 |
| detected_at | TIMESTAMPTZ | 最近一次检测到冲突的时间 |

## 端点规格（全部需 JWT）

### GET /api/v1/conflicts

```json
{
  "data": [
    {
      "config_id": "uuid",
      "local_version": { …MockConfiguration… },
      "central_version": { …MockConfiguration… },
      "detected_at": "2026-08-15T10:00:00Z"
    }
  ],
  "total": 1
}
```

### GET /api/v1/conflicts/:configId

单个上述对象；不存在返回 404。

### PUT /api/v1/conflicts/:configId/resolve

请求体：
```json
{
  "resolution": "keep_local" | "keep_central" | "merge",
  "merged_config": { …MockConfiguration… }   // 仅 merge 必填
}
```

行为：
- `keep_local`：保存 local_version 覆盖中心，version_vector 递增
- `keep_central`：中心配置不动
- `merge`：保存 merged_config，其 version_vector 由调用方合并双侧后提供；服务端校验 merged_config.id 必须等于 configId，否则 400
- 三者成功后删除冲突记录，返回 200 + 解决后的配置
- resolution 非法 → 400；merge 缺 merged_config → 400；冲突不存在 → 404

### DELETE /api/v1/conflicts/:configId

忽略冲突：仅删除记录，中心配置保持不动（放弃 local 版）。返回 204。不存在返回 404。

## 冲突产生源

`POST /api/v1/sync/push` 中检测到 `is_concurrent_with` 时，upsert 一条冲突记录（响应体中的 `conflicts` 数组保持不变，向后兼容）。

## 兼容性

旧端点保留：`GET /sync/conflicts`（现改为返回真实数据）、`POST /sync/conflicts/:id/resolve`（strategy 参数版）。

## 验收标准（真实运行实测）

1. push 并发版本后 `GET /conflicts` 返回该冲突（total=1，两版内容正确）
2. 同一配置重复 push 冲突 → 仍 1 条（upsert）
3. resolve keep_local → 配置名变 local 版、冲突消失
4. resolve merge 缺 merged_config → 400；带完整配置 → 合并保存
5. DELETE → 204，配置保持 central 版
6. `cargo test` 全绿 + 新模块覆盖率 ≥70% + Actions 全绿
