# MystiProxy 微服务模拟器 Spec

## Why
在微服务架构中，服务迁移、调试、中间态验证等问题常常困扰开发者。MystiProxy 旨在提供一个灵活的网络代理工具，支持 4 层和 7 层协议转发、Mock 响应、网关能力，帮助开发者解决服务调试和网络隔离问题。

## What Changes
- 实现 4 层协议转发（TCP、UDP、UDS）
- 实现 7 层 HTTP/HTTPS 代理转发
- 实现静态文件服务能力
- 实现 Mock 响应能力
- 实现网关路由映射能力
- 实现 TLS 鉴权能力（单向/双向）
- 实现 HTTP Header 鉴权能力

## Impact
- 这是一个新项目，无现有代码影响
- 需要设计模块化的配置系统
- 需要支持多种监听和转发协议

## ADDED Requirements

### Requirement: 4层协议转发
系统应提供 4 层协议转发能力，支持 TCP、UDP、UDS 之间的互相转发。

#### Scenario: TCP 监听转发到 UDS
- **WHEN** 用户配置 `listen: tcp://0.0.0.0:3128` 和 `target: unix:///var/run/docker.sock`
- **THEN** 系统应监听 TCP 端口并将流量转发到 UDS 目标

#### Scenario: UDS 监听转发到 TCP
- **WHEN** 用户配置 `listen: unix:///var/run/proxy.sock` 和 `target: tcp://127.0.0.1:8080`
- **THEN** 系统应监听 UDS 并将流量转发到 TCP 目标

### Requirement: 7层 HTTP 代理转发
系统应提供 HTTP/HTTPS 代理转发能力，支持请求和响应的转换。

#### Scenario: HTTP 代理转发
- **WHEN** 用户配置 HTTP 代理规则
- **THEN** 系统应解析 HTTP 请求，转发到目标地址，并返回响应

#### Scenario: HTTPS 双向认证
- **WHEN** 用户配置 HTTPS 监听并启用双向认证
- **THEN** 系统应验证客户端证书并建立安全连接

### Requirement: URI 路由映射
系统应提供多种 URI 匹配模式。

#### Scenario: 全路径匹配
- **WHEN** 配置 `mode: Full` 且 `uri: /a/b/c`
- **THEN** 只有完全匹配 `/a/b/c` 的请求才会被处理

#### Scenario: 前缀匹配
- **WHEN** 配置 `mode: Prefix` 且 `uri: /a/b/`
- **THEN** 所有以 `/a/b/` 开头的请求都会被处理

#### Scenario: 正则匹配
- **WHEN** 配置 `mode: Regex` 且 `uri: /a/{id}/c`
- **THEN** 系统应提取 `{id}` 作为参数并匹配请求

### Requirement: Mock 响应
系统应提供 Mock 响应能力，支持根据请求条件返回预设响应。

#### Scenario: 根据 URI Mock
- **WHEN** 配置 Mock 规则匹配特定 URI
- **THEN** 系统应直接返回预设的响应内容

#### Scenario: 根据条件 Mock
- **WHEN** 配置 Mock 规则包含请求头或 Body 条件
- **THEN** 系统应在条件满足时返回预设响应

### Requirement: 静态文件服务
系统应提供静态文件服务能力。

#### Scenario: 目录映射
- **WHEN** 配置 `provider: static` 和 `alias: /var/www/html/`
- **THEN** 系统应将请求的 URI 映射到文件系统路径并返回文件内容

### Requirement: 请求/响应转换
系统应提供请求和响应的转换能力。

#### Scenario: Header 转换
- **WHEN** 配置 Header 增删改规则
- **THEN** 系统应在转发前/返回前执行 Header 转换

#### Scenario: Body 转换
- **WHEN** 配置 Body JSON 转换规则
- **THEN** 系统应使用 JSONPath 处理 JSON Body

### Requirement: 配置系统
系统应支持 YAML 格式的配置文件。

#### Scenario: 多 Engine 配置
- **WHEN** 配置文件包含多个 engine
- **THEN** 系统应同时启动多个代理服务

#### Scenario: 证书配置
- **WHEN** 配置 TLS 证书
- **THEN** 系统应使用证书进行 TLS 加密和认证

## MODIFIED Requirements
无（新项目）

## REMOVED Requirements
无（新项目）
