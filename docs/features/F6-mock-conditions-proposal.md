# F6 Mock 条件匹配与模版响应 — Proposal

## 需求深度理解

### 背景与问题

`docs/DEVELOPMENT.md` 的 Mock 响应清单 5 项全部未勾选，但代码考古发现**引擎已存在、只差接线**：

- `mock/mod.rs` 已有 `MockBuilder::matches_conditions(uri, headers, body, conditions)`，支持 uri/path/query/header/body(json) 五种条件类型，还有 13 个测试
- **但全仓库没有任何调用方**——`http/handler.rs` 的 mock 分支只按 location 匹配，从不检查条件
- `LocationConfig`（config/mod.rs）没有 conditions 字段，YAML 里写不进去
- 清单第 5 项“Body 模版，特殊处理后响应”（如 `{{query.user_id}}` 替换进响应体）完全没有实现——`build_mock_response` 目前只能返回空体

即：**匹配引擎是孤岛，模版能力是空白**。

### 深层需求分析

1. **按请求内容响应**的价值：同一 location 下按 header 版本灰度（`X-Version: v2` 返回新版假数据）、按 query 参数分支（`?beta=1`）、按 body 字段分支（`$.type == premium`）——这是 Mock 服务从“静态桩”到“行为模拟”的分水岭。
2. **模版响应**的价值：让响应体携带请求上下文（回显 user_id、echo 场景），常见于测试客户端序列化。
3. **配置面**：YAML 的 `locations[].response` 需要扩展 `conditions` 与 `body.template`；`body` 现在只有 `type: static`（且 build_mock_response 对 static 只给空串——这本身像个未完成点，一并修复：static 应支持 `content` 字段返回固定内容）。

### 设计决策

- **条件从配置到引擎**：`ResponseConfig.conditions: Option<Vec<MockCondition>>`（serde 直接复用既有 `Condition` 结构新增配置版），handler 在命中 location 且 provider=mock 后调用 `matches_conditions`，不匹配则**继续尝试下一个 location**（多 location 同前缀时按序回退），全部不匹配走默认代理——这是“按内容路由”的自然语义。
- **模版语法**：`{{...}}` 双花括号占位符，来源限定 query 参数与 JSON body 的 JSONPath 一级取值（复用 body.rs 已有的 JSONPath 能力思路，做轻量子集）。不引第三方模版引擎（信心与体积都不划算）。

### 成功标准

1. `conditions` 配置 header/query/body 命中时返回 mock，不命中回退代理（实测）
2. `body.type: static` + `content` 返回固定内容（修复现状空体）
3. `body.template` 中 `{{query.x}}` / `{{body.$.path}}` 被实际值替换（实测回显）
4. 占位符无值时保留原样（调试友好）或替换为空——选择**保留原样**并在日志 warn
5. 覆盖率新逻辑 ≥70%，Actions 全绿

### 范围

做：配置字段、handler 接线（含 location 回退）、static content、模版渲染、单测+集成、文档勾选。
不做：多 location 之外的优先级权重、模版条件语法（if/for）、响应头模版（后续可加）。

## 信心评估

- 条件引擎复用：已写好的 `matches_conditions` + 13 测试，纯接线，**信心 96%**
- YAML serde 扩展：项目内 F5 刚做过同类（allow/deny），**信心 95%**
- 手写 `{{}}` 渲染器：字符串扫描替换，无嵌套需求，**信心 92%**
- JSONPath 子集：body.rs 已有完整 JSONPath 转换实现可参考，**信心 90%`
- 全部 >85%，无需网络调研。
