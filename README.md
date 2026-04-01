# MystiProxy

灵活的 HTTP/TCP 代理服务器，支持路由匹配和 Mock 响应。

## 已实现功能

### 代理类型

| 类型 | 说明 |
|------|------|
| TCP | 4 层转发，支持 `tcp://` 和 `unix://` 地址 |
| HTTP | 7 层转发，支持路由匹配、Mock 和静态文件服务 |

### HTTP 路由能力

- **匹配模式**: Full、Prefix、Regex、PrefixRegex
- **提供者类型**: proxy（转发）、mock（返回 Mock 响应）、static（静态文件）
- **请求改写**: 支持 method、URI path/query、headers 的基础改写
- **Mock 响应**: 支持按 location 返回自定义 status 和 headers
- **Header 操作**: 当前主流程已验证 `overwrite`、`missed`

### 配置字段

- `listen` / `target`: 支持 `tcp://` 和 `unix://`
- `proxy_type`: `tcp` 或 `http`
- `request_timeout` / `connection_timeout`: 超时配置
- `header`: 全局请求头转换
- `locations`: 路由规则数组

## 额外扩展 / 实验性能力

以下模块已实现但未完全集成到默认主流程：

- **TLS 认证** (`src/tls/`): 支持单向认证和双向 mTLS（需手动配置）
- **HTTP 鉴权** (`src/http/auth.rs`): Header 鉴权和 JWT 验证（需手动集成）
- **请求体 JSON 转换** (`src/http/body.rs`): JSONPath body 修改（部分支持）

## 快速开始

### 编译

```bash
cargo build --release
```

### 运行

```bash
# 配置文件启动
./target/release/mystiproxy --config config.yaml

# 命令行快速启动（TCP 代理）
./target/release/mystiproxy --listen tcp://0.0.0.0:3128 --target tcp://127.0.0.1:3306

# 调试日志
RUST_LOG=debug ./target/release/mystiproxy --config config.yaml
```

### 测试

```bash
cargo test
```

## 最小配置示例

```yaml
mysti:
  engine:
    docker:
      listen: tcp://0.0.0.0:3128
      target: unix:///var/run/docker.sock
      proxy_type: http
      request_timeout: 10s
      connection_timeout: 5s
      header:
        Host:
          value: localhost
          action: overwrite
      locations:
        - location: /health
          mode: Full
          provider: mock
          response:
            status: 200
        - location: /static
          mode: Prefix
          provider: static
          root: /var/www/html
```

## 项目结构

```
src/
├── main.rs           # 入口
├── config/           # 配置解析
├── proxy/            # TCP/Unix 代理
├── http/             # HTTP 服务器和处理
│   ├── handler.rs    # 请求处理器
│   ├── client.rs     # HTTP 客户端
│   ├── header.rs     # Header 处理
│   ├── auth.rs       # 鉴权（实验性）
│   └── body.rs       # Body 转换（实验性）
├── router/           # 路由匹配
├── mock/             # Mock 服务
├── tls/              # TLS 模块（实验性）
└── io/               # I/O 抽象
```

> 注: `README.org` 包含历史和规划内容，`README.md` 为当前简化文档。

## 依赖

- tokio - 异步运行时
- hyper - HTTP 库
- serde_yaml - 配置解析
- tracing - 日志
