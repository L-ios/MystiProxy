use thiserror::Error;

/// MystiProxy 核心错误类型
#[derive(Error, Debug)]
pub enum MystiProxyError {
    /// 配置错误
    #[error("配置错误: {0}")]
    Config(String),

    /// 配置文件读取错误
    #[error("配置文件读取失败: {0}")]
    ConfigFileRead(#[source] std::io::Error),

    /// 配置解析错误
    #[error("配置解析失败: {0}")]
    ConfigParse(#[from] serde_yaml::Error),

    /// JSON 解析错误
    #[error("JSON 解析错误: {0}")]
    JsonParse(#[from] serde_json::Error),

    /// HTTP 错误
    #[error("HTTP 错误: {0}")]
    Http(#[from] http::Error),

    /// Hyper 错误
    #[error("Hyper 错误: {0}")]
    Hyper(String),

    /// 代理错误
    #[error("代理错误: {0}")]
    Proxy(String),

    /// 路由错误
    #[error("路由错误: {0}")]
    Router(String),

    /// Mock 错误
    #[error("Mock 错误: {0}")]
    Mock(String),

    /// TLS 错误
    #[error("TLS 错误: {0}")]
    Tls(String),

    /// 正则表达式错误
    #[error("正则表达式错误: {0}")]
    Regex(#[from] regex::Error),

    /// 无效的正则表达式
    #[error("无效的正则表达式: {0}")]
    InvalidRegex(String),

    /// JSONPath 错误
    #[error("JSONPath 错误: {0}")]
    JsonPath(String),

    /// IO 错误
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// 地址解析错误
    #[error("地址解析错误: {0}")]
    AddrParse(#[from] std::net::AddrParseError),

    /// 超时错误
    #[error("操作超时")]
    Timeout,

    /// 认证错误
    #[error("认证错误: {0}")]
    Auth(String),

    /// JWT 错误
    #[error("JWT 错误: {0}")]
    Jwt(String),

    /// 通用错误
    #[error("{0}")]
    Other(String),
}

impl From<tokio::time::error::Elapsed> for MystiProxyError {
    fn from(_: tokio::time::error::Elapsed) -> Self {
        MystiProxyError::Timeout
    }
}

/// 结果类型别名
pub type Result<T> = std::result::Result<T, MystiProxyError>;
