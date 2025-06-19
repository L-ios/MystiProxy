#[cfg(test)]
#[macro_use]
extern crate test_case;
extern crate core;

use chrono::Utc;
use std::fs::File;
use std::io::{BufReader, Write};
use std::process::exit;
use std::sync::{Arc, Mutex};
use std::{env, fs, thread};

use crate::arg::{Config, MystiArg, MystiEngine};
use crate::engine::Engine;
use crate::io::{SocketStream, StreamListener};
use clap::Parser;
use env_logger::Env;
use futures::future::join_all;
use hyper_util::rt::TokioExecutor;
use hyper_util::{
    rt::TokioIo,
    server::{conn::auto::Builder as ServerAutoBuilder, graceful::GracefulShutdown},
};
use log::{error, info};
use tokio::io::copy_bidirectional;
use tokio::runtime::Builder as RuntimeBuilder;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use futures::{FutureExt, SinkExt};

use kube::runtime::watcher;
use notify::{Config as nConfig, Error, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;

mod arg;
mod engine;
mod ex_proxy;
mod gateway;
mod io;
mod k8s;
mod mocker;
mod proxy;
mod tls;
mod utils;

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
    let cli_arg = MystiArg::parse();

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
                None => "tcp://0.0.0.0:3000".to_string(),
                Some(addr) => addr,
            },
            target: match cli_arg.target.clone() {
                None => {
                    error!("target is none");
                    println!("ca.crt: {}", fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/ca.crt").expect("无法读取ca.crt"));
                    println!("token: {}", fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/token").expect("无法读取token"));
                    env::var("KUBERNETES_PORT").unwrap()
                }
                Some(target) => target.to_string(),
            },
            // protocol: match cli_arg.protocol.clone() {
            //     None => "tcp".to_string(),
            //     Some(protocol) => protocol,
            // },
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

    let handles = services
        .into_iter()
        .map(|service| {
            runtime.spawn({
                async move {
                    stream_proxy(Arc::new(service)).await;
                    // let _ = match service.protocol.as_str() {
                    //     "http" | "https" => uds_http_proxy(Arc::new(service)).await,
                    //     "tcp" =>
                    //     _ => {
                    //         error!("protocol not support");
                    //         exit(1)
                    //         //Err("protocol not support".into())
                    //     }
                    // };
                }
            })
        })
        .collect::<Vec<JoinHandle<()>>>();

    join_all(handles).await;

    Ok(())
}

async fn stream_proxy(service: Arc<MystiEngine>) -> Result<(), Box<dyn std::error::Error + Send>> {
    let runtime = RuntimeBuilder::new_multi_thread()
        .worker_threads(4)
        .enable_io()
        .thread_name(format!("{}-proxy-", service.name))
        .build()
        .expect("failed to create tokio runtime");

    let listener = StreamListener::new(service.listen.clone()).await.unwrap();
    info!("stream proxy start: {}", service.listen);

    let graceful = GracefulShutdown::new();
    let engine = Engine::new(service.clone());

    loop {
        tokio::select! {
            conn = listener.accept() => {
                let (mut inbound, con) = match conn {
                    Ok(x) => x,
                    Err(e) => {
                        error!("failed to adccept connection: {e}");
                        continue
                    }
                };

                info!("connect from {}", con);
                // runtime.spawn(async move {
                //     if let Err(err) = conn.await {
                //         eprintln!("connection error: {}", err);
                //     }
                //     eprintln!("connection dropped: {}", con);
                // });

                let mut outbound = SocketStream::connect(service.target.to_string()).await.expect("failed to connect");

                copy_bidirectional(&mut inbound, &mut outbound)
                .map(|r| {
                    if let Err(e) = r {
                        println!("Failed to transfer; error={}", e);
                    }
                })
                .await
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
                        error!("failed to accept connection: {e}");
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
