# 部署与运维

## 编译

使用 Release 模式编译可执行文件，生成优化后的二进制文件：

```bash
cargo build --release
```

编译产物位于 `target/release/mystiproxy`。如需交叉编译，配置相应的 target 并确保交叉编译工具链已安装。

## 运行

支持两种运行模式：配置文件模式和命令行模式。

配置文件模式启动代理服务器：

```bash
./mystiproxy --config config.yaml
```

命令行模式直接指定监听地址和目标地址：

```bash
./mystiproxy --listen tcp://0.0.0.0:3128 --target tcp://127.0.0.1:3306
```

可通过环境变量调整日志级别：

```bash
RUST_LOG=debug ./mystiproxy --config config.yaml
RUST_LOG=info,hyper=debug ./mystiproxy --config config.yaml
```

支持 Ctrl+C 和 SIGTERM 信号，程序会优雅关闭并停止接受新连接。

## Docker 部署

项目设计目标之一是使用 `FROM scratch` 构建极简镜像：

```dockerfile
FROM rust:1.75 as builder
WORKDIR /build
COPY . .
RUN cargo build --release

FROM scratch
COPY --from=builder /build/target/release/mystiproxy /mystiproxy
COPY config.yaml /config.yaml
ENTRYPOINT ["/mystiproxy", "--config", "/config.yaml"]
```

构建并运行镜像：

```bash
docker build -t mystiproxy .
docker run -d -p 3128:3128 -v $(pwd)/config.yaml:/config.yaml mystiproxy
```

## Kubernetes 部署

使用 ConfigMap 挂载配置文件：

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: mystiproxy-config
data:
  config.yaml: |
    listen: tcp://0.0.0.0:3128
    target: tcp://127.0.0.1:3306
```

如使用 TLS，创建 Secret 存放证书：

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: mystiproxy-tls
type: Opaque
stringData:
  cert.pem: |
    -----BEGIN CERTIFICATE-----
    ...
    -----END CERTIFICATE-----
```

Deployment 示例：

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: mystiproxy
spec:
  replicas: 1
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
      volumes:
      - name: config
        configMap:
          name: mystiproxy-config
```

## systemd 服务

创建 systemd 服务单元文件 `/etc/systemd/system/mystiproxy.service`：

```ini
[Unit]
Description=MystiProxy HTTP/TCP Proxy
After=network.target

[Service]
Type=simple
User=mystiproxy
Group=mystiproxy
WorkingDirectory=/opt/mystiproxy
ExecStart=/opt/mystiproxy/mystiproxy --config /opt/mystiproxy/config.yaml
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
```

启用并启动服务：

```bash
sudo systemctl daemon-reload
sudo systemctl enable mystiproxy
sudo systemctl start mystiproxy
```

## 测试

运行所有测试用例：

```bash
cargo test
```

如需查看详细输出：

```bash
cargo test -- --nocapture
```

代码质量检查：

```bash
cargo clippy
cargo fmt -- --check
```

## 故障排查

### 端口被占用

检查端口占用情况：

```bash
lsof -i :8080
ss -tlnp | grep 8080
```

如端口被占用，修改配置中的 listen 地址或关闭占用进程。

### Unix Socket 权限不足

检查 socket 文件权限：

```bash
ls -la /run/mystiproxy/
```

确保运行用户有读写权限，必要时使用 sudo 或调整用户组。

### 配置文件格式错误

使用 yaml 验证工具检查语法：

```bash
yamllint config.yaml
python3 -c "import yaml; yaml.safe_load(open('config.yaml'))"
```

确保缩进正确、无多余字符。

### 目标不可达

确认目标地址可达：

```bash
ping 127.0.0.1
telnet 127.0.0.1 3306
```

检查防火墙规则和网络连通性。

### 日志调试

设置详细日志级别排查问题：

```bash
RUST_LOG=debug ./mystiproxy --config config.yaml
```

查看日志输出中的连接错误、超时信息、配置解析问题等。