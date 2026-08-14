//! HTTP handlers for MystiCentral API
//!
//! Provides Axum handlers for all API endpoints.

pub(crate) mod auth;
pub(crate) mod conflicts;
mod routes;
pub(crate) mod settings;
pub(crate) mod users;

pub use routes::{create_protected_routes, create_routes, AppState};

// Test shim: integration tests reference `mysticentral::handlers::login`
#[doc(hidden)]
#[allow(unused_imports)]
pub use auth::login;
