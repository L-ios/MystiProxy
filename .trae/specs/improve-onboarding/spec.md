# 新手入门指南优化 Spec

## Why
现有文档虽然详细，但对第一次使用的用户来说可能信息量过大，缺乏循序渐进的引导。需要优化文档结构，让新手能够快速上手。

## What Changes
- 创建独立的 `GETTING-STARTED.md` 新手入门指南
- 添加 5 分钟快速体验教程
- 优化 `config.example.yaml` 的注释结构
- 添加常见使用场景的配置模板

## Impact
- Affected specs: 文档系统
- Affected code: 
  - GETTING-STARTED.md (新建)
  - config.example.yaml (优化)
  - README.org (添加快速入门链接)

## ADDED Requirements

### Requirement: 5 分钟快速体验
新手应能在 5 分钟内完成 MystiProxy 的首次运行。

#### Scenario: 快速启动
- **WHEN** 用户下载 MystiProxy
- **THEN** 用户能在 5 分钟内完成配置并启动服务

### Requirement: 渐进式学习路径
文档应提供从简单到复杂的学习路径。

#### Scenario: 学习路径
- **WHEN** 用户阅读文档
- **THEN** 用户能按 "快速体验 → 基础配置 → 高级功能" 的路径学习

### Requirement: 场景化配置模板
提供常见使用场景的配置模板，用户可直接复制使用。

#### Scenario: 配置模板
- **WHEN** 用户需要配置特定场景
- **THEN** 用户能找到对应的配置模板并直接使用

## MODIFIED Requirements
无

## REMOVED Requirements
无
