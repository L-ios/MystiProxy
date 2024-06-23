#![warn(rust_2018_idioms)]

use std::collections::HashMap;
use std::error::Error;
use std::io;

use futures::FutureExt;
use tokio::io::{AsyncRead, AsyncWrite, copy_bidirectional};
use tokio::net::{TcpListener, TcpStream};

pub trait Proxy {
    async fn proxy<A, B>(&self, a: &mut A, b: &mut B) -> io::Result<(u64, u64)>
        where
            A: AsyncRead + AsyncWrite + Unpin + ?Sized,
            B: AsyncRead + AsyncWrite + Unpin + ?Sized;
}

enum Protocol {
    TCP,
    Http
}

pub struct Tunnel {
    pub local: String,
    pub target: String,
    pub protocol: Protocol,
    pub rewrite_header: Box<HashMap<String, String>>
}

impl Tunnel {
    pub fn new(local: String, target: String, protocol: Protocol, rewrite_header: Box<HashMap<String, String>>) -> Self {
        Tunnel {
            local: local,
            target: target,
            protocol: protocol,
            rewrite_header: rewrite_header
        }
    }

    pub fn proxys(&self) {
        match self.protocol {
            Protocol::TCP => {}
            Protocol::Http => {}
        }
    }

    async fn uds_proxy(&self, listen_addr: String, server_addr: String) {
        todo!()
        // let listener = TcpListener::bind(listen_addr).await?;
    }

    async fn tcp_proxy(&self, listen_addr: String, server_addr: String) -> Result<(), Box<dyn Error>>  {

        let listener = TcpListener::bind(listen_addr).await?;
        while let Ok((mut inbound, _)) = listener.accept().await {
            let mut outbound = TcpStream::connect(server_addr.clone()).await?;
            tokio::spawn(async move {
                copy_bidirectional(&mut inbound, &mut outbound)
                    .map(|r| {
                        if let Err(e) = r {
                            println!("Failed to transfer; error={}", e);
                        }
                    })
                    .await
            });
        }
        Ok(())
    }
}

impl Proxy for Tunnel {
    async fn proxy<A, B>(&self, inbound: &mut A, outbound: &mut B) -> io::Result<(u64, u64)>
        where A: AsyncRead + AsyncWrite + Unpin + ?Sized,
              B: AsyncRead + AsyncWrite + Unpin + ?Sized
    {
            copy_bidirectional(inbound, outbound)
                .map(|r| {
                    if let Err(e) = r {
                        println!("Failed to transfer; error={}", e);
                        return Err(e);
                    }
                    return r;
                }).await
    }
}