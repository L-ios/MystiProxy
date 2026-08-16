//! 配置文件监控器（热重载支持）

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{error, info};

use crate::config::loader::EnhancedConfigLoader;
use crate::config::manager::ConfigurationManager;
use crate::config::validation::ConfigValidationError;
use crate::config::MystiConfig;

/// 配置文件监控器
pub struct ConfigFileWatcher {
    /// 保持 watch 注册存活的句柄（drop 即停止监听）
    #[allow(dead_code)]
    watcher: RecommendedWatcher,
    config_path: String,
    debounce_interval: Duration,
    reload_tx: mpsc::Sender<()>,
}

impl ConfigFileWatcher {
    /// 被监控的配置文件路径
    pub fn config_path(&self) -> &str {
        &self.config_path
    }

    /// debounce 间隔
    pub fn debounce_interval(&self) -> Duration {
        self.debounce_interval
    }

    /// 手动触发一次重载（走与文件事件相同的 debounce 路径）
    pub fn trigger_reload(&self) {
        let _ = self.reload_tx.try_send(());
    }

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

        // macOS FSEvents 对"单个文件 + NonRecursive"不投递事件，
        // 统一改为 watch 父目录并按文件名过滤（跨平台一致）
        let config_path_ = Path::new(&config_path);
        let file_name = config_path_.file_name().map(|n| n.to_os_string());
        let watch_dir = match config_path_.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => std::path::PathBuf::from("."),
        };

        let mut watcher = RecommendedWatcher::new(
            {
                let tx = tx.clone();
                let file_name = file_name.clone();
                move |res: Result<Event, notify::Error>| {
                    if let Ok(event) = res {
                        if !matches!(
                            event.kind,
                            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                        ) {
                            return;
                        }
                        // 仅响应目标文件自身的事件
                        let matched = match &file_name {
                            Some(name) => event
                                .paths
                                .iter()
                                .any(|p| p.file_name().map(|n| n == name).unwrap_or(false)),
                            None => true,
                        };
                        if matched {
                            // 不能用 blocking_send：notify 回调线程若阻塞在满 channel，
                            // 而 FsEventWatcher::drop→stop() 在等回调返回，会形成死锁。
                            // channel(1) 满时丢弃信号即 debounce 语义。
                            let _ = tx.try_send(());
                        }
                    }
                }
            },
            Config::default(),
        )
        .map_err(|e| ConfigValidationError::Watch(e.to_string()))?;

        watcher
            .watch(&watch_dir, RecursiveMode::NonRecursive)
            .map_err(|e| ConfigValidationError::Watch(e.to_string()))?;

        // 启动后台任务处理重载
        let callback_bg = callback;
        let config_path_bg = config_path.clone();
        let debounce = Duration::from_millis(debounce_ms);
        tokio::spawn(async move {
            while rx.recv().await.is_some() {
                sleep(debounce).await;

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

/// 创建并启动配置监控任务
pub async fn start_config_watcher(
    config_path: String,
    debounce_ms: u64,
    manager: Arc<ConfigurationManager>,
) -> Result<tokio::task::JoinHandle<()>, ConfigValidationError> {
    let manager_clone = manager.clone();

    let watcher =
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

    // watcher 必须保持存活：watch 注册随结构 drop 而取消。
    // 将其 move 进长活任务，handle abort/结束时一并释放。
    let handle = tokio::spawn(async move {
        let _watcher = watcher;
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });

    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EngineConfig, Mysti, ProxyType};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::Instant;

    fn make_yaml(engine_count: usize) -> String {
        let mut engines = String::new();
        for i in 0..engine_count {
            engines.push_str(&format!(
                "    e{}:\n      listen: tcp://0.0.0.0:{}\n      target: tcp://127.0.0.1:90{}\n      proxy_type: tcp\n",
                i, 9000 + i, i
            ));
        }
        format!("mysti:\n  engine:\n{engines}")
    }

    fn make_config(engine_count: usize) -> MystiConfig {
        let mut engines = HashMap::new();
        for i in 0..engine_count {
            engines.insert(
                format!("e{i}"),
                EngineConfig {
                    listen: format!("tcp://0.0.0.0:{}", 9000 + i),
                    target: format!("tcp://127.0.0.1:90{i}"),
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
                    management: None,
                },
            );
        }
        MystiConfig {
            mysti: Mysti { engine: engines },
            cert: vec![],
        }
    }

    // ---------- T1 创建 ----------

    #[tokio::test]
    async fn test_watcher_creation_with_valid_path() {
        let dir = tempfile::tempdir().unwrap();
        let f = tempfile::NamedTempFile::new_in(&dir).unwrap();
        std::fs::write(f.path(), make_yaml(1)).unwrap();
        let w = ConfigFileWatcher::new(f.path().to_string_lossy().to_string(), 250, |_| Ok(()));
        assert!(w.is_ok());
        let w = w.unwrap();
        assert!(w.config_path().ends_with(".tmp") || w.config_path().contains('.'));
        assert_eq!(w.debounce_interval(), Duration::from_millis(250));
    }

    #[tokio::test]
    async fn test_trigger_reload_api() {
        let dir = tempfile::tempdir().unwrap();
        let f = tempfile::NamedTempFile::new_in(&dir).unwrap();
        std::fs::write(f.path(), make_yaml(1)).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let w = ConfigFileWatcher::new(f.path().to_string_lossy().to_string(), 100, move |_cfg| {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        w.trigger_reload(); // 手动触发走相同路径
        tokio::time::timeout(Duration::from_secs(5), async {
            while calls.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("manual trigger reload");
    }

    #[tokio::test]
    async fn test_watcher_creation_with_invalid_path() {
        let w = ConfigFileWatcher::new("/nonexistent/path/x.yaml".to_string(), 100, |_| Ok(()));
        match w {
            Err(ConfigValidationError::Watch(_)) => {}
            other => panic!("expected Watch error, got {:?}", other.map(|_| ())),
        }
    }

    #[tokio::test]
    async fn test_watcher_creation_with_invalid_yaml_content() {
        let dir = tempfile::tempdir().unwrap();
        let f = tempfile::NamedTempFile::new_in(&dir).unwrap();
        std::fs::write(f.path(), "::: not yaml :::\n  - broken").unwrap();
        // new 不预加载内容，仍应 Ok（加载失败在后台任务中处理）
        assert!(
            ConfigFileWatcher::new(f.path().to_string_lossy().to_string(), 100, |_| Ok(())).is_ok()
        );
    }

    // ---------- T2 事件过滤 ----------

    #[tokio::test]
    async fn test_modify_triggers_reload() {
        let dir = tempfile::tempdir().unwrap();
        let f = tempfile::NamedTempFile::new_in(&dir).unwrap();
        std::fs::write(f.path(), make_yaml(1)).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let _w = ConfigFileWatcher::new(f.path().to_string_lossy().to_string(), 100, move |_cfg| {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await; // 等 watch 建立
        std::fs::write(f.path(), make_yaml(2)).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while calls.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("reload triggered by modify");
    }

    #[tokio::test]
    async fn test_create_event_triggers_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cfg.yaml");
        // 先不创建文件：watch 建立后新文件创建即 Create 事件
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let _w = ConfigFileWatcher::new(path.to_string_lossy().to_string(), 100, move |_cfg| {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        std::fs::write(&path, make_yaml(1)).unwrap(); // 新建文件
        tokio::time::timeout(Duration::from_secs(5), async {
            while calls.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("reload triggered by create");
    }

    #[tokio::test]
    async fn test_access_event_does_not_trigger() {
        let dir = tempfile::tempdir().unwrap();
        let f = tempfile::NamedTempFile::new_in(&dir).unwrap();
        std::fs::write(f.path(), make_yaml(1)).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let _w = ConfigFileWatcher::new(f.path().to_string_lossy().to_string(), 100, move |_cfg| {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        for _ in 0..3 {
            let _ = std::fs::read(f.path()).unwrap(); // Access 事件
        }
        tokio::time::sleep(Duration::from_millis(700)).await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "Access must not trigger reload"
        );
    }

    // ---------- T3 debounce ----------

    #[tokio::test]
    async fn test_debounce_uses_configured_interval() {
        let dir = tempfile::tempdir().unwrap();
        let f = tempfile::NamedTempFile::new_in(&dir).unwrap();
        std::fs::write(f.path(), make_yaml(1)).unwrap();
        let fired_at = Arc::new(Mutex::new(None::<Instant>));
        let fa = fired_at.clone();
        let start = Instant::now();
        let _w =
            ConfigFileWatcher::new(f.path().to_string_lossy().to_string(), 1000, move |_cfg| {
                *fa.lock().unwrap() = Some(Instant::now());
                Ok(())
            })
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        std::fs::write(f.path(), make_yaml(2)).unwrap();
        tokio::time::timeout(Duration::from_secs(6), async {
            loop {
                if fired_at.lock().unwrap().is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("reload fired");
        let elapsed = fired_at.lock().unwrap().unwrap().duration_since(start);
        // debounce_ms=1000 应驱动等待：写入+1000ms 之后才触发（容忍 200ms 抖动）
        assert!(
            elapsed >= Duration::from_millis(900),
            "debounce interval not respected: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn test_debounce_zero_ms_still_works() {
        let dir = tempfile::tempdir().unwrap();
        let f = tempfile::NamedTempFile::new_in(&dir).unwrap();
        std::fs::write(f.path(), make_yaml(1)).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let _w = ConfigFileWatcher::new(f.path().to_string_lossy().to_string(), 0, move |_cfg| {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        std::fs::write(f.path(), make_yaml(2)).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while calls.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("reload with zero debounce");
    }

    // ---------- T4 回调 ----------

    #[tokio::test]
    async fn test_callback_receives_new_config() {
        let dir = tempfile::tempdir().unwrap();
        let f = tempfile::NamedTempFile::new_in(&dir).unwrap();
        std::fs::write(f.path(), make_yaml(1)).unwrap();
        let got = Arc::new(Mutex::new(Vec::<MystiConfig>::new()));
        let g = got.clone();
        let _w = ConfigFileWatcher::new(f.path().to_string_lossy().to_string(), 100, move |cfg| {
            g.lock().unwrap().push(cfg);
            Ok(())
        })
        .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        std::fs::write(f.path(), make_yaml(3)).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if !got.lock().unwrap().is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("callback received config");
        let configs = got.lock().unwrap();
        assert_eq!(configs.last().unwrap().mysti.engine.len(), 3);
    }

    #[tokio::test]
    async fn test_callback_error_does_not_crash_watcher() {
        let dir = tempfile::tempdir().unwrap();
        let f = tempfile::NamedTempFile::new_in(&dir).unwrap();
        std::fs::write(f.path(), make_yaml(1)).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let _w = ConfigFileWatcher::new(f.path().to_string_lossy().to_string(), 100, move |_cfg| {
            // 第一次 Err，之后 Ok
            if c.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err(ConfigValidationError::Load("first fails".into()));
            }
            Ok(())
        })
        .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        std::fs::write(f.path(), make_yaml(2)).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while calls.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("first call");
        // 回调失败不影响 watcher：再改一次仍触发
        std::fs::write(f.path(), make_yaml(3)).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while calls.load(Ordering::SeqCst) < 2 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("watcher survived callback error");
    }

    // ---------- T5 start_config_watcher ----------

    #[tokio::test]
    async fn test_start_config_watcher_updates_manager() {
        let dir = tempfile::tempdir().unwrap();
        let f = tempfile::NamedTempFile::new_in(&dir).unwrap();
        std::fs::write(f.path(), make_yaml(1)).unwrap();
        let manager = Arc::new(ConfigurationManager::new(make_config(1)).unwrap());
        let _h = start_config_watcher(f.path().to_string_lossy().to_string(), 100, manager.clone())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        std::fs::write(f.path(), make_yaml(2)).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while manager.get_current().await.mysti.engine.len() != 2 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("manager updated by watcher");
    }

    #[tokio::test]
    async fn test_start_config_watcher_invalid_path_returns_err() {
        let manager = Arc::new(ConfigurationManager::new(make_config(1)).unwrap());
        let r = start_config_watcher("/nonexistent/x.yaml".to_string(), 100, manager).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn test_start_config_watcher_invalid_yaml_does_not_update_manager() {
        let dir = tempfile::tempdir().unwrap();
        let f = tempfile::NamedTempFile::new_in(&dir).unwrap();
        std::fs::write(f.path(), make_yaml(1)).unwrap();
        let manager = Arc::new(ConfigurationManager::new(make_config(1)).unwrap());
        let _h = start_config_watcher(f.path().to_string_lossy().to_string(), 100, manager.clone())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        std::fs::write(f.path(), "::: broken yaml").unwrap();
        tokio::time::sleep(Duration::from_millis(800)).await;
        // 加载失败：manager 保持初始配置
        assert_eq!(manager.get_current().await.mysti.engine.len(), 1);
    }

    // ---------- T6 集成 ----------

    #[tokio::test]
    async fn test_watcher_with_manager_subscribe() {
        let dir = tempfile::tempdir().unwrap();
        let f = tempfile::NamedTempFile::new_in(&dir).unwrap();
        std::fs::write(f.path(), make_yaml(1)).unwrap();
        let manager = Arc::new(ConfigurationManager::new(make_config(1)).unwrap());
        let mut rx = manager.subscribe();
        let _h = start_config_watcher(f.path().to_string_lossy().to_string(), 100, manager.clone())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        std::fs::write(f.path(), make_yaml(2)).unwrap();
        let ev = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("event received")
            .expect("channel open");
        assert!(ev.validation_success);
        assert_eq!(ev.new_config.mysti.engine.len(), 2);
    }

    #[tokio::test]
    async fn test_watcher_full_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let f = tempfile::NamedTempFile::new_in(&dir).unwrap();
        std::fs::write(f.path(), make_yaml(1)).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let _w = ConfigFileWatcher::new(f.path().to_string_lossy().to_string(), 100, move |_cfg| {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;

        // 1) 合法修改 → 触发
        std::fs::write(f.path(), make_yaml(2)).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while calls.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("lifecycle: first reload");

        // 2) 非法 YAML → 回调不增加
        let before = calls.load(Ordering::SeqCst);
        std::fs::write(f.path(), "::: broken").unwrap();
        tokio::time::sleep(Duration::from_millis(800)).await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            before,
            "invalid yaml must not invoke callback"
        );

        // 3) 改回合法 → 再次触发
        std::fs::write(f.path(), make_yaml(3)).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while calls.load(Ordering::SeqCst) <= before {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("lifecycle: recovered");
    }

    #[test]
    fn test_should_trigger_reload() {
        // 保留原占位（兼容历史命名）：结构验证已由上方用例覆盖
    }
}
