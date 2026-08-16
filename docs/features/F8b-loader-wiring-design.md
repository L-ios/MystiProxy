# F8b 验证加载器接线 — Design

## 代码设计

### main.rs 变更

```rust
fn load_config(args: &MystiArg) -> Result<MystiConfig> {
    if let Some(config_path) = &args.config {
        let level = match args.validation_level.as_str() {
            "loose" => ValidationLevel::Loose,
            "warn" => ValidationLevel::Warn,
            _ => ValidationLevel::Strict,
        };
        match MystiConfig::load_validated_with_level(config_path, level) {
            Ok(c) => Ok(c),
            Err(e) => {
                eprintln!("{}", format_validation_error(&e));  // user_interface
                Err(e)
            }
        }
    }
    ...
}
```

### MystiConfig::load_validated_with_level

新增（保留 load_validated 为 Strict 别名）：
```rust
pub fn load_validated_with_level(path: &str, level: ValidationLevel) -> crate::Result<Self> {
    let loader = EnhancedConfigLoader::new()
        .with_validation_level(level)
        .add_source(ConfigSource::File(path.to_string()));
    loader.load::<Self>().map_err(...)
}
```

### arg.rs

```rust
#[arg(long, default_value = "strict", value_parser = ["loose","warn","strict"])]
pub validation_level: String,
```

### 错误展示

启动失败路径统一经 `user_interface::format_validation_error`（若该函数名不同则用实际 API），输出：引擎名 + 字段 + 问题 + 修复建议，多错误全量列出。

## 测试（TDD）

- 单元：level 字符串→枚举映射三态；load_validated_with_level 对坏/好 YAML 行为
- e2e（真实进程）：坏 CIDR 退出码非 0 + stderr 含条目；loose 放行；好配置回归 200

## 覆盖率目标：新增分支 ≥70%
