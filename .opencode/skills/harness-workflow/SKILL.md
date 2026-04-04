# Harness Workflow Skill

基于 Anthropic 长期运行 Agent 方法论的可续跑工作流。

## 何时使用

当项目存在 `.sisyphus/harness/active/` 目录且包含 `feature_list.json` 时，自动激活此工作流。

## 会话启动协议

每次会话开始时，必须按顺序执行以下步骤：

### STEP 1: 确认工作目录
```bash
pwd
```
确保在项目根目录。所有文件操作都基于此目录。

### STEP 2: 读取进度日志
读取 `.sisyphus/harness/active/progress.txt`，了解：
- 上次会话做了什么
- 当前完成了多少功能
- 下一步建议做什么
- 是否有阻塞问题

### STEP 3: 读取功能清单
读取 `.sisyphus/harness/active/feature_list.json`，找到：
- 最高优先级的 `passes: false` 功能
- 该功能的验证步骤
- 当前进度（X/Y passing）

### STEP 4: 检查 Git 历史
```bash
git log --oneline -20
```
了解近期变更，确认上次会话的提交状态。

### STEP 5: 初始化环境
```bash
bash .sisyphus/harness/active/init.sh
```
如果 init.sh 失败，**立即停止**并修复环境问题。

### STEP 6: 基础验证
验证 1-2 个已通过（`passes: true`）的核心功能仍然工作。
如果发现已通过功能坏了：
1. 立即将该功能标记为 `passes: false`
2. **先修复所有问题**，再开始新功能
3. 绝不在坏掉的基础上开发新功能

## 功能实现协议

### STEP 1: 选择功能
从 `feature_list.json` 中选择**最高优先级**的 `passes: false` 功能。
一次只选一个。不贪多。

### STEP 2: 理解验证步骤
仔细阅读该功能的 `steps` 数组和 `verification` 字段。
明确"通过"的标准是什么。

### STEP 3: 实现代码
- 遵循项目现有代码模式（检查 AGENTS.md）
- 匹配现有风格（imports 组织、命名规范、错误处理）
- 如果不确定模式，先阅读同模块的现有代码

### STEP 4: 运行验证
```bash
bash .sisyphus/harness/active/verify.sh
```
验证脚本会运行：格式检查、lint、编译、测试。
如果任何检查失败 → **禁止提交**，必须先修复。

### STEP 5: 端到端验证
按照功能清单中的 `steps` 逐一验证：
- 不能跳过任何步骤
- 不能因为"看起来差不多"就标记通过
- 每一步必须有明确的验证结果

### STEP 6: 标记通过
**只有**在所有验证步骤都通过后，才能修改 `feature_list.json`：
- 将 `passes` 改为 `true`
- 将 `passed_at` 改为当前时间戳

### STEP 7: Git 提交
```bash
git add -A
git commit -m "feat(harness): implement FXXX - 功能描述"
```
提交信息必须包含功能 ID。

### STEP 8: 更新进度
追加到 `.sisyphus/harness/active/progress.txt`：

```
───────────────────────────────────────────────────────────────
SESSION [N] - YYYY-MM-DD HH:MM
───────────────────────────────────────────────────────────────
Action:      [做了什么]
Features:    [通过的 feature ID 列表]
Issues:      [发现的问题]
Bugs fixed:  [修复的 bug]
Progress:    X/Y features passing (Z%)
Next:        [下一个最高优先级的功能 ID]
Status:      READY / BLOCKED / NEEDS_REVIEW
```

### STEP 9: 更新功能清单
确认 `feature_list.json` 已正确更新（只改了 passes 字段）。

### STEP 10: 确保干净状态
- 无编译错误
- 无测试失败
- 无 clippy 警告
- 代码格式正确
- 环境可安全开始下一个功能

## 铁律

### 绝对禁止（CATASTROPHIC）

- **绝不**删除 feature_list.json 中的功能条目
- **绝不**修改功能的 description 或 steps
- **绝不**合并或重排功能
- **绝不**在 verify.sh 失败时提交代码
- **绝不**留下无法编译的代码
- **绝不**跳过验证步骤就标记 passes=true

### 必须遵守

1. **每次只做一个功能** — 一个做完再做下一个
2. **做完必须验证** — verify.sh + 手动验证步骤
3. **先修后建** — 已有功能坏了先修，再开始新功能
4. **Clean State** — 每次结束代码必须可合并到 main
5. **只改 passes 字段** — feature_list.json 的其他字段不可变
6. **禁止自欺** — 不能因为"看起来差不多"就标记通过

## 故障恢复协议

### verify.sh 失败
1. 分析失败原因
2. 修复问题
3. 重新运行 verify.sh
4. 重复直到通过

### 修复 3 次仍失败
1. `git stash` 或 `git checkout .` 恢复到上一个工作状态
2. 记录问题到 progress.txt
3. 跳过该功能，选择下一个

### git revert 后仍失败
1. 记录到 progress.txt，标记 Status: BLOCKED
2. 结束当前会话
3. 等待人工介入

### 永远不要
- 在失败的代码上继续开发
- 删除失败的测试来"通过"
- 用 `as any`、`unwrap()` 等方式掩盖问题

## 进度追踪

### 读取进度
```bash
cat .sisyphus/harness/active/progress.txt | head -20
```

### 统计通过率
```python
import json
with open('.sisyphus/harness/active/feature_list.json') as f:
    data = json.load(f)
    total = len(data['features'])
    passing = sum(1 for f in data['features'] if f.get('passes', False))
    print(f"{passing}/{total} features passing ({passing*100//total}%)")
```

### 查看下一个待做功能
```python
import json
with open('.sisyphus/harness/active/feature_list.json') as f:
    data = json.load(f)
    for f in sorted(data['features'], key=lambda x: x['priority']):
        if not f.get('passes', False):
            print(f"Next: {f['id']} - {f['description']}")
            break
```

## 会话结束检查清单

- [ ] `bash .sisyphus/harness/active/verify.sh` 通过
- [ ] 功能的每个验证步骤都通过
- [ ] `feature_list.json` 已更新（只改 passes 字段）
- [ ] `progress.txt` 已追加本次会话记录
- [ ] Git commit 已创建（包含功能 ID）
- [ ] 无编译错误、无测试失败、无 lint 警告
- [ ] 代码格式正确
- [ ] 环境干净，可安全开始下一个功能
