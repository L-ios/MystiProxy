//! 配置文件监控器（热重载支持）

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{error, info, warn};

use crate::config::loader::EnhancedConfigLoader;
use crate::config::manager::ConfigurationManager;
use crate::config::validation::ConfigValidationError;
use crate::config::MystiConfig;

/// 配置文件监控器
pub struct ConfigFileWatcher {
    watcher: RecommendedWatcher,
    config_path: String,
    debounce_interval: Duration,
    reload_tx: mpsc::Sender<()>,
}

impl ConfigFileWatcher {
    /// 创建新的配置文件监控器
    pub fn new<F>(
        config_path: String,
        debounce_ms: u64,
        reload_callback: F,
    ) -> Result<Self, ConfigValidationError>
    where
        F: Fn(MystiConfig) -> Result<(), ConfigValidationError> + Send + Sync + 'static,
    {
        let (tx, mut rx) = mpsc::channel(1);

        let callback = Arc::new(reload_callback);
        let callback_clone = callback.clone();

        let mut watcher = RecommendedWatcher::new(
            {
                let tx = tx.clone();
                move |res: Result<Event, notify::Error>| {
                    if let Ok(event) = res {
                        if matches!(
                            event.kind,
                            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                        ) {
                            let _ = tx.blocking_send(());
                        }
                    }
                }
            },
            Config::default(),
        )
        .map_err(|e| ConfigValidationError::Watch(e.to_string()))?;

        watcher
            .watch(Path::new(&config_path), RecursiveMode::NonRecursive)
            .map_err(|e| ConfigValidationError::Watch(e.to_string()))?;

        // 启动后台任务处理重载
        let callback_bg = callback_clone;
        let config_path_bg = config_path.clone();
        tokio::spawn(async move {
            while rx.recv().await.is_some() {
                sleep(Duration::from_millis(500)).await; // debounce

                let loader = EnhancedConfigLoader::new().add_source(
                    crate::config::loader::ConfigSource::File(config_path_bg.clone()),
                );

                match loader.load::<MystiConfig>() {
                    Ok(new_config) => {
                        info!("Configuration loaded successfully");
                        if let Err(e) = callback_bg(new_config) {
                            error!("Failed to apply config: {:?}", e);
                        }
                    }
                    Err(e) => {
                        error!("Failed to reload configuration: {:?}", e);
                    }
                }
            }
        });

        Ok(Self {
            watcher,
            config_path,
            debounce_interval: Duration::from_millis(debounce_ms),
            reload_tx: tx,
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_should_trigger_reload() {
        // 这个测试需要实际的文件系统事件，这里仅验证编译通过
    }
}

/// 创建并启动配置监控任务
pub async fn start_config_watcher(
    config_path: String,
    debounce_ms: u64,
    manager: Arc<ConfigurationManager>,
) -> Result<tokio::task::JoinHandle<()>, ConfigValidationError> {
    let manager_clone = manager.clone();

    let mut watcher =
        match ConfigFileWatcher::new(config_path.clone(), debounce_ms, move |new_config| {
            let manager = manager_clone.clone();
            tokio::spawn(async move {
                if let Err(e) = manager.update_config(new_config).await {
                    error!("Failed to update config: {}", e);
                }
            });
            Ok(())
        }) {
            Ok(w) => w,
            Err(e) => {
                error!("Failed to create config watcher: {}", e);
                return Err(e);
            }
        };

    let handle = tokio::spawn(async move {
        // watcher 已在 new() 中启动监控，这里只需保持任务存活
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });

    Ok(handle)
}
