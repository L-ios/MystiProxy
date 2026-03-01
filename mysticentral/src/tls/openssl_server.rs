//! OpenSSL-based TLS server implementation
//!
//! Provides async TLS acceptor using tokio-openssl for supporting
//! legacy TLS versions (1.0/1.1) that rustls doesn't support.

use anyhow::{Context, Result};
use openssl::ssl::SslContext;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_openssl::SslStream;

/// TLS acceptor for OpenSSL
pub struct OpenTlsAcceptor {
    context: Arc<SslContext>,
}

impl OpenTlsAcceptor {
    /// Create a new TLS acceptor with the given SSL context
    pub fn new(context: Arc<SslContext>) -> Self {
        Self { context }
    }

    /// Accept a TLS connection
    ///
    /// Performs the TLS handshake and returns an encrypted stream.
    ///
    /// # Example
    /// ```no_run
    /// use mysticentral::tls::OpenTlsAcceptor;
    /// use std::sync::Arc;
    /// use tokio::net::TcpListener;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     // Assume ssl_context is already configured
    ///     // let acceptor = OpenTlsAcceptor::new(ssl_context);
    ///     // let listener = TcpListener::bind("0.0.0.0:443").await?;
    ///     // let (stream, _) = listener.accept().await?;
    ///     // let tls_stream = acceptor.accept(stream).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn accept<T>(&self, stream: T) -> Result<OpenTlsStream<T>>
    where
        T: AsyncRead + AsyncWrite + Unpin,
    {
        let ssl = openssl::ssl::Ssl::new(&self.context).context("Failed to create SSL object")?;
        let mut stream = SslStream::new(ssl, stream).context("Failed to create SSL stream")?;

        // Perform TLS handshake using tokio_openssl
        futures::future::poll_fn(|cx| Pin::new(&mut stream).poll_accept(cx))
            .await
            .context("TLS handshake failed")?;

        Ok(OpenTlsStream { inner: stream })
    }
}

/// TLS stream wrapper
pub struct OpenTlsStream<T> {
    inner: SslStream<T>,
}

impl<T> OpenTlsStream<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    /// Get a reference to the inner stream
    pub fn get_ref(&self) -> &SslStream<T> {
        &self.inner
    }

    /// Get a mutable reference to the inner stream
    pub fn get_mut(&mut self) -> &mut SslStream<T> {
        &mut self.inner
    }

    /// Get the SSL connection object
    pub fn ssl(&self) -> &openssl::ssl::SslRef {
        self.inner.ssl()
    }

    /// Get the negotiated ALPN protocol
    pub fn alpn_protocol(&self) -> Option<&[u8]> {
        self.inner.ssl().selected_alpn_protocol()
    }

    /// Get the TLS version used for this connection
    pub fn tls_version(&self) -> Option<TlsVersion> {
        let version_str = self.inner.ssl().version_str();
        match version_str {
            "TLSv1" => Some(TlsVersion::V1_0),
            "TLSv1.1" => Some(TlsVersion::V1_1),
            "TLSv1.2" => Some(TlsVersion::V1_2),
            "TLSv1.3" => Some(TlsVersion::V1_3),
            _ => None,
        }
    }

    /// Get the peer certificate (if mTLS is enabled)
    pub fn peer_certificate(&self) -> Option<openssl::x509::X509> {
        self.inner.ssl().peer_certificate()
    }

    /// Get the cipher suite used for this connection
    pub fn cipher_name(&self) -> Option<&str> {
        self.inner.ssl().current_cipher().map(|c| c.name())
    }
}

impl<T> AsyncRead for OpenTlsStream<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_read(cx, buf)
    }
}

impl<T> AsyncWrite for OpenTlsStream<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

/// TLS version enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsVersion {
    /// TLS 1.0
    V1_0,
    /// TLS 1.1
    V1_1,
    /// TLS 1.2
    V1_2,
    /// TLS 1.3
    V1_3,
}

impl std::fmt::Display for TlsVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TlsVersion::V1_0 => write!(f, "TLS 1.0"),
            TlsVersion::V1_1 => write!(f, "TLS 1.1"),
            TlsVersion::V1_2 => write!(f, "TLS 1.2"),
            TlsVersion::V1_3 => write!(f, "TLS 1.3"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_version_display() {
        assert_eq!(format!("{}", TlsVersion::V1_0), "TLS 1.0");
        assert_eq!(format!("{}", TlsVersion::V1_1), "TLS 1.1");
        assert_eq!(format!("{}", TlsVersion::V1_2), "TLS 1.2");
        assert_eq!(format!("{}", TlsVersion::V1_3), "TLS 1.3");
    }
}
