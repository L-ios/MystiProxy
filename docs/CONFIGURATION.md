# MystiProxy 配置指南

MystiProxy 是一个功能强大的 HTTP(S) 代理服务器，支持 Mock 功能、NTLM 认证、上游代理链等特性。

## 目录

- [HTTP(S) 代理配置](#https-代理配置)
- [代理认证配置](#代理认证配置)
- [上游代理配置](#上游代理配置)
- [NTLM 认证配置](#ntlm-认证配置)
- [线程上下文配置](#线程上下文配置)
- [完整示例](#完整示例)

---

## HTTP(S) 代理配置

### 基础 HTTP 代理

```rust
use mystiproxy::http::{HttpProxyConfig, HttpProxyService, ProxyAuthConfig};

// 创建无需认证的 HTTP 代理
let config = HttpProxyConfig::new()
    .connect_timeout(std::time::Duration::from_secs(30))
    .request_timeout(std::time::Duration::from_secs(60));

let service = HttpProxyService::new(config);
```

### 带认证的 HTTP 代理

```rust
// 创建带 Basic Auth 认证的 HTTP 代理
let auth_config = ProxyAuthConfig::new()
    .add_user("admin".to_string(), "secret123".to_string())
    .add_user("guest".to_string(), "guest".to_string())
    .enable()
    .realm("MystiProxy");

let config = HttpProxyConfig::new()
    .auth(auth_config)
    .connect_timeout(std::time::Duration::from_secs(30));
```

### 主机过滤

```rust
// 允许/禁止特定主机
let config = HttpProxyConfig::new()
    .allow_host("example.com")        // 允许 example.com
    .allow_host("api.example.com")    // 允许 api.example.com
    .block_host("blocked.com")        // 禁止 blocked.com
    .block_host("malicious.org");     // 禁止 malicious.org
```

### HTTPS CONNECT 隧道

```rust
use mystiproxy::http::HttpProxyAcceptor;

// 创建支持 HTTPS CONNECT 隧道的代理
let config = HttpProxyConfig::new()
    .allow_connect(true)  // 允许 CONNECT 方法
    .auth(auth_config);

let acceptor = HttpProxyAcceptor::new(config);

// 处理客户端连接
loop {
    let (stream, _) = listener.accept().await?;
    let acceptor = acceptor.clone();
    tokio::spawn(async move {
        let _ = acceptor.handle_connection(stream).await;
    });
}
```

---

## 代理认证配置

### Basic Auth 认证

```rust
use mystiproxy::http::ProxyAuthConfig;

// 创建认证配置
let auth = ProxyAuthConfig::new()
    .add_user("alice".to_string(), "password1".to_string())
    .add_user("bob".to_string(), "password2".to_string())
    .enable()
    .realm("Corporate Proxy");

// 验证请求
if let Some(username) = auth.authenticate(&request.headers()) {
    println!("Authenticated as: {}", username);
} else {
    // 返回 407 Proxy Authentication Required
    let response = auth.create_auth_required_response();
}
```

### 密码安全

密码使用 SHA-256 哈希存储：

```rust
// 内部实现
fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}
```

---

## 上游代理配置

### HTTP 上游代理

```rust
use mystiproxy::http::{UpstreamProxyConfig, UpstreamProxyConnector};

// 配置上游 HTTP 代理
let config = UpstreamProxyConfig::http("proxy.company.com", 8080)
    .connect_timeout(std::time::Duration::from_secs(30));

let connector = UpstreamProxyConnector::new(config);

// 通过上游代理建立 CONNECT 隧道
let stream = connector.connect_tunnel("target.com", 443).await?;
```

### HTTPS 上游代理

```rust
// 配置上游 HTTPS 代理（TLS 连接到代理）
let config = UpstreamProxyConfig::https("secure-proxy.company.com", 8443)
    .tls_verify(true);
```

### 带认证的上游代理

```rust
// 配置带认证的上游代理
let config = UpstreamProxyConfig::http("proxy.company.com", 8080)
    .auth("proxyuser", "proxypass")
    .connect_timeout(std::time::Duration::from_secs(30));
```

### 从 URL 解析配置

```rust
// 支持多种 URL 格式
let config = UpstreamProxyConfig::from_url("http://user:pass@proxy.example.com:8080")?;
let config = UpstreamProxyConfig::from_url("https://proxy.example.com:8443")?;
let config = UpstreamProxyConfig::from_url("socks5://proxy.example.com:1080")?;
```

### 代理转换器（类似 Cntlm）

```rust
use mystiproxy::http::ProxyConverter;

// 将需要认证的上游代理转换为本地无需认证的代理
let upstream_config = UpstreamProxyConfig::http("corporate-proxy.com", 8080)
    .auth("domain\\user", "password");

let converter = ProxyConverter::new(upstream_config, 3128);

// 本地客户端可以无认证连接到 localhost:3128
// 转换器会自动处理上游代理的认证
loop {
    let (client_stream, _) = listener.accept().await?;
    let converter = converter.clone();
    tokio::spawn(async move {
        let _ = converter.handle_client(client_stream).await;
    });
}
```

---

## NTLM 认证配置

### NTLMv2 认证（推荐）

```rust
use mystiproxy::http::{NtlmAuthenticator, NtlmConfig, NtlmVersion};

// 创建 NTLM 配置
let config = NtlmConfig::new("username", "password")
    .domain("CORPORATE")
    .workstation("MYWORKSTATION")
    .version(NtlmVersion::V2);

let auth = NtlmAuthenticator::new(config);

// 生成 Type 1 消息 (Negotiate)
let type1 = auth.create_type1_message();
println!("Type 1: NTLM {}", type1);

// 解析服务器返回的 Type 2 消息
let type2_message = "TlRMTVNTAAAAB..."; // 从 Proxy-Authenticate 头获取

// 生成 Type 3 消息 (Authenticate)
let auth_header = auth.authenticate(type2_message)?;
println!("Authorization: {}", auth_header);
```

### NTLMv1 认证（不推荐）

```rust
let config = NtlmConfig::new("username", "password")
    .domain("CORPORATE")
    .version(NtlmVersion::V1);
```

### 完整 NTLM 代理认证流程

```rust
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn connect_via_ntlm_proxy(
    proxy_host: &str,
    proxy_port: u16,
    target_host: &str,
    target_port: u16,
    ntlm_config: NtlmConfig,
) -> Result<TcpStream> {
    let mut stream = TcpStream::connect((proxy_host, proxy_port)).await?;
    let auth = NtlmAuthenticator::new(ntlm_config);

    // Step 1: 发送 CONNECT 请求带 Type 1
    let type1 = auth.create_type1_message();
    let request = format!(
        "CONNECT {}:{} HTTP/1.1\r\nHost: {}:{}\r\nProxy-Authorization: NTLM {}\r\n\r\n",
        target_host, target_port, target_host, target_port, type1
    );
    stream.write_all(request.as_bytes()).await?;

    // Step 2: 读取 Type 2 响应
    let mut response = vec![0u8; 4096];
    let n = stream.read(&mut response).await?;
    let response_str = String::from_utf8_lossy(&response[..n]);

    // 提取 Type 2 消息
    let type2 = extract_ntlm_message(&response_str)?;

    // Step 3: 发送 Type 3 认证
    let type3 = auth.authenticate(&type2)?;
    let request = format!(
        "CONNECT {}:{} HTTP/1.1\r\nHost: {}:{}\r\nProxy-Authorization: {}\r\n\r\n",
        target_host, target_port, target_host, target_port, type3
    );
    stream.write_all(request.as_bytes()).await?;

    // 读取最终响应
    let n = stream.read(&mut response).await?;
    let response_str = String::from_utf8_lossy(&response[..n]);

    if response_str.contains("200") {
        Ok(stream)
    } else {
        Err(MystiProxyError::Proxy("NTLM authentication failed".to_string()))
    }
}
```

---

## 线程上下文配置

### 设置引擎名称

```rust
use mystiproxy::{set_engine_name, get_engine_name, thread_identity, with_engine};

// 设置当前线程的引擎名称
set_engine_name("docker");

// 获取线程标识
let identity = thread_identity();
// 输出: "docker:1:tokio-runtime-worker-1"
```

### 使用 with_engine 辅助函数

```rust
// 在引擎上下文中执行代码
with_engine("containerd", || {
    // 这里的日志会包含引擎名称前缀
    println!("Thread identity: {}", thread_identity());
    // 输出: "containerd:2:tokio-runtime-worker-2"
});

// 退出上下文后，引擎名称被清除
assert!(get_engine_name().is_none());
```

### 日志中的线程标识

日志格式：`[引擎名称:线程ID:线程名称]`

```
DEBUG [docker:1:tokio-runtime-worker-1] Forwarding HTTP request: GET http://example.com/
DEBUG [containerd:2:tokio-runtime-worker-2] CONNECT tunnel established: example.com:443
```

---

## 完整示例

### 企业代理转换器

将企业 NTLM 代理转换为本地无认证代理：

```rust
use mystiproxy::http::{
    NtlmAuthenticator, NtlmConfig, NtlmVersion,
    ProxyConverter, UpstreamProxyConfig,
};

#[tokio::main]
async fn main() -> Result<()> {
    // 配置企业代理的 NTLM 认证
    let ntlm_config = NtlmConfig::new("domain\\user", "password")
        .domain("CORPORATE")
        .workstation("MYWORKSTATION")
        .version(NtlmVersion::V2);

    // 配置上游代理
    let upstream_config = UpstreamProxyConfig::http("proxy.corporate.com", 8080);

    // 创建代理转换器
    let converter = ProxyConverter::new(upstream_config, 3128);

    // 启动本地代理服务器
    let listener = TcpListener::bind("127.0.0.1:3128").await?;
    println!("Proxy converter listening on 127.0.0.1:3128");

    loop {
        let (client_stream, _) = listener.accept().await?;
        let converter = converter.clone();
        tokio::spawn(async move {
            let _ = converter.handle_client(client_stream).await;
        });
    }
}
```

### 多用户代理服务器

```rust
use mystiproxy::http::{HttpProxyConfig, HttpProxyService, ProxyAuthConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // 配置多用户认证
    let auth = ProxyAuthConfig::new()
        .add_user("admin".to_string(), "admin123".to_string())
        .add_user("developer".to_string(), "dev456".to_string())
        .add_user("guest".to_string(), "guest".to_string())
        .enable()
        .realm("MystiProxy");

    // 配置代理
    let config = HttpProxyConfig::new()
        .auth(auth)
        .connect_timeout(std::time::Duration::from_secs(30))
        .request_timeout(std::time::Duration::from_secs(60))
        .block_host("malicious.com")
        .block_host("phishing.org");

    // 启动代理服务器
    let service = HttpProxyService::new(config);
    // ... 启动服务器代码
}
```

### 代理链

```rust
// 客户端 -> MystiProxy -> 企业代理 -> 目标服务器

// 配置上游企业代理
let upstream = UpstreamProxyConfig::http("proxy.company.com", 8080)
    .auth("user", "pass");

let converter = ProxyConverter::new(upstream, 3128);

// 客户端连接到 localhost:3128
// 无需认证，MystiProxy 自动处理上游代理认证
```

---

## 配置文件示例

### YAML 配置

```yaml
# config.yaml
server:
  listen: "0.0.0.0:3128"
  connect_timeout: 30s
  request_timeout: 60s

auth:
  enabled: true
  realm: "MystiProxy"
  users:
    - username: admin
      password: admin123
    - username: guest
      password: guest

upstream:
  enabled: true
  url: "http://proxy.company.com:8080"
  auth:
    type: ntlm
    username: "domain\\user"
    password: "password"
    domain: "CORPORATE"
    workstation: "MYWORKSTATION"

host_filter:
  allowed:
    - example.com
    - api.example.com
  blocked:
    - malicious.com
    - phishing.org
```

### 环境变量配置

```bash
# 代理服务器配置
export MYSTIPROXY_LISTEN="0.0.0.0:3128"
export MYSTIPROXY_CONNECT_TIMEOUT="30"
export MYSTIPROXY_REQUEST_TIMEOUT="60"

# 认证配置
export MYSTIPROXY_AUTH_ENABLED="true"
export MYSTIPROXY_AUTH_REALM="MystiProxy"
export MYSTIPROXY_AUTH_USERS="admin:admin123,guest:guest"

# 上游代理配置
export MYSTIPROXY_UPSTREAM_URL="http://proxy.company.com:8080"
export MYSTIPROXY_UPSTREAM_AUTH_TYPE="ntlm"
export MYSTIPROXY_UPSTREAM_USERNAME="domain\\user"
export MYSTIPROXY_UPSTREAM_PASSWORD="password"
export MYSTIPROXY_UPSTREAM_DOMAIN="CORPORATE"
```

---

## 故障排除

### NTLM 认证失败

1. 检查域名格式：使用 `DOMAIN\username` 或 `username@domain`
2. 确认密码正确
3. 检查工作站名是否正确
4. 尝试使用 NTLMv1（如果服务器不支持 NTLMv2）

### 代理连接超时

1. 检查网络连接
2. 增加超时时间
3. 检查防火墙设置

### 认证循环

1. 检查 Proxy-Authorization 头是否正确发送
2. 确认服务器返回 407 状态码
3. 检查认证类型是否匹配（Basic vs NTLM）

---

## 参考资料

- [NTLM Authentication Protocol](https://docs.microsoft.com/en-us/openspecs/windows_protocols/ms-nlmp/)
- [HTTP Proxy Authentication](https://developer.mozilla.org/en-US/docs/Web/HTTP/Proxy_servers)
- [Cntlm Documentation](http://cntlm.sourceforge.net/)
