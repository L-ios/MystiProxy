//! 增强配置加载器

use config::{Config, Environment, File as ConfigFile, FileFormat};
use serde::de::DeserializeOwned;
use std::path::Path;

use crate::config::validation::{validate_engine_config, ConfigValidationError, ValidationResult};
use crate::config::{EngineConfig, MystiConfig, ProxyType};

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
                    builder = builder.add_source(Environment::with_prefix(&prefix).separator("__"));
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
                                for (name, engine_value) in engines_map {
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
                                for (name, engine_value) in engines_map {
                                    if let Ok(engine) =
                                        serde_json::from_value::<EngineConfig>(engine_value.clone())
                                    {
                                        if let Err(e) = validate_engine_config(&engine) {
                                            tracing::warn!(
                                                "Engine '{}' validation warnings: {}",
                                                name,
                                                e
                                            );
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
    use crate::config::{EngineConfig, ProxyType};

    #[test]
    fn test_load_valid_config() {
        let yaml = r#"
mysti:
  engine:
    test:
      listen: tcp://0.0.0.0:8080
      target: tcp://127.0.0.1:80
      proxy_type: http
"#;
        let config: MystiConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.mysti.engine.contains_key("test"));
    }

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
        // TCP 代理要求 target 也是 tcp://
        assert!(validate_engine_config(&engine).is_ok());

        // 但如果 target 是 http:// 则应失败
        let mut engine2 = engine.clone();
        engine2.target = "http://example.com".to_string();
        assert!(validate_engine_config(&engine2).is_err());
    }
}
