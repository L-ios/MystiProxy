use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::io::Write;
use std::net::{SocketAddr, ToSocketAddrs};
use bytes::Bytes;

use http_body_util::{BodyExt, Empty, Full};
use hyper::body::{Body, Incoming};
use hyper::client::conn::http1::{Builder, SendRequest};
use hyper::Request;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, TcpStream, UnixStream};
use clap::Parser;
use futures::{AsyncRead, AsyncWrite, ready};
use crate::arg::TUds;


mod arg;
mod proxy;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut uds = TUds::parse();

    let addr = match uds.listen.clone() {
        None => {"127.0.0.1:3000".to_string()}
        Some(addr) => {addr}
    };

    match TcpListener::bind(addr).await {
        Ok(listener) => {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let io = TokioIo::new(stream);
                        tokio::task::spawn(async move {
                            // Finally, we bind the incoming connection to our `hello` service
                            if let Err(err) = http1::Builder::new()
                                // `service_fn` converts our function in a `Service`
                                .serve_connection(io, service_fn(|mut req: Request<Incoming>| async move {
                                    let mut r_builder = Request::builder()
                                        .method(req.method())
                                        .uri(req.uri());

                                    for (k, v) in req.headers().iter() {
                                        r_builder = match k {
                                            &hyper::header::HOST => r_builder.header(hyper::header::HOST, "localhost"),
                                            _ => r_builder.header(k, v),
                                        };
                                    }

                                    let request = r_builder.body(req.into_body()).unwrap();
                                    let mut sender = get_target().await.unwrap();
                                    sender.send_request(request).await
                                }))
                                .await
                            {
                                eprintln!("Error serving connection: {:?}", err);
                            }
                        });

                    }
                    Err(e) => {}
                }
            }
        }
        Err(e) => {
            Ok(())
        }
    }
}

async fn get_target_stream() -> io::Result<UnixStream>  {
    let sock_file: Option<&'static str> = option_env!("TARGET_SOCKET");
    UnixStream::connect(sock_file.unwrap_or("/var/run/docker.sock")).await
}

async fn get_target() -> Result<SendRequest<Incoming>, Box<dyn std::error::Error + Send + Sync>> {
    let stream = get_target_stream().await?;
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
