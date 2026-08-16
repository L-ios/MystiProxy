# F8 WebSocket 真正代理 — Spec

## 概述

替换本地假握手：升级请求真实转发到 engine.target，上游 101 后双向字节桥接；上游不可达回 502。

## 行为

1. **握手转发**：代理向上游发起标准 WS 握手（GET path HTTP/1.1 + Upgrade/Connection/Key/Version/Host 重写；透传 Origin 与 Sec-WebSocket-Protocol）
2. **成功**：向客户端回 101（Accept 基于客户端 Key 计算；上游协商的 Protocol/Extensions 头透传），随后 `copy_bidirectional` 双向透传直至任一侧关闭
3. **失败**：上游连接失败/非 101 → 客户端收到 502，连接关闭
4. **超时**：握手阶段受 engine request_timeout 约束；桥接阶段为长连接不超时

## 边界

- engine.target 为 unix:// → 返回 502 + warn 日志（WS over UDS 暂不支持）
- 非 Upgrade 请求不进入该路径（既有判断不变）

## 验收（真实运行实测）

1. Python `websockets` echo 上游：经代理 send("ping") → recv 得 "ping"
2. 上游连接计数 ≥1（握手真实到达）
3. 死端口 target：升级请求 → 502
4. 同引擎普通 GET → 200（回归）
5. workspace 全绿 + 新逻辑覆盖 ≥70% + Actions 全绿
