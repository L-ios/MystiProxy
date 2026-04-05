# 配置说明

## 基本配置结构

MystiProxy 使用 YAML 格式的配置文件，主要包含以下部分：

1. **mysti.engine**: 引擎配置，定义多个代理服务
2. **cert**: 证书配置，用于 TLS 加密

## 监听地址格式

支持以下格式的监听地址：

| 格式 | 示例 | 说明 |
|------|------|------|
| TCP | `tcp://0.0.0.0:8080` | TCP 监听 |
| Unix Socket | `unix:///var/run/proxy.sock` | Unix Domain Socket 监听 |

## 代理类型

| 类型 | 说明 | 适用场景 |
|------|------|----------|
| `tcp` | 4 层代理 | 数据库、消息队列等 TCP 服务 |
| `http` | 7 层代理 | HTTP/HTTPS 服务，支持请求/响应转换 |

## 匹配模式

| 模式 | 说明 | 示例 |
|------|------|------|
| `Full` | 完全匹配 | `/api/health` 精确匹配 |
| `Prefix` | 前缀匹配 | `/api` 匹配 `/api/v1`、`/api/v2` 等 |
| `Regex` | 正则匹配 | `/api/users/\d+` 匹配数字 ID |
| `PrefixRegex` | 带正则的前缀匹配 | 组合前缀和正则匹配 |

### 匹配模式详解

```text
当 baseUri = /时，in_uri = /a/b/c 时，返回 Some(Prefix)
当 baseUri = /a/b/c 时，in_uri = /a/b/c/d/e 时，返回 Some(Prefix)
当 baseUri = /a/b/c 时，in_uri = /a/b/c 时，返回 Some(Full)
当 baseUri = /a/{id}/c 时，in_uri = /a/b/c 时，返回 Some(Regex)，其中 {id} 是参数，匹配 inUri 中的 b
当 baseUri = /a/{id}/c 时，in_uri = /a/b/c/d/e 时，返回 Some(PrefixRegex)，其中 {id} 是参数，匹配 inUri 中的 b
当 baseUri = /a/{id}/c 时，in_uri = /a/b/d/e/f 时，返回 None
```

## 提供者类型

| 类型 | 说明 | 用途 |
|------|------|------|
| `static` | 静态文件服务 | 提供静态资源访问 |
| `mock` | Mock 响应 | 模拟 API 响应，用于测试 |
| `proxy` | 代理转发 | 转发请求到后端服务 |

## 头部动作类型

| 动作 | 说明 | 使用场景 |
|------|------|----------|
| `overwrite` | 强制覆盖 | 替换现有头部值 |
| `missed` | 仅在缺失时添加 | 添加默认值 |
| `forceDelete` | 强制删除 | 移除敏感头部 |

## 配置文件示例

### 基础配置结构

```yaml
mysti:
  engine:
    docker:
      listen: tcp://0.0.0.0:3128
      target: unix:///var/run/docker.sock
      proxy_type: http # tcp
      timeout: 10s
      header:
        Host:
          value: localhost
          action: 'overwrite' # 默认就是 overwrite
          condition: '' # 默认值为 true，如果编写，则结果为 true 后，才能执行
      locations:
        # 采用网管的形式进行匹配，优先前缀匹配
        - location: '/a/b'
          mode: Prefix # 默认采用【5. 前缀匹配 => Prefix】，支持【1. 全量匹配 => Exact】，【3. 正则匹配 => Regex】，【4. 变量前缀匹配=>VariablePrefix】，【2. 变量形式匹配=>Variable】

          response:
            status: 200
            headers:
              test:
                value: good
            body:
              type: static
              alias: 'bbb'
    request:
      type: static
    request:
      method: 'get'
      uri: # 可能需要定义为 uriMapping
        path: '/a/c'
        query: 'a=b&c=d'
      headers:
        Host:
          value: localhost
          action: 'overwrite' # 默认值，强制复写
          condition: '' # 条件，为 true 才会执行
        forward-host:
          value: localhost
          action: 'missed' # 缺少，则添加
        x-host:
          action: 'forceDelete' # 有就删除
      body: # 只支持 json，并使用 jsonpath 进行处理
        json:
           path: '$.name'
           value: 'test'
           action: 'overwrite'
           condition: ''
    response:
      headers:
        Host:
          value: localhost
          action: 'overwrite' # 默认值，强制复写
          condition: '' # 条件，为 true 才会执行
      body:
        json:
          '$.name':
            value: 'test'
            action: 'overwrite'
            condition: ''
    containerd:
      listen: tcp://0.0.0.0:3128
      target: tcp://127.0.0.1:2765
      proxy_type: tcp

# 证书 单独声明,engine 中进行引用啊
cert:
  - name: client1
    root_key: ""
  - {}
```

### Location 配置示例

```yaml
- location: /a/b/c
  mode: Prefix
  provider: static # 静态私服
  alias: /var/www/html/
- location: /a/b/d # 多种匹配模式，估计才行
  provider: mock
  condition:
    - a: b
    - b: c
- path: /a/b/d # 全量匹配
  query: a=b&c=d # 需要动态匹配
  method: get # 固定匹配
  header:
    auth:
      value: good
      condition: xxxx
```

### JSON 配置示例

```json
[
  {
    "method": "GET,POST,put,*",
    "mode": "Full",
    "service": "test",
    "target_protocol": "http",
    "target_service": "test",
    "target_uri": "http://127.0.0.1:8080",
    "uri": "/test",
    "var_pattern": "test"
  },
  {
    "method": "GET,POST,put,*",
    "mode": "Full",
    "service": "test",
    "target_protocol": "http",
    "target_service": "test",
    "target_uri": "http://127.0.0.1:8080",
    "uri": "/test",
    "var_pattern": "test"
  }
]
```

## 下一步

- 查看 [使用示例](./EXAMPLES.md) 了解具体应用场景
- 查看 [高级配置](./ADVANCED.md) 了解更多配置选项
- 查看 [故障排查](./TROUBLESHOOTING.md) 解决常见问题
