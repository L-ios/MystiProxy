//! Configuration module for MystiCentral
//!
//! Supports loading configuration from CLI arguments, environment variables, and config files.

use anyhow::{Context, Result};
use clap::Parser;
use serde::Deserialize;

/// MystiCentral - Central Management System for MystiProxy
#[derive(Parser, Debug, Clone)]
#[command(name = "mysticentral")]
#[command(author, version, about, long_about = None)]
pub struct CliArgs {
    /// Server listen address (e.g., "0.0.0.0:8080")
    #[arg(short, long, env = "MYSTICENTRAL_SERVER_ADDR", default_value = "0.0.0.0:8080")]
    pub addr: String,

    /// PostgreSQL connection URL
    #[arg(long, env = "MYSTICENTRAL_DATABASE_URL")]
    pub database_url: String,

    /// Maximum number of database connections
    #[arg(long, env = "MYSTICENTRAL_DATABASE_MAX_CONNECTIONS", default_value = "10")]
    pub database_max_connections: u32,

    /// JWT secret key for signing tokens
    #[arg(long, env = "MYSTICENTRAL_JWT_SECRET")]
    pub jwt_secret: String,

    /// JWT token expiration time in hours
    #[arg(long, env = "MYSTICENTRAL_JWT_EXPIRATION_HOURS", default_value = "24")]
    pub jwt_expiration_hours: i64,

    /// Path to the TLS certificate file (PEM format)
    #[arg(long, env = "MYSTICENTRAL_TLS_CERT_PATH")]
    pub tls_cert_path: Option<String>,

    /// Path to the TLS private key file (PEM format)
    #[arg(long, env = "MYSTICENTRAL_TLS_KEY_PATH")]
    pub tls_key_path: Option<String>,

    /// Path to client CA certificate for mTLS
    #[arg(long, env = "MYSTICENTRAL_TLS_CLIENT_CA_PATH")]
    pub tls_client_ca_path: Option<String>,

    /// Minimum TLS version supported (1.0, 1.1, 1.2, 1.3)
    #[arg(long, env = "MYSTICENTRAL_TLS_MIN_VERSION", default_value = "1.0")]
    pub tls_min_version: String,

    /// Maximum TLS version supported (1.0, 1.1, 1.2, 1.3)
    #[arg(long, env = "MYSTICENTRAL_TLS_MAX_VERSION", default_value = "1.3")]
    pub tls_max_version: String,

    /// Enable ALPN (Application-Layer Protocol Negotiation)
    #[arg(long, env = "MYSTICENTRAL_TLS_ENABLE_ALPN", default_value = "true")]
    pub tls_enable_alpn: bool,

    /// ALPN protocols to advertise (comma-separated, e.g., "h2,http/1.1")
    #[arg(long, env = "MYSTICENTRAL_TLS_ALPN_PROTOCOLS", default_value = "h2,http/1.1")]
    pub tls_alpn_protocols: String,

    /// Enable certificate hot reload
    #[arg(long, env = "MYSTICENTRAL_TLS_ENABLE_HOT_RELOAD")]
    pub tls_enable_hot_reload: bool,
}

/// Main configuration structure
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub jwt: JwtConfig,
    /// Optional TLS configuration
    pub tls: Option<TlsConfig>,
}

/// Server configuration
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Server listen address (e.g., "0.0.0.0:8080")
    pub addr: String,
}

/// TLS version configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TlsVersion {
    /// TLS 1.0 (legacy, requires OpenSSL)
    #[default]
    V1_0,
    /// TLS 1.1 (legacy, requires OpenSSL)
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

/// TLS configuration for HTTPS support
#[derive(Debug, Clone, Deserialize)]
pub struct TlsConfig {
    /// Path to the TLS certificate file (PEM format)
    pub cert_path: String,
    /// Path to the TLS private key file (PEM format)
    pub key_path: String,
    /// Optional path to client CA certificate for mTLS (mutual TLS authentication)
    pub client_ca_path: Option<String>,
    /// Minimum TLS version supported (default: TLS 1.0 for maximum compatibility)
    #[serde(default)]
    pub min_version: TlsVersion,
    /// Maximum TLS version supported (default: TLS 1.3)
    #[serde(default = "default_max_tls_version")]
    pub max_version: TlsVersion,
    /// Enable ALPN (Application-Layer Protocol Negotiation)
    #[serde(default = "default_enable_alpn")]
    pub enable_alpn: bool,
    /// ALPN protocols to advertise (default: h2, http/1.1)
    #[serde(default = "default_alpn_protocols")]
    pub alpn_protocols: Vec<String>,
    /// Enable certificate hot reload
    #[serde(default)]
    pub enable_hot_reload: bool,
}

fn default_max_tls_version() -> TlsVersion {
    TlsVersion::V1_3
}

fn default_enable_alpn() -> bool {
    true
}

fn default_alpn_protocols() -> Vec<String> {
    vec!["h2".to_string(), "http/1.1".to_string()]
}

/// Database configuration
#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    /// PostgreSQL connection URL
    pub url: String,
    /// Maximum number of connections in the pool
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

/// JWT configuration
#[derive(Debug, Clone, Deserialize)]
pub struct JwtConfig {
    /// JWT secret key for signing tokens
    pub secret: String,
    /// JWT token expiration time in hours
    #[serde(default = "default_jwt_expiration_hours")]
    pub expiration_hours: i64,
}

fn default_max_connections() -> u32 {
    10
}

fn default_jwt_expiration_hours() -> i64 {
    24
}

impl Config {
    /// Load configuration from CLI arguments
    pub fn from_args() -> Result<Self> {
        let args = CliArgs::parse();

        let server = ServerConfig {
            addr: args.addr,
        };

        let database = DatabaseConfig {
            url: args.database_url,
            max_connections: args.database_max_connections,
        };

        let jwt = JwtConfig {
            secret: args.jwt_secret,
            expiration_hours: args.jwt_expiration_hours,
        };

        let tls = if let Some(cert_path) = args.tls_cert_path {
            let key_path = args.tls_key_path
                .context("TLS key path is required when cert path is provided. Use --tls-key-path or MYSTICENTRAL_TLS_KEY_PATH")?;
            
            let min_version = parse_tls_version(&args.tls_min_version)
                .unwrap_or_default();
            
            let max_version = parse_tls_version(&args.tls_max_version)
                .unwrap_or_else(default_max_tls_version);
            
            let alpn_protocols = args.tls_alpn_protocols
                .split(',')
                .map(|p| p.trim().to_string())
                .collect();
            
            Some(TlsConfig {
                cert_path,
                key_path,
                client_ca_path: args.tls_client_ca_path,
                min_version,
                max_version,
                enable_alpn: args.tls_enable_alpn,
                alpn_protocols,
                enable_hot_reload: args.tls_enable_hot_reload,
            })
        } else {
            None
        };

        Ok(Config { server, database, jwt, tls })
    }

    /// Load configuration from environment variables (legacy method)
    pub fn from_env() -> Result<Self> {
        let _args = CliArgs::parse();
        Self::from_args()
    }
}

/// Parse TLS version from string
fn parse_tls_version(s: &str) -> Option<TlsVersion> {
    match s.to_lowercase().as_str() {
        "1.0" | "tls1.0" | "v1.0" => Some(TlsVersion::V1_0),
        "1.1" | "tls1.1" | "v1.1" => Some(TlsVersion::V1_1),
        "1.2" | "tls1.2" | "v1.2" => Some(TlsVersion::V1_2),
        "1.3" | "tls1.3" | "v1.3" => Some(TlsVersion::V1_3),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tls_version() {
        assert_eq!(parse_tls_version("1.0"), Some(TlsVersion::V1_0));
        assert_eq!(parse_tls_version("1.1"), Some(TlsVersion::V1_1));
        assert_eq!(parse_tls_version("1.2"), Some(TlsVersion::V1_2));
        assert_eq!(parse_tls_version("1.3"), Some(TlsVersion::V1_3));
        assert_eq!(parse_tls_version("TLS1.2"), Some(TlsVersion::V1_2));
        assert_eq!(parse_tls_version("V1.3"), Some(TlsVersion::V1_3));
        assert_eq!(parse_tls_version("invalid"), None);
    }

    #[test]
    fn test_tls_version_display() {
        assert_eq!(format!("{}", TlsVersion::V1_0), "TLS 1.0");
        assert_eq!(format!("{}", TlsVersion::V1_1), "TLS 1.1");
        assert_eq!(format!("{}", TlsVersion::V1_2), "TLS 1.2");
        assert_eq!(format!("{}", TlsVersion::V1_3), "TLS 1.3");
    }
}
