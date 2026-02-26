# Tasks

## Phase 1: 项目基础架构
- [x] Task 1: 初始化 Rust 项目结构
  - [x] SubTask 1.1: 创建 Cargo 项目并配置依赖（tokio、hyper、serde、yaml 等）
  - [x] SubTask 1.2: 设计模块结构（config、proxy、http、mock、tls 等）
  - [x] SubTask 1.3: 定义核心错误类型和 Result 别名

## Phase 2: 配置系统
- [x] Task 2: 实现配置解析系统
  - [x] SubTask 2.1: 定义配置结构体（Engine、Location、Header、Body 等）
  - [x] SubTask 2.2: 实现 YAML 配置文件解析
  - [x] SubTask 2.3: 实现配置验证逻辑

## Phase 3: 4层协议转发
- [x] Task 3: 实现 TCP 监听和转发
  - [x] SubTask 3.1: 实现 TCP Listener
  - [x] SubTask 3.2: 实现 TCP 到 TCP 的转发
  - [x] SubTask 3.3: 实现 TCP 到 UDS 的转发
- [x] Task 4: 实现 UDS 监听和转发
  - [x] SubTask 4.1: 实现 UDS Listener
  - [x] SubTask 4.2: 实现 UDS 到 TCP 的转发
  - [x] SubTask 4.3: 实现 UDS 到 UDS 的转发

## Phase 4: 7层 HTTP 代理
- [x] Task 5: 实现 HTTP 代理核心
  - [x] SubTask 5.1: 实现 HTTP 请求解析
  - [x] SubTask 5.2: 实现 HTTP 客户端转发
  - [x] SubTask 5.3: 实现 HTTP 响应返回
- [x] Task 6: 实现 URI 路由匹配
  - [x] SubTask 6.1: 实现 Full 全路径匹配
  - [x] SubTask 6.2: 实现 Prefix 前缀匹配
  - [x] SubTask 6.3: 实现 Regex 正则匹配
  - [x] SubTask 6.4: 实现 PrefixRegex 带正则的前缀匹配

## Phase 5: 请求/响应转换
- [x] Task 7: 实现 Header 转换
  - [x] SubTask 7.1: 实现 Header 增加逻辑
  - [x] SubTask 7.2: 实现 Header 替换逻辑
  - [x] SubTask 7.3: 实现 Header 删除逻辑
  - [x] SubTask 7.4: 实现条件判断逻辑
- [x] Task 8: 实现 Body 转换
  - [x] SubTask 8.1: 实现 JSON Body 解析
  - [x] SubTask 8.2: 实现 JSONPath 查询和修改

## Phase 6: Mock 和静态服务
- [x] Task 9: 实现 Mock 响应
  - [x] SubTask 9.1: 实现 URI 条件 Mock
  - [x] SubTask 9.2: 实现 Header 条件 Mock
  - [x] SubTask 9.3: 实现 Body 条件 Mock
- [x] Task 10: 实现静态文件服务
  - [x] SubTask 10.1: 实现目录映射
  - [x] SubTask 10.2: 实现默认索引文件处理

## Phase 7: TLS/鉴权
- [x] Task 11: 实现 TLS 支持
  - [x] SubTask 11.1: 实现 TLS 服务端配置
  - [x] SubTask 11.2: 实现单向 TLS 认证
  - [x] SubTask 11.3: 实现双向 TLS 认证
- [x] Task 12: 实现 HTTP 鉴权
  - [x] SubTask 12.1: 实现 Header 鉴权
  - [x] SubTask 12.2: 实现 JWT 验证

## Phase 8: 测试和文档
- [x] Task 13: 编写单元测试和集成测试
- [x] Task 14: 编写使用文档和示例配置

# Task Dependencies
- Task 2 依赖 Task 1
- Task 3、4 依赖 Task 2
- Task 5、6 依赖 Task 2
- Task 7、8 依赖 Task 5
- Task 9、10 依赖 Task 5、Task 6
- Task 11 依赖 Task 3、Task 5
- Task 12 依赖 Task 5
- Task 13、14 依赖所有前置任务
