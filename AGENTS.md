# AGENTS.md - MystiProxy Development Guide

This document provides essential information for AI coding agents working in the MystiProxy codebase.

## Project Overview

MystiProxy is a flexible HTTP/TCP proxy server with mock support, written in Rust (edition 2021). It supports:
- 4-layer TCP/Unix socket forwarding
- 7-layer HTTP proxying with request/response transformation
- Mock responses for testing
- Static file serving
- TLS support
- Multiple routing modes (Full, Prefix, Regex, PrefixRegex)
- HTTP authentication (Header and JWT)
- WebSocket support
- HTTP connection pooling
- Request body JSON transformation using JSONPath
- Local management module with SQLite storage
- Performance monitoring with Prometheus metrics
- NTLM authentication support
- Upstream proxy configuration
- Gateway module with URI mapping and transformation

## Build Commands

### Build the project
```bash
cargo build
cargo build --release
```

### Run the application
```bash
./target/release/mystiproxy
./target/release/mystiproxy --config config.yaml
RUST_LOG=debug ./target/release/mystiproxy
```

## Test Commands

### Run all tests
```bash
cargo test
cargo test --all
```

### Run a single test
```bash
cargo test test_name
cargo test test_parse_duration
cargo test test_route_match_full
```

### Run tests with verbose output
```bash
cargo test -- --nocapture
cargo test --verbose
```

### Run tests in a specific module
```bash
cargo test config::tests
cargo test http::handler::tests
```

## Lint and Format Commands

### Format code
```bash
cargo fmt
cargo fmt -- --check
```

### Run Clippy linter
```bash
cargo clippy
cargo clippy --all-targets --all-features
cargo clippy --fix
```

### Type check without building
```bash
cargo check
```

## Code Style Guidelines

### Imports Organization
Organize imports in this order, separated by blank lines:
1. Standard library imports (`use std::...`)
2. External crate imports (`use tokio::...`, `use hyper::...`)
3. Internal crate imports (`use crate::...`)

Example:
```rust
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use hyper::body::Incoming;
use tracing::{debug, info};

use crate::error::{MystiProxyError, Result};
use crate::config::EngineConfig;
```

### Module Structure
- Use module-level doc comments (`//!`) at the top of mod.rs files
- Use `//` for inline comments, `///` for doc comments
- Re-export public APIs in mod.rs using `pub use`
- Group private modules first, then re-exports

Example:
```rust
//! HTTP 处理模块
//!
//! 提供 HTTP 代理的核心功能

mod handler;
mod client;

pub use handler::HttpRequestHandler;
pub use client::HttpClient;
```

### Naming Conventions
- **Types**: PascalCase (`HttpRequestHandler`, `ProxyConfig`)
- **Functions/Methods**: snake_case (`send_request`, `establish_connection`)
- **Variables**: snake_case (`let client_pool = ...`)
- **Constants**: SCREAMING_SNAKE_CASE (`const MAX_CONNECTIONS: usize = 100`)
- **Modules**: snake_case (`mod http_client`)
- **Type aliases**: PascalCase (`pub type BoxBody = ...`)

### Struct and Enum Design
- Use `#[derive]` attributes for common traits
- Place `#[derive]` on a single line with multiple traits
- Use `pub` for public fields, private fields should come first if mixed

Example:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub listen: String,
    pub target: String,
    #[serde(default)]
    pub timeout: Option<Duration>,
}
```

### Error Handling
- Use `thiserror::Error` for custom error types
- Use `anyhow` for application-level errors if needed
- Define a crate-level `Result<T>` type alias
- Provide descriptive error messages

Example:
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MystiProxyError {
    #[error("配置错误: {0}")]
    Config(String),
    
    #[error("HTTP 错误: {0}")]
    Http(#[from] http::Error),
}

pub type Result<T> = std::result::Result<T, MystiProxyError>;
```

### Async Code Patterns
- Use `tokio` as the async runtime
- Use `Arc<Mutex<T>>` for shared mutable state
- Prefer `tokio::sync::Mutex` over `std::sync::Mutex` in async contexts
- Use `tokio::spawn` for concurrent tasks
- Apply timeouts using `tokio::time::timeout`

Example:
```rust
pub async fn send_request(&self, request: Request) -> Result<Response> {
    let response = if let Some(timeout) = self.timeout {
        tokio::time::timeout(timeout, sender.send_request(request))
            .await
            .map_err(|_| MystiProxyError::Timeout)?
    } else {
        sender.send_request(request).await?
    };
    Ok(response)
}
```

### Testing Guidelines
- Place tests in the same file using `#[cfg(test)] mod tests { ... }`
- Use descriptive test names starting with `test_`
- Use `use super::*;` to import parent module items
- Test both success and error cases

Example:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("10s").unwrap(), Duration::from_secs(10));
        assert!(parse_duration("invalid").is_err());
    }
}
```

### Logging
- Use the `tracing` crate for logging
- Use appropriate log levels: `error!`, `warn!`, `info!`, `debug!`, `trace!`
- Include relevant context in log messages

Example:
```rust
use tracing::{debug, info, error};

info!("Starting proxy server on {}", addr);
debug!("Request headers: {:?}", request.headers());
error!("Failed to connect: {}", e);
```

### Serde Configuration
- Use `#[serde(rename_all = "...")]` for consistent naming
- Use `#[serde(default)]` for optional fields
- Use `#[serde(rename = "...")]` for custom field names

Example:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyType {
    Tcp,
    Http,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    #[serde(default)]
    pub timeout: Option<Duration>,
    #[serde(rename = "type")]
    pub config_type: String,
}
```

### Documentation Comments
- Use `///` for documenting public items
- Use `//!` for module-level documentation
- Include examples in documentation when helpful

Example:
```rust
/// HTTP 客户端连接
/// 
/// 提供到目标服务器的连接管理和请求转发功能
pub struct HttpClient {
    /// 目标地址
    target: String,
}
```

### Workspace Structure
```
MystiProxy/                  ← Cargo workspace root
├── Cargo.toml               ← Pure workspace definition
├── mystiproxy/              ← HTTP/TCP proxy server
│   ├── Cargo.toml
│   ├── src/
│   ├── tests/
│   └── examples/
├── mysticentral/            ← Central management console
│   ├── Cargo.toml
│   └── src/
└── mysti-common/            ← Shared library
    ├── Cargo.toml
    └── src/
```

### mystiproxy Source Structure
- `mystiproxy/src/config/` - Configuration parsing and validation
- `mystiproxy/src/http/` - HTTP server, client, handler, and utilities
  - `mystiproxy/src/http/auth.rs` - HTTP authentication
  - `mystiproxy/src/http/body.rs` - Request body transformation
  - `mystiproxy/src/http/client.rs` - HTTP client with connection pooling
  - `mystiproxy/src/http/handler.rs` - HTTP request handler
  - `mystiproxy/src/http/ntlm.rs` - NTLM authentication support
  - `mystiproxy/src/http/proxy.rs` - HTTP proxy functionality
  - `mystiproxy/src/http/server.rs` - HTTP server
  - `mystiproxy/src/http/static_files.rs` - Static file serving
  - `mystiproxy/src/http/upstream.rs` - Upstream proxy configuration
  - `mystiproxy/src/http/websocket.rs` - WebSocket support
- `mystiproxy/src/proxy/` - TCP/Unix socket proxy implementation
- `mystiproxy/src/io/` - Stream and listener abstractions
- `mystiproxy/src/mock/` - Mock response generation
- `mystiproxy/src/management/` - Local management module with SQLite storage
- `mystiproxy/src/router/` - Routing functionality
- `mystiproxy/src/tls/` - TLS support
- `mystiproxy/src/error.rs` - Error types and Result alias
- `mystiproxy/src/gateway.rs` - Gateway module with URI mapping
- `mystiproxy/src/metrics.rs` - Performance monitoring with Prometheus
- `mystiproxy/src/main.rs` - Application entry point
- `mystiproxy/src/lib.rs` - Library root with re-exports

## Key Dependencies
- `tokio` - Async runtime
- `hyper` - HTTP library
- `serde` / `serde_yaml` / `serde_json` - Serialization
- `tracing` / `tracing-subscriber` - Logging
- `thiserror` / `anyhow` - Error handling
- `clap` - CLI argument parsing
- `prometheus` - Performance monitoring
- `tokio-tungstenite` / `tungstenite` - WebSocket support
- `sqlx` - SQLite database support
- `uuid` - Unique identifier generation
- `axum` - HTTP framework for management API
- `rustls` - TLS support
- `sha1` - WebSocket handshake and NTLM authentication

## Configuration Files
- Use YAML format for configuration
- Configuration files should be validated on startup
- Support both file-based and command-line configuration

## Commit Guidelines
- Write clear, concise commit messages
- Reference issues when applicable
- Run tests before committing: `cargo test`
- Run clippy before committing: `cargo clippy`
- Format code before committing: `cargo fmt`
