# Checklist

## 项目基础架构
- [x] Cargo.toml 包含必要依赖（tokio、hyper、serde、serde_yaml 等）
- [x] 项目模块结构清晰（config、proxy、http、mock、tls 等）
- [x] 核心错误类型定义完整

## 配置系统
- [x] 配置结构体能正确解析 YAML 文件
- [x] 配置验证逻辑能检测无效配置
- [x] 支持多 Engine 配置

## 4层协议转发
- [x] TCP 监听功能正常
- [x] UDS 监听功能正常
- [x] TCP 到 TCP 转发正常
- [x] TCP 到 UDS 转发正常
- [x] UDS 到 TCP 转发正常
- [x] UDS 到 UDS 转发正常

## 7层 HTTP 代理
- [x] HTTP 请求解析正确
- [x] HTTP 客户端转发正常
- [x] HTTP 响应返回正确
- [x] Full 全路径匹配正确
- [x] Prefix 前缀匹配正确
- [x] Regex 正则匹配正确
- [x] PrefixRegex 带正则的前缀匹配正确

## 请求/响应转换
- [x] Header 增加逻辑正确
- [x] Header 替换逻辑正确
- [x] Header 删除逻辑正确
- [x] 条件判断逻辑正确
- [x] JSON Body 解析正确
- [x] JSONPath 查询和修改正确

## Mock 和静态服务
- [x] URI 条件 Mock 正常
- [x] Header 条件 Mock 正常
- [x] Body 条件 Mock 正常
- [x] 静态文件服务正常
- [x] 默认索引文件处理正常

## TLS/鉴权
- [x] TLS 服务端配置正确
- [x] 单向 TLS 认证正常
- [x] 双向 TLS 认证正常
- [x] Header 鉴权正常
- [x] JWT 验证正常

## 测试
- [x] 单元测试覆盖核心功能 (72 tests passed)
- [x] 文档测试通过 (11 tests passed)

## 文档
- [x] 示例配置文件 (config.example.yaml)
- [x] 使用文档 (README.org 更新)
