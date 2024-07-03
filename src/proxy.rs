use std::collections::HashMap;
use std::io;

use futures::FutureExt;
use tokio::io::{AsyncRead, AsyncWrite, copy_bidirectional};
use crate::io::{SocketStream, StreamListener};

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
    pub local: StreamListener,
    pub target: SocketStream,
    pub protocol: Protocol,
}

impl Tunnel {
    pub fn new(local: String, target: String, protocol: Protocol) -> Self {
        todo!("Tunnel::new")
    }

    pub fn listen() {
        todo!("Tunnel::listen")
    }

    pub fn accept() {
        todo!("Tunnel::accept")
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