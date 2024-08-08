#[cfg(test)]
#[macro_use]
extern crate test_case;
extern crate core;

use chrono::Utc;
use std::fs::File;
use std::io::{BufReader, Write};
use std::process::exit;
use std::sync::Arc;
use std::thread;

use clap::Parser;
use env_logger::Env;
use futures::future::join_all;
use hyper_util::rt::TokioExecutor;
use hyper_util::{
    rt::TokioIo,
    server::{conn::auto::Builder as ServerAutoBuilder, graceful::GracefulShutdown},
};
use tokio::runtime::{Builder as RuntimeBuilder};
use log::{error, info};
use tokio::task::JoinHandle;
use crate::arg::{CliArg, Config, MystiEngine};
use crate::engine::Engine;
use crate::io::StreamListener;

mod arg;
mod engine;
mod gateway;
mod io;
mod mocker;
mod proxy;
mod tls;
mod utils;
mod k8s;

type MainError = Box<dyn std::error::Error + Send + Sync>;

#[tokio::main]
async fn main() -> Result<(), MainError> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format(|buf, record| {
            writeln!(
                buf,
                "{} {} [{}] {}:{} {}",
                format!("{}", Utc::now().format("%Y-%m-%d %H:%M:%S")),
                record.level(),
                thread::current().name().unwrap_or("main"), //统一长度
                record.file().unwrap_or(""),
                record.line().unwrap_or(0),
                record.args()
            )
        })
        .init();
    let cli_arg = CliArg::parse();

    info!("start");

    let services = if cli_arg.config.is_some() {
        let config_path = cli_arg.config.unwrap();
        let config_reader = match File::open(config_path) {
            Ok(file) => Ok(BufReader::new(file)),
            Err(err) => Err(err),
        }
        .unwrap();

        let config: Config = serde_yaml::from_reader(config_reader)?;

        config.service
    } else {
        vec![MystiEngine {
            name: "default".to_string(),
            listen: match cli_arg.listen.clone() {
                None => "127.0.0.1:3000".to_string(),
                Some(addr) => addr,
            },
            target: match cli_arg.target.clone() {
                None => {
                    error!("target is none");
                    exit(1)
                }
                Some(target) => target.to_string(),
            },
            protocol: match cli_arg.protocol.clone() {
                None => "http".to_string(),
                Some(protocol) => protocol,
            },
            timeout: None,
            header: None,
            uri_mapping: None,
        }]
    };

    // fix 当前for循环存在问题
    let runtime = RuntimeBuilder::new_multi_thread()
        .enable_all()
        .thread_name_fn(|| format!("odd-"))
        .build()
        .expect("failed to create tokio runtime");

    let handles = services.into_iter().map(|service| {
        runtime.spawn({
            async move {
                let _ = match service.protocol.as_str() {
                    "http" => uds_http_proxy(Arc::new(service)).await,
                    _ => {
                        error!("protocol not support");
                        exit(1)
                        //Err("protocol not support".into())
                    }
                };
            }
        })
    }).collect::<Vec<JoinHandle<()>>>();

    join_all(handles).await;

    Ok(())
}

async fn uds_http_proxy(
    service: Arc<MystiEngine>,
) -> Result<(), Box<dyn std::error::Error + Send>> {
    let runtime = RuntimeBuilder::new_multi_thread()
        .worker_threads(4)
        .enable_io()
        .thread_name(format!("{}-proxy-", service.name))
        .build()
        .expect("failed to create tokio runtime");

    let listener = StreamListener::new(service.listen.clone()).await.unwrap();
    info!("uds http proxy start: {}", service.listen);

    let graceful = GracefulShutdown::new();
    let engine = Engine::new(service.clone());
    let server = ServerAutoBuilder::new(TokioExecutor::new());
    loop {
        tokio::select! {
            conn = listener.accept() => {
                let (stream, con) = match conn {
                    Ok(x) => x,
                    Err(e) => {
                        error!("failed to adccept connection: {e}");
                        continue
                    }
                };
                info!("connect from {}", con);
                let e_clone = engine.clone();
                let conn = server.serve_connection_with_upgrades(TokioIo::new(Box::pin(stream)), e_clone);
                let conn = graceful.watch(conn.into_owned());
                runtime.spawn(async move {
                    if let Err(err) = conn.await {
                        eprintln!("connection error: {}", err);
                    }
                    eprintln!("connection dropped: {}", con);
                });
            }
        }
    }

    tokio::select! {
        _ = graceful.shutdown() => {
            info!("shutdown");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {}
