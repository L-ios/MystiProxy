//! TLS module for MystiCentral
//!
//! Provides comprehensive TLS support including:
//! - TLS 1.0/1.1/1.2/1.3 via OpenSSL
//! - ALPN (Application-Layer Protocol Negotiation)
//! - Certificate hot reload
//! - Mutual TLS (mTLS) support

mod openssl_server;
mod reloader;

pub use openssl_server::OpenTlsAcceptor;
pub use reloader::CertificateReloader;

use crate::config::{TlsConfig as ConfigTlsConfig, TlsVersion};
use anyhow::{Context, Result};
use openssl::ssl::{SslAcceptor, SslContext, SslMethod, SslVerifyMode};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// TLS configuration wrapper
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// Path to certificate file
    pub cert_path: String,
    /// Path to private key file
    pub key_path: String,
    /// Path to client CA certificate (for mTLS)
    pub client_ca_path: Option<String>,
    /// Minimum TLS version
    pub min_version: TlsVersion,
    /// Maximum TLS version
    pub max_version: TlsVersion,
    /// Enable ALPN
    pub enable_alpn: bool,
    /// ALPN protocols
    pub alpn_protocols: Vec<String>,
}

impl From<ConfigTlsConfig> for TlsConfig {
    fn from(config: ConfigTlsConfig) -> Self {
        Self {
            cert_path: config.cert_path,
            key_path: config.key_path,
            client_ca_path: config.client_ca_path,
            min_version: config.min_version,
            max_version: config.max_version,
            enable_alpn: config.enable_alpn,
            alpn_protocols: config.alpn_protocols,
        }
    }
}

/// Build OpenSSL SSL context from configuration
///
/// # Safety
/// This function constructs SSL context with proper certificate validation.
/// The context is configured to enforce TLS version constraints and optional
/// client certificate verification.
pub fn build_ssl_context(config: &TlsConfig) -> Result<SslContext> {
    let mut builder = SslAcceptor::mozilla_intermediate_v5(SslMethod::tls())
        .context("Failed to create SSL acceptor builder")?;

    // Set minimum and maximum TLS versions
    set_tls_version_constraints(&mut builder, config.min_version, config.max_version)?;

    // Load certificate chain
    builder
        .set_certificate_file(Path::new(&config.cert_path), openssl::ssl::SslFiletype::PEM)
        .with_context(|| format!("Failed to load certificate from {}", config.cert_path))?;

    // Load private key
    builder
        .set_private_key_file(Path::new(&config.key_path), openssl::ssl::SslFiletype::PEM)
        .with_context(|| format!("Failed to load private key from {}", config.key_path))?;

    // Verify private key matches certificate
    builder
        .check_private_key()
        .context("Private key does not match certificate")?;

    // Configure mTLS if client CA is provided
    if let Some(ref client_ca_path) = config.client_ca_path {
        configure_mtls(&mut builder, client_ca_path)?;
    }

    // Configure ALPN
    if config.enable_alpn && !config.alpn_protocols.is_empty() {
        configure_alpn(&mut builder, &config.alpn_protocols)?;
    }

    // Build the context
    Ok(builder.build().into_context())
}

/// Set TLS version constraints on the SSL builder
fn set_tls_version_constraints(
    builder: &mut openssl::ssl::SslAcceptorBuilder,
    min_version: TlsVersion,
    max_version: TlsVersion,
) -> Result<()> {
    use openssl::ssl::SslVersion;

    let min = match min_version {
        TlsVersion::V1_0 => SslVersion::TLS1,
        TlsVersion::V1_1 => SslVersion::TLS1_1,
        TlsVersion::V1_2 => SslVersion::TLS1_2,
        TlsVersion::V1_3 => SslVersion::TLS1_3,
    };

    let max = match max_version {
        TlsVersion::V1_0 => SslVersion::TLS1,
        TlsVersion::V1_1 => SslVersion::TLS1_1,
        TlsVersion::V1_2 => SslVersion::TLS1_2,
        TlsVersion::V1_3 => SslVersion::TLS1_3,
    };

    // Validate version range
    if min_version as u8 > max_version as u8 {
        anyhow::bail!(
            "Invalid TLS version range: min ({:?}) > max ({:?})",
            min_version,
            max_version
        );
    }

    builder
        .set_min_proto_version(Some(min))
        .context("Failed to set minimum TLS version")?;
    builder
        .set_max_proto_version(Some(max))
        .context("Failed to set maximum TLS version")?;

    Ok(())
}

/// Configure mutual TLS (mTLS) with client certificate verification
fn configure_mtls(
    builder: &mut openssl::ssl::SslAcceptorBuilder,
    client_ca_path: &str,
) -> Result<()> {
    // Load CA certificate(s) for client verification
    let ca_file = std::fs::File::open(client_ca_path)
        .with_context(|| format!("Failed to open client CA file: {}", client_ca_path))?;
    let mut ca_reader = std::io::BufReader::new(ca_file);

    // Parse all certificates in the CA file
    let certs = rustls_pemfile::certs(&mut ca_reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("Failed to parse client CA certificates")?;

    if certs.is_empty() {
        anyhow::bail!(
            "No certificates found in client CA file: {}",
            client_ca_path
        );
    }

    // Create a CA store for client certificate verification
    let ca_store = builder.cert_store_mut();

    for cert_der in certs {
        let cert = openssl::x509::X509::from_der(&cert_der)
            .context("Failed to parse client CA certificate from DER")?;
        ca_store
            .add_cert(cert)
            .map_err(|e| anyhow::anyhow!("Failed to add client CA certificate to store: {}", e))?;
    }

    // Set verification mode to require client certificate
    builder.set_verify(SslVerifyMode::PEER | SslVerifyMode::FAIL_IF_NO_PEER_CERT);

    tracing::info!("mTLS configured with client CA: {}", client_ca_path);

    Ok(())
}

/// Configure ALPN (Application-Layer Protocol Negotiation)
fn configure_alpn(
    builder: &mut openssl::ssl::SslAcceptorBuilder,
    protocols: &[String],
) -> Result<()> {
    // Build ALPN protocol list in wire format
    let mut alpn_wire = Vec::new();
    for protocol in protocols {
        let bytes = protocol.as_bytes();
        if bytes.len() > 255 {
            anyhow::bail!("ALPN protocol name too long: {}", protocol);
        }
        alpn_wire.push(bytes.len() as u8);
        alpn_wire.extend_from_slice(bytes);
    }

    builder
        .set_alpn_protos(&alpn_wire)
        .context("Failed to set ALPN protocols")?;

    tracing::info!("ALPN configured with protocols: {:?}", protocols);

    Ok(())
}

/// TLS server wrapper that supports hot reload
pub struct TlsServer {
    /// Current SSL context (can be reloaded)
    context: Arc<RwLock<Arc<SslContext>>>,
    /// Optional certificate reloader
    reloader: Option<Arc<CertificateReloader>>,
}

impl TlsServer {
    /// Create a new TLS server with the given configuration
    pub fn new(config: &TlsConfig, enable_hot_reload: bool) -> Result<Self> {
        let context = build_ssl_context(config)?;
        let context = Arc::new(RwLock::new(Arc::new(context)));

        let reloader = if enable_hot_reload {
            let reloader = CertificateReloader::new(
                Path::new(&config.cert_path).to_path_buf(),
                Path::new(&config.key_path).to_path_buf(),
                context.clone(),
            )?;
            Some(Arc::new(reloader))
        } else {
            None
        };

        Ok(Self { context, reloader })
    }

    /// Get the current SSL context
    #[allow(dead_code)]
    pub async fn context(&self) -> Arc<SslContext> {
        self.context.read().await.clone()
    }

    /// Create an acceptor for incoming connections
    pub async fn acceptor(&self) -> OpenTlsAcceptor {
        let context = self.context.read().await.clone();
        OpenTlsAcceptor::new(context)
    }

    /// Start watching for certificate changes (if hot reload is enabled)
    pub async fn start_reload_watcher(&self) -> Result<()> {
        if let Some(ref reloader) = self.reloader {
            reloader.start_watching().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_cert() -> (NamedTempFile, NamedTempFile) {
        // Generate a self-signed certificate for testing
        let rsa = openssl::rsa::Rsa::generate(2048).unwrap();
        let pkey = openssl::pkey::PKey::from_rsa(rsa).unwrap();

        let mut builder = openssl::x509::X509Builder::new().unwrap();
        builder.set_version(2).unwrap();
        builder
            .set_subject_name(
                openssl::x509::X509NameBuilder::new()
                    .unwrap()
                    .build()
                    .as_ref(),
            )
            .unwrap();
        builder
            .set_issuer_name(
                openssl::x509::X509NameBuilder::new()
                    .unwrap()
                    .build()
                    .as_ref(),
            )
            .unwrap();
        builder.set_pubkey(&pkey).unwrap();
        builder
            .set_not_before(openssl::asn1::Asn1Time::days_from_now(0).unwrap().as_ref())
            .unwrap();
        builder
            .set_not_after(
                openssl::asn1::Asn1Time::days_from_now(365)
                    .unwrap()
                    .as_ref(),
            )
            .unwrap();
        builder
            .sign(&pkey, openssl::hash::MessageDigest::sha256())
            .unwrap();
        let cert = builder.build();

        let mut cert_file = NamedTempFile::new().unwrap();
        let mut key_file = NamedTempFile::new().unwrap();

        cert_file.write_all(&cert.to_pem().unwrap()).unwrap();
        key_file
            .write_all(&pkey.private_key_to_pem_pkcs8().unwrap())
            .unwrap();

        (cert_file, key_file)
    }

    fn create_expired_cert() -> (NamedTempFile, NamedTempFile) {
        // Generate a certificate that expires soon (for testing)
        // Note: OpenSSL allows creating context with expired/expiring certificates
        // The expiration check happens during TLS handshake
        let rsa = openssl::rsa::Rsa::generate(2048).unwrap();
        let pkey = openssl::pkey::PKey::from_rsa(rsa).unwrap();

        let mut builder = openssl::x509::X509Builder::new().unwrap();
        builder.set_version(2).unwrap();
        builder
            .set_subject_name(
                openssl::x509::X509NameBuilder::new()
                    .unwrap()
                    .build()
                    .as_ref(),
            )
            .unwrap();
        builder
            .set_issuer_name(
                openssl::x509::X509NameBuilder::new()
                    .unwrap()
                    .build()
                    .as_ref(),
            )
            .unwrap();
        builder.set_pubkey(&pkey).unwrap();
        // Set short validity period (expires in 1 day)
        builder
            .set_not_before(openssl::asn1::Asn1Time::days_from_now(0).unwrap().as_ref())
            .unwrap();
        builder
            .set_not_after(openssl::asn1::Asn1Time::days_from_now(1).unwrap().as_ref())
            .unwrap();
        builder
            .sign(&pkey, openssl::hash::MessageDigest::sha256())
            .unwrap();
        let cert = builder.build();

        let mut cert_file = NamedTempFile::new().unwrap();
        let mut key_file = NamedTempFile::new().unwrap();

        cert_file.write_all(&cert.to_pem().unwrap()).unwrap();
        key_file
            .write_all(&pkey.private_key_to_pem_pkcs8().unwrap())
            .unwrap();

        (cert_file, key_file)
    }

    fn create_ca_cert() -> NamedTempFile {
        // Generate a CA certificate for mTLS testing
        let rsa = openssl::rsa::Rsa::generate(2048).unwrap();
        let pkey = openssl::pkey::PKey::from_rsa(rsa).unwrap();

        let mut builder = openssl::x509::X509Builder::new().unwrap();
        builder.set_version(2).unwrap();

        builder
            .set_subject_name(
                openssl::x509::X509NameBuilder::new()
                    .unwrap()
                    .build()
                    .as_ref(),
            )
            .unwrap();
        builder
            .set_issuer_name(
                openssl::x509::X509NameBuilder::new()
                    .unwrap()
                    .build()
                    .as_ref(),
            )
            .unwrap();
        builder.set_pubkey(&pkey).unwrap();
        builder
            .set_not_before(openssl::asn1::Asn1Time::days_from_now(0).unwrap().as_ref())
            .unwrap();
        builder
            .set_not_after(
                openssl::asn1::Asn1Time::days_from_now(365)
                    .unwrap()
                    .as_ref(),
            )
            .unwrap();
        builder
            .sign(&pkey, openssl::hash::MessageDigest::sha256())
            .unwrap();
        let cert = builder.build();

        let mut cert_file = NamedTempFile::new().unwrap();
        cert_file.write_all(&cert.to_pem().unwrap()).unwrap();

        cert_file
    }

    #[test]
    fn test_build_ssl_context() {
        let (cert_file, key_file) = create_test_cert();

        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            enable_alpn: true,
            alpn_protocols: vec!["h2".to_string(), "http/1.1".to_string()],
        };

        let result = build_ssl_context(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_tls_version_range_invalid() {
        let (cert_file, key_file) = create_test_cert();

        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: TlsVersion::V1_3,
            max_version: TlsVersion::V1_0, // Invalid: min > max
            enable_alpn: false,
            alpn_protocols: vec![],
        };

        let result = build_ssl_context(&config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid TLS version range"));
    }

    #[test]
    fn test_missing_certificate() {
        let config = TlsConfig {
            cert_path: "/nonexistent/cert.pem".to_string(),
            key_path: "/nonexistent/key.pem".to_string(),
            client_ca_path: None,
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            enable_alpn: false,
            alpn_protocols: vec![],
        };

        let result = build_ssl_context(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_alpn_configuration() {
        let (cert_file, key_file) = create_test_cert();

        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            enable_alpn: true,
            alpn_protocols: vec!["h2".to_string()],
        };

        let result = build_ssl_context(&config);
        assert!(result.is_ok());
    }

    // ==================== TLS Version Tests ====================

    #[test]
    fn test_tls_1_0_configuration() {
        let (cert_file, key_file) = create_test_cert();

        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: TlsVersion::V1_0,
            max_version: TlsVersion::V1_0,
            enable_alpn: false,
            alpn_protocols: vec![],
        };

        let result = build_ssl_context(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_tls_1_1_configuration() {
        let (cert_file, key_file) = create_test_cert();

        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: TlsVersion::V1_1,
            max_version: TlsVersion::V1_1,
            enable_alpn: false,
            alpn_protocols: vec![],
        };

        let result = build_ssl_context(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_tls_1_2_configuration() {
        let (cert_file, key_file) = create_test_cert();

        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_2,
            enable_alpn: false,
            alpn_protocols: vec![],
        };

        let result = build_ssl_context(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_tls_1_3_configuration() {
        let (cert_file, key_file) = create_test_cert();

        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: TlsVersion::V1_3,
            max_version: TlsVersion::V1_3,
            enable_alpn: false,
            alpn_protocols: vec![],
        };

        let result = build_ssl_context(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_tls_version_range_1_0_to_1_3() {
        let (cert_file, key_file) = create_test_cert();

        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: TlsVersion::V1_0,
            max_version: TlsVersion::V1_3,
            enable_alpn: false,
            alpn_protocols: vec![],
        };

        let result = build_ssl_context(&config);
        assert!(result.is_ok());
    }

    // ==================== ALPN Tests ====================

    #[test]
    fn test_alpn_http2_only() {
        let (cert_file, key_file) = create_test_cert();

        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            enable_alpn: true,
            alpn_protocols: vec!["h2".to_string()],
        };

        let result = build_ssl_context(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_alpn_http1_only() {
        let (cert_file, key_file) = create_test_cert();

        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            enable_alpn: true,
            alpn_protocols: vec!["http/1.1".to_string()],
        };

        let result = build_ssl_context(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_alpn_multiple_protocols() {
        let (cert_file, key_file) = create_test_cert();

        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            enable_alpn: true,
            alpn_protocols: vec![
                "h2".to_string(),
                "http/1.1".to_string(),
                "http/1.0".to_string(),
            ],
        };

        let result = build_ssl_context(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_alpn_disabled() {
        let (cert_file, key_file) = create_test_cert();

        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            enable_alpn: false,
            alpn_protocols: vec!["h2".to_string()], // Should be ignored
        };

        let result = build_ssl_context(&config);
        assert!(result.is_ok());
    }

    // ==================== mTLS Tests ====================

    #[test]
    fn test_mtls_configuration() {
        let (cert_file, key_file) = create_test_cert();
        let ca_file = create_ca_cert();

        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: Some(ca_file.path().to_string_lossy().to_string()),
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            enable_alpn: true,
            alpn_protocols: vec!["h2".to_string()],
        };

        let result = build_ssl_context(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_mtls_missing_ca_file() {
        let (cert_file, key_file) = create_test_cert();

        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: Some("/nonexistent/ca.pem".to_string()),
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            enable_alpn: false,
            alpn_protocols: vec![],
        };

        let result = build_ssl_context(&config);
        assert!(result.is_err());
    }

    // ==================== Error Certificate Tests ====================

    #[test]
    fn test_invalid_certificate_content() {
        let mut cert_file = NamedTempFile::new().unwrap();
        let mut key_file = NamedTempFile::new().unwrap();

        cert_file.write_all(b"invalid certificate content").unwrap();
        key_file.write_all(b"invalid key content").unwrap();

        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            enable_alpn: false,
            alpn_protocols: vec![],
        };

        let result = build_ssl_context(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_mismatched_key_and_certificate() {
        // Create two different key pairs
        let (cert_file, _) = create_test_cert();
        let (_, key_file) = create_test_cert();

        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            enable_alpn: false,
            alpn_protocols: vec![],
        };

        let result = build_ssl_context(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_expiring_certificate() {
        let (cert_file, key_file) = create_expired_cert();

        // OpenSSL allows creating context with expiring certificates
        // The expiration check happens during TLS handshake
        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            enable_alpn: false,
            alpn_protocols: vec![],
        };

        let result = build_ssl_context(&config);
        // Context creation should succeed
        assert!(result.is_ok());
    }

    // ==================== TlsServer Tests ====================

    #[tokio::test]
    async fn test_tls_server_creation() {
        let (cert_file, key_file) = create_test_cert();

        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            enable_alpn: true,
            alpn_protocols: vec!["h2".to_string()],
        };

        let result = TlsServer::new(&config, false);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_tls_server_with_hot_reload() {
        let (cert_file, key_file) = create_test_cert();

        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            enable_alpn: true,
            alpn_protocols: vec!["h2".to_string()],
        };

        let result = TlsServer::new(&config, true);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_tls_server_acceptor_creation() {
        let (cert_file, key_file) = create_test_cert();

        let config = TlsConfig {
            cert_path: cert_file.path().to_string_lossy().to_string(),
            key_path: key_file.path().to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            enable_alpn: true,
            alpn_protocols: vec!["h2".to_string()],
        };

        let server = TlsServer::new(&config, false).unwrap();
        let acceptor = server.acceptor().await;

        // Just verify the acceptor can be created
        let _ = acceptor;
    }

    // ==================== Config Conversion Tests ====================

    #[test]
    fn test_config_conversion() {
        let config_typed = ConfigTlsConfig {
            cert_path: "/path/to/cert.pem".to_string(),
            key_path: "/path/to/key.pem".to_string(),
            client_ca_path: Some("/path/to/ca.pem".to_string()),
            min_version: TlsVersion::V1_2,
            max_version: TlsVersion::V1_3,
            enable_alpn: true,
            alpn_protocols: vec!["h2".to_string()],
            enable_hot_reload: false,
        };

        let tls_config: TlsConfig = config_typed.into();

        assert_eq!(tls_config.cert_path, "/path/to/cert.pem");
        assert_eq!(tls_config.key_path, "/path/to/key.pem");
        assert_eq!(
            tls_config.client_ca_path,
            Some("/path/to/ca.pem".to_string())
        );
        assert_eq!(tls_config.min_version, TlsVersion::V1_2);
        assert_eq!(tls_config.max_version, TlsVersion::V1_3);
        assert!(tls_config.enable_alpn);
        assert_eq!(tls_config.alpn_protocols, vec!["h2"]);
    }
}
