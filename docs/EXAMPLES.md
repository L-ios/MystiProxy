# 使用示例

本文档包含 MystiProxy 的各种使用场景示例。

## 示例 1: Docker Socket 代理

将 Docker Socket 代理到 TCP 端口，允许远程访问 Docker API：

```yaml
mysti:
  engine:
    docker:
      listen: tcp://0.0.0.0:3128
      target: unix:///var/run/docker.sock
      proxy_type: http
      timeout: 10s
      header:
        Host:
          value: localhost
          action: overwrite
```

使用方式：

```bash
# 列出容器
curl http://localhost:3128/containers/json

# 查看容器详情
curl http://localhost:3128/containers/{container_id}/json
```

## 示例 2: MySQL TCP 代理

代理 MySQL 数据库连接：

```yaml
mysti:
  engine:
    mysql:
      listen: tcp://0.0.0.0:3307
      target: tcp://127.0.0.1:3306
      proxy_type: tcp
      timeout: 30s
```

使用方式：

```bash
# 连接到代理端口
mysql -h 127.0.0.1 -P 3307 -u root -p
```

## 示例 3: 静态文件服务

提供静态文件访问：

```yaml
mysti:
  engine:
    static-server:
      listen: tcp://0.0.0.0:8080
      target: tcp://127.0.0.1:8080
      proxy_type: http
      locations:
        - location: /static
          mode: Prefix
          provider: static
          alias: /var/www/html
          response:
            headers:
              Cache-Control:
                value: "public, max-age=3600"
                action: overwrite
```

使用方式：

```bash
# 访问静态文件
curl http://localhost:8080/static/index.html
```

## 示例 4: Mock API 响应

模拟 API 响应，用于前端开发或测试：

```yaml
mysti:
  engine:
    mock-server:
      listen: tcp://0.0.0.0:8080
      target: tcp://127.0.0.1:8080
      proxy_type: http
      locations:
        # 健康检查
        - location: /health
          mode: Full
          provider: mock
          response:
            status: 200
            headers:
              Content-Type:
                value: "application/json"
                action: overwrite
            body:
              type: static
              content: '{"status": "healthy"}'

        # 条件 Mock
        - location: /api
          mode: Prefix
          provider: mock
          condition:
            - type: header
              value: "X-Mock-Mode=true"
          response:
            status: 200
            body:
              type: static
              content: '{"mocked": true, "data": []}'
```

使用方式：

```bash
# 健康检查
curl http://localhost:8080/health

# 条件 Mock（需要特定头部）
curl -H "X-Mock-Mode: true" http://localhost:8080/api/users
```

## 示例 5: API 网关

作为 API 网关，转发请求到后端服务：

```yaml
mysti:
  engine:
    api-gateway:
      listen: tcp://0.0.0.0:8080
      target: tcp://backend.example.com:80
      proxy_type: http
      timeout: 30s
      header:
        X-Proxy-By:
          value: "MystiProxy"
          action: missed
      locations:
        # 用户服务
        - location: /api/users
          mode: Prefix
          provider: proxy
          request:
            uri:
              path: /users
            headers:
              X-Real-IP:
                value: "$client_ip"
                action: missed

        # 订单服务
        - location: /api/orders
          mode: Prefix
          provider: proxy
          request:
            uri:
              path: /orders
```

## 示例 6: TLS 双向认证

配置 TLS 双向认证（需要证书）：

```yaml
mysti:
  engine:
    secure-proxy:
      listen: tcp://0.0.0.0:8443
      target: tcp://127.0.0.1:8080
      proxy_type: http
      # TLS 配置（需要实现）
      # tls:
      #   cert: /path/to/server.crt
      #   key: /path/to/server.key
      #   client_ca: /path/to/ca.crt

cert:
  - name: server-cert
    root_key: ""
  - name: client-ca
    root_key: ""
```

## 示例 7: 请求/响应转换

转换请求和响应内容：

```yaml
mysti:
  engine:
    transform-proxy:
      listen: tcp://0.0.0.0:8080
      target: tcp://backend.example.com:80
      proxy_type: http
      locations:
        - location: /api/v1
          mode: Prefix
          provider: proxy
          request:
            # URI 转换
            uri:
              path: /api/v2
              query: "version=1"

            # 请求头转换
            headers:
              Host:
                value: backend.example.com
                action: overwrite
              X-Auth-Token:
                action: forceDelete

            # 请求体转换（JSON）
            body:
              type: json
              json:
                path: "$.user.name"
                value: "anonymous"
                action: overwrite

          response:
            # 响应头转换
            headers:
              Server:
                value: "MystiProxy"
                action: overwrite

            # 响应体转换（JSON）
            body:
              type: json
              json:
                path: "$.timestamp"
                value: "2024-01-01T00:00:00Z"
                action: add
```

## 下一步

- 查看 [配置说明](./CONFIGURATION.md) 了解详细配置选项
- 查看 [高级配置](./ADVANCED.md) 了解更多高级功能
- 查看 [部署建议](./DEPLOYMENT.md) 了解生产环境部署
