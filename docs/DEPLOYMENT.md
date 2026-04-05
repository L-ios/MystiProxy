# 部署建议

本文档提供 MystiProxy 在生产环境中的部署建议和安全配置。

## Docker 部署

### 基础 Dockerfile

```dockerfile
FROM scratch
COPY mystiproxy /mystiproxy
COPY config.yaml /config.yaml
CMD ["/mystiproxy", "--config", "/config.yaml"]
```

### 构建和运行

```bash
# 构建镜像
docker build -t mystiproxy:latest .

# 运行容器
docker run -d \
  --name mystiproxy \
  -p 3128:3128 \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v $(pwd)/config.yaml:/config.yaml \
  mystiproxy:latest
```

### Docker Compose

```yaml
version: '3.8'

services:
  mystiproxy:
    image: mystiproxy:latest
    container_name: mystiproxy
    restart: unless-stopped
    ports:
      - "3128:3128"
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
      - ./config.yaml:/config.yaml:ro
    environment:
      - RUST_LOG=info
    networks:
      - proxy-network

networks:
  proxy-network:
    driver: bridge
```

## Kubernetes 部署

### 使用 Helm Chart

```bash
# 安装 Helm Chart
helm install mystiproxy ./chart -f values.yaml
```

### Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: mystiproxy
  labels:
    app: mystiproxy
spec:
  replicas: 3
  selector:
    matchLabels:
      app: mystiproxy
  template:
    metadata:
      labels:
        app: mystiproxy
    spec:
      containers:
      - name: mystiproxy
        image: mystiproxy:latest
        ports:
        - containerPort: 3128
        volumeMounts:
        - name: config
          mountPath: /config.yaml
          subPath: config.yaml
        env:
        - name: RUST_LOG
          value: "info"
        resources:
          requests:
            memory: "64Mi"
            cpu: "250m"
          limits:
            memory: "128Mi"
            cpu: "500m"
      volumes:
      - name: config
        configMap:
          name: mystiproxy-config
---
apiVersion: v1
kind: Service
metadata:
  name: mystiproxy-service
spec:
  selector:
    app: mystiproxy
  ports:
    - protocol: TCP
      port: 80
      targetPort: 3128
  type: LoadBalancer
```

### ConfigMap 配置

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: mystiproxy-config
data:
  config.yaml: |
    mysti:
      engine:
        docker:
          listen: tcp://0.0.0.0:3128
          target: unix:///var/run/docker.sock
          proxy_type: http
          timeout: 10s
```

## 系统服务部署

### Systemd 服务文件

创建 systemd 服务文件 `/etc/systemd/system/mystiproxy.service`：

```ini
[Unit]
Description=MystiProxy Service
After=network.target

[Service]
Type=simple
User=mystiproxy
Group=mystiproxy
ExecStart=/usr/local/bin/mystiproxy --config /etc/mystiproxy/config.yaml
Restart=on-failure
RestartSec=5s

# 安全配置
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/log/mystiproxy

# 资源限制
LimitNOFILE=65536
LimitNPROC=4096

[Install]
WantedBy=multi-user.target
```

### 安装和启动

```bash
# 创建用户
sudo useradd -r -s /bin/false mystiproxy

# 创建目录
sudo mkdir -p /etc/mystiproxy /var/log/mystiproxy

# 复制文件
sudo cp mystiproxy /usr/local/bin/
sudo cp config.yaml /etc/mystiproxy/

# 设置权限
sudo chown -R mystiproxy:mystiproxy /etc/mystiproxy /var/log/mystiproxy
sudo chmod 600 /etc/mystiproxy/config.yaml

# 启动服务
sudo systemctl daemon-reload
sudo systemctl enable mystiproxy
sudo systemctl start mystiproxy

# 查看状态
sudo systemctl status mystiproxy
```

## 安全建议

### 1. 使用 TLS 加密

在生产环境中启用 TLS：

```yaml
mysti:
  engine:
    secure-proxy:
      listen: tcp://0.0.0.0:8443
      target: tcp://127.0.0.1:8080
      proxy_type: http
      tls:
        cert: /etc/mystiproxy/server.crt
        key: /etc/mystiproxy/server.key
        client_ca: /etc/mystiproxy/ca.crt
```

### 2. 限制访问

使用防火墙规则限制访问来源：

```bash
# 使用 iptables
iptables -A INPUT -p tcp --dport 3128 -s 10.0.0.0/8 -j ACCEPT
iptables -A INPUT -p tcp --dport 3128 -j DROP

# 使用 firewalld
firewall-cmd --permanent --add-rich-rule='rule family="ipv4" source address="10.0.0.0/8" port protocol="tcp" port="3128" accept'
firewall-cmd --reload
```

### 3. 鉴权配置

启用 Header 或 JWT 鉴权：

```yaml
mysti:
  engine:
    api-gateway:
      listen: tcp://0.0.0.0:8080
      proxy_type: http
      auth:
        type: jwt
        secret: your-jwt-secret
        header: Authorization
```

### 4. 日志审计

记录所有访问日志：

```yaml
mysti:
  engine:
    api-gateway:
      logging:
        enabled: true
        level: info
        format: json
        output: /var/log/mystiproxy/access.log
```

### 5. 定期更新

保持依赖库最新版本：

```bash
# 检查依赖更新
cargo outdated

# 更新依赖
cargo update

# 安全审计
cargo audit
```

## 监控和日志

### Prometheus 监控

```yaml
mysti:
  engine:
    api-gateway:
      metrics:
        enabled: true
        port: 9090
        path: /metrics
```

### 日志轮转

创建日志轮转配置 `/etc/logrotate.d/mystiproxy`：

```
/var/log/mystiproxy/*.log {
    daily
    rotate 7
    compress
    delaycompress
    missingok
    notifempty
    create 0640 mystiproxy mystiproxy
    postrotate
        systemctl reload mystiproxy > /dev/null 2>&1 || true
    endscript
}
```

## 高可用部署

### 负载均衡配置

使用 Nginx 作为负载均衡器：

```nginx
upstream mystiproxy {
    server 10.0.1.10:3128;
    server 10.0.1.11:3128;
    server 10.0.1.12:3128;
}

server {
    listen 80;
    server_name proxy.example.com;

    location / {
        proxy_pass http://mystiproxy;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }
}
```

## 下一步

- 查看 [故障排查](./TROUBLESHOOTING.md) 解决常见问题
- 查看 [性能优化](./ADVANCED.md#性能优化) 优化性能
- 查看 [安全建议](#安全建议) 加强安全配置
