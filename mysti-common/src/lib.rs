//! Shared models for MystiProxy ecosystem
//!
//! This crate contains common data models used across multiple components:
//! - mysticentral: Central management server
//! - http_proxy: Local proxy with offline support

pub mod models;

pub use models::*;
