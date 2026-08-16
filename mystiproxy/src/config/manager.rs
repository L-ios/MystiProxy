//! 配置管理器（支持热重载和版本管理）

use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, RwLock as AsyncRwLock};

use crate::config::validation::{ConfigValidationError, ValidationResult};
use crate::config::{EngineConfig, MystiConfig};

/// 配置快照
#[derive(Debug, Clone)]
pub struct ConfigSnapshot {
    pub config: MystiConfig,
    pub timestamp: SystemTime,
    pub version: String,
    pub source: String,
}

/// 配置变更事件
#[derive(Debug, Clone)]
pub struct ConfigChangeEvent {
    pub old_config: MystiConfig,
    pub new_config: MystiConfig,
    pub timestamp: SystemTime,
    pub validation_success: bool,
}

/// 配置管理器
pub struct ConfigurationManager {
    current_config: Arc<AsyncRwLock<MystiConfig>>,
    config_history: Arc<RwLock<Vec<ConfigSnapshot>>>,
    reload_notifier: broadcast::Sender<ConfigChangeEvent>,
    max_history_size: usize,
}

impl ConfigurationManager {
    /// 创建新的配置管理器
    pub fn new(initial_config: MystiConfig) -> Result<Self, ConfigValidationError> {
        let (sender, _) = broadcast::channel(100);

        let manager = Self {
            current_config: Arc::new(AsyncRwLock::new(initial_config.clone())),
            config_history: Arc::new(RwLock::new(Vec::new())),
            reload_notifier: sender,
            max_history_size: 10,
        };

        // 保存初始配置快照
        manager.save_snapshot(&initial_config, "initial".to_string())?;

        Ok(manager)
    }

    /// 获取当前配置
    pub async fn get_current(&self) -> MystiConfig {
        self.current_config.read().await.clone()
    }

    /// 更新配置
    pub async fn update_config(&self, new_config: MystiConfig) -> ValidationResult<()> {
        // 获取旧配置
        let old_config = self.get_current().await;

        // 验证新配置
        let mysti_json: serde_json::Value = serde_json::to_value(&new_config)
            .map_err(|e| ConfigValidationError::Parse(e.to_string()))?;
        let mysti_obj = mysti_json.as_object().ok_or(ConfigValidationError::Parse(
            "missing mysti object".to_string(),
        ))?;
        let mysti_inner = mysti_obj
            .get("mysti")
            .ok_or(ConfigValidationError::Parse("missing mysti".to_string()))?;
        let engines_val = mysti_inner
            .get("engine")
            .ok_or(ConfigValidationError::Parse("missing engine".to_string()))?;
        let engines_map = engines_val.as_object().ok_or(ConfigValidationError::Parse(
            "engine not object".to_string(),
        ))?;

        for (_name, engine_value) in engines_map {
            let engine_value: &serde_json::Value = engine_value;
            if let Ok(engine) = serde_json::from_value::<EngineConfig>(engine_value.clone()) {
                crate::config::validation::validate_engine_config(&engine)?;
            }
        }

        // 更新配置
        {
            let mut config_guard = self.current_config.write().await;
            *config_guard = new_config.clone();
        }

        // 保存快照
        self.save_snapshot(&new_config, "reload".to_string())?;

        // 发送变更通知
        let event = ConfigChangeEvent {
            old_config,
            new_config,
            timestamp: SystemTime::now(),
            validation_success: true,
        };
        let _ = self.reload_notifier.send(event);

        Ok(())
    }

    /// 回滚到上一个版本
    pub async fn rollback_to_previous(&self) -> ValidationResult<()> {
        // 缩短 std RwLock guard 生命周期：取出所需数据后立即释放，避免跨 await 持锁
        let previous_config = {
            let history = self.config_history.read().unwrap();
            if history.len() < 2 {
                return Err(ConfigValidationError::Load(
                    "No previous version to rollback to".to_string(),
                ));
            }
            history[history.len() - 2].config.clone()
        };

        self.update_config(previous_config).await
    }

    /// 获取配置历史
    pub fn get_history(&self) -> Vec<ConfigSnapshot> {
        self.config_history.read().unwrap().clone()
    }

    /// 订阅配置变更
    pub fn subscribe(&self) -> broadcast::Receiver<ConfigChangeEvent> {
        self.reload_notifier.subscribe()
    }

    /// 保存配置快照
    fn save_snapshot(
        &self,
        config: &MystiConfig,
        source: String,
    ) -> Result<(), ConfigValidationError> {
        let mut history = self.config_history.write().unwrap();

        let version = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string();

        let snapshot = ConfigSnapshot {
            config: config.clone(),
            timestamp: SystemTime::now(),
            version,
            source,
        };

        history.push(snapshot);

        // 限制历史大小
        if history.len() > self.max_history_size {
            history.remove(0);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EngineConfig, MystiConfig, ProxyType};
    use std::collections::HashMap;

    fn create_test_config() -> MystiConfig {
        let mut engines = HashMap::new();
        engines.insert(
            "test".to_string(),
            EngineConfig {
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
                management: None,
            },
        );
        MystiConfig {
            mysti: crate::config::Mysti { engine: engines },
            cert: vec![],
        }
    }

    #[tokio::test]
    async fn test_config_manager_creation() {
        let config = create_test_config();
        let manager = ConfigurationManager::new(config.clone()).unwrap();
        let current = manager.get_current().await;
        assert_eq!(current.mysti.engine.len(), 1);
    }

    #[tokio::test]
    async fn test_config_update() {
        let config = create_test_config();
        let manager = ConfigurationManager::new(config).unwrap();

        let mut new_config = create_test_config();
        new_config
            .mysti
            .engine
            .get_mut("test")
            .unwrap()
            .request_timeout = Some(std::time::Duration::from_secs(30));

        let _result: crate::config::validation::ValidationResult<()> =
            manager.update_config(new_config).await;
        _result.unwrap();
        let current = manager.get_current().await;
        assert_eq!(
            current.mysti.engine["test"].request_timeout,
            Some(std::time::Duration::from_secs(30))
        );
    }

    #[tokio::test]
    async fn test_config_history() {
        let config = create_test_config();
        let manager = ConfigurationManager::new(config).unwrap();

        let history = manager.get_history();
        assert_eq!(history.len(), 1); // 初始快照

        let mut new_config = create_test_config();
        new_config
            .mysti
            .engine
            .get_mut("test")
            .unwrap()
            .request_timeout = Some(std::time::Duration::from_secs(30));
        let _result: crate::config::validation::ValidationResult<()> =
            manager.update_config(new_config).await;
        _result.unwrap();

        let history = manager.get_history();
        assert_eq!(history.len(), 2);
    }

    #[tokio::test]
    async fn test_rollback_to_previous() {
        let config = create_test_config();
        let manager = ConfigurationManager::new(config).unwrap();

        let mut new_config = create_test_config();
        new_config
            .mysti
            .engine
            .get_mut("test")
            .unwrap()
            .request_timeout = Some(std::time::Duration::from_secs(30));
        manager.update_config(new_config).await.unwrap();

        // 当前 = 30s，回滚到初始（None）
        manager.rollback_to_previous().await.unwrap();
        let current = manager.get_current().await;
        assert_eq!(current.mysti.engine["test"].request_timeout, None);
    }

    #[tokio::test]
    async fn test_rollback_without_previous_fails() {
        let config = create_test_config();
        let manager = ConfigurationManager::new(config).unwrap();
        // 只有初始快照，无上一版本
        assert!(manager.rollback_to_previous().await.is_err());
    }

    #[tokio::test]
    async fn test_subscribe_receives_change_event() {
        let config = create_test_config();
        let manager = ConfigurationManager::new(config).unwrap();
        let mut rx = manager.subscribe();

        let mut new_config = create_test_config();
        new_config
            .mysti
            .engine
            .get_mut("test")
            .unwrap()
            .request_timeout = Some(std::time::Duration::from_secs(30));
        manager.update_config(new_config.clone()).await.unwrap();

        let ev = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("event within 2s")
            .expect("channel open");
        assert!(ev.validation_success);
        assert_eq!(
            ev.new_config.mysti.engine["test"].request_timeout,
            Some(std::time::Duration::from_secs(30))
        );
    }

    #[tokio::test]
    async fn test_update_config_invalid_rejected() {
        let config = create_test_config();
        let manager = ConfigurationManager::new(config).unwrap();

        let mut bad = create_test_config();
        bad.mysti.engine.get_mut("test").unwrap().target = "http://example.com".to_string();
        // tcp 代理 target 必须是 tcp:// → 验证失败
        assert!(manager.update_config(bad).await.is_err());

        // 配置未变
        let current = manager.get_current().await;
        assert_eq!(current.mysti.engine["test"].target, "tcp://127.0.0.1:80");
    }

    #[tokio::test]
    async fn test_history_capped_at_max() {
        let config = create_test_config();
        let manager = ConfigurationManager::new(config).unwrap();

        // 连续 15 次更新（> max_history_size=10）
        for i in 0..15 {
            let mut c = create_test_config();
            c.mysti.engine.get_mut("test").unwrap().listen = format!("tcp://0.0.0.0:{i}");
            manager.update_config(c).await.unwrap();
        }

        let history = manager.get_history();
        assert_eq!(history.len(), 10, "history must be capped");
        // 最新一条是最后更新的配置
        let last = history.last().unwrap();
        assert_eq!(last.config.mysti.engine["test"].listen, "tcp://0.0.0.0:14");
    }

    #[tokio::test]
    async fn test_snapshot_metadata() {
        let config = create_test_config();
        let manager = ConfigurationManager::new(config).unwrap();
        let history = manager.get_history();
        assert_eq!(history[0].source, "initial");
        assert!(!history[0].version.is_empty());
    }
}
