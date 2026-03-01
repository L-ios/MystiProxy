//! HTTP handlers for MystiCentral API
//!
//! Provides Axum handlers for all API endpoints.

mod routes;

pub use routes::{create_routes, AppState};
