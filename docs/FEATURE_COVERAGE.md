# MystiProxy 功能覆盖报告

> 生成时间：2026-08-15 · 基于 commit e59a7f6 的全量盘点与实测

## 一、验证方法

- **代码侧**：扫描 `mystiproxy/src` 全部 20+ 模块的 public API，追踪每个 API 在主链路（main.rs → server → handler）中的真实引用
- **文档侧**：交叉比对 `doc/features.md`、`docs/OVERVIEW.md`、`config.example.yaml`、`docs/features/F1-F5`、`README.md` 五处功能声明
- **运行时侧**：shell e2e（真实二进制 + 真实 HTTP 流量，18 项断言）+ Rust e2e（484 个测试）

## 二、功能矩阵（实测结果）

| 功能 | 实测状态 | 验证方式 |
|------|---------|---------|
| TCP 四层转发（tcp/unix 互转） | ✅ | shell e2e + tcp_proxy_integration |
| HTTP 七层代理（头/体透传） | ✅ | shell e2e + proxy_e2e |
| Mock：status/headers/静态 content | ✅ | mock_body_e2e（4 个） |
| Mock：条件命中（uri/query/header/body） | ✅ | 远端 ddbadfa 测试套件 |
| Mock：模板响应体（`{{query.x}}`/`{{body.$.a}}`） | ✅ | 远端 ddbadfa 测试套件 |
| 静态文件：root/子目录/Range 206/目录列表 | ✅ | shell e2e + features_e2e |
| 路由：Full / Prefix / Regex / PrefixRegex | ✅ | shell e2e + router 测试 |
| Header 鉴权（401/200） | ✅ | auth_e2e（4 个） |
| JWT 鉴权（需 iat+exp 声明） | ✅ | jwt_pool_e2e（7 个） |
| 入站 IP 过滤（deny 优先/allow 白名单/非法 CIDR fail-fast） | ✅ | ip_filter_e2e（6 个，本次新增） |
| 上游代理 upstream（CONNECT 隧道全链路） | ✅ | upstream_e2e（1 个，本次修复断裂链路） |
| Body JSON 转换（JSONPath add/overwrite/delete） | ✅ | body_transform_e2e（3 个） |
| 请求改写（method/uri/header overwrite/missed/forceDelete） | ✅ | rewrite_e2e + force_delete_e2e |
| Metrics 导出（:9090/metrics，5 指标） | ✅ | shell e2e（F4） |
| 连接池复用（同 target 单 client） | ✅ | shell e2e（5 请求 1 client） |
| 多引擎并存（同进程 3 引擎） | ✅ | shell e2e |
| TLS 单向/双向（mTLS） | ✅ | tls_integration_test（6 个） |
| WebSocket 升级握手 | ✅ | features_e2e |
| Forward 代理（CONNECT 隧道） | ✅ | shell 实测（curl -x） |
| 本地管理（local-management feature，SQLite） | ✅ | 181 单测 |
| 本地管理二进制入口（engine.management 配置段） | ✅ | F9 真实全链路：健康检查/SQLite 持久化/中心注册/周期 pull/push-all 下发 |

## 三、本次盘点修复的问题

| 问题 | 严重度 | 修复 |
|------|-------|------|
| upstream 配置完全无效（`get_or_create` 硬编码 None，链路断裂） | 高 | 新增 `get_or_create_with_upstream`，handler 代理分支透传 |
| Mock 静态 body 不支持（文档 schema 与代码不符） | 中 | `BodyConfig` 增加 `content` 字段（与远端 ddbadfa 实现合并） |
| ip_filter 无 e2e 覆盖 | 中 | 新增 ip_filter_e2e（6 个场景） |
| 测试死代码（jwt_pool_e2e 未使用 helper） | 低 | 已删除 |

## 四、死代码处置结论

`gateway`、`MockService`/`MockBuilder`、`HttpProxyService`/`create_simple_server` 为 lib 公共 API（嵌入式场景），均有测试覆盖，保留。clippy `never used/read/constructed` 警告为 0。

## 五、已知设计现状（非缺陷）

1. （已解决）Forward 普通 HTTP 代理：上游 3d723c6 已实现绝对 URI 转发
2. （已解决）本地管理无启用入口：F9 新增 engine.management 配置段 + main.rs 接线
2. NTLM 认证仅在 upstream 模块内部可用，配置面未暴露独立开关
3. UDP 未实现（地址解析层即拒绝，文档已声明）
4. `http://` target 不被 `SocketStream` 接受，需写 `tcp://`（见 config.example.yaml）

## 六、持续验证

CI（`.github/workflows/rust.yml`）在每次 push 时执行：Test Suite（含 PostgreSQL 服务容器）、5 平台 Release 构建、前端构建、Docker 多架构镜像。当前 main 全绿。
