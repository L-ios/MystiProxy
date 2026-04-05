# 故障排查

本文档提供 MystiProxy 常见问题的排查方法和解决方案。

## 查看日志

### 设置日志级别

```bash
# 设置日志级别
RUST_LOG=debug ./target/release/mystiproxy

# 或在运行时设置
export RUST_LOG=info,hyper=debug
./target/release/mystiproxy
```

### 日志级别说明

| 级别 | 说明 | 使用场景 |
|------|------|----------|
| `error` | 仅错误信息 | 生产环境 |
| `warn` | 警告和错误 | 生产环境 |
| `info` | 一般信息 | 生产环境 |
| `debug` | 调试信息 | 开发和测试 |
| `trace` | 详细跟踪 | 深度调试 |

### 日志格式

```bash
# JSON 格式日志
RUST_LOG=info LOG_FORMAT=json ./target/release/mystiproxy

# 结构化日志
RUST_LOG=info LOG_FORMAT=structured ./target/release/mystiproxy
```

## 常见问题

### 1. 端口被占用

**症状：** 启动失败，提示端口已被占用

**排查：**

```bash
# 查看端口占用
lsof -i :8080

# 或使用 netstat
netstat -tunlp | grep 8080

# 或使用 ss
ss -tunlp | grep 8080
```

**解决方案：**

```bash
# 方案 1: 停止占用端口的进程
kill -9 <PID>

# 方案 2: 修改配置文件使用其他端口
# config.yaml
mysti:
  engine:
    api-gateway:
      listen: tcp://0.0.0.0:8081  # 使用其他端口
```

### 2. 权限不足（Unix Socket）

**症状：** 无法连接到 Unix Socket 文件

**排查：**

```bash
# 检查 socket 文件权限
ls -la /var/run/docker.sock

# 检查当前用户
whoami

# 检查用户组
groups
```

**解决方案：**

```bash
# 方案 1: 使用 sudo 运行
sudo ./target/release/mystiproxy

# 方案 2: 将用户添加到相应组
sudo usermod -aG docker $USER

# 方案 3: 修改 socket 文件权限
sudo chmod 666 /var/run/docker.sock
```

### 3. 配置文件格式错误

**症状：** 启动失败，提示配置文件解析错误

**排查：**

```bash
# 验证 YAML 格式
cat config.yaml | python -m yaml

# 或使用 yamllint
yamllint config.yaml

# 或使用在线 YAML 验证工具
```

**解决方案：**

```bash
# 检查常见 YAML 错误：
# 1. 缩进不一致（使用空格，不要用 Tab）
# 2. 冒号后缺少空格
# 3. 特殊字符未加引号
# 4. 列表格式错误

# 使用配置验证工具
./target/release/mystiproxy --validate-config config.yaml
```

### 4. 连接超时

**症状：** 请求超时，无法连接到目标服务

**排查：**

```bash
# 测试目标服务是否可达
curl -v http://target-service:8080/health

# 检查网络连接
ping target-service

# 检查防火墙规则
iptables -L -n | grep 8080

# 检查 DNS 解析
nslookup target-service
```

**解决方案：**

```yaml
# 增加超时时间
mysti:
  engine:
    api-gateway:
      timeout:
        connect: 30s
        read: 60s
        write: 60s
```

### 5. 内存占用过高

**症状：** MystiProxy 占用大量内存

**排查：**

```bash
# 查看进程内存使用
ps aux | grep mystiproxy

# 查看详细内存信息
pmap -x <PID>

# 使用 top 或 htop 监控
top -p <PID>
```

**解决方案：**

```yaml
# 启用流式处理
mysti:
  engine:
    api-gateway:
      streaming:
        enabled: true
        chunk_size: 8192

# 限制缓冲区大小
mysti:
  engine:
    api-gateway:
      buffer:
        max_body_size: 10MB
```

### 6. 性能问题

**症状：** 响应缓慢，吞吐量低

**排查：**

```bash
# 查看系统资源使用
top
iostat -x 1
vmstat 1

# 查看网络连接状态
netstat -an | grep ESTABLISHED | wc -l

# 查看进程状态
ps -ef | grep mystiproxy
```

**解决方案：**

```yaml
# 优化连接池
mysti:
  engine:
    api-gateway:
      pool:
        max_connections: 1000
        max_idle_connections: 100

# 增加工作线程
mysti:
  runtime:
    worker_threads: 8

# 启用压缩
mysti:
  engine:
    api-gateway:
      compression:
        enabled: true
        level: 6
```

### 7. TLS 证书问题

**症状：** TLS 握手失败，证书验证错误

**排查：**

```bash
# 检查证书文件
openssl x509 -in server.crt -text -noout

# 检查证书链
openssl verify -CAfile ca.crt server.crt

# 测试 TLS 连接
openssl s_client -connect localhost:8443 -showcerts
```

**解决方案：**

```yaml
# 确保证书路径正确
mysti:
  engine:
    secure-proxy:
      tls:
        cert: /etc/mystiproxy/server.crt
        key: /etc/mystiproxy/server.key
        client_ca: /etc/mystiproxy/ca.crt

# 或使用证书内容
cert:
  - name: server-cert
    cert_pem: |
      -----BEGIN CERTIFICATE-----
      ...
      -----END CERTIFICATE-----
    key_pem: |
      -----BEGIN PRIVATE KEY-----
      ...
      -----END PRIVATE KEY-----
```

### 8. 路由不匹配

**症状：** 请求未按预期路由到目标服务

**排查：**

```bash
# 启用调试日志
RUST_LOG=debug ./target/release/mystiproxy

# 查看路由匹配日志
# 日志会显示请求的 URI 和匹配的路由规则
```

**解决方案：**

```yaml
# 检查路由配置
mysti:
  engine:
    api-gateway:
      locations:
        # 确保路由顺序正确
        - location: /api/users
          mode: Prefix
          priority: 100  # 更高优先级

        - location: /api
          mode: Prefix
          priority: 10
```

## 性能调优

### 系统参数优化

```bash
# 增加文件描述符限制
ulimit -n 65536

# 优化 TCP 参数
sudo sysctl -w net.core.somaxconn=65536
sudo sysctl -w net.ipv4.tcp_max_syn_backlog=65536
sudo sysctl -w net.ipv4.ip_local_port_range="1024 65535"
```

### 监控指标

```bash
# 查看 Prometheus 指标
curl http://localhost:9090/metrics

# 关键指标：
# - mystiproxy_requests_total
# - mystiproxy_request_duration_seconds
# - mystiproxy_active_connections
# - mystiproxy_errors_total
```

## 获取帮助

如果以上方法无法解决问题，可以通过以下方式获取帮助：

1. **查看文档**：[文档目录](./)
2. **提交 Issue**：[GitHub Issues](https://github.com/your-repo/mystiproxy/issues)
3. **社区讨论**：[GitHub Discussions](https://github.com/your-repo/mystiproxy/discussions)

提交问题时，请提供以下信息：

- MystiProxy 版本
- 操作系统和版本
- 配置文件（去除敏感信息）
- 错误日志
- 复现步骤

## 下一步

- 查看 [高级配置](./ADVANCED.md) 优化性能
- 查看 [部署建议](./DEPLOYMENT.md) 了解生产环境部署
- 查看 [架构设计](./ARCHITECTURE.md) 了解系统架构
