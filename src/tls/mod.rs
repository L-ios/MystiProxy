//! TLS 模块 - 提供单向和双向 TLS 认证支持

use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_rustls::server::TlsStream;
use tokio_rustls::{TlsAcceptor, TlsConnector};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use rustls_pemfile::{certs, private_key};

/// TLS 配置
pub struct TlsConfig {
    /// 证书链
    cert_chain: Vec<CertificateDer<'static>>,
    /// 私钥
    key: PrivateKeyDer<'static>,
    /// 客户端 CA 证书（用于双向认证）
    client_ca: Option<Vec<CertificateDer<'static>>>,
}

impl std::fmt::Debug for TlsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsConfig")
            .field("cert_chain_count", &self.cert_chain.len())
            .field("key", &"[private key]")
            .field("client_ca_count", &self.client_ca.as_ref().map(|v| v.len()))
            .finish()
    }
}

impl TlsConfig {
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

    /// 创建服务端配置（单向认证）
    ///
    /// 服务器向客户端提供证书，客户端验证服务器证书
    ///
    /// # 返回
    /// 成功返回 Arc<ServerConfig>，失败返回错误
    pub fn to_server_config(&self) -> crate::Result<Arc<ServerConfig>> {
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(self.cert_chain.clone(), self.key.clone_key())
            .map_err(|e| crate::MystiProxyError::Tls(format!("TLS 配置创建失败: {}", e)))?;

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

        let config = ServerConfig::builder()
            .with_client_cert_verifier(verifier)
            .with_single_cert(self.cert_chain.clone(), self.key.clone_key())
            .map_err(|e| crate::MystiProxyError::Tls(format!("TLS 配置创建失败: {}", e)))?;

        Ok(Arc::new(config))
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
        // 由于需要有效的证书和私钥，这里我们测试 client_ca 为 None 的情况
        let config = TlsConfig {
            cert_chain: vec![],
            key: PrivateKeyDer::Pkcs1(vec![].into()),
            client_ca: None,
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
        };
        
        // 使用无效的 CA 内容
        let result = config.with_client_ca_content("invalid ca");
        assert!(result.is_err());
    }
}
