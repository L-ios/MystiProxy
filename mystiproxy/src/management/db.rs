//! SQLite database module for local mock management
//!
//! Provides connection pool management and schema migrations.

use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::Executor;
use std::path::Path;
use tracing::{info, warn};

use super::error::Result;

/// Database schema version
const SCHEMA_VERSION: i32 = 1;

/// Create a new SQLite connection pool
pub async fn create_pool(db_path: &Path) -> Result<SqlitePool> {
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
    
    info!("Connecting to SQLite database: {}", db_path.display());
    
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;
    
    // Run migrations
    run_migrations(&pool).await?;
    
    Ok(pool)
}

/// Create an in-memory SQLite pool (for testing)
pub async fn create_memory_pool() -> Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    
    run_migrations(&pool).await?;
    
    Ok(pool)
}

/// Run database migrations
async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    info!("Running database migrations...");
    
    // Create schema_version table if not exists
    pool.execute(
        r#"
        CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )
        "#,
    )
    .await?;
    
    // Get current version
    let current_version: Option<i32> = sqlx::query_scalar(
        "SELECT MAX(version) FROM schema_version",
    )
    .fetch_optional(pool)
    .await?
    .flatten();
    
    let current_version = current_version.unwrap_or(0);
    
    if current_version < SCHEMA_VERSION {
        info!("Migrating database from version {} to {}", current_version, SCHEMA_VERSION);
        
        // Run migrations in order
        for version in (current_version + 1)..=SCHEMA_VERSION {
            migrate_to_version(pool, version).await?;
            
            sqlx::query("INSERT INTO schema_version (version) VALUES (?)")
                .bind(version)
                .execute(pool)
                .await?;
        }
    } else {
        info!("Database schema is up to date (version {})", current_version);
    }
    
    Ok(())
}

/// Migrate to a specific schema version
async fn migrate_to_version(pool: &SqlitePool, version: i32) -> Result<()> {
    match version {
        1 => migrate_v1(pool).await,
        _ => {
            warn!("Unknown migration version: {}", version);
            Ok(())
        }
    }
}

/// Migration to version 1: Initial schema
async fn migrate_v1(pool: &SqlitePool) -> Result<()> {
    info!("Applying migration v1: Initial schema");
    
    pool.execute(
        r#"
        -- Mock configurations table
        CREATE TABLE IF NOT EXISTS mock_configurations (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            path TEXT NOT NULL,
            method TEXT NOT NULL,
            matching_rules TEXT NOT NULL,
            response_config TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'local',
            version_vector TEXT NOT NULL DEFAULT '{}',
            content_hash TEXT NOT NULL DEFAULT '',
            is_active INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        
        -- Index for path + method lookups
        CREATE INDEX IF NOT EXISTS idx_mock_path_method 
        ON mock_configurations(path, method);
        
        -- Index for active status
        CREATE INDEX IF NOT EXISTS idx_mock_active 
        ON mock_configurations(is_active);
        
        -- Index for content hash (for sync)
        CREATE INDEX IF NOT EXISTS idx_mock_content_hash 
        ON mock_configurations(content_hash);
        
        -- Sync records table (for tracking sync operations)
        CREATE TABLE IF NOT EXISTS sync_records (
            id TEXT PRIMARY KEY,
            config_id TEXT NOT NULL,
            operation_type TEXT NOT NULL,
            source TEXT NOT NULL,
            conflict_status TEXT NOT NULL DEFAULT 'none',
            timestamp TEXT NOT NULL,
            payload_snapshot TEXT,
            FOREIGN KEY (config_id) REFERENCES mock_configurations(id) ON DELETE CASCADE
        );
        
        -- Index for sync records by config
        CREATE INDEX IF NOT EXISTS idx_sync_config 
        ON sync_records(config_id);
        
        -- Index for sync records by timestamp
        CREATE INDEX IF NOT EXISTS idx_sync_timestamp 
        ON sync_records(timestamp);
        
        -- Offline queue table (for pending sync operations)
        CREATE TABLE IF NOT EXISTS offline_queue (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            operation_type TEXT NOT NULL,
            config_id TEXT,
            payload TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            retry_count INTEGER NOT NULL DEFAULT 0,
            last_error TEXT
        );
        
        -- Index for offline queue processing
        CREATE INDEX IF NOT EXISTS idx_offline_queue_created 
        ON offline_queue(created_at);
        "#,
    )
    .await?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_create_memory_pool() {
        let pool = create_memory_pool().await.unwrap();
        
        // Verify tables exist
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='mock_configurations'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        
        assert_eq!(count, 1);
    }
    
    #[tokio::test]
    async fn test_schema_version() {
        let pool = create_memory_pool().await.unwrap();
        
        let version: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(version) FROM schema_version"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        
        let version = version.unwrap_or(0) as i32;
        
        assert_eq!(version, SCHEMA_VERSION);
    }
}
