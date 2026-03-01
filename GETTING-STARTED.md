# MystiProxy 新手入门指南

## 30 秒了解 MystiProxy

用一句话介绍：MystiProxy 是一个灵活的代理服务器，可以将 TCP/Unix Socket 流量转发到目标地址，支持 HTTP 请求转换和 Mock 响应。

## 5 分钟快速体验

### 步骤 1: 获取程序

```bash
# 编译
cargo build --release
```

### 步骤 2: 创建最简配置

创建 `config.yaml` 文件：

```yaml
mysti:
  engine:
    demo:
      listen: tcp://127.0.0.1:8080
      target: tcp://127.0.0.1:80
      proxy_type: tcp
```

### 步骤 3: 启动服务

```bash
./target/release/mystiproxy --config config.yaml
```

### 步骤 4: 测试

```bash
curl http://127.0.0.1:8080
```

## 核心概念

### 监听地址 (listen)

服务监听的位置，支持：

- `tcp://0.0.0.0:8080` - TCP 监听
- `unix:///var/run/proxy.sock` - Unix Socket 监听

### 目标地址 (target)

流量转发的目的地，支持：

- `tcp://127.0.0.1:80` - TCP 连接
- `unix:///var/run/docker.sock` - Unix Socket 连接

### 代理类型 (proxy_type)

- `tcp` - 4 层代理，直接转发 TCP 流量
- `http` - 7 层代理，解析 HTTP 协议，支持请求转换

## 常见使用场景

### 场景 1: 代理 Docker Socket

将 Docker Socket 代理到 TCP 端口，允许远程访问 Docker API：

```yaml
mysti:
  engine:
    docker-proxy:
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

### 场景 2: 代理数据库

代理 MySQL 数据库连接：

```yaml
mysti:
  engine:
    mysql-proxy:
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

### 场景 3: Mock API

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

## 下一步学习

- 阅读 [README.org](file:///Users/lionseun/Documents/RustProjects/MystiProxy/README.org) 了解完整功能
- 查看 [config.example.yaml](file:///Users/lionseun/Documents/RustProjects/MystiProxy/config.example.yaml) 了解所有配置选项
- 浏览 [dev/](file:///Users/lionseun/Documents/RustProjects/MystiProxy/dev/) 目录获取开发配置示例
