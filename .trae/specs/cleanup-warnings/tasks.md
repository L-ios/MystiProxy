# Tasks

- [x] Task 1: 清理 src/arg.rs 中未使用的代码
  - [x] SubTask 1.1: 删除 `MystiEngine` 结构体
  - [x] SubTask 1.2: 删除 `Config` 结构体
  - [x] SubTask 1.3: 保留 `MystiArg` 命令行参数结构体

- [x] Task 2: 清理 src/io/listener.rs 中未使用的代码
  - [x] SubTask 2.1: 删除 `Socket` enum
  - [x] SubTask 2.2: 删除未使用的 import (`TcpSocket`, `UnixSocket`)

- [x] Task 3: 验证构建无警告
  - [x] SubTask 3.1: 运行 `cargo build` 确认无警告
  - [x] SubTask 3.2: 运行 `cargo test` 确认测试通过

# Task Dependencies
- Task 1 和 Task 2 可以并行执行
- Task 3 依赖 Task 1 和 Task 2
