//! TLS 模块 - 提供单向和双向 TLS 认证支持
//!
//! 支持特性：
//! - TLS 1.2/1.3 通过 rustls（默认）
//! - TLS 1.0/1.1 通过 OpenSSL（需要启用 `legacy-tls` feature）
//! - ALPN 协议协商
//! TLS 模块 - 提供单向和双向 TLS 认证支持
//!
//! 本模块提供了完整的 TLS/SSL 功能，支持：
//! - 单向 TLS 认证（服务器认证）
//! - 双向 TLS 认证（mTLS，客户端和服务器相互认证）
//! - 证书加载和管理
//! - TLS 连接器和服务器
//!
//! # 安全建议
//!
//! 1. **证书管理**
//!    - 使用受信任的证书颁发机构（CA）签发的证书
//!    - 定期轮换证书，避免使用过期证书
//!    - 私钥文件应设置严格的文件权限（例如 600）
//!    - 不要在代码中硬编码证书或私钥
//!
//! 2. **TLS 版本**
//!    - 本模块默认支持 TLS 1.2 和 TLS 1.3
//!    - 不支持已弃用的 SSLv2、SSLv3 和 TLS 1.0/1.1
//!
//! 3. **密码套件**
//!    - 使用 rustls 默认的安全密码套件
//!    - 避免使用弱加密算法（如 RC4、DES、3DES）
//!
//! 4. **证书验证**
//!    - 在生产环境中，始终验证服务器证书
//!    - 在 mTLS 场景中，验证客户端证书
//!    - 检查证书的有效期、域名和用途
//!
//! # 示例
//!
//! ## 单向 TLS（服务器认证）
//!
//! ```no_run
//! use mystiproxy::tls::{TlsConfig, TlsServer};
//! use std::path::Path;
//! use tokio::net::TcpListener;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 从文件加载证书和私钥
//!     let config = TlsConfig::from_pem_files(
//!         Path::new("server.crt"),
//!         Path::new("server.key")
//!     )?;
//!
//!     // 创建服务器配置
//!     let server_config = config.to_server_config()?;
//!     let tls_server = TlsServer::new(server_config);
//!
//!     // 接受 TLS 连接
//!     let listener = TcpListener::bind("0.0.0.0:443").await?;
//!     let (stream, _) = listener.accept().await?;
//!     let tls_stream = tls_server.accept(stream).await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## 双向 TLS（mTLS）
//!
//! ```no_run
//! use mystiproxy::tls::{TlsConfig, TlsServer};
//! use std::path::Path;
//!
//! // 加载服务器证书、私钥和客户端 CA 证书
//! let config = TlsConfig::from_pem_files(
//!     Path::new("server.crt"),
//!     Path::new("server.key")
//! )?
//! .with_client_ca(Path::new("client-ca.crt"))?;
//!
//! // 创建 mTLS 服务器配置
//! let server_config = config.to_server_config_mutual()?;
//! let tls_server = TlsServer::new(server_config);
//! # Ok::<(), mystiproxy::MystiProxyError>(())
//! ```
//!
//! ## TLS 客户端
//!
//! ```no_run
//! use mystiproxy::tls::create_tls_connector;
//! use tokio::net::TcpStream;
//! use std::path::Path;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 创建 TLS 连接器（使用系统默认 CA）
//!     let connector = create_tls_connector(None)?;
//!
//!     // 或者使用自定义 CA 证书
//!     // let connector = create_tls_connector(Some(Path::new("ca.crt")))?;
//!
//!     // 连接到 TLS 服务器
//!     let stream = TcpStream::connect("example.com:443").await?;
//!     let domain = "example.com".try_into()?;
//!     let tls_stream = connector.connect(domain, stream).await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## 带客户端证书的 TLS 客户端（用于 mTLS）
//!
//! ```no_run
//! use mystiproxy::tls::create_tls_connector_with_client_cert;
//! use tokio::net::TcpStream;
//! use std::path::Path;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // 创建带客户端证书的 TLS 连接器
//!     let connector = create_tls_connector_with_client_cert(
//!         Path::new("server-ca.crt"),     // 服务器 CA 证书
//!         Path::new("client.crt"),         // 客户端证书
//!         Path::new("client.key")          // 客户端私钥
//!     )?;
//!
//!     // 连接到需要客户端证书的 TLS 服务器
//!     let stream = TcpStream::connect("mtls.example.com:443").await?;
//!     let domain = "mtls.example.com".try_into()?;
//!     let tls_stream = connector.connect(domain, stream).await?;
//!
//!     Ok(())
//! }
//! ```

use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::server::TlsStream;
use tokio_rustls::{TlsAcceptor, TlsConnector};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use rustls_pemfile::{certs, private_key};

#[cfg(feature = "legacy-tls")]
mod openssl_tls;

#[cfg(feature = "hot-reload")]
mod reloader;

#[cfg(feature = "hot-reload")]
pub use reloader::CertificateReloader;

/// TLS 版本配置
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TlsVersion {
    /// TLS 1.0（遗留版本，需要 OpenSSL）
    V1_0,
    /// TLS 1.1（遗留版本，需要 OpenSSL）
    V1_1,
    /// TLS 1.2
    V1_2,
    /// TLS 1.3
    #[default]
    V1_3,
}

/// TLS 配置
pub struct TlsConfig {
    /// 证书链
    cert_chain: Vec<CertificateDer<'static>>,
    /// 私钥
    key: PrivateKeyDer<'static>,
    /// 客户端 CA 证书（用于双向认证）
    client_ca: Option<Vec<CertificateDer<'static>>>,
    /// 最小 TLS 版本
    min_version: TlsVersion,
    /// 最大 TLS 版本
    max_version: TlsVersion,
    /// ALPN 协议列表
    alpn_protocols: Vec<Vec<u8>>,
}

impl std::fmt::Debug for TlsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsConfig")
            .field("cert_chain_count", &self.cert_chain.len())
            .field("key", &"[private key]")
            .field("client_ca_count", &self.client_ca.as_ref().map(|v| v.len()))
            .field("min_version", &self.min_version)
            .field("max_version", &self.max_version)
            .field("alpn_protocols", &self.alpn_protocols.iter().map(|p| String::from_utf8_lossy(p).to_string()).collect::<Vec<_>>())
            .finish()
    }
}

impl TlsConfig {
    /// 创建新的 TLS 配置构建器
    pub fn builder() -> TlsConfigBuilder {
        TlsConfigBuilder::default()
    }

    /// 从 PEM 文件加载 TLS 配置
    ///
    /// # 参数
    /// - `cert_path`: 证书文件路径
    /// - `key_path`: 私钥文件路径
    ///
    /// # 返回
    /// 成功返回 TlsConfig，失败返回错误
    ///
    /// # 示例
    /// ```no_run
    /// use mystiproxy::tls::TlsConfig;
    /// use std::path::Path;
    ///
    /// let config = TlsConfig::from_pem_files(
    ///     Path::new("/path/to/cert.pem"),
    ///     Path::new("/path/to/key.pem")
    /// ).unwrap();
    /// ```
    pub fn from_pem_files(cert_path: &Path, key_path: &Path) -> crate::Result<Self> {
        let cert_content = std::fs::read(cert_path)?;
        let key_content = std::fs::read(key_path)?;

        Self::from_pem_content(
            String::from_utf8_lossy(&cert_content).as_ref(),
            String::from_utf8_lossy(&key_content).as_ref(),
        )
    }

    /// 从 PEM 内容加载 TLS 配置
    ///
    /// # 参数
    /// - `cert_pem`: 证书 PEM 格式内容
    /// - `key_pem`: 私钥 PEM 格式内容
    ///
    /// # 返回
    /// 成功返回 TlsConfig，失败返回错误
    ///
    /// # 示例
    /// ```no_run
    /// use mystiproxy::tls::TlsConfig;
    ///
    /// let cert_pem = r#"-----BEGIN CERTIFICATE-----
    /// ...
    /// -----END CERTIFICATE-----"#;
    ///
    /// let key_pem = r#"-----BEGIN PRIVATE KEY-----
    /// ...
    /// -----END PRIVATE KEY-----"#;
    ///
    /// let config = TlsConfig::from_pem_content(cert_pem, key_pem).unwrap();
    /// ```
    pub fn from_pem_content(cert_pem: &str, key_pem: &str) -> crate::Result<Self> {
        let cert_chain = certs(&mut cert_pem.as_bytes())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| {
                crate::MystiProxyError::Tls(format!("证书解析失败: {}", e))
            })?;

        if cert_chain.is_empty() {
            return Err(crate::MystiProxyError::Tls("未找到证书".to_string()));
        }

        let key = private_key(&mut key_pem.as_bytes())?
            .ok_or_else(|| crate::MystiProxyError::Tls("未找到私钥".to_string()))?;

        Ok(Self {
            cert_chain,
            key,
            client_ca: None,
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            alpn_protocols: vec![b"h2".to_vec(), b"http/1.1".to_vec()],
        })
    }

    /// 设置客户端 CA 证书（用于双向认证）
    ///
    /// # 参数
    /// - `ca_path`: CA 证书文件路径
    ///
    /// # 返回
    /// 成功返回更新后的 TlsConfig，失败返回错误
    pub fn with_client_ca(mut self, ca_path: &Path) -> crate::Result<Self> {
        let ca_content = std::fs::read(ca_path)?;
        let client_ca = certs(&mut ca_content.as_slice())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| {
                crate::MystiProxyError::Tls(format!("客户端 CA 证书解析失败: {}", e))
            })?;

        if client_ca.is_empty() {
            return Err(crate::MystiProxyError::Tls(
                "未找到客户端 CA 证书".to_string(),
            ));
        }

        self.client_ca = Some(client_ca);
        Ok(self)
    }

    /// 从 PEM 内容设置客户端 CA 证书
    ///
    /// # 参数
    /// - `ca_pem`: CA 证书 PEM 格式内容
    ///
    /// # 返回
    /// 成功返回更新后的 TlsConfig，失败返回错误
    pub fn with_client_ca_content(mut self, ca_pem: &str) -> crate::Result<Self> {
        let client_ca = certs(&mut ca_pem.as_bytes())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| {
                crate::MystiProxyError::Tls(format!("客户端 CA 证书解析失败: {}", e))
            })?;

        if client_ca.is_empty() {
            return Err(crate::MystiProxyError::Tls(
                "未找到客户端 CA 证书".to_string(),
            ));
        }

        self.client_ca = Some(client_ca);
        Ok(self)
    }

    /// 设置 TLS 版本范围
    pub fn with_version_range(mut self, min: TlsVersion, max: TlsVersion) -> crate::Result<Self> {
        if min as u8 > max as u8 {
            return Err(crate::MystiProxyError::Tls(
                format!("无效的 TLS 版本范围: min ({:?}) > max ({:?})", min, max)
            ));
        }
        self.min_version = min;
        self.max_version = max;
        Ok(self)
    }

    /// 设置 ALPN 协议列表
    pub fn with_alpn_protocols(mut self, protocols: Vec<&[u8]>) -> Self {
        self.alpn_protocols = protocols.into_iter().map(|p| p.to_vec()).collect();
        self
    }

    /// 创建服务端配置（单向认证）
    ///
    /// 服务器向客户端提供证书，客户端验证服务器证书
    ///
    /// # 返回
    /// 成功返回 Arc<ServerConfig>，失败返回错误
    pub fn to_server_config(&self) -> crate::Result<Arc<ServerConfig>> {
        let mut config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(self.cert_chain.clone(), self.key.clone_key())
            .map_err(|e| crate::MystiProxyError::Tls(format!("TLS 配置创建失败: {}", e)))?;

        // 配置 ALPN
        if !self.alpn_protocols.is_empty() {
            config.alpn_protocols = self.alpn_protocols.clone();
        }

        Ok(Arc::new(config))
    }

    /// 创建服务端配置（双向认证）
    ///
    /// 服务器验证客户端证书，客户端也验证服务器证书
    ///
    /// # 返回
    /// 成功返回 Arc<ServerConfig>，失败返回错误
    ///
    /// # 注意
    /// 调用此方法前必须先通过 `with_client_ca` 或 `with_client_ca_content` 设置客户端 CA 证书
    pub fn to_server_config_mutual(&self) -> crate::Result<Arc<ServerConfig>> {
        let client_ca = self.client_ca.as_ref().ok_or_else(|| {
            crate::MystiProxyError::Tls("双向认证需要设置客户端 CA 证书".to_string())
        })?;

        // 创建 CA 证书存储
        let mut root_cert_store = RootCertStore::empty();
        for cert in client_ca {
            root_cert_store
                .add(cert.clone())
                .map_err(|e| {
                    crate::MystiProxyError::Tls(format!("添加 CA 证书失败: {}", e))
                })?;
        }

        // 配置客户端证书验证
        let verifier = rustls::server::WebPkiClientVerifier::builder(root_cert_store.into())
            .build()
            .map_err(|e| {
                crate::MystiProxyError::Tls(format!("创建客户端验证器失败: {}", e))
            })?;

        let mut config = ServerConfig::builder()
            .with_client_cert_verifier(verifier)
            .with_single_cert(self.cert_chain.clone(), self.key.clone_key())
            .map_err(|e| crate::MystiProxyError::Tls(format!("TLS 配置创建失败: {}", e)))?;

        // 配置 ALPN
        if !self.alpn_protocols.is_empty() {
            config.alpn_protocols = self.alpn_protocols.clone();
        }

        Ok(Arc::new(config))
    }
}

/// TLS 配置构建器
#[derive(Default)]
pub struct TlsConfigBuilder {
    cert_chain: Vec<CertificateDer<'static>>,
    key: Option<PrivateKeyDer<'static>>,
    client_ca: Option<Vec<CertificateDer<'static>>>,
    min_version: TlsVersion,
    max_version: TlsVersion,
    alpn_protocols: Vec<Vec<u8>>,
}

impl TlsConfigBuilder {
    /// 从 PEM 文件加载证书和私钥
    pub fn from_pem_files(mut self, cert_path: &Path, key_path: &Path) -> crate::Result<Self> {
        let cert_content = std::fs::read(cert_path)?;
        let key_content = std::fs::read(key_path)?;

        self.cert_chain = certs(&mut cert_content.as_slice())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| crate::MystiProxyError::Tls(format!("证书解析失败: {}", e)))?;

        if self.cert_chain.is_empty() {
            return Err(crate::MystiProxyError::Tls("未找到证书".to_string()));
        }

        self.key = Some(private_key(&mut key_content.as_slice())?
            .ok_or_else(|| crate::MystiProxyError::Tls("未找到私钥".to_string()))?);

        Ok(self)
    }

    /// 设置客户端 CA 证书
    pub fn with_client_ca(mut self, ca_path: &Path) -> crate::Result<Self> {
        let ca_content = std::fs::read(ca_path)?;
        self.client_ca = Some(certs(&mut ca_content.as_slice())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| crate::MystiProxyError::Tls(format!("CA 证书解析失败: {}", e)))?);
        Ok(self)
    }

    /// 设置 TLS 版本范围
    pub fn with_version_range(mut self, min: TlsVersion, max: TlsVersion) -> crate::Result<Self> {
        if min as u8 > max as u8 {
            return Err(crate::MystiProxyError::Tls(
                format!("无效的 TLS 版本范围: min ({:?}) > max ({:?})", min, max)
            ));
        }
        self.min_version = min;
        self.max_version = max;
        Ok(self)
    }

    /// 设置 ALPN 协议列表
    pub fn with_alpn_protocols(mut self, protocols: Vec<&[u8]>) -> Self {
        self.alpn_protocols = protocols.into_iter().map(|p| p.to_vec()).collect();
        self
    }

    /// 构建 TLS 配置
    pub fn build(self) -> crate::Result<TlsConfig> {
        let key = self.key.ok_or_else(|| 
            crate::MystiProxyError::Tls("未设置私钥".to_string())
        )?;

        Ok(TlsConfig {
            cert_chain: self.cert_chain,
            key,
            client_ca: self.client_ca,
            min_version: self.min_version,
            max_version: self.max_version,
            alpn_protocols: self.alpn_protocols,
        })
    }
}

/// TLS 服务器
pub struct TlsServer {
    /// TLS 接受器
    acceptor: TlsAcceptor,
}

impl TlsServer {
    /// 创建新的 TLS 服务器
    ///
    /// # 参数
    /// - `config`: TLS 服务端配置
    ///
    /// # 返回
    /// 返回 TlsServer 实例
    pub fn new(config: Arc<ServerConfig>) -> Self {
        Self {
            acceptor: TlsAcceptor::from(config),
        }
    }

    /// 接受 TLS 连接
    ///
    /// # 参数
    /// - `stream`: 底层 TCP 流
    ///
    /// # 返回
    /// 成功返回 TLS 流，失败返回错误
    ///
    /// # 示例
    /// ```no_run
    /// use mystiproxy::tls::{TlsConfig, TlsServer};
    /// use std::sync::Arc;
    /// use tokio::net::TcpListener;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let config = TlsConfig::from_pem_files(
    ///         std::path::Path::new("cert.pem"),
    ///         std::path::Path::new("key.pem")
    ///     )?;
    ///     let server_config = config.to_server_config()?;
    ///     let tls_server = TlsServer::new(server_config);
    ///
    ///     let listener = TcpListener::bind("0.0.0.0:443").await?;
    ///     let (stream, _) = listener.accept().await?;
    ///     let tls_stream = tls_server.accept(stream).await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn accept<T>(&self, stream: T) -> crate::Result<TlsStream<T>>
    where
        T: AsyncRead + AsyncWrite + Unpin,
    {
        self.acceptor
            .accept(stream)
            .await
            .map_err(|e| crate::MystiProxyError::Tls(format!("TLS 握手失败: {}", e)))
    }
}

/// 创建 TLS 连接器（客户端）
///
/// # 参数
/// - `ca_cert`: 可选的 CA 证书路径，用于验证服务器证书
///
/// # 返回
/// 成功返回 TlsConnector，失败返回错误
///
/// # 示例
/// ```no_run
/// use mystiproxy::tls::create_tls_connector;
/// use tokio::net::TcpStream;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // 不验证服务器证书
///     let connector = create_tls_connector(None)?;
///
///     // 使用自定义 CA 证书验证服务器证书
///     let connector = create_tls_connector(Some(std::path::Path::new("ca.pem")))?;
///
///     let stream = TcpStream::connect("example.com:443").await?;
///     let tls_stream = connector.connect("example.com".try_into()?, stream).await?;
///
///     Ok(())
/// }
/// ```
pub fn create_tls_connector(ca_cert: Option<&Path>) -> crate::Result<TlsConnector> {
    let config = if let Some(ca_path) = ca_cert {
        // 使用自定义 CA 证书
        let ca_content = std::fs::read(ca_path)?;
        let ca_certs = certs(&mut ca_content.as_slice())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| {
                crate::MystiProxyError::Tls(format!("CA 证书解析失败: {}", e))
            })?;

        let mut root_cert_store = RootCertStore::empty();
        for cert in ca_certs {
            root_cert_store.add(cert).map_err(|e| {
                crate::MystiProxyError::Tls(format!("添加 CA 证书失败: {}", e))
            })?;
        }

        ClientConfig::builder()
            .with_root_certificates(root_cert_store)
            .with_no_client_auth()
    } else {
        // 使用系统默认的根证书
        let root_cert_store = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };

        ClientConfig::builder()
            .with_root_certificates(root_cert_store)
            .with_no_client_auth()
    };

    Ok(TlsConnector::from(Arc::new(config)))
}

/// 创建 TLS 连接器（带客户端证书，用于双向认证）
///
/// # 参数
/// - `ca_cert`: CA 证书路径，用于验证服务器证书
/// - `client_cert`: 客户端证书路径
/// - `client_key`: 客户端私钥路径
///
/// # 返回
/// 成功返回 TlsConnector，失败返回错误
///
/// # 示例
/// ```no_run
/// use mystiproxy::tls::create_tls_connector_with_client_cert;
/// use tokio::net::TcpStream;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let connector = create_tls_connector_with_client_cert(
///         std::path::Path::new("ca.pem"),
///         std::path::Path::new("client-cert.pem"),
///         std::path::Path::new("client-key.pem")
///     )?;
///
///     let stream = TcpStream::connect("example.com:443").await?;
///     let tls_stream = connector.connect("example.com".try_into()?, stream).await?;
///
///     Ok(())
/// }
/// ```
pub fn create_tls_connector_with_client_cert(
    ca_cert: &Path,
    client_cert: &Path,
    client_key: &Path,
) -> crate::Result<TlsConnector> {
    // 加载 CA 证书
    let ca_content = std::fs::read(ca_cert)?;
    let ca_certs = certs(&mut ca_content.as_slice())
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| crate::MystiProxyError::Tls(format!("CA 证书解析失败: {}", e)))?;

    let mut root_cert_store = RootCertStore::empty();
    for cert in ca_certs {
        root_cert_store
            .add(cert)
            .map_err(|e| crate::MystiProxyError::Tls(format!("添加 CA 证书失败: {}", e)))?;
    }

    // 加载客户端证书和私钥
    let cert_content = std::fs::read(client_cert)?;
    let cert_chain = certs(&mut cert_content.as_slice())
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| crate::MystiProxyError::Tls(format!("客户端证书解析失败: {}", e)))?;

    let key_content = std::fs::read(client_key)?;
    let key = private_key(&mut key_content.as_slice())?
        .ok_or_else(|| crate::MystiProxyError::Tls("未找到客户端私钥".to_string()))?;

    let config = ClientConfig::builder()
        .with_root_certificates(root_cert_store)
        .with_client_auth_cert(cert_chain, key)
        .map_err(|e| crate::MystiProxyError::Tls(format!("客户端 TLS 配置创建失败: {}", e)))?;

    Ok(TlsConnector::from(Arc::new(config)))
}

/// 保留原有的 TlsLoader 以兼容现有代码
pub struct TlsLoader;

impl TlsLoader {
    /// 从文件加载 TLS 配置
    #[deprecated(note = "请使用 TlsConfig::from_pem_files 代替")]
    pub fn load_server_config(cert_path: &str, key_path: &str) -> crate::Result<Arc<ServerConfig>> {
        let config = TlsConfig::from_pem_files(Path::new(cert_path), Path::new(key_path))?;
        config.to_server_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_config_from_invalid_content() {
        let result = TlsConfig::from_pem_content("invalid cert", "invalid key");
        assert!(result.is_err());
    }

    #[test]
    fn test_tls_config_missing_key() {
        let cert_pem = r#"-----BEGIN CERTIFICATE-----
MIIBkTCB+wIJAKHBfpLxAAAAADANBgkqhkiG9w0BAQsFADANMQswCQYDVQQDDAJj
-----END CERTIFICATE-----"#;
        let result = TlsConfig::from_pem_content(cert_pem, "");
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("未找到私钥"));
        }
    }

    #[test]
    fn test_tls_config_mutual_without_client_ca() {
        // 测试在没有设置客户端 CA 的情况下调用双向认证会失败
        let config = TlsConfig {
            cert_chain: vec![],
            key: PrivateKeyDer::Pkcs1(vec![].into()),
            client_ca: None,
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            alpn_protocols: vec![],
        };
        
        let result = config.to_server_config_mutual();
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("双向认证需要设置客户端 CA 证书"));
        }
    }

    #[test]
    fn test_tls_config_with_client_ca_content() {
        // 测试设置客户端 CA 内容
        let config = TlsConfig {
            cert_chain: vec![],
            key: PrivateKeyDer::Pkcs1(vec![].into()),
            client_ca: None,
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            alpn_protocols: vec![],
        };
        
        // 使用无效的 CA 内容
        let result = config.with_client_ca_content("invalid ca");
        assert!(result.is_err());
    }

    #[test]
    fn test_tls_version_range_invalid() {
        let result = TlsConfig::builder()
            .with_version_range(TlsVersion::V1_3, TlsVersion::V1_0);
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("无效的 TLS 版本范围"));
        }
    }

    #[test]
    fn test_tls_config_builder_missing_key() {
        let result = TlsConfig::builder()
            .build();
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("未设置私钥"));
        }
    }

    #[test]
    fn test_tls_version_default() {
        let version = TlsVersion::default();
        assert_eq!(version, TlsVersion::V1_3);
    }

    #[test]
    fn test_tls_config_with_alpn() {
        let config = TlsConfig {
            cert_chain: vec![],
            key: PrivateKeyDer::Pkcs1(vec![].into()),
            client_ca: None,
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            alpn_protocols: vec![b"h2".to_vec()],
        };
        
        assert_eq!(config.alpn_protocols, vec![b"h2".to_vec()]);
    }
}
