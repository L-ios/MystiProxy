# F6 Mock 条件匹配与模版响应 — Task 规划

## 任务分解（TDD 顺序）

### T1 配置字段（先测后写）
- [ ] YAML 解析测试（conditions/template/content）
- [ ] ConditionCfg / BodyType::Template / content / template 字段

### T2 渲染器
- [ ] render_template 单测矩阵（query/body/缺省/混合/无占位）
- [ ] 实现轻量 JSONPath walker

### T3 候选回退
- [ ] match_uri_candidates + 原 API 委托测试
- [ ] handler 循环：mock 条件不命中 continue

### T4 static content 修复
- [ ] build_mock_response 使用 content

### T5 验证闭环
- [ ] 真实运行 4 条验收
- [ ] llvm-cov ≥70%（render/回退分支）
- [ ] workspace 全绿、fmt/clippy

### T6 推送 CI
- [ ] 提交 push、盯 Actions 全绿
- [ ] 勾选 DEVELOPMENT.md Mock 5 项、更新 OVERVIEW

## 信心评估
匹配引擎现成（96%）；serde 扩展同类刚做过（95%）；手写渲染器无嵌套（92%）；候选回退是循环重排（90%）。全部达标，无需网络调研。
