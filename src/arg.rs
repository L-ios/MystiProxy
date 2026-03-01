use clap::Parser;

/// MystiProxy - 灵活的代理服务器
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct MystiArg {
    /// 目标地址 (支持 tcp://, unix://)
    ///
    /// 例如: tcp://127.0.0.1:8080 或 unix:///var/run/docker.sock
    #[arg(short, long)]
    pub target: Option<String>,

    /// 监听地址 (支持 tcp://, unix://)
    ///
    /// 例如: tcp://0.0.0.0:3128 或 unix:///tmp/proxy.sock
    #[arg(short, long)]
    pub listen: Option<String>,

    /// 配置文件路径 (YAML 格式)
    ///
    /// 如果指定了配置文件，将忽略其他命令行参数
    #[arg(short, long)]
    pub config: Option<String>,

    /// 代理类型 (tcp, http)
    ///
    /// 默认为 tcp
    #[arg(long, default_value = "tcp")]
    pub proxy_type: String,

    /// 连接超时时间
    ///
    /// 例如: 10s, 5m, 1h
    #[arg(long)]
    pub timeout: Option<String>,

    /// TLS 证书文件路径 (PEM 格式)
    ///
    /// 启用 TLS 加密连接
    #[arg(long, env = "MYSTIPROXY_TLS_CERT_PATH")]
    pub tls_cert: Option<String>,

    /// TLS 私钥文件路径 (PEM 格式)
    ///
    /// 与 --tls-cert 一起使用
    #[arg(long, env = "MYSTIPROXY_TLS_KEY_PATH")]
    pub tls_key: Option<String>,

    /// 客户端 CA 证书路径 (用于 mTLS)
    ///
    /// 启用双向 TLS 认证
    #[arg(long, env = "MYSTIPROXY_TLS_CLIENT_CA")]
    pub tls_client_ca: Option<String>,

    /// 最小 TLS 版本 (1.0, 1.1, 1.2, 1.3)
    ///
    /// 默认为 1.0 以支持旧客户端
    #[arg(long, default_value = "1.0")]
    pub tls_min_version: String,

    /// 最大 TLS 版本 (1.0, 1.1, 1.2, 1.3)
    ///
    /// 默认为 1.3
    #[arg(long, default_value = "1.3")]
    pub tls_max_version: String,

    /// 启用 ALPN (Application-Layer Protocol Negotiation)
    ///
    /// 支持 HTTP/2 协议协商
    #[arg(long, default_value = "true")]
    pub tls_enable_alpn: bool,

    /// ALPN 协议列表 (逗号分隔)
    ///
    /// 例如: h2,http/1.1
    #[arg(long, default_value = "h2,http/1.1")]
    pub tls_alpn_protocols: String,

    /// 启用证书热加载
    ///
    /// 证书文件更新后自动重新加载
    #[arg(long)]
    pub tls_hot_reload: bool,

    /// 目标服务器 TLS 证书路径 (用于验证上游服务器)
    ///
    /// 用于代理到 HTTPS 后端时验证证书
    #[arg(long, env = "MYSTIPROXY_UPSTREAM_TLS_CA")]
    pub upstream_tls_ca: Option<String>,

    /// 目标服务器 TLS 证书路径 (客户端证书)
    ///
    /// 用于 mTLS 连接到上游服务器
    #[arg(long, env = "MYSTIPROXY_UPSTREAM_TLS_CERT")]
    pub upstream_tls_cert: Option<String>,

    /// 目标服务器 TLS 私钥路径 (客户端私钥)
    ///
    /// 用于 mTLS 连接到上游服务器
    #[arg(long, env = "MYSTIPROXY_UPSTREAM_TLS_KEY")]
    pub upstream_tls_key: Option<String>,

    /// 跳过上游服务器证书验证 (不安全)
    ///
    /// 仅用于测试环境
    #[arg(long)]
    pub upstream_tls_skip_verify: bool,
}
