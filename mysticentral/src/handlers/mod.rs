//! HTTP handlers for MystiCentral API
//!
//! Provides Axum handlers for all API endpoints.

pub(crate) mod auth;
mod routes;
pub(crate) mod users;

pub use auth::{login, logout};
pub use routes::{create_protected_routes, create_routes, AppState};
pub use users::{
    change_own_password, create_user, delete_user, get_current_user, get_user, list_users,
    update_user,
};
