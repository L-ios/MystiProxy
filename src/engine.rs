use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use http_body_util::combinators::BoxBody;
use hyper::{body::Incoming as IncomingBody, header, Request, Response};
use hyper::client::conn::http2::{Builder, SendRequest};
use hyper::service::Service;
use hyper_util::rt::TokioExecutor;
use hyper_util::rt::TokioIo;

use crate::arg::MystiEngine;
use crate::io::SocketStream;

#[derive(Debug, Clone)]
pub struct Engine {
    engine: Arc<MystiEngine>,
}

impl Engine {
    pub fn new(engine: Arc<MystiEngine>) -> Self {
        Engine { engine }
    }
}

impl Service<Request<IncomingBody>> for Engine {
    type Response = Response<Full<Bytes>>;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<IncomingBody>) -> Self::Future {
        Box::pin(async move {
            let rb = req.collect().await.unwrap().to_bytes();
            let response = Response::builder()
                .header(header::CONTENT_TYPE, "text/plain")
                .body(Full::new(rb))
                .expect("values provided to the builder should be valid");

            Ok(response)
        })
    }
}

async fn a(service: Arc<MystiEngine>) {
    let request_handler = |mut req: Request<IncomingBody>| {
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
}

async fn handler_request(
    mut request: Request<IncomingBody>,
) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible> {
    let response = Response::builder()
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Full::new(Bytes::from("Hello, world!\n")).boxed())
        .expect("values provided to the builder should be valid");

    Ok(response)
}

async fn get_target_stream(target: &str) -> std::io::Result<SocketStream> {
    SocketStream::connect(target.to_string()).await
}

async fn get_target(
    target: &str,
) -> Result<SendRequest<IncomingBody>, Box<dyn std::error::Error + Send + Sync>> {
    let stream = get_target_stream(target).await?;
    let io = TokioIo::new(stream);

    // Create the Hyper client
    let (sender, conn) = Builder::new(TokioExecutor::new())
        // .preserve_header_case(true)
        // .title_case_headers(true)
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
