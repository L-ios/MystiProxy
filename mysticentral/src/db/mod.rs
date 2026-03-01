//! Database module for MystiCentral
//!
//! Provides database connection pool and repository implementations.

pub mod pool;

pub use pool::create_pool;
