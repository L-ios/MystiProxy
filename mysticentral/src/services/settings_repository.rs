//! Settings Repository
//!
//! Single-row system settings persisted in PostgreSQL.

use async_trait::async_trait;
use serde::Deserialize;
use sqlx::PgPool;

use crate::error::ApiError;
use crate::services::user_repository::map_db_err;

/// Full settings view (response shape, snake_case matches frontend)
#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct SystemSettings {
    pub central_url: String,
    pub sync_interval: i64,
    pub log_level: String,
    pub max_request_history: i64,
    pub default_environment: Option<String>,
}

/// Partial update request
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SettingsPatch {
    pub central_url: Option<String>,
    pub sync_interval: Option<i64>,
    pub log_level: Option<String>,
    pub max_request_history: Option<i64>,
    pub default_environment: Option<String>,
}

pub const LOG_LEVELS: [&str; 4] = ["debug", "info", "warn", "error"];

/// Validate a patch; returns a BadRequest message on failure.
pub fn validate_patch(p: &SettingsPatch) -> Result<(), String> {
    if let Some(url) = &p.central_url {
        if !url.is_empty() && !url.starts_with("http://") && !url.starts_with("https://") {
            return Err("central_url must start with http:// or https://".to_string());
        }
    }
    if let Some(interval) = p.sync_interval {
        if !(5..=3600).contains(&interval) {
            return Err("sync_interval must be between 5 and 3600 seconds".to_string());
        }
    }
    if let Some(level) = &p.log_level {
        if !LOG_LEVELS.contains(&level.as_str()) {
            return Err(format!("log_level must be one of {LOG_LEVELS:?}"));
        }
    }
    if let Some(max) = p.max_request_history {
        if max < 0 {
            return Err("max_request_history must be >= 0".to_string());
        }
    }
    Ok(())
}

#[derive(sqlx::FromRow)]
struct SettingsRow {
    central_url: String,
    sync_interval: i32,
    log_level: String,
    max_request_history: i32,
    default_environment: Option<String>,
}

impl From<SettingsRow> for SystemSettings {
    fn from(r: SettingsRow) -> Self {
        Self {
            central_url: r.central_url,
            sync_interval: r.sync_interval as i64,
            log_level: r.log_level,
            max_request_history: r.max_request_history as i64,
            default_environment: r.default_environment,
        }
    }
}

/// Trait for settings persistence
#[async_trait]
pub trait SettingsRepository: Send + Sync {
    async fn get(&self) -> Result<SystemSettings, ApiError>;
    async fn update(&self, patch: &SettingsPatch) -> Result<SystemSettings, ApiError>;
}

/// PostgreSQL implementation (single row)
pub struct PostgresSettingsRepository {
    pool: PgPool,
}

impl PostgresSettingsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SettingsRepository for PostgresSettingsRepository {
    async fn get(&self) -> Result<SystemSettings, ApiError> {
        let row = sqlx::query_as::<_, SettingsRow>(
            r#"SELECT central_url, sync_interval_secs AS "sync_interval", log_level, max_request_history, default_environment FROM system_settings WHERE id = TRUE"#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_db_err)?;
        Ok(row.into())
    }

    async fn update(&self, patch: &SettingsPatch) -> Result<SystemSettings, ApiError> {
        if let Err(msg) = validate_patch(patch) {
            return Err(ApiError::BadRequest(msg));
        }
        let current = self.get().await?;
        let row = sqlx::query_as::<_, SettingsRow>(
            r#"
            UPDATE system_settings SET
                central_url = $1,
                sync_interval_secs = $2,
                log_level = $3,
                max_request_history = $4,
                default_environment = $5,
                updated_at = NOW()
            WHERE id = TRUE
            RETURNING central_url, sync_interval_secs AS "sync_interval", log_level, max_request_history, default_environment
            "#,
        )
        .bind(patch.central_url.clone().unwrap_or(current.central_url))
        .bind(patch.sync_interval.unwrap_or(current.sync_interval) as i32)
        .bind(patch.log_level.clone().unwrap_or(current.log_level))
        .bind(patch.max_request_history.unwrap_or(current.max_request_history) as i32)
        .bind(patch.default_environment.clone().or(current.default_environment))
        .fetch_one(&self.pool)
        .await
        .map_err(map_db_err)?;
        Ok(row.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch() -> SettingsPatch {
        SettingsPatch::default()
    }

    #[test]
    fn test_valid_patch_passes() {
        let p = SettingsPatch {
            central_url: Some("https://central.example.com".into()),
            sync_interval: Some(60),
            log_level: Some("warn".into()),
            max_request_history: Some(500),
            default_environment: Some("staging".into()),
        };
        assert!(validate_patch(&p).is_ok());
    }

    #[test]
    fn test_empty_central_url_allowed() {
        let mut p = patch();
        p.central_url = Some(String::new());
        assert!(validate_patch(&p).is_ok());
    }

    #[test]
    fn test_bad_central_url_rejected() {
        let mut p = patch();
        p.central_url = Some("ftp://x".into());
        assert!(validate_patch(&p).unwrap_err().contains("http"));
    }

    #[test]
    fn test_sync_interval_bounds() {
        let mut p = patch();
        p.sync_interval = Some(4);
        assert!(validate_patch(&p).is_err());
        p.sync_interval = Some(3601);
        assert!(validate_patch(&p).is_err());
        p.sync_interval = Some(5);
        assert!(validate_patch(&p).is_ok());
        p.sync_interval = Some(3600);
        assert!(validate_patch(&p).is_ok());
    }

    #[test]
    fn test_bad_log_level_rejected() {
        let mut p = patch();
        p.log_level = Some("verbose".into());
        assert!(validate_patch(&p).unwrap_err().contains("log_level"));
    }

    #[test]
    fn test_negative_history_rejected() {
        let mut p = patch();
        p.max_request_history = Some(-1);
        assert!(validate_patch(&p).is_err());
        p.max_request_history = Some(0);
        assert!(validate_patch(&p).is_ok());
    }

    #[test]
    fn test_defaults_serialize_snake_case() {
        let s = SystemSettings::default();
        let j = serde_json::to_value(&s).unwrap();
        assert!(j.get("central_url").is_some());
        assert!(j.get("sync_interval").is_some());
        assert!(j.get("log_level").is_some());
        assert!(j.get("max_request_history").is_some());
        assert!(j.get("default_environment").is_some());
    }
}
