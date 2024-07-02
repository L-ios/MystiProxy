#[cfg(test)]
#[macro_use]
extern crate test_case;
extern crate core;

use std::fs::File;
use std::thread;
use std::io as stdio;
use std::io::{BufReader, Write};
use std::net::ToSocketAddrs;
use std::process::exit;
use std::sync::Arc;

use clap::Parser;
use env_logger::Env;
use futures::{AsyncRead, AsyncWrite, FutureExt};
use futures::future::join_all;
use http_body_util::BodyExt;
use hyper::body::{Body, Incoming};
use hyper::client::conn::http1::{Builder, SendRequest};
use hyper::{Request, service};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use log::{error, info};
use tokio::net::{TcpListener, UnixStream};
use tokio::runtime::Runtime;

use crate::arg::{Config, Service, TUds};
use crate::gateway::UriMapping;

mod arg;
mod proxy;
mod gateway;
mod io;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format(|buf, record| {
            writeln!(buf, "[{}]- {}", thread::current().name().unwrap_or("main"), record.args())
        }).init();
    let uds = TUds::parse();

    info!("start");

    let (services, uri_mapping) = if uds.config.is_some() {
        let config_path = uds.config.unwrap();
        let config_reader = match File::open(config_path) {
            Ok(file) => {Ok(BufReader::new(file))},
            Err(err) => Err(err)
        }.unwrap();

        let config: Config = serde_yaml::from_reader(config_reader)?;

        let uri_mapping: Vec<UriMapping> = if config.uri_mapping.is_some() {
            let uri_mapping = &config.uri_mapping.clone().unwrap();
            let uri_mapping = match File::open(uri_mapping) {
                Ok(file) => {Ok(BufReader::new(file))},
                Err(_) => Err(format!("not found uri_mapping file: {}", uri_mapping))
            }.unwrap();
            serde_json::from_reader(uri_mapping)?
        } else {
            vec![]
        };

        (config.service, uri_mapping)
    } else {
        (vec![Service {
            name: "default".to_string(),
            listen: match uds.listen.clone() {
                None => { "127.0.0.1:3000".to_string() }
                Some(addr) => { addr }
            },
            target: match uds.target.clone() {
                None => { "/var/run/docker.sock".to_string() }
                Some(target) => { target.to_string() }
            },
            protocol: match uds.protocol.clone() {
                None => { "http".to_string() }
                Some(protocol) => { protocol }
            },
            timeout: None,
            http_header: None,
        }],
        vec![])
    };

    let mut runtimes= vec![];
    // fix 当前for循环存在问题
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .enable_io()
        .thread_name_fn(|| {
            format!("odd-")
        })
        // .thread_name(format!("{}-", service.name))
        .build()
        .expect("failed to create tokio runtime");

    // let services = Box::new(services);
    for service in services {
        let handler = runtime.spawn({
            async move {
                match service.protocol.as_str() {
                    "http" => uds_http_proxy(Arc::new(service)).await,
                    // "tcp" => uds_tcp_proxy(addr, target).await,
                    _ => {
                        println!("protocol not support");
                        Err("protocol not support".into())
                    }
                }
            }
        });
        runtimes.push(handler);
    } // 此处销毁runtime，会存在问题
    join_all(runtimes).await;

    Ok(())
}

async fn uds_http_proxy(service: Arc<Service>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_io()
        .thread_name(format!("{}-proxy-", service.name))
        .build()
        .expect("failed to create tokio runtime");

    let listener = TcpListener::bind(service.listen.as_str()).await.unwrap();
    info!("uds http proxy start: {}", service.listen);
    while let Ok((stream, con)) = listener.accept().await {
        // 加入到队列中，由统一线程处理
        runtime.spawn({
            let service_arc = service.clone();
            async move {
                let io = TokioIo::new(stream);

                // Finally, we bind the incoming connection to our `hello` service
                if let Err(err) = http1::Builder::new()
                    // `service_fn` converts our function in a `Service`
                    .serve_connection(io, service_fn(|mut req: Request<Incoming>| {
                        let service_arc = service_arc.clone();
                        async move {
                            let mut r_builder = Request::builder()
                                .method(req.method())
                                .uri(req.uri());
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
                    }))
                    .await
                {
                    eprintln!("Error serving connection: {:?}", err);
                }
            }
        });
    }

    Ok(())
}

async fn get_target_stream(target: &str) -> stdio::Result<UnixStream> {
    // let sock_file: Option<&'static str> = option_env!("TARGET_SOCKET");
    UnixStream::connect(target).await
}

async fn get_target(target: &str) -> Result<SendRequest<Incoming>, Box<dyn std::error::Error + Send + Sync>> {
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
