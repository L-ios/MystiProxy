//! 增强配置加载器

use config::{Config, Environment, File as ConfigFile, FileFormat};
use serde::de::DeserializeOwned;

use crate::config::validation::{validate_engine_config, ConfigValidationError, ValidationResult};
use crate::config::{EngineConfig, MystiConfig};

/// 验证级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationLevel {
    /// 严格模式：验证失败即报错
    Strict,
    /// 警告模式：验证失败仅记录警告
    Warning,
    /// 无验证模式
    None,
}

/// 配置源
pub enum ConfigSource {
    /// YAML 文件
    File(String),
    /// 环境变量
    Environment(String),
    /// 默认值
    Default(serde_json::Value),
}

/// 增强配置加载器
pub struct EnhancedConfigLoader {
    sources: Vec<ConfigSource>,
    validation_level: ValidationLevel,
}

impl Default for EnhancedConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl EnhancedConfigLoader {
    /// 创建新的加载器
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            validation_level: ValidationLevel::Strict,
        }
    }

    /// 设置验证级别
    pub fn with_validation_level(mut self, level: ValidationLevel) -> Self {
        self.validation_level = level;
        self
    }

    /// 添加配置源
    pub fn add_source(mut self, source: ConfigSource) -> Self {
        self.sources.push(source);
        self
    }

    /// 加载并验证配置
    pub fn load<T>(self) -> ValidationResult<T>
    where
        T: DeserializeOwned + serde::Serialize,
    {
        let mut builder = Config::builder();

        // 按优先级加载配置源（后添加的优先级更高）
        for source in self.sources {
            match source {
                ConfigSource::File(path) => {
                    builder = builder.add_source(ConfigFile::new(&path, FileFormat::Yaml));
                }
                ConfigSource::Environment(prefix) => {
                    // prefix_separator 与 separator 保持一致（"__"），
                    // 否则含下划线的前缀无法正确剥离（如 MYSTI_TEST__ENGINE__T）
                    builder = builder.add_source(
                        Environment::with_prefix(&prefix)
                            .prefix_separator("__")
                            .separator("__"),
                    );
                }
                ConfigSource::Default(value) => {
                    let json_str = serde_json::to_string(&value)
                        .map_err(|e| ConfigValidationError::Parse(e.to_string()))?;
                    builder = builder.add_source(ConfigFile::from_str(&json_str, FileFormat::Json));
                }
            }
        }

        let config = builder
            .build()
            .map_err(|e| ConfigValidationError::Load(e.to_string()))?;
        let parsed: T = config
            .try_deserialize()
            .map_err(|e| ConfigValidationError::Parse(e.to_string()))?;

        // 根据验证级别执行验证
        match self.validation_level {
            ValidationLevel::Strict => {
                // 对 EngineConfig 进行额外验证
                if let Ok(mysti_config) = serde_json::to_value(&parsed) {
                    if let Some(mysti) = mysti_config.get("mysti") {
                        if let Some(engines) = mysti.get("engine") {
                            if let Some(engines_map) = engines.as_object() {
                                for (_name, engine_value) in engines_map {
                                    if let Ok(engine) =
                                        serde_json::from_value::<EngineConfig>(engine_value.clone())
                                    {
                                        if let Err(e) = validate_engine_config(&engine) {
                                            return Err(ConfigValidationError::Validation(e));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(parsed)
            }
            ValidationLevel::Warning => {
                if let Ok(mysti_config) = serde_json::to_value(&parsed) {
                    if let Some(mysti) = mysti_config.get("mysti") {
                        if let Some(engines) = mysti.get("engine") {
                            if let Some(engines_map) = engines.as_object() {
                                for (_name, engine_value) in engines_map {
                                    if let Ok(engine) =
                                        serde_json::from_value::<EngineConfig>(engine_value.clone())
                                    {
                                        if let Err(e) = validate_engine_config(&engine) {
                                            tracing::warn!("Engine validation warnings: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(parsed)
            }
            ValidationLevel::None => Ok(parsed),
        }
    }

    /// 从文件加载 MystiConfig（便捷方法）
    pub fn load_mysti_config(path: &str) -> ValidationResult<MystiConfig> {
        Self::new()
            .add_source(ConfigSource::File(path.to_string()))
            .load()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EngineConfig, MystiConfig, ProxyType};

    fn write_temp_yaml(content: &str) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.yaml");
        std::fs::write(&path, content).unwrap();
        let p = path.to_string_lossy().to_string();
        (dir, p)
    }

    fn valid_yaml() -> String {
        "mysti:\n  engine:\n    t:\n      listen: tcp://0.0.0.0:18080\n      target: tcp://127.0.0.1:90\n      proxy_type: tcp\n".to_string()
    }

    fn invalid_engine_yaml() -> String {
        // proxy_type: tcp 但 target 是 http:// → 校验失败
        "mysti:\n  engine:\n    bad:\n      listen: tcp://0.0.0.0:18080\n      target: http://example.com\n      proxy_type: tcp\n".to_string()
    }

    // ---------- T1 基础结构 ----------

    #[test]
    fn test_validation_level_default_is_strict() {
        assert_eq!(
            EnhancedConfigLoader::new().validation_level,
            ValidationLevel::Strict
        );
    }

    #[test]
    fn test_new_creates_empty_loader() {
        assert!(EnhancedConfigLoader::new().sources.is_empty());
    }

    #[test]
    fn test_with_validation_level_changes_level() {
        let l = EnhancedConfigLoader::new().with_validation_level(ValidationLevel::Warning);
        assert_eq!(l.validation_level, ValidationLevel::Warning);
    }

    #[test]
    fn test_add_source_appends_to_sources() {
        let l = EnhancedConfigLoader::new()
            .add_source(ConfigSource::File("a.yaml".into()))
            .add_source(ConfigSource::Environment("MY".into()));
        assert_eq!(l.sources.len(), 2);
    }

    #[test]
    fn test_default_matches_new() {
        let d: EnhancedConfigLoader = Default::default();
        assert_eq!(
            d.validation_level,
            EnhancedConfigLoader::new().validation_level
        );
    }

    // ---------- T2 多源合并 load ----------

    #[test]
    fn test_load_single_file_valid() {
        let (_d, p) = write_temp_yaml(&valid_yaml());
        let cfg: MystiConfig = EnhancedConfigLoader::new()
            .add_source(ConfigSource::File(p))
            .load()
            .unwrap();
        assert!(cfg.mysti.engine.contains_key("t"));
    }

    #[test]
    fn test_load_file_not_found() {
        let r: Result<MystiConfig, _> = EnhancedConfigLoader::new()
            .add_source(ConfigSource::File("nonexistent-file.yaml".into()))
            .load();
        match r {
            Err(crate::config::validation::ConfigValidationError::Load(_)) => {}
            other => panic!("expected Load error, got ok={:?}", other.is_ok()),
        }
    }

    #[test]
    fn test_load_default_as_fallback() {
        let (_d, p) = write_temp_yaml(&valid_yaml());
        let defaults = serde_json::json!({ "mysti": { "engine": {} } });
        let cfg: MystiConfig = EnhancedConfigLoader::new()
            .add_source(ConfigSource::Default(defaults))
            .add_source(ConfigSource::File(p))
            .load()
            .unwrap();
        assert!(cfg.mysti.engine.contains_key("t"));
    }

    #[test]
    fn test_load_deserialize_type_mismatch() {
        // engine 非对象（标量）→ Parse 错误
        let (_d, p) = write_temp_yaml("mysti:\n  engine: 123\n");
        let r: Result<MystiConfig, _> = EnhancedConfigLoader::new()
            .add_source(ConfigSource::File(p))
            .load();
        match r {
            Err(crate::config::validation::ConfigValidationError::Parse(_)) => {}
            other => panic!("expected Parse error, got ok={:?}", other.is_ok()),
        }
    }

    #[test]
    fn test_load_scalar_coerced_then_validation_error() {
        // listen: 123 被 config crate 宽松转为 "123"，随后 Strict 验证报 unsupported_protocol
        let (_d, p) = write_temp_yaml("mysti:\n  engine:\n    t:\n      listen: 123\n      target: tcp://127.0.0.1:90\n      proxy_type: tcp\n");
        let r: Result<MystiConfig, _> = EnhancedConfigLoader::new()
            .add_source(ConfigSource::File(p))
            .load();
        match r {
            Err(crate::config::validation::ConfigValidationError::Validation(_)) => {}
            other => panic!("expected Validation error, got ok={:?}", other.is_ok()),
        }
    }

    #[test]
    fn test_load_env_overrides_file() {
        // 环境变量覆盖文件字段（剥前缀后需含 mysti 层级）
        let (_d, p) = write_temp_yaml(&valid_yaml());
        std::env::set_var(
            "MYSTI_TEST_LOADER__MYSTI__ENGINE__T__LISTEN",
            "tcp://0.0.0.0:19999",
        );
        let cfg: MystiConfig = EnhancedConfigLoader::new()
            .add_source(ConfigSource::File(p))
            .add_source(ConfigSource::Environment("MYSTI_TEST_LOADER".into()))
            .load()
            .unwrap();
        std::env::remove_var("MYSTI_TEST_LOADER__MYSTI__ENGINE__T__LISTEN");
        assert_eq!(cfg.mysti.engine["t"].listen, "tcp://0.0.0.0:19999");
    }

    // ---------- T3 验证级别分支 ----------

    #[test]
    fn test_strict_level_rejects_invalid_engine() {
        let (_d, p) = write_temp_yaml(&invalid_engine_yaml());
        let r: Result<MystiConfig, _> = EnhancedConfigLoader::new()
            .with_validation_level(ValidationLevel::Strict)
            .add_source(ConfigSource::File(p))
            .load();
        match r {
            Err(crate::config::validation::ConfigValidationError::Validation(_)) => {}
            other => panic!("expected Validation error, got ok={:?}", other.is_ok()),
        }
    }

    #[test]
    fn test_warning_level_returns_ok_with_invalid_engine() {
        let (_d, p) = write_temp_yaml(&invalid_engine_yaml());
        let r: Result<MystiConfig, _> = EnhancedConfigLoader::new()
            .with_validation_level(ValidationLevel::Warning)
            .add_source(ConfigSource::File(p))
            .load();
        assert!(r.is_ok(), "Warning level should not block: {:?}", r.err());
    }

    #[test]
    fn test_none_level_skips_validation() {
        let (_d, p) = write_temp_yaml(&invalid_engine_yaml());
        let r: Result<MystiConfig, _> = EnhancedConfigLoader::new()
            .with_validation_level(ValidationLevel::None)
            .add_source(ConfigSource::File(p))
            .load();
        assert!(r.is_ok());
    }

    #[test]
    fn test_load_mysti_config_convenience() {
        let (_d, p) = write_temp_yaml(&valid_yaml());
        let cfg = EnhancedConfigLoader::load_mysti_config(&p).unwrap();
        assert!(cfg.mysti.engine.contains_key("t"));
    }

    // ---------- 引擎校验（原有测试保留） ----------

    #[test]
    fn test_validate_engine_config_valid() {
        let engine = EngineConfig {
            listen: "tcp://0.0.0.0:8080".to_string(),
            target: "tcp://127.0.0.1:80".to_string(),
            proxy_type: ProxyType::Http,
            request_timeout: None,
            connection_timeout: None,
            header: None,
            locations: None,
            tls: None,
            auth: None,
            upstream: None,
            allow: None,
            deny: None,
        };
        assert!(validate_engine_config(&engine).is_ok());
    }

    #[test]
    fn test_validate_engine_config_invalid() {
        let engine = EngineConfig {
            listen: "tcp://0.0.0.0:8080".to_string(),
            target: "tcp://127.0.0.1:80".to_string(),
            proxy_type: ProxyType::Tcp,
            request_timeout: None,
            connection_timeout: None,
            header: None,
            locations: None,
            tls: None,
            auth: None,
            upstream: None,
            allow: None,
            deny: None,
        };
        assert!(validate_engine_config(&engine).is_ok());

        let mut engine2 = engine.clone();
        engine2.target = "http://example.com".to_string();
        assert!(validate_engine_config(&engine2).is_err());
    }
}
