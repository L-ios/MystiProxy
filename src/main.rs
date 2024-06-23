#[cfg(test)]
#[macro_use]
extern crate test_case;

use std::fs::File;
use std::future::Future;
use std::io;
use std::io::{BufReader, Write};
use std::net::ToSocketAddrs;

use clap::Parser;
use futures::{AsyncRead, AsyncWrite};
use http_body_util::BodyExt;
use hyper::body::{Body, Incoming};
use hyper::client::conn::http1::{Builder, SendRequest};
use hyper::Request;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, UnixStream};
use tokio::runtime;
use tokio::runtime::Runtime;

use crate::arg::{Config, TUds};
use crate::gateway::UriMapping;

mod arg;
mod proxy;
mod gateway;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let uds = TUds::parse();

    if uds.config.is_none() {
        let addr = match uds.listen.clone() {
            None => { "127.0.0.1:3000".to_string() }
            Some(addr) => { addr }
        };

        let target = match uds.target.clone() {
            None => { "/var/run/docker.sock".to_string() }
            Some(target) => { target.to_string() }
        };

        let protocol = match uds.protocol.clone() {
            None => { "http".to_string() }
            Some(protocol) => { protocol }
        };

        match protocol.as_str() {
            "http" => uds_http_proxy(addr, target).await,
            // "tcp" => uds_tcp_proxy(addr, target).await,
            _ => {
                println!("protocol not support");
                Err("protocol not support".into())
            }
        }
    } else {
        let config_path = uds.config.unwrap();
        let config_reader = match File::open(config_path) {
            Ok(file) => {Ok(BufReader::new(file))},
            Err(err) => Err(err)
        }.unwrap();

        let config: Config = serde_yaml::from_reader(config_reader)?;

        println!("config: {:?}", config);

        let uri_mapping: Vec<UriMapping> = if config.uri_mapping.is_some() {
            let uri_mapping = &config.uri_mapping.clone().unwrap();
            let uri_mapping = match File::open(uri_mapping) {
                Ok(file) => {Ok(BufReader::new(file))},
                Err(err) => Err(format!("not found uri_mapping file: {}", uri_mapping))
            }.unwrap();
            serde_json::from_reader(uri_mapping)?
        } else {
            vec![]
        };

        println!("uri_mapping: {:?}", uri_mapping);

        let mut runtimes: Vec<Runtime> = vec![];

        for service in &config.service {
            // fix 当前for循环存在问题
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .thread_name(format!("{}-", service.name))
                .build()
                .expect("failed to create tokio runtime");
            runtime.spawn({
                let target = service.target.clone();
                let listen_addr = service.listen.clone();
                async move {
                    uds_http_proxy(listen_addr, target);
                }
            });
            runtimes.push(runtime);
        }
        Ok(())

        // Err("config file not support".into())
    }
}

async fn uds_http_proxy(listen_addr: String, server_addr: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(listen_addr.clone()).await.unwrap();
    println!("listen on: {}", listen_addr);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_io()
        .thread_name("sock-")
        .build()
        .expect("failed to create tokio runtime");
    loop {
        let (stream, _) = listener.accept().await.unwrap();
        runtime.spawn({
            let target = server_addr.clone();
            async move {
                let io = TokioIo::new(stream);

                // Finally, we bind the incoming connection to our `hello` service
                if let Err(err) = http1::Builder::new()
                    // `service_fn` converts our function in a `Service`
                    .serve_connection(io, service_fn(|mut req: Request<Incoming>| {
                        let target = target.clone();
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
                            let mut sender = get_target(target.as_str()).await.unwrap();
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
}

async fn get_target_stream(target: &str) -> io::Result<UnixStream> {
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


mod tests {}
