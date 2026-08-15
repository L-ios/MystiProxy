# F7 Forward 引擎普通 HTTP 请求代理 — Proposal

## 需求深度理解

### 背景与问题

Forward 引擎（`proxy_type: forward`）的 `HttpProxyAcceptor::handle_connection`（http/proxy.rs:452）只处理 CONNECT：非 CONNECT 方法直接回 `400 Use HTTP proxy for HTTP requests`。实测 `curl -x proxy http://target/`（标准正向代理绝对 URL 形态）得到 400。

代码考古：**`HttpProxyService::handle_http_request`（同文件 279 行起，87 行完整实现）就是为这个场景写的，但没有任何调用方**——又一个孤岛。它支持转发、Host 重写、错误响应；`Service for HttpProxyService` 的 `call`（376 行）也在 Service 层支持了非 CONNECT 请求。

### 深层需求分析

正向代理的两种标准流量形态（RFC 7230 + 业界实现）：
1. **CONNECT method**：HTTPS 隧道——已实现 ✅
2. **绝对 URI 请求**（`GET http://example.com/path HTTP/1.1`）：HTTP 明文代理——**缺失** ❌

浏览器/客户端对 HTTP 目标默认走形态 2。缺它意味着该引擎只能代理 HTTPS 站点，作为“通用正向代理”名不副实。

上游 e2e（upstream_e2e.rs）验证了 CONNECT 隧道链路，但没有绝对 URL 转发的用例——所以此缺陷一直没被测试网捕获。

### 设计要点

- `handle_connection` 解析首行后，非 CONNECT 且 target 形如 `http://host[:port]/path` → 走 `handle_http_request` 转发
- 相对路径 target（`GET /path`）在正向代理语义下无 Host 可寻 → 维持 400（这不是缺陷，是协议要求）
- 响应需按原始字节回写客户端（当前 acceptor 是手写 TCP 解析，不走 hyper service 栈；handle_http_request 返回 hyper Response，需 to_bytes 回写）
- 超时与 allow/block host 过滤沿用 config

### 成功标准

1. `curl -x fwd http://upstream/` → 200，上游日志收到请求（实测）
2. Host 头正确（上游看到的 Host = 目标 host，非代理）
3. 相对路径请求仍 400（协议正确性）
4. CONNECT 回归不受影响
5. 覆盖率 ≥70%，Actions 全绿

### 范围

做：acceptor 非 CONNECT 分支接入 handle_http_request、原始请求字节重建（首行改绝对 URI→保留即可直传）、单测 + e2e。
不做：HTTP/2 代理、缓存、Via 头追加（后续可选）。

## 信心评估

- handle_http_request 现成且带测试：**信心 94%**
- 字节级转发回写（hyper Response → bytes → client_stream）：F4 metrics 服务器同模式：**信心 92%**
- 判定绝对 URI：字符串前缀检查：**信心 96%**
- 全部 >85%，无需网络调研。
