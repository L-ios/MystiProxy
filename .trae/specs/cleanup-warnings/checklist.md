# Checklist

## 代码清理
- [x] src/arg.rs 中不再有未使用的 `MystiEngine` 和 `Config`
- [x] src/io/listener.rs 中不再有未使用的 `Socket` enum
- [x] `cargo build` 无 dead_code 警告

## 功能验证
- [x] `cargo test --workspace` 全部通过 (72 单元测试 + 11 文档测试)
- [x] `cargo run` 正常显示帮助信息
