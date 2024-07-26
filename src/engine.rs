use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{body::Incoming as IncomingBody, header, Request, Response};
use hyper::service::Service;

#[derive(Debug, Clone)]
pub struct Engine {
    counter: Arc<String>,
}

impl Service<Request<IncomingBody>> for Engine {
    type Response = Response<Full<Bytes>>;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<IncomingBody>) -> Self::Future {
        let counter = self.counter.clone();
        Box::pin(async move {
            let rb = req.collect().await.unwrap().to_bytes();
            let good = counter.as_str();
            let response = Response::builder()
                .header(header::CONTENT_TYPE, "text/plain")
                .header("count", good)
                .body(Full::new(rb))
                .expect("values provided to the builder should be valid");

            Ok(response)
        })
    }
}
