# F5 入站 IP 过滤 — Spec（说明文档）

## 概述

为 mystiproxy 的 TCP 与 HTTP 引擎提供入站连接的 CIDR 级访问控制：`allow` 白名单与 `deny` 黑名单，deny 优先，未配置则不过滤。

## 配置

```yaml
mysti:
  engine:
    web:
      listen: tcp://0.0.0.0:8080
      target: tcp://10.0.0.1:80
      proxy_type: http
      allow:              # 可选；非空时仅这些网段可用
        - 10.0.0.0/8
        - 192.168.0.0/16
      deny:               # 可选；优先于 allow
        - 192.168.1.5/32
```

- 元素为 CIDR；无 `/n` 的 IPv4 视为 /32，IPv6 视为 /128
- 非法 CIDR → 启动失败并指出具体条目

## 判定语义

1. peer 命中任一 `deny` → 拒绝
2. `allow` 为空或未配置 → 放行
3. 命中任一 `allow` → 放行；否则拒绝
4. IPv4 与 IPv6 规则互不匹配（不做映射）
5. 拒绝 = 立即关闭连接 + `warn` 日志（含 peer 地址），不进入协议处理

## 作用范围

TCP 引擎（`proxy_type: tcp`）与 HTTP 引擎（`proxy_type: http`）的 accept 层；Forward 引擎与 TLS 握手同样经过该判定（连接先于协议）。

## 验收标准（真实运行实测）

1. 无 allow/deny → 既有功能回归通过（代理请求 200）
2. `allow: [10.0.0.0/8]` → 本机 127.x 被拒（连接关闭/重置），日志有拒绝记录
3. `allow: [127.0.0.0/8]` → 本机正常代理
4. `deny: [127.0.0.1/32]` → 本机被拒
5. 坏 CIDR 启动失败
6. cargo test 全绿，ip_filter 覆盖率 ≥70%，Actions 全绿
