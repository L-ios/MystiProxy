use std::sync::Arc;

use clap::Parser;
use mystiproxy::config::{EngineConfig, MystiConfig, ProxyType};
use mystiproxy::http::{create_handler, HttpServer, HttpServerConfig};
use mystiproxy::proxy::ProxyServer;
use mystiproxy::Result;
use std::collections::HashMap;
use tokio::signal;
use tokio::task::JoinSet;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod arg;

use arg::MystiArg;

#[tokio::main]
async fn main() -> Result<()> {
    let args = MystiArg::parse();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("MystiProxy 启动中...");

    let config = load_config(&args)?;

    let engines = config.mysti.engine;
    if engines.is_empty() {
        warn!("没有配置任何代理引擎");
        return Ok(());
    }

    info!("共配置 {} 个代理引擎", engines.len());

    let mut tasks: JoinSet<Result<()>> = JoinSet::new();

    for (name, engine_config) in engines {
        let name_clone = name.clone();
        match engine_config.proxy_type {
            ProxyType::Tcp => match ProxyServer::from_engine_config(&engine_config) {
                Ok(mut server) => {
                    if let Err(e) = server.start().await {
                        error!("代理引擎 '{}' 启动失败: {}", name_clone, e);
                        continue;
                    }

                    info!(
                        "代理引擎 '{}' 已启动: {} -> {} (TCP)",
                        name_clone,
                        server.listen_addr(),
                        server.target_addr(),
                    );

                    tasks.spawn(async move { server.run().await });
                }
                Err(e) => {
                    error!("创建代理引擎 '{}' 失败: {}", name_clone, e);
                }
            },
            ProxyType::Http => {
                let handler = match create_handler(Arc::new(engine_config.clone())) {
                    Ok(h) => h,
                    Err(e) => {
                        error!("创建 HTTP 处理器 '{}' 失败: {}", name_clone, e);
                        continue;
                    }
                };

                let mut server = HttpServer::new(
                    HttpServerConfig::new(
                        engine_config.listen.clone(),
                        engine_config.request_timeout,
                    ),
                    handler,
                );

                if let Err(e) = server.start().await {
                    error!("HTTP 引擎 '{}' 启动失败: {}", name_clone, e);
                    continue;
                }

                info!(
                    "HTTP 引擎 '{}' 已启动: {} -> {} (HTTP)",
                    name_clone,
                    server.listen_addr(),
                    engine_config.target,
                );

                tasks.spawn(async move { server.run().await });
            }
        }
    }

    let ctrl_c = async {
        signal::ctrl_c().await.expect("无法监听 Ctrl+C 信号");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("无法监听 SIGTERM 信号")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("收到 Ctrl+C 信号，正在关闭...");
        }
        _ = terminate => {
            info!("收到 SIGTERM 信号，正在关闭...");
        }
    }

    tasks.shutdown().await;
    info!("所有代理服务已关闭");

    Ok(())
}

fn load_config(args: &MystiArg) -> Result<MystiConfig> {
    if let Some(config_path) = &args.config {
        info!("从配置文件加载: {}", config_path);
        return MystiConfig::from_yaml_file(config_path);
    }

    if let (Some(target), Some(listen)) = (&args.target, &args.listen) {
        info!("使用命令行参数创建配置: {} -> {}", listen, target);

        let engine_config = EngineConfig {
            listen: listen.clone(),
            target: target.clone(),
            proxy_type: ProxyType::Tcp,
            request_timeout: None,
            connection_timeout: None,
            header: None,
            locations: None,
        };

        let mut engine_map = HashMap::new();
        engine_map.insert("default".to_string(), engine_config);

        return Ok(MystiConfig {
            mysti: mystiproxy::config::Mysti { engine: engine_map },
            cert: vec![],
        });
    }

    Err(mystiproxy::MystiProxyError::Config(
        "请提供配置文件 (--config) 或指定监听地址和目标地址 (--listen 和 --target)".to_string(),
    ))
}
