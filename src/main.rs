#[cfg(test)]
#[macro_use]
extern crate test_case;
extern crate core;

use bytes::Bytes;
use chrono::Utc;
use std::convert::Infallible;
use std::fs::File;
use std::io as stdio;
use std::io::{BufReader, Stderr, Write};
use std::net::ToSocketAddrs;
use std::process::exit;
use std::sync::Arc;
use std::{result, thread};

use clap::Parser;
use env_logger::Env;
use futures::future::join_all;
use futures::FutureExt;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::client::conn::http1::{Builder, SendRequest};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{header, Request, Response};
use hyper_util::rt::TokioExecutor;
use hyper_util::{
    rt::TokioIo,
    server::{conn::auto::Builder as ServerAutoBuilder, graceful::GracefulShutdown},
};
use log::{error, info};
use tokio::runtime::Runtime;

use crate::arg::{CliArg, Config, Service};
use crate::gateway::UriMapping;
use crate::io::{SocketStream, StreamListener};

mod arg;
mod gateway;
mod io;
mod mocker;
mod proxy;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
        vec![Service {
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

    let mut handles = vec![];
    // fix 当前for循环存在问题
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .enable_io()
        .thread_name_fn(|| format!("odd-"))
        // .thread_name(format!("{}-", service.name))
        .build()
        .expect("failed to create tokio runtime");

    // let services = Box::new(services);
    for service in services {
        let handler = runtime.spawn({
            async move {
                match service.protocol.as_str() {
                    "http" => uds_http_proxy(Arc::new(service)).await,
                    _ => {
                        error!("protocol not support");
                        exit(1)
                        //Err("protocol not support".into())
                    }
                }
            }
        });
        handles.push(handler);
    } // 此处销毁runtime，会存在问题
    join_all(handles).await;

    Ok(())
}

async fn uds_http_proxy(service: Arc<Service>) -> Result<(), Box<dyn std::error::Error + Send>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_io()
        .thread_name(format!("{}-proxy-", service.name))
        .build()
        .expect("failed to create tokio runtime");

    let listener = StreamListener::new(service.listen.clone()).await.unwrap();
    info!("uds http proxy start: {}", service.listen);

    let graceful = GracefulShutdown::new();

    let request_handler = |mut req: Request<Incoming>| {
        let service_arc = service.clone();
        async move {
            let mut r_builder = Request::builder().method(req.method()).uri(req.uri());
            // uri mapping 查找

            for (k, v) in req.headers().iter() {
                r_builder = match k {
                    &hyper::header::HOST => r_builder.header(hyper::header::HOST, "localhost"),
                    _ => r_builder.header(k, v),
                };
            }

            let request = r_builder.body(req.into_body()).unwrap();
            let mut sender = get_target(service_arc.target.as_str()).await.unwrap();
            sender.send_request(request).await
        }
    };

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

                let conn = server.serve_connection_with_upgrades(TokioIo::new(Box::pin(stream)), service_fn(handler_request));

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

async fn handler_request(
    mut request: Request<Incoming>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible> {
    let response = Response::builder()
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Full::new(Bytes::from("Hello, world!\n")).boxed())
        .expect("values provided to the builder should be valid");

    Ok(response)
}

async fn get_target_stream(target: &str) -> stdio::Result<SocketStream> {
    SocketStream::connect(target.to_string()).await
}

async fn get_target(
    target: &str,
) -> Result<SendRequest<Incoming>, Box<dyn std::error::Error + Send + Sync>> {
    let stream = get_target_stream(target).await?;
    let io = TokioIo::new(stream);

    // Create the Hyper client
    let (sender, conn) = Builder::new()
        .preserve_header_case(true)
        .title_case_headers(true)
        .handshake(io)
        .await?;

    // Spawn a task to poll the connection, driving the HTTP state
    tokio::task::spawn(async move {
        if let Err(err) = conn.await {
            println!("Connection failed: {:?}", err);
        }
    });
    return Ok(sender);
}

#[cfg(test)]
mod tests {}
