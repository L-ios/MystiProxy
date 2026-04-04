//! Database connection pool module
//!
//! Provides PostgreSQL connection pool creation and management.

use anyhow::Result;
use sqlx::postgres::{PgPoolOptions, PgConnectOptions};
use sqlx::PgPool;
use std::str::FromStr;

/// Database configuration for connection pool
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

/// Create a new PostgreSQL connection pool
#[allow(dead_code)]
pub async fn create_pool(config: &DatabaseConfig) -> Result<PgPool> {
    let options = PgConnectOptions::from_str(&config.url)?;
    
    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .connect_with(options)
        .await?;

    tracing::info!(
        "Created database connection pool with max {} connections",
        config.max_connections
    );

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_config() {
        let config = DatabaseConfig {
            url: "postgresql://user:pass@localhost:5432/test".to_string(),
            max_connections: 20,
        };
        assert_eq!(config.max_connections, 20);
    }
}
