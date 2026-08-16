# F8 WebSocket 真正代理 — Proposal

## 需求深度理解

### 背景与问题

复检实锤：WebSocket 升级请求经代理得到 101 + 合法 `Sec-WebSocket-Accept`，但**上游服务器零请求记录**——`handle_websocket_upgrade`（websocket.rs:24）是本地假握手：算完 accept key 直接回 101，从不连接 target，连接随即悬死。依赖里的 `tokio-tungstenite`/`tungstenite` 在全仓库零调用。

### 深层需求分析

这不是"锦上添花"：7 层代理声称支持 WebSocket（docs/features.md 亦然），而所有 WS 客户端连上后会**永久挂死收不到任何数据**——比 502 更糟的静默失败。mock/代理混布场景下，WS 流量被路由命中后直接黑洞。

正确语义（与 nginx `proxy_pass` WS 行为一致）：
1. 收到升级请求 → 连接上游 target
2. 向上游发起 WebSocket 握手（透传原始 path/query/关键头）
3. 上游 101 → 向客户端回 101 → **双向字节桥接**
4. 上游拒绝/不可达 → 向客户端回 502（不是假 101）

### 设计要点

- **协议桥接层选择**：客户端侧用 `hyper::upgrade::on()` 拿到 `Upgraded` 字节流（hyper 1.x 标准升级路径）；上游侧用 `tokio::net::TcpStream` 手写 WS 握手（发标准 Upgrade 请求），成功后同样得到字节流——两侧都是字节流后 `tokio::io::copy_bidirectional` 桥接。**不引 tungstenite 运行时**（帧解析多余：代理不检帧内容，透传即可，且零新依赖风险）。
- **头透传**：Host（按 target 重写）、Sec-WebSocket-Key 原样（保证 Accept 链一致）、Origin/子协议透传；丢弃 hop-by-hop 头（Connection/Upgrade 除外，需重建）。
- **Accept 一致性**：客户端 101 的 `Sec-WebSocket-Accept` 由本代理基于客户端 Key 计算（现状逻辑保留），与上游实际选择的子协议头合并透传。

### 成功标准（真实运行实测）

1. 真 WS 上游（Python `websockets` 库起 echo 服务）：客户端经代理收发 echo 帧（实测字节往返）
2. 上游日志记录到代理转发的握手请求
3. 上游不可达 → 客户端收到 502（非 101）
4. 非 WS 请求路径零回归（普通 HTTP 照旧）
5. 覆盖率新逻辑 ≥70%，Actions 全绿

### 范围

做：真握手 + 字节桥 + 502 语义 + e2e（真实 echo 上游）。
不做：WSS 上游（TLS 连接复用后续）、WS 帧级路由/改写、压缩扩展协商。

## 信心评估

- `hyper::upgrade::on()` + `copy_bidirectional`：hyper/tokio 一级 API，项目 handler 已是 Service 模式：**信心 91%**
- 上游手写 WS 握手：RFC 6455 固定几行头，accept key 校验复用现有 sha1 逻辑：**信心 90%**
- 需确认点（实现前验证）：hyper 1.x 升级要求响应携带 `connection: upgrade` 且 IO 在 service future 返回后仍存活——现有 handler 返回 Response<Empty> 的结构是否兼容 on_upgrade 的时机。信心底线 86%，若实测受阻则改为 handler 内直接持有升级后 IO 的方案。
