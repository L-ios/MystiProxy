# F8b 验证加载器接线 — Spec

## CLI

`--validation-level loose|warn|strict`（默认 strict）。仅与 `--config` 路径相关。

## 行为

- strict：任一 Error 级问题 → 启动失败，stderr 列出全部问题（引擎/字段/原因/建议），退出码 1
- warn：Error 仍拒（保持 infra 立场），Warning 打印但放行——若需纯放行用 loose
- loose：仅解析错误才拒；语义问题全放行（兼容存量）
- 命令行 `--listen/--target` 快捷路径不经验证（构造即受控）

## 验收（实测）

1. 坏 CIDR（/99）strict → 启动失败，stderr 指明
2. 同配置 loose → 启动成功可代理
3. 坏正则 location strict → 拒
4. 正常配置 strict → 代理 200（回归）
5. workspace 全绿 + Actions 全绿
