# 配置指南

本指南介绍如何编写 MystiProxy 的 YAML 配置文件。

## 配置文件格式

MystiProxy 使用 YAML 格式的配置文件。通过 `--config` 参数指定配置文件路径：

```bash
./target/release/mystiproxy --config config.yaml
```

## 顶层结构

配置文件顶层包含两个字段：

| 字段 | 类型 | 描述 |
|------|------|------|
| `mysti` | Mysti | 引擎容器，包含所有代理服务配置 |
| `cert` | Vec<CertConfig> | TLS 证书配置（可选） |

```yaml
mysti:
  engine:
    # 引擎配置
cert:
  - name: client1
    root_key: ""
```

### CertConfig 字段

| 字段 | 类型 | 描述 |
|------|------|------|
| `name` | String | 证书名称 |
| `root_key` | String | 根密钥 |

## EngineConfig 字段

每个引擎配置代表一个独立的代理服务。

| 字段 | 类型 | 描述 |
|------|------|------|
| `listen` | String | 监听地址，支持 `tcp://` 和 `unix://` 协议 |
| `target` | String | 目标地址，支持 `tcp://` 和 `unix://` 协议 |
| `proxy_type` | ProxyType | 代理类型：`tcp` 或 `http` |
| `request_timeout` | Option<Duration> | 请求超时时间（注：`timeout` 为兼容别名） |
| `connection_timeout` | Option<Duration> | 连接超时时间 |
| `header` | Option<HashMap<String, HeaderAction>> | 全局请求头修改配置 |
| `locations` | Option<Vec<LocationConfig>> | 路由规则配置 |

### ProxyType 枚举值

- `tcp`：4 层 TCP 转发
- `http`：7 层 HTTP 代理，支持路由匹配和请求改写

## LocationConfig 字段

用于 HTTP 代理的路由规则配置。

| 字段 | 类型 | 描述 |
|------|------|------|
| `location` | String | 路径匹配规则 |
| `mode` | MatchMode | 匹配模式 |
| `provider` | Option<ProviderType> | 请求处理者类型：proxy/mock/static |
| `root` | Option<String> | 静态文件根目录（provider 为 static 时使用） |
| `response` | Option<ResponseConfig> | 响应配置（provider 为 mock 时使用） |
| `request` | Option<RequestConfig> | 请求改写配置 |

### MatchMode 枚举值

- `Full`：完全匹配（精确路径）
- `Prefix`：前缀匹配
- `Regex`：正则表达式匹配
- `PrefixRegex`：前缀正则匹配

### ProviderType 枚举值

- `proxy`：代理转发（默认）
- `mock`：返回自定义响应
- `static`：静态文件服务

## HeaderAction 字段

用于修改 HTTP 请求头或响应头。

| 字段 | 类型 | 描述 |
|------|------|------|
| `value` | String | 头部值 |
| `action` | HeaderActionType | 动作类型 |
| `condition` | Option<String> | 条件（可选） |

### HeaderActionType 枚举值

- `overwrite`：覆盖已有值
- `missed`：仅在头部不存在时添加
- `forceDelete`：强制删除头部

## ResponseConfig 字段

配置 Mock 响应。

| 字段 | 类型 | 描述 |
|------|------|------|
| `status` | Option<u16> | HTTP 状态码 |
| `headers` | Option<HashMap<String, HeaderAction>> | 响应头 |
| `body` | Option<BodyConfig> | 响应体 |

## RequestConfig 字段

配置请求改写。

| 字段 | 类型 | 描述 |
|------|------|------|
| `method` | Option<String> | 请求方法 |
| `uri` | Option<UriConfig> | URI 配置 |
| `headers` | Option<HashMap<String, HeaderAction>> | 请求头 |
| `body` | Option<BodyConfig> | 请求体 |

### UriConfig 字段

| 字段 | 类型 | 描述 |
|------|------|------|
| `path` | Option<String> | 路径 |
| `query` | Option<String> | 查询参数 |

### BodyConfig 字段

| 字段 | 类型 | 描述 |
|------|------|------|
| `type` | Option<BodyType> | 请求体类型 |
| `json` | Option<JsonBodyConfig> | JSON 改写配置 |

### BodyType 枚举值

- `static`：静态文本
- `json`：JSON 格式

### JsonBodyConfig 字段

| 字段 | 类型 | 描述 |
|------|------|------|
| `path` | String | JSONPath 路径 |
| `value` | String | 值 |
| `action` | JsonBodyAction | 动作：overwrite/add/delete |

## 时间格式

Duration 字段支持以下时间格式：

| 格式 | 示例 | 说明 |
|------|------|------|
| `数字s` | `10s` | 秒 |
| `数字m` | `5m` | 分钟 |
| `数字h` | `1h` | 小时 |
| `数字ms` | `500ms` | 毫秒 |
| 小数 | `1.5s` | 支持小数 |

## 使用场景示例

### Docker Socket 代理

将本地 TCP 端口代理到 Docker socket：

```yaml
mysti:
  engine:
    docker:
      listen: tcp://0.0.0.0:3128
      target: unix:///var/run/docker.sock
      proxy_type: http
      request_timeout: 30s
      header:
        Host:
          value: localhost
          action: overwrite
```

说明：
- 监听 `0.0.0.0:3128` 接受 HTTP 请求
- 转发到 `unix:///var/run/docker.sock`
- 将 Host 头覆盖为 `localhost`

### MySQL TCP 代理

简单的 TCP 转发代理：

```yaml
mysti:
  engine:
    mysql:
      listen: tcp://0.0.0.0:3306
      target: tcp://192.168.1.100:3306
      proxy_type: tcp
      connection_timeout: 10s
```

说明：
- 4 层 TCP 代理，无 HTTP 路由功能
- 适用于 MySQL、Redis 等 TCP 协议代理

### 静态文件服务

基于目录的静态文件服务器：

```yaml
mysti:
  engine:
    static:
      listen: tcp://0.0.0.0:8080
      target: tcp://127.0.0.1:8080
      proxy_type: http
      locations:
        - location: /static
          mode: Prefix
          provider: static
          root: /var/www/html
```

说明：
- `/static/*` 请求映射到 `/var/www/html/` 目录
- 支持目录浏览和文件下载

### Mock API

模拟 API 响应：

```yaml
mysti:
  engine:
    api-mock:
      listen: tcp://0.0.0.0:8080
      target: tcp://127.0.0.1:8080
      proxy_type: http
      locations:
        - location: /api/users
          mode: Full
          provider: mock
          response:
            status: 200
            headers:
              Content-Type:
                value: application/json
                action: overwrite
            body:
              type: static
        - location: /api/health
          mode: Full
          provider: mock
          response:
            status: 204
```

说明：
- `/api/users` 返回 JSON 格式响应
- `/api/health` 返回 204 无内容

### API 网关

多路由配置与请求改写：

```yaml
mysti:
  engine:
    gateway:
      listen: tcp://0.0.0.0:8080
      target: tcp://127.0.0.1:8080
      proxy_type: http
      header:
        X-Forwarded-For:
          value: proxy
          action: missed
      locations:
        - location: /api/v1/users
          mode: Prefix
          provider: proxy
          request:
            uri:
              path: /users
              query: version=v1
            headers:
              X-API-Version:
                value: v1
                action: overwrite
        - location: /api/v1/orders
          mode: Prefix
          provider: proxy
          request:
            uri:
              path: /orders
        - location: /docs
          mode: Prefix
          provider: static
          root: /var/www/docs
        - location: /internal/error
          mode: Full
          provider: mock
          response:
            status: 500
            body:
              type: static
```

说明：
- `/api/v1/users/*` 前缀匹配，路径重写为 `/users`，添加 API 版本头
- `/api/v1/orders/*` 前缀匹配，路径重写为 `/orders`
- `/docs/*` 静态文件服务
- `/internal/error` 返回 500 错误响应

### 多引擎配置

同时运行多个代理服务：

```yaml
mysti:
  engine:
    docker-proxy:
      listen: tcp://0.0.0.0:3128
      target: unix:///var/run/docker.sock
      proxy_type: http
      request_timeout: 30s
      header:
        Host:
          value: localhost
          action: overwrite

    mysql-proxy:
      listen: tcp://0.0.0.0:3306
      target: tcp://192.168.1.100:3306
      proxy_type: tcp

    redis-proxy:
      listen: tcp://0.0.0.0:6379
      target: tcp://192.168.1.101:6379
      proxy_type: tcp
      connection_timeout: 5s

    web-static:
      listen: tcp://0.0.0.0:8080
      target: tcp://127.0.0.1:8080
      proxy_type: http
      locations:
        - location: /
          mode: Prefix
          provider: static
          root: /var/www/public

cert:
  - name: server-cert
    root_key: ""
```

说明：
- `docker-proxy`：HTTP 代理到 Docker socket
- `mysql-proxy`：MySQL TCP 代理
- `redis-proxy`：Redis TCP 代理
- `web-static`：静态网站服务
- 证书配置示例