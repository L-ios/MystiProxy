use mystiproxy::config::{EngineConfig, MystiConfig};
use mystiproxy::proxy::ProxyServer;
use mystiproxy::{set_engine_name, thread_identity, Result};
use clap::Parser;
use std::collections::HashMap;
use tokio::signal;
use tokio::task::JoinSet;
use tracing::{error, info, warn};
use tracing_subscriber::{
    EnvFilter,
    fmt::{
        format::Writer,
        FmtContext,
        FormatEvent,
        FormatFields,
    },
    registry::Registry,
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
    // 解析命令行参数（在日志初始化之前，这样帮助信息不会被日志干扰）
    let args = MystiArg::parse();

    // 初始化日志 - 使用 fmt::SubscriberBuilder 来正确配置自定义格式
    let filter = EnvFilter::new(
        std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
    );
    
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .event_format(CustomFormatter)
        .init();

    tracing::info!("MystiProxy 启动中...");

    // 加载配置
    let config = load_config(&args)?;

    // 启动代理服务器
    let engines = config.mysti.engine;
    if engines.is_empty() {
        warn!("没有配置任何代理引擎");
        return Ok(());
    }

    info!("共配置 {} 个代理引擎", engines.len());

    // 创建任务集合来管理所有代理服务
    let mut tasks: JoinSet<Result<()>> = JoinSet::new();

    // 启动所有代理引擎
    for (name, engine_config) in engines {
        let name_clone = name.clone();
        match ProxyServer::from_engine_config(&engine_config) {
            Ok(mut server) => {
                // 先启动服务器（绑定端口）
                if let Err(e) = server.start().await {
                    error!("代理引擎 '{}' 启动失败: {}", name_clone, e);
                    continue;
                }

                info!(
                    "代理引擎 '{}' 已启动: {} -> {}",
                    name_clone,
                    server.listen_addr(),
                    server.target_addr(),
                );

                // 将服务器运行任务添加到任务集合
                let engine_name = name_clone.clone();
                tasks.spawn(async move {
                    // 设置引擎名称到线程上下文
                    set_engine_name(&engine_name);
                    server.run().await
                });
            }
            Err(e) => {
                error!("创建代理引擎 '{}' 失败: {}", name_clone, e);
            }
        }
    }

    // 等待关闭信号
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("无法监听 Ctrl+C 信号");
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

    // 关闭所有任务
    tasks.shutdown().await;
    info!("所有代理服务已关闭");

    Ok(())
}

/// 加载配置
///
/// 优先级：
/// 1. 如果指定了 --config，从配置文件加载
/// 2. 如果指定了 --target 和 --listen，使用命令行参数创建配置
/// 3. 否则返回错误
fn load_config(args: &MystiArg) -> Result<MystiConfig> {
    // 如果指定了配置文件，从文件加载
    if let Some(config_path) = &args.config {
        info!("从配置文件加载: {}", config_path);
        return MystiConfig::from_yaml_file(config_path);
    }

    // 如果指定了 target 和 listen，使用命令行参数创建配置
    if let (Some(target), Some(listen)) = (&args.target, &args.listen) {
        info!("使用命令行参数创建配置: {} -> {}", listen, target);

        let engine_config = EngineConfig {
            listen: listen.clone(),
            target: target.clone(),
            timeout: None,
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

    // 没有提供任何配置
    Err(mystiproxy::MystiProxyError::Config(
        "请提供配置文件 (--config) 或指定监听地址和目标地址 (--listen 和 --target)".to_string(),
    ))
}
