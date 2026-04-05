# 高级配置

本文档介绍 MystiProxy 的高级配置选项和性能优化方法。

## 超时设置

支持多种时间单位：

```yaml
timeout: 10s    # 10 秒
timeout: 5m     # 5 分钟
timeout: 1h     # 1 小时
timeout: 500ms  # 500 毫秒
```

### 连接超时

```yaml
mysti:
  engine:
    api-gateway:
      listen: tcp://0.0.0.0:8080
      proxy_type: http
      timeout:
        connect: 10s      # 连接超时
        read: 30s         # 读取超时
        write: 30s        # 写入超时
        idle: 60s         # 空闲超时
```

## 条件匹配

支持多种条件类型，用于精细控制请求处理：

### 头部条件

```yaml
condition:
  - type: header
    key: X-Debug
    value: "true"
    operator: equals  # equals, contains, regex
```

### 查询参数条件

```yaml
condition:
  - type: query
    key: mock
    value: "true"
```

### 请求体条件

```yaml
condition:
  - type: body
    path: "$.test"
    value: "true"
```

### 组合条件

```yaml
condition:
  logic: and  # and, or
  rules:
    - type: header
      key: X-Debug
      value: "true"
    - type: query
      key: mock
      value: "true"
```

## JSONPath 表达式

在请求/响应体转换中使用 JSONPath：

### 基本语法

```yaml
body:
  type: json
  json:
    # 访问对象属性
    path: "$.user.name"

    # 访问数组元素
    path: "$.users[0].id"

    # 访问所有匹配项
    path: "$.users[*].name"

    # 条件过滤
    path: "$.users[?(@.age > 18)]"
```

### 转换操作

```yaml
body:
  type: json
  json:
    # 覆盖值
    - path: "$.user.name"
      value: "anonymous"
      action: overwrite

    # 添加新字段
    - path: "$.timestamp"
      value: "2024-01-01T00:00:00Z"
      action: add

    # 删除字段
    - path: "$.sensitive"
      action: delete
```

## 性能优化

### 连接池配置

MystiProxy 自动管理连接池，可以通过以下参数调整：

```yaml
mysti:
  engine:
    api-gateway:
      pool:
        max_connections: 1000      # 最大连接数
        max_idle_connections: 100  # 最大空闲连接数
        idle_timeout: 90s          # 空闲连接超时
```

### 并发处理

基于 Tokio 异步运行时，支持高并发连接：

```yaml
mysti:
  runtime:
    worker_threads: 4        # 工作线程数
    max_blocking_threads: 8  # 最大阻塞线程数
    thread_stack_size: 2MB   # 线程栈大小
```

### 内存优化

使用流式处理，避免大内存占用：

```yaml
mysti:
  engine:
    api-gateway:
      streaming:
        enabled: true
        chunk_size: 8192     # 流式传输块大小
        max_body_size: 10MB  # 最大请求体大小
```

### 缓冲区配置

```yaml
mysti:
  engine:
    api-gateway:
      buffer:
        read_buffer_size: 8192    # 读缓冲区大小
        write_buffer_size: 8192   # 写缓冲区大小
```

## 高级路由

### 路由优先级

```yaml
mysti:
  engine:
    api-gateway:
      locations:
        # 精确匹配优先级最高
        - location: /api/health
          mode: Full
          priority: 100

        # 前缀匹配优先级较低
        - location: /api
          mode: Prefix
          priority: 10
```

### 路由权重

```yaml
mysti:
  engine:
    load-balancer:
      locations:
        - location: /api
          mode: Prefix
          backends:
            - url: http://backend1:8080
              weight: 70
            - url: http://backend2:8080
              weight: 30
```

### 健康检查

```yaml
mysti:
  engine:
    load-balancer:
      health_check:
        enabled: true
        interval: 30s
        timeout: 5s
        unhealthy_threshold: 3
        healthy_threshold: 2
        path: /health
```

## 请求/响应转换

### 高级 URI 转换

```yaml
request:
  uri:
    # 路径重写
    path:
      from: "/api/v1/(.*)"
      to: "/api/v2/$1"

    # 查询参数转换
    query:
      add:
        version: "2.0"
      remove:
        - debug
      rename:
        old_name: new_name
```

### 高级头部转换

```yaml
request:
  headers:
    # 条件性添加
    - name: X-Request-ID
      value: "$uuid"
      action: missed
      condition:
        type: header
        key: X-Request-ID
        operator: not_exists

    # 正则替换
    - name: Authorization
      value: "Bearer $1"
      pattern: "Token (.*)"
      action: replace
```

### 高级 Body 转换

```yaml
request:
  body:
    type: template
    template: |
      {
        "original": {{ .RequestBody }},
        "timestamp": "{{ now }}",
        "user": "{{ .Header "X-User" }}",
        "transformed": true
      }
```

## 限流和熔断

### 请求限流

```yaml
mysti:
  engine:
    api-gateway:
      rate_limit:
        enabled: true
        requests_per_second: 100
        burst: 200
        per_ip: true
```

### 熔断器

```yaml
mysti:
  engine:
    api-gateway:
      circuit_breaker:
        enabled: true
        failure_threshold: 5
        success_threshold: 2
        timeout: 60s
```

## 监控和指标

### Prometheus 指标

```yaml
mysti:
  engine:
    api-gateway:
      metrics:
        enabled: true
        port: 9090
        path: /metrics
        labels:
          environment: production
          service: mystiproxy
```

### 健康检查端点

```yaml
mysti:
  engine:
    api-gateway:
      health_check:
        enabled: true
        path: /health
        detailed: true
```

## 下一步

- 查看 [故障排查](./TROUBLESHOOTING.md) 解决常见问题
- 查看 [部署建议](./DEPLOYMENT.md) 了解生产环境部署
- 查看 [架构设计](./ARCHITECTURE.md) 了解系统架构
