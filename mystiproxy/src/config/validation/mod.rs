//! 配置验证模块

pub mod error;
pub mod rules;

pub use error::{ConfigValidationError, ValidationResult};
pub use rules::{validate_cidr, validate_engine_config};
