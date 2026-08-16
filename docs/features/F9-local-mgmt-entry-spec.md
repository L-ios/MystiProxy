# F9 本地管理 API 启用入口 — Spec

## 概述

为 mystiproxy 二进制提供启用 local-management 模块的配置入口，落地 FR-068（本地 REST API 同中心契约），并让中心的实例下发链路有真实对端。

## 配置

```yaml
mysti:
  engine:
    web:
      listen: tcp://0.0.0.0:8080
      target: tcp://10.0.0.1:80
      proxy_type: http
      management:
        listen: tcp://127.0.0.1:8081      # 必填（缺省视为未启用）
        db_path: ./mgmt.db                # 可选，默认 mystiproxy-mgmt.db
        central_url: http://central:8090  # 可选；设置即启用同步
        sync_interval: 30                 # 可选秒，默认 30
        enabled: true                     # 可选；false 强制关闭
```

生效条件：`enabled != false && listen 存在`。需要二进制以 `--features local-management` 编译。

## 行为

1. 本地 API：`GET /api/v1/health`、mock CRUD（含 batch）、`GET /sync/status`、`POST /sync/trigger`
2. SQLite 持久化：本地增删改重启不丢
3. 同步（配置 central_url 后）：后台向中心注册实例、按 interval 心跳、pull 中心变更；离线时操作入队重试
4. 无 management 段：零行为差异（不监听、不建库）

## 验收（真实运行实测）

1. `GET :8081/api/v1/health` → 200
2. `POST :8081/api/v1/mocks` → 201；重启进程后 `GET` 仍在（SQLite）
3. 中心 `/api/v1/instances` 列表出现该实例
4. 中心 `POST /instances/push-all` → 实例日志记录 trigger 调用
5. 无配置回归：不新增监听端口
6. workspace 全绿 + Actions 全绿
