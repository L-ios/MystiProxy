//! 配置验证错误定义

use thiserror::Error;
use validator::ValidationErrors;

/// 配置验证错误
#[derive(Error, Debug, serde::Serialize)]
pub enum ConfigValidationError {
    #[error("配置验证失败: {0}")]
    Validation(#[from] ValidationErrors),

    #[error("配置加载失败: {0}")]
    Load(String),

    #[error("配置解析失败: {0}")]
    Parse(String),

    #[error("安全验证失败: {0}")]
    Security(String),

    #[error("配置热重载失败: {0}")]
    HotReload(String),

    #[error("配置文件监控失败: {0}")]
    Watch(String),
}

/// 验证结果类型别名
pub type ValidationResult<T> = Result<T, ConfigValidationError>;
