use std::net::SocketAddr;
use std::sync::Arc;

use clap::Parser;
use mystiproxy::config::{EngineConfig, MystiConfig, ProxyType};
use mystiproxy::http::{create_handler, HttpProxyAcceptor, HttpProxyConfig, HttpServer, HttpServerConfig};
use mystiproxy::metrics::MetricsManager;
use mystiproxy::proxy::ProxyServer;
use mystiproxy::{set_engine_name, thread_identity, Result};
use std::collections::HashMap;
use tokio::signal;
use tokio::task::JoinSet;
use tracing::{error, info, warn};
use tracing_subscriber::{
    fmt::{format::Writer, FmtContext, FormatEvent, FormatFields},
    registry::Registry,
    EnvFilter,
};

mod arg;

use arg::MystiArg;

/// 自定义日志格式化器
struct CustomFormatter;

impl<N> FormatEvent<Registry, N> for CustomFormatter
where
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, Registry, N>,
        mut writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> std::fmt::Result {
        // 获取元数据
        let meta = event.metadata();

        // 获取级别
        let level = meta.level();

        // 获取时间
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");

        // 获取线程标识
        let thread_id = thread_identity();

        // 写入格式化的日志
        write!(
            writer,
            "{} {} [{}] {}: ",
            timestamp,
            level,
            thread_id,
            meta.target().split("::").next().unwrap_or("unknown")
        )?;

        // 格式化字段
        ctx.format_fields(writer.by_ref(), event)?;

        writeln!(writer)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = MystiArg::parse();

    // 初始化日志 - 使用 fmt::SubscriberBuilder 来正确配置自定义格式
    let filter = EnvFilter::new(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .event_format(CustomFormatter)
        .init();

    // 初始化监控指标
    let mut metrics_manager = MetricsManager::new();
    metrics_manager.init();

    // 启动指标导出服务器
    let metrics_addr: SocketAddr = "127.0.0.1:9090".parse().unwrap();
    metrics_manager.start_server(metrics_addr).await;

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

                    let engine_name = name_clone.clone();
                    tasks.spawn(async move {
                        set_engine_name(&engine_name);
                        server.run().await
                    });
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

                let mut server = if let Some(tls_config) = &engine_config.tls {
                    match HttpServer::new_with_tls(
                        HttpServerConfig::new(
                            engine_config.listen.clone(),
                            engine_config.request_timeout,
                        ),
                        handler,
                        tls_config,
                    ) {
                        Ok(s) => s,
                        Err(e) => {
                            error!("创建 HTTPS 服务器 '{}' 失败: {}", name_clone, e);
                            continue;
                        }
                    }
                } else {
                    HttpServer::new(
                        HttpServerConfig::new(
                            engine_config.listen.clone(),
                            engine_config.request_timeout,
                        ),
                        handler,
                        None,
                    )
                };

                if let Err(e) = server.start().await {
                    error!("HTTP 引擎 '{}' 启动失败: {}", name_clone, e);
                    continue;
                }

                info!(
                    "HTTP 引擎 '{}' 已启动: {} -> {} ({})",
                    name_clone,
                    server.listen_addr(),
                    engine_config.target,
                    if engine_config.tls.is_some() { "HTTPS" } else { "HTTP" }
                );

                let engine_name = name_clone.clone();
                tasks.spawn(async move {
                    set_engine_name(&engine_name);
                    server.run().await
                });
            }
            ProxyType::Forward => {
                let listen_addr = engine_config.listen.clone();
                let mut proxy_config = HttpProxyConfig::new();
                if let Some(timeout) = engine_config.request_timeout {
                    proxy_config = proxy_config.connect_timeout(timeout).request_timeout(timeout);
                }
                if let Some(ref upstream) = engine_config.upstream {
                    proxy_config = proxy_config.upstream_proxy(upstream);
                }

                let acceptor = HttpProxyAcceptor::new(proxy_config);
                let engine_name = name_clone.clone();
                tasks.spawn(async move {
                    set_engine_name(&engine_name);
                    let listener = match tokio::net::TcpListener::bind(&listen_addr).await {
                        Ok(l) => l,
                        Err(e) => {
                            error!("Forward proxy bind failed '{}': {}", engine_name, e);
                            return Err(e.into());
                        }
                    };
                    info!("Forward proxy '{}' listening on {}", engine_name, listen_addr);
                    loop {
                        match listener.accept().await {
                            Ok((stream, _addr)) => {
                                let acceptor = acceptor.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = acceptor.handle_connection(stream).await {
                                        warn!("Forward proxy connection error: {}", e);
                                    }
                                });
                            }
                            Err(e) => {
                                warn!("Forward proxy accept error: {}", e);
                            }
                        }
                    }
                });
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
            auth: None,
            tls: None,
            upstream: None,
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
