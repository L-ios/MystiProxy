# 代码清理与架构优化 Spec

## Why
当前项目存在未使用的代码（dead_code warnings），这些代码反映了配置系统的设计混乱：存在两套配置结构体（`arg.rs` 中的旧版和 `config/mod.rs` 中的新版）。清理这些代码并统一架构设计，有助于提高代码可维护性和减少混淆。

## What Changes
- 删除 `src/arg.rs` 中未使用的 `MystiEngine` 和 `Config` 结构体
- 删除 `src/io/listener.rs` 中未使用的 `Socket` enum
- 统一配置系统设计，明确各模块职责

## Impact
- Affected specs: 配置系统
- Affected code: 
  - src/arg.rs - 清理未使用代码
  - src/io/listener.rs - 清理未使用代码

## ADDED Requirements

### Requirement: 代码清理
系统应保持代码整洁，不存在未使用的死代码。

#### Scenario: 构建无警告
- **WHEN** 执行 `cargo build`
- **THEN** 不应产生任何 dead_code 警告

### Requirement: 配置系统统一
配置系统应有清晰的设计，避免重复定义。

#### Scenario: 单一配置来源
- **WHEN** 需要读取配置
- **THEN** 只使用 `config/mod.rs` 中定义的结构体

## MODIFIED Requirements
无

## REMOVED Requirements

### Requirement: 旧版配置结构体
**Reason**: 与新版配置结构体重复，造成混淆
**Migration**: 使用 `config/mod.rs` 中的 `EngineConfig` 和 `MystiConfig`

### Requirement: 未使用的 Socket enum
**Reason**: 当前未使用，如未来需要可重新添加
**Migration**: 无需迁移
