# MystiProxy

灵活的 HTTP/TCP 代理服务器，支持 Mock、静态文件和路由匹配。

## 特性

- **TCP 代理** — 4 层转发，支持 `tcp://` 和 `unix://`
- **HTTP 代理** — 7 层转发，支持路由匹配和请求改写
- **路由匹配** — Full / Prefix / Regex / PrefixRegex 四种模式
- **Mock 响应** — 按 location 返回自定义 status 和 headers
- **静态文件** — 目录映射为 HTTP 静态资源
- **请求改写** — method、URI、headers 基础变换

> 详细文档见 [`doc/`](doc/)

## 快速开始

```bash
cargo build --release

# 配置文件启动
./target/release/mystiproxy --config config.yaml

# 命令行快速启动（TCP 代理）
./target/release/mystiproxy --listen tcp://0.0.0.0:3128 --target tcp://127.0.0.1:3306
```

## 最小配置

```yaml
mysti:
  engine:
    docker:
      listen: tcp://0.0.0.0:3128
      target: unix:///var/run/docker.sock
      proxy_type: http
      request_timeout: 10s
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
├── config/     配置解析
├── proxy/      TCP 代理
├── http/       HTTP 服务器、处理器、客户端
├── router/     路由匹配
├── mock/       Mock 服务
├── tls/        TLS 模块（实验性）
└── io/         I/O 抽象
```

## License

MIT
