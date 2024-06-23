use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::io::Write;
use std::net::{SocketAddr, ToSocketAddrs};
use bytes::Bytes;

use http_body_util::{BodyExt, Empty, Full};
use hyper::body::{Body, Incoming};
use hyper::client::conn::http1::{Builder, SendRequest};
use hyper::{Request, Uri};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, TcpStream, UnixStream};
use clap::Parser;
use futures::{AsyncRead, AsyncWrite, ready};
use hyper_util::client::legacy::pool::Error;
use tokio::runtime;
use crate::arg::TUds;



mod arg;
mod proxy;
mod gateway;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

    let uds = TUds::parse();

    let addr = match uds.listen.clone() {
        None => {"127.0.0.1:3000".to_string()}
        Some(addr) => {
            println!("listen on : {}", addr);
            addr
        }
    };

    let target = match uds.target.clone() {
        None => {"/var/run/docker.sock".to_string()}
        Some(target) => {target.to_string()}
    };

    let listener = TcpListener::bind(addr).await.unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_io()
        .thread_name("sock-")
        .build()
        .expect("failed to create tokio runtime");
    loop {
        let (stream, _) = listener.accept().await.unwrap();
        runtime.spawn({
            let target = target.clone();
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

async fn get_target_stream(target: &str) -> io::Result<UnixStream>  {
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
#[macro_use]
extern crate test_case;
mod tests {
    use super::*;
}