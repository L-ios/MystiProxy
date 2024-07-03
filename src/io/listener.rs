use core::net;
use std::fmt::{Display, Formatter};
use std::io;

use tokio::net::{TcpListener, TcpSocket, UnixListener, UnixSocket};
use crate::io::stream::SocketStream;

pub enum StreamListener {
    TCP(TcpListener),
    UDS(UnixListener),
}

impl StreamListener {
    pub async fn new(listen: String) -> io::Result<Self> {
        if listen.starts_with("tcp://") {
            let listen = listen.replace("tcp://", "");
            let listener = TcpListener::bind(listen).await?;
            Ok(Self::TCP(listener))
        } else if listen.starts_with("uds://") {
            let listen = listen.replace("uds://", "");
            let listener = UnixListener::bind(listen)?;
            Ok(Self::UDS(listener))
        } else {
            todo!("Invalid listen")
        }
    }

    pub async fn accept(&self) -> io::Result<(SocketStream, SocketAddr)> {
        match self {
            Self::TCP(listener) => {
                let (stream, _addr) = listener.accept().await?;
                Ok((SocketStream::Tcp(stream), SocketAddr::Tcp(_addr)))
            },
            Self::UDS(listener) => {
                let (stream, _addr) = listener.accept().await?;
                Ok((SocketStream::Uds(stream), SocketAddr::Uds(_addr)))
            },
        }
    }

}



pub enum SocketAddr {
    Tcp(net::SocketAddr),
    Uds(tokio::net::unix::SocketAddr),
}

impl Display for SocketAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

pub enum Socket {
    Tcp(TcpSocket),
    Uds(UnixSocket),
}