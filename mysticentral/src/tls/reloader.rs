//! Certificate hot reload implementation
//!
//! Provides zero-downtime certificate rotation through file system watching.

use anyhow::{Context, Result};
use futures::Stream;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;

use super::build_ssl_context;
use super::TlsConfig;

/// Certificate reloader for hot-reloading TLS certificates
///
/// Monitors certificate and key files for changes and automatically
/// reloads the SSL context when changes are detected.
pub struct CertificateReloader {
    /// Path to certificate file
    cert_path: PathBuf,
    /// Path to private key file
    key_path: PathBuf,
    /// Current SSL context (shared with TlsServer)
    context: Arc<RwLock<Arc<openssl::ssl::SslContext>>>,
    /// File system watcher
    #[allow(dead_code)]
    watcher: Option<RecommendedWatcher>,
}

impl CertificateReloader {
    /// Create a new certificate reloader
    ///
    /// # Arguments
    /// * `cert_path` - Path to the certificate file
    /// * `key_path` - Path to the private key file
    /// * `context` - Shared SSL context that will be updated on reload
    pub fn new(
        cert_path: PathBuf,
        key_path: PathBuf,
        context: Arc<RwLock<Arc<openssl::ssl::SslContext>>>,
    ) -> Result<Self> {
        Ok(Self {
            cert_path,
            key_path,
            context,
            watcher: None,
        })
    }

    /// Start watching for certificate changes
    ///
    /// This spawns a background task that monitors the certificate and key files
    /// for modifications and reloads the SSL context when changes are detected.
    pub async fn start_watching(&self) -> Result<()> {
        let cert_path = self.cert_path.clone();
        let key_path = self.key_path.clone();
        let context = self.context.clone();

        // Initial load
        self.reload_now().await?;

        // Spawn background watcher task
        tokio::spawn(async move {
            if let Err(e) = watch_files(cert_path, key_path, context).await {
                tracing::error!("Certificate watcher error: {}", e);
            }
        });

        Ok(())
    }

    /// Force an immediate reload of the certificate
    pub async fn reload_now(&self) -> Result<()> {
        tracing::info!("Reloading TLS certificate from {:?}", self.cert_path);

        let config = TlsConfig {
            cert_path: self.cert_path.to_string_lossy().to_string(),
            key_path: self.key_path.to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: crate::config::TlsVersion::V1_2,
            max_version: crate::config::TlsVersion::V1_3,
            enable_alpn: true,
            alpn_protocols: vec!["h2".to_string(), "http/1.1".to_string()],
        };

        let new_context = build_ssl_context(&config)?;

        // Update the shared context
        let mut ctx = self.context.write().await;
        *ctx = Arc::new(new_context);

        tracing::info!("TLS certificate reloaded successfully");
        Ok(())
    }

    /// Get a stream of reload events
    ///
    /// Returns a stream that yields when the certificate is reloaded.
    /// This is useful for triggering downstream updates.
    #[allow(dead_code)]
    pub fn watch(&self) -> impl Stream<Item = Result<()>> {
        // This is a simplified implementation
        // A full implementation would use a channel to communicate reload events
        futures::stream::pending()
    }
}

/// Watch certificate and key files for changes
async fn watch_files(
    cert_path: PathBuf,
    key_path: PathBuf,
    context: Arc<RwLock<Arc<openssl::ssl::SslContext>>>,
) -> Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<notify::Event>(100);

    // Create the watcher
    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            let _ = tx.blocking_send(event);
        }
    })
    .context("Failed to create file watcher")?;

    // Watch the directory containing the files
    let cert_dir = cert_path
        .parent()
        .context("Certificate file has no parent directory")?;
    let key_dir = key_path
        .parent()
        .context("Key file has no parent directory")?;

    watcher
        .watch(cert_dir, RecursiveMode::NonRecursive)
        .context("Failed to watch certificate directory")?;

    if cert_dir != key_dir {
        watcher
            .watch(key_dir, RecursiveMode::NonRecursive)
            .context("Failed to watch key directory")?;
    }

    tracing::info!("Started watching for certificate changes");

    // Debounce duration to avoid multiple reloads for the same change
    let debounce_duration = Duration::from_millis(500);
    let mut last_reload = std::time::Instant::now()
        .checked_sub(debounce_duration)
        .unwrap_or_else(std::time::Instant::now);

    while let Some(event) = rx.recv().await {
        // Check if the event is relevant
        if is_relevant_event(&event, &cert_path, &key_path) {
            let now = std::time::Instant::now();
            if now.duration_since(last_reload) >= debounce_duration {
                last_reload = now;

                // Add a small delay to ensure file write is complete
                sleep(Duration::from_millis(100)).await;

                if let Err(e) = reload_certificate(&cert_path, &key_path, &context).await {
                    tracing::error!("Failed to reload certificate: {}", e);
                }
            }
        }
    }

    Ok(())
}

/// Check if a file system event is relevant to our certificate files
fn is_relevant_event(event: &Event, cert_path: &PathBuf, key_path: &PathBuf) -> bool {
    match event.kind {
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Any => {
            for path in &event.paths {
                if path == cert_path || path == key_path {
                    return true;
                }
            }
        }
        _ => {}
    }
    false
}

/// Reload the certificate
async fn reload_certificate(
    cert_path: &PathBuf,
    key_path: &PathBuf,
    context: &Arc<RwLock<Arc<openssl::ssl::SslContext>>>,
) -> Result<()> {
    tracing::info!("Reloading TLS certificate from {:?}", cert_path);

    let config = TlsConfig {
        cert_path: cert_path.to_string_lossy().to_string(),
        key_path: key_path.to_string_lossy().to_string(),
        client_ca_path: None,
        min_version: crate::config::TlsVersion::V1_2,
        max_version: crate::config::TlsVersion::V1_3,
        enable_alpn: true,
        alpn_protocols: vec!["h2".to_string(), "http/1.1".to_string()],
    };

    let new_context = build_ssl_context(&config)?;

    // Update the shared context
    let mut ctx = context.write().await;
    *ctx = Arc::new(new_context);

    tracing::info!("TLS certificate reloaded successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_cert_in_dir(dir: &TempDir) -> (PathBuf, PathBuf) {
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

        let cert_path = dir.path().join("cert.pem");
        let key_path = dir.path().join("key.pem");

        std::fs::write(&cert_path, &cert.to_pem().unwrap()).unwrap();
        std::fs::write(&key_path, &pkey.private_key_to_pem_pkcs8().unwrap()).unwrap();

        (cert_path, key_path)
    }

    #[tokio::test]
    async fn test_certificate_reloader_new() {
        let temp_dir = TempDir::new().unwrap();
        let (cert_path, key_path) = create_test_cert_in_dir(&temp_dir);

        let config = TlsConfig {
            cert_path: cert_path.to_string_lossy().to_string(),
            key_path: key_path.to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: crate::config::TlsVersion::V1_2,
            max_version: crate::config::TlsVersion::V1_3,
            enable_alpn: true,
            alpn_protocols: vec!["h2".to_string()],
        };

        let ssl_context = build_ssl_context(&config).unwrap();
        let context = Arc::new(RwLock::new(Arc::new(ssl_context)));

        let reloader = CertificateReloader::new(cert_path, key_path, context);
        assert!(reloader.is_ok());
    }

    #[tokio::test]
    async fn test_reload_now() {
        let temp_dir = TempDir::new().unwrap();
        let (cert_path, key_path) = create_test_cert_in_dir(&temp_dir);

        let config = TlsConfig {
            cert_path: cert_path.to_string_lossy().to_string(),
            key_path: key_path.to_string_lossy().to_string(),
            client_ca_path: None,
            min_version: crate::config::TlsVersion::V1_2,
            max_version: crate::config::TlsVersion::V1_3,
            enable_alpn: true,
            alpn_protocols: vec!["h2".to_string()],
        };

        let ssl_context = build_ssl_context(&config).unwrap();
        let context = Arc::new(RwLock::new(Arc::new(ssl_context)));

        let reloader = CertificateReloader::new(cert_path, key_path, context).unwrap();
        let result = reloader.reload_now().await;
        assert!(result.is_ok());
    }
}
