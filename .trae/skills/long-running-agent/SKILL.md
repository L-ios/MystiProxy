---
name: "long-running-agent"
description: "Implements Anthropic's long-running agent methodology for resumable, traceable tasks. Invoke when user requests persistent agent workflows, feature development, or needs task continuation across sessions."
---

# Long-Running Agent Workflow

基于 Anthropic《Effective harnesses for long-running agents》的永久化 Agent 工作流。

## 核心理念

1. **可验收** - 每步产出清晰可验证
2. **可续跑** - 中断后可无缝继续
3. **不烂尾** - 状态机控制完成流程
4. **可回溯** - 完整执行日志

## 文件结构

```
./
├── feature_list.json     # 功能清单（来源需求）
├── progress.txt          # 进度日志
├── init.sh               # 恢复脚本
└── .trae/
    └── skills/
        └── long-running-agent/
            └── SKILL.md
```

## 初始化流程

### 1. 创建 feature_list.json

```json
{
  "project": "项目名称",
  "created_at": "ISO时间戳",
  "features": [
    {
      "id": "F001",
      "name": "功能名称",
      "description": "功能描述",
      "status": "pending|working|done|blocked",
      "dependencies": [],
      "subtasks": []
    }
  ]
}
```

### 2. 创建 progress.txt

```
# Progress Log
# 格式: [时间戳] [状态] [功能ID] 描述

[2026-04-05T10:00:00Z] INIT project_name started
```

### 3. 创建 init.sh

```bash
#!/bin/bash
# 初始化/恢复脚本

if [ -f "feature_list.json" ]; then
    echo "Resuming from feature_list.json..."
    # 读取状态，决定从哪个功能继续
else
    echo "No existing state. Run initialization first."
    exit 1
fi
```

## 执行流程

### 单次执行循环

```
┌─────────────────────────────────────┐
│  1. READ progress.txt               │
│     └─ 找到最后一个未完成的功能      │
├─────────────────────────────────────┤
│  2. READ feature_list.json          │
│     └─ 加载该功能的详细信息          │
├─────────────────────────────────────┤
│  3. EXECUTE 功能开发                 │
│     └─ 只做一个功能，不贪多          │
├─────────────────────────────────────┤
│  4. VERIFY 验证产出                 │
│     └─ 编译 + 测试 + lint           │
├─────────────────────────────────────┤
│  5. UPDATE 更新状态                 │
│     └─ progress.txt + feature_list  │
├─────────────────────────────────────┤
│  6. COMMIT (可选)                   │
│     └─ 保持干净可合并状态            │
└─────────────────────────────────────┘
```

### 状态转移

```
pending → working → done
              ↓
           blocked → (解除) → working
```

## 命令

### 初始化项目

```bash
# 创建初始文件
cat > feature_list.json << 'EOF'
{
  "project": "mystiproxy-clippy-fix",
  "created_at": "2026-04-05T10:00:00Z",
  "features": [
    {
      "id": "F001",
      "name": "修复 src/tls/mod.rs clippy 警告",
      "description": "修复 format 字符串和 doc comment 问题",
      "status": "pending",
      "subtasks": [
        "F001-1: 修复 doc_lazy_continuation",
        "F001-2: 修复 uninlined_format_args"
      ]
    }
  ]
}
EOF

cat > progress.txt << 'EOF'
# Progress Log
[2026-04-05T10:00:00Z] INIT mystiproxy-clippy-fix started
EOF

cat > init.sh << 'EOF'
#!/bin/bash
# 恢复脚本
jq '.features[] | select(.status != "done") | .id' feature_list.json | head -1
EOF
chmod +x init.sh
```

### 查看状态

```bash
# 查看当前进度
cat progress.txt | tail -5

# 查看功能状态
jq '.features[].status' feature_list.json
```

### 更新进度

```bash
# 更新单个功能状态
jq '.features[] | select(.id == "F001") | .status = "done"' feature_list.json > tmp.json && mv tmp.json feature_list.json

# 追加进度日志
echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] DONE F001 修复完成" >> progress.txt
```

## 开发原则

### 执行前

- [ ] 读取 progress.txt 找到当前位置
- [ ] 读取 feature_list.json 了解功能详情
- [ ] 确认依赖已满足

### 执行中

- [ ] 只做一个功能
- [ ] 频繁保存中间状态
- [ ] 保持工作区干净

### 执行后

- [ ] 运行验证：`cargo build && cargo test`
- [ ] 更新 feature_list.json
- [ ] 追加 progress.txt
- [ ] 确认无 uncommitted 重要变更

## 与 Skill 集成

当用户说 "继续上次的任务" 或 "修复 clippy 警告"：

1. 先运行 `init.sh` 获取当前进度
2. 从 progress.txt 读取上下文
3. 继续执行下一个 pending 功能
