//! MystiCentral - Central Management System for MystiProxy
//!
//! This crate provides the central management server for HTTP mock configurations,
//! supporting team collaboration, environment management, and distributed synchronization.

#[cfg(not(feature = "legacy-tls"))]
use anyhow::Result;
#[cfg(feature = "legacy-tls")]
use anyhow::{Context, Result};
use axum::middleware::from_fn_with_state;
use axum::Router;
#[cfg(feature = "legacy-tls")]
use hyper_util::rt::TokioIo;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod db;
mod error;
mod handlers;
mod middleware;
mod models;
mod services;
#[cfg(feature = "legacy-tls")]
mod tls;

pub use config::Config;
pub use error::ApiError;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "mysticentral=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config = Config::from_env()?;
    tracing::info!("Loaded configuration: {:?}", config.server);

    // Create database connection pool
    let db_pool = PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .connect(&config.database.url)
        .await?;
    tracing::info!("Database connection pool created");

    // Run migrations
    sqlx::migrate!("./src/db/migrations").run(&db_pool).await?;
    tracing::info!("Database migrations completed");

    // Create auth service
    let auth_service =
        services::AuthService::new(config.jwt.secret.clone(), config.jwt.expiration_hours)?;
    tracing::info!("Authentication service initialized");

    // Bootstrap initial admin user on empty database
    if let Err(e) = services::ensure_admin_user(&db_pool).await {
        tracing::warn!("Admin bootstrap failed: {}", e);
    }

    // Build application state
    let app_state = handlers::AppState::new(db_pool.clone(), auth_service.clone());

    // Auth middleware state
    let mw_state = middleware::auth::AuthMiddlewareState {
        auth_service,
        pool: db_pool.clone(),
    };

    // Build application router
    let app = Router::new()
        .merge(handlers::create_routes())
        .merge(
            handlers::create_protected_routes()
                .layer(from_fn_with_state(
                    mw_state.clone(),
                    middleware::auth::auth_middleware,
                ))
                .layer(axum::Extension(mw_state)),
        )
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(app_state);

    // Start server
    let addr: SocketAddr = config.server.addr.parse()?;
    tracing::info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Check if TLS is configured
    #[cfg(feature = "legacy-tls")]
    if let Some(ref tls_config) = config.tls {
        tracing::info!(
            "TLS enabled with version range: {:?} to {:?}",
            tls_config.min_version,
            tls_config.max_version
        );

        // Convert config to TLS config
        let tls_cfg = tls::TlsConfig::from(tls_config.clone());

        // Create TLS server with hot reload support
        let tls_server = tls::TlsServer::new(&tls_cfg, tls_config.enable_hot_reload)
            .context("Failed to create TLS server")?;

        // Start certificate watcher if hot reload is enabled
        if tls_config.enable_hot_reload {
            tls_server.start_reload_watcher().await?;
            tracing::info!("Certificate hot reload enabled");
        }

        // Handle TLS connections
        loop {
            let (tcp_stream, remote_addr) = listener.accept().await?;
            let tls_acceptor = tls_server.acceptor().await;
            let app = app.clone();

            tokio::spawn(async move {
                // Accept TLS connection
                let tls_stream = match tls_acceptor.accept(tcp_stream).await {
                    Ok(stream) => stream,
                    Err(e) => {
                        tracing::warn!("TLS handshake failed from {}: {}", remote_addr, e);
                        return;
                    }
                };

                // Log TLS connection details
                if let Some(version) = tls_stream.tls_version() {
                    tracing::debug!(
                        "TLS handshake successful from {} with {}",
                        remote_addr,
                        version
                    );
                }

                // Log ALPN protocol if negotiated
                if let Some(alpn) = tls_stream.alpn_protocol() {
                    tracing::debug!(
                        "ALPN protocol negotiated: {}",
                        String::from_utf8_lossy(alpn)
                    );
                }

                // Log client certificate if mTLS is enabled
                if let Some(_cert) = tls_stream.peer_certificate() {
                    tracing::debug!("Client certificate presented from {}", remote_addr);
                }

                // Wrap the TLS stream and serve HTTP over it
                let io = TokioIo::new(tls_stream);

                // Convert tower service to hyper service
                let hyper_service = hyper_util::service::TowerToHyperService::new(app);

                if let Err(e) = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, hyper_service)
                    .await
                {
                    tracing::warn!("HTTP connection error from {}: {}", remote_addr, e);
                }
            });
        }
    } else {
        tracing::info!("TLS not configured, starting HTTP server");
        axum::serve(listener, app).await?;
    }

    #[cfg(not(feature = "legacy-tls"))]
    {
        tracing::info!("TLS not configured, starting HTTP server");
        axum::serve(listener, app).await?;
    }

    Ok(())
}
