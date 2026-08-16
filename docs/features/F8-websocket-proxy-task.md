# F8 — Task

- [ ] T1 单元：握手请求构造（头集合/hop-by-hop 丢弃/Host 重写）、502 形状
- [ ] T2 实现 upstream_handshake + proxy_websocket + 桥接 spawn
- [ ] T3 handler 接线（WS 分支传 config.target）
- [ ] T4 e2e：websockets echo 上游 4 条实测
- [ ] T5 fmt/clippy/全量
- [ ] T6 push 盯 CI 全绿，勾选文档

信心 86-91%，无需网络调研（hyper upgrade 为一级 API）。
