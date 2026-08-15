# F6 Mock 条件匹配与模版响应 — Spec

## 概述

打通 mock 条件匹配引擎到请求链路，支持按 URI/Header/Query/Body 内容决定是否返回 mock，并支持模版化响应体。

## 配置示例

```yaml
locations:
  - location: '/api/user'
    mode: Prefix
    provider: mock
    response:
      status: 200
      conditions:
        - {condition_type: header, value: 'X-Version=v2'}
      body:
        type: template
        template: '{"id": "{{query.user_id}}", "tier": "{{body.$.tier}}"}'
  - location: '/api/user'
    mode: Prefix
    provider: mock
    response:
      body: {type: static, content: '{"fallback": true}'}
```

## 条件语义

- `condition_type`: `uri|path|query|header|body|json`（既有引擎支持集，值语义沿用其 =/包含/正则实现）
- 多条件 AND；任一不命中即该 location 不适用
- 同一路径多个 location 依配置顺序回退：条件不命中尝试下一个，全不命中走默认代理

## 响应体

- `type: static` + `content`：原样返回（修复此前恒空体）
- `type: template`：`{{query.<name>}}` 与 `{{body.$.<path>}}` 替换为请求实际值；无法解析的占位符保留原文并 warn
- 不设 body：保持现状（空体）

## 兼容性

- `match_uri` 公共 API 不变；新增 `match_uri_candidates`
- 纯代理/静态路径不读 body（惰性策略保持）

## 验收标准（真实运行实测）

1. header 条件命中返回 mock 内容，不命中时上游收到该请求（回退生效）
2. template 回显 query 与 body 值（curl 断言 JSON 字段）
3. static content 返回固定串
4. 未知占位符原样出现在响应中
5. cargo test 全绿 + 新逻辑覆盖率 ≥70% + Actions 全绿
