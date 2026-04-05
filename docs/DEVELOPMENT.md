# 开发计划

本文档记录 MystiProxy 的开发计划和功能进度。

## 开发环境

### 系统要求

- Rust 1.75+ (Edition 2021)
- Cargo
- Git

### 构建项目

```bash
# 克隆项目
git clone <repository-url>
cd MystiProxy

# 构建项目
cargo build

# 构建发布版本
cargo build --release

# 运行测试
cargo test

# 运行代码检查
cargo clippy
```

## 功能进度 [0%]

### 核心功能

- [ ] 启动时不检测目标 URL 是否可达

### 4 层协议转发 [0%]

主要集中在 Socket 流上的转发：

| 监听方式 | 目标类型 | TCP | UDP | UDS |
|----------|----------|-----|-----|-----|
| 监听 | ✓ | ✓ | - | ✓ |
| TCP | ✓ | - | - | - |
| UDP | - | - | - | - |
| UDS | ✓ | - | - | - |

#### 待实现功能

- [ ] tcp://ip:port
- [ ] udp://ip:port
- [ ] unix://file
- [ ] 根据请求的 IP 过滤请求，或者拒绝连接

### 7 层协议转发 [0%]

主要是将 Socket 流解析为 HTTP 协议：

- [ ] http://ip:port
- [ ] https://ip:port
- [ ] unix+http:///run/var/unix/http.sock
- [ ] unix+https:///run/var/unix/https.sock [1](#footnote-1)
- [ ] HTTPS 协议的监听
- [ ] 双向认证的 HTTPS 协议的监听

### HTTP 内容解析 [0%]

- [ ] URI 的处理
- [ ] Header 的处理
- [ ] Body 的处理

### URI-Mapping 路由映射 [0%]

路由映射主要提供 4 种模式：

- **Full**: 全路径匹配
- **Prefix**: 前缀匹配
- **Regex**: 带参数的正则匹配
- **PrefixRegex**: 带正则的前缀匹配

### Mock 响应 [0%]

主要是根据请求中某些内容，直接进行响应，或者处理响应后再响应：

- [ ] 根据请求中的 URI，进行响应
- [ ] 根据请求头，进行响应
- [ ] 根据请求 Body，进行响应
  - [ ] Body 获取是 URI 中的 Query 部分
  - [ ] Query 型的 Body 进行匹配后响应
- [ ] 提供 Body 模版，然后特殊处理后，再进行响应
  - 例如结合一些请求转发的能力，将 Mock 的能力进行提升

## 开发路线图

### Phase 1: 核心功能 [进行中]

- [x] 项目结构搭建
- [ ] 配置文件解析
- [ ] 基础 TCP 代理
- [ ] 基础 HTTP 代理
- [ ] 日志系统

### Phase 2: 高级功能 [计划中]

- [ ] TLS 支持
- [ ] 路由映射
- [ ] 请求/响应转换
- [ ] Mock 功能

### Phase 3: 生产就绪 [计划中]

- [ ] 性能优化
- [ ] 监控指标
- [ ] 健康检查
- [ ] 文档完善

### Phase 4: 扩展功能 [计划中]

- [ ] 插件系统
- [ ] Web UI
- [ ] 集群支持
- [ ] 高级鉴权

## 贡献指南

### 开发流程

1. Fork 项目
2. 创建功能分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'Add some amazing feature'`)
4. 推送到分支 (`git push origin feature/amazing-feature`)
5. 创建 Pull Request

### 代码规范

- 遵循 Rust 标准代码规范
- 使用 `cargo fmt` 格式化代码
- 使用 `cargo clippy` 检查代码质量
- 编写单元测试和集成测试
- 更新相关文档

### 提交信息规范

使用约定式提交：

- `feat`: 新功能
- `fix`: 修复 Bug
- `docs`: 文档更新
- `style`: 代码格式调整
- `refactor`: 代码重构
- `test`: 测试相关
- `chore`: 构建/工具相关

示例：

```
feat: add TLS support for HTTPS proxy
fix: resolve connection timeout issue
docs: update configuration examples
```

## 测试

### 单元测试

```bash
# 运行所有测试
cargo test

# 运行特定测试
cargo test test_name

# 运行测试并显示输出
cargo test -- --nocapture
```

### 集成测试

```bash
# 运行集成测试
cargo test --test integration_tests

# 运行特定集成测试
cargo test --test integration_tests -- test_name
```

### 性能测试

```bash
# 运行性能测试
cargo bench
```

## 发布流程

### 版本号规范

使用语义化版本：

- MAJOR: 不兼容的 API 更改
- MINOR: 向后兼容的功能新增
- PATCH: 向后兼容的问题修复

### 发布步骤

1. 更新版本号
2. 更新 CHANGELOG
3. 创建 Git 标签
4. 构建发布版本
5. 创建 GitHub Release
6. 发布到 Crates.io（可选）

## 获取帮助

- **文档**: [docs/](./)
- **问题反馈**: [GitHub Issues](https://github.com/your-repo/mystiproxy/issues)
- **讨论**: [GitHub Discussions](https://github.com/your-repo/mystiproxy/discussions)

## Footnotes

<a name="footnote-1">[1]</a>: Unix 中也可以传输 HTTPS 协议的内容，HTTPS 主要是 TCP Socket 上将其通过 TLS 进行加密
