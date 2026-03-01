---
alwaysApply: false
---
0. 项目的定义: MystiProxy 是一个用 Rust 编写的跨平台灵活代理服务器，专注于解决网络协议转换和服务模拟问题，支持 TCP/UDP/Unix Socket/Named Pipe 等多种监听方式，具备 HTTP 请求转换和 Mock 响应能力，兼容 Linux、macOS、Windows 等主流操作系统。

1. 项目的名称: MystiProxy（微服务模拟器/神秘代理）

2. 项目的描述: 一个跨平台的灵活代理服务器，可以将 TCP/UDP/Unix Socket/Named Pipe 流量转发到目标地址，支持 HTTP 请求转换和 Mock 响应。主要解决服务迁移过程中导致的调试、中间态验证等问题，特别适用于观察 Docker 等以 UDS 文件进行通信的应用。

3. 项目的目标:
   - 打通通信端点之间的网络隔离
   - 提供协议转换能力（4层↔7层、TCP↔UDP）
   - 支持 Mock 测试和 API 网关功能
   - 实现请求/响应的灵活转换
   - 解决服务迁移导致的调试问题
   - 实现跨平台兼容（Linux/macOS/Windows）

4. 项目的受众: 需要解决网络问题的用户，特别是：
   - 需要进行服务迁移的开发者
   - 需要调试 Docker/容器网络的运维人员
   - 需要模拟 API 响应的前端/测试人员
   - 需要解决协议转换问题的架构师
   - Windows 平台开发者（通过 Named Pipe）

5. 项目的功能:
   - 监听方式（跨平台）:
     - TCP Socket: 全平台支持（Linux/macOS/Windows/FreeBSD）
     - UDP Socket: 全平台支持（Linux/macOS/Windows/FreeBSD）
     - Unix Domain Socket (AF_UNIX): Linux/macOS/FreeBSD 原生支持，Windows 10 (Build 17063+) 支持
     - Windows Named Pipe: 仅 Windows 平台，格式 `\\.\pipe\xxx`
   - 4层代理: TCP 转发、UDP 转发、UDS 转发、Named Pipe 转发（Windows）
   - 协议转换: TCP↔UDP 双向隧道（支持帧格式定义）
   - 7层代理: HTTP/HTTPS 转发、静态文件服务、Mock 响应、API 网关
   - TLS 鉴权: 单向认证、双向认证
   - HTTP 鉴权: Header 鉴权、JWT 鉴权
   - 请求转换: URI 映射（Full/Prefix/Regex/PrefixRegex）、Header 处理、Body 处理
   - Mock 能力: 条件 Mock、静态响应、健康检查

5.1. 跨平台监听方式详细说明:
   | 监听方式 | 配置格式 | Linux | macOS | Windows 10+ | 说明 |
   |---------|---------|-------|-------|-------------|------|
   | TCP | `tcp://0.0.0.0:8080` | ✅ | ✅ | ✅ | 全平台首选方案 |
   | UDP | `udp://0.0.0.0:5353` | ✅ | ✅ | ✅ | DNS、游戏、流媒体 |
   | Unix Domain Socket | `unix:///var/run/proxy.sock` | ✅ | ✅ | ✅* | 本地高性能 IPC |
   | Windows Named Pipe | `pipe://mystiproxy` | ❌ | ❌ | ✅ | Windows 专用 IPC |
   | Loopback TCP | `tcp://127.0.0.1:8080` | ✅ | ✅ | ✅ | 兼容方案 |
   
   *Windows 10 Build 17063+ 支持 AF_UNIX，但不支持文件描述符传递

5.2. Windows 平台特殊支持:
   - Windows Named Pipe:
     - 地址格式: `pipe://name` 映射为 `\\.\pipe\name`
     - 支持跨机器通信（通过 SMB 协议）
     - 使用 tokio::net::windows::named_pipe 模块
   - Windows AF_UNIX (Build 17063+):
     - 支持文件系统路径绑定
     - 不支持 SCM_RIGHTS（文件描述符传递）
     - 与 Linux/macOS 语义基本一致

5.3. TCP↔UDP 协议转换:
   - TCP 转 UDP: TCP 字节流通过帧格式封装为 UDP 数据报
   - UDP 转 TCP: UDP 数据报通过帧格式封装为 TCP 字节流
   - 帧格式支持: 长度前缀、分隔符
   - 应用场景: DNS over TCP、游戏加速、VPN 隧道、QUIC 代理

6. 项目的非功能需求:
   - 基于 Tokio 异步运行时，支持高并发
   - 流式处理，避免大内存占用
   - 配置驱动，YAML 格式配置文件
   - 零成本抽象，内存安全保证
   - 跨平台条件编译（#[cfg(unix)], #[cfg(windows)]）

7. 项目的性能需求:
   - 支持连接池自动管理
   - 高并发连接处理
   - 低延迟转发
   - 可配置超时时间（支持 ms/s/m/h 单位）
   - Unix Domain Socket/Named Pipe 本地通信零拷贝优化

8. 项目的安全需求:
   - TLS 加密传输（tokio-rustls）
   - 双向认证支持
   - Header/JWT 鉴权
   - 访问日志审计
   - 敏感 Header 删除能力
   - Windows Named Pipe 访问控制（ACL）

9. 项目的部署需求:
   - Docker 容器部署（scratch 镜像）
   - Kubernetes Helm Chart 部署
   - systemd 系统服务部署
   - 支持 x86_64 和 aarch64 架构
   - 跨平台构建: Linux (gnu/musl)、macOS (Intel/Apple Silicon)、Windows (MSVC/GNU)

10. 项目的监控需求:
    - 健康检查端点 (/health)
    - 日志级别控制 (RUST_LOG 环境变量)
    - 结构化日志输出

11. 项目的日志需求:
    - 使用 tracing 和 tracing-subscriber
    - 支持环境变量控制日志级别
    - 支持 debug/info/warn/error 等级别
    - 示例: RUST_LOG=debug,hyper=info

12. 项目的指标需求:
    - (待实现) Prometheus 指标暴露
    - (待实现) 请求计数、延迟统计
    - (待实现) 连接池状态监控

13. 项目的文档需求:
    - README.org - 完整功能文档
    - GETTING-STARTED.md - 新手入门指南
    - config.example.yaml - 配置示例
    - examples/ - 场景配置模板（api-gateway、docker-proxy、mock-api、mysql-proxy、static-files）
    - 跨平台配置示例（Windows Named Pipe、AF_UNIX、UDP）
    - 架构设计文档（使用 UML 图表说明系统设计）

13.1. 文档语言规范:
    - 核心语言: 中文（文档主体、说明文字、注释）
    - 允许英文: 专有名词、技术术语、代码示例、命令行指令、配置项名称
    - 图表标注: 优先中文，技术术语可保留英文
    - 示例:
      - ✅ "监听地址支持 TCP 和 UDP 两种协议"
      - ✅ "使用 tokio::net::TcpListener 创建监听器"
      - ❌ "The listener supports TCP and UDP protocols"

13.2. 文档图表需求:
    - 图表工具: PlantUML 和 Mermaid.js 双支持
    - 图表类型:
      - 架构图: 系统整体架构、模块依赖关系
      - 流程图: 请求处理流程、数据流转过程
      - 时序图: 组件交互、API 调用链路
      - 类图: 核心数据结构、模块关系
      - 状态图: 连接状态机、请求生命周期
      - 组件图: 各功能模块及其接口
    - 图表存储位置: docs/diagrams/ 或嵌入 Markdown/Org 文件
    - 渲染支持: GitHub 原生 Mermaid 渲染、PlantUML Server 或本地生成

14. 项目的版本需求:
    - 当前版本: 0.1.0
    - Rust Edition: 2021
    - 语义化版本控制

15. 项目的贡献需求:
    - (待补充 CONTRIBUTING.md)
    - 代码需通过 clippy 检查
    - 代码需通过 fmt 格式化
    - 测试需全部通过
    - 跨平台测试（Linux/macOS/Windows）

16. 项目的许可证需求: MIT License

17. 项目的代码质量需求:
    - Rust 2021 Edition
    - 零成本抽象
    - 内存安全保证
    - 无 unsafe 代码块（尽可能）
    - 错误处理使用 thiserror 和 anyhow
    - 跨平台条件编译规范

18. 项目的代码质量检查工具需求:
    - cargo clippy - Lint 检查
    - cargo fmt - 代码格式化
    - cargo test - 单元测试
    - cargo build --release - 发布构建
    - cross - 跨平台交叉编译

19. 项目的代码质量检查工具的配置需求:
    - .cargo/config.toml - Cargo 配置
    - rust-toolchain - Rust 工具链版本（通过 Cargo.toml edition = "2021" 指定）
    - Cross.toml - 交叉编译配置

20. 项目的代码质量检查工具的运行需求:
    - cargo clippy --all-targets --all-features -- -D warnings
    - cargo fmt -- --check
    - cargo test --all
    - cargo build --release
    - cross build --target x86_64-pc-windows-msvc (Windows 构建)
    - cross build --target aarch64-unknown-linux-musl (ARM Linux 构建)

21. 项目的代码质量检查工具的结果需求:
    - 无编译警告
    - 无 Clippy 警告
    - 测试全部通过
    - 代码格式符合 rustfmt 标准
    - 跨平台编译成功

22. 项目的代码质量检查工具的结果处理需求: 通过 CI/CD 流水线自动运行检查，并在 PR 中展示结果

23. 项目的代码质量检查工具的结果处理需求是将检查工具的结果展示给开发人员，帮助他们修复代码中的问题: 通过 GitHub Actions CI 流水线自动运行所有检查，失败时阻止合并，并在 PR 检查结果中详细展示问题位置和修复建议

24. 项目的跨平台兼容性需求:
    - 目标平台:
      - Linux (x86_64, aarch64) - glibc 和 musl
      - macOS (x86_64 Intel, aarch64 Apple Silicon)
      - Windows (x86_64 MSVC, x86_64 GNU)
      - FreeBSD (可选支持)
    - 条件编译策略:
      - `#[cfg(unix)]` - Unix Domain Socket 支持
      - `#[cfg(windows)]` - Named Pipe 支持
      - `#[cfg(all(unix, not(target_os = "windows")))]` - 纯 Unix 特性
    - 配置文件兼容:
      - 自动识别平台支持的监听方式
      - 不支持的监听方式返回明确错误提示
    - CI/CD 多平台测试:
      - Linux: Ubuntu 22.04
      - macOS: macOS 12+
      - Windows: Windows Server 2022
