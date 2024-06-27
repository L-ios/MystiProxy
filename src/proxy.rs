use std::collections::HashMap;
use std::io;

use futures::FutureExt;
use tokio::io::{AsyncRead, AsyncWrite, copy_bidirectional};

pub trait Proxy {
    async fn proxy<A, B>(&self, a: &mut A, b: &mut B) -> io::Result<(u64, u64)>
    where
        A: AsyncRead + AsyncWrite + Unpin + ?Sized,
        B: AsyncRead + AsyncWrite + Unpin + ?Sized;
}

enum Protocol {
    TCP,
    Http,
}

pub struct Tunnel {
    pub local: String,
    pub target: String,
    pub protocol: Protocol,
    pub rewrite_header: Box<HashMap<String, String>>,
}

impl Tunnel {
    pub fn new(local: String, target: String, protocol: Protocol, rewrite_header: Box<HashMap<String, String>>) -> Self {
        Tunnel {
            local: local,
            target: target,
            protocol: protocol,
            rewrite_header: rewrite_header,
        }
    }
}

impl Proxy for Tunnel {
    async fn proxy<A, B>(&self, inbound: &mut A, outbound: &mut B) -> io::Result<(u64, u64)>
    where
        A: AsyncRead + AsyncWrite + Unpin + ?Sized,
        B: AsyncRead + AsyncWrite + Unpin + ?Sized,
    {
        copy_bidirectional(inbound, outbound)
            .map(|r| {
                if let Err(e) = r {
                    println!("Failed to transfer; error={}", e);
                    return Err(e);
                }
                r
            }).await
    }
}