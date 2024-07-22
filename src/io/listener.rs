use core::net;
use std::fmt::{Display, Formatter};
use std::io;

use crate::io::stream::SocketStream;
use tokio::net::{unix, TcpListener, TcpSocket, UnixListener, UnixSocket};

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
        } else if listen.starts_with("unix://") {
            let listen = listen.replace("unix://", "");
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
            }
            Self::UDS(listener) => {
                let (stream, _addr) = listener.accept().await?;
                Ok((SocketStream::Uds(stream), SocketAddr::Uds(_addr)))
            }
        }
    }
}

pub enum SocketAddr {
    Tcp(net::SocketAddr),
    Uds(unix::SocketAddr),
}

impl Display for SocketAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tcp(addr) => write!(f, "tcp://{}", addr),
            Self::Uds(addr) => write!(f, "unix://{:?}", addr),
        }
    }
}

pub enum Socket {
    Tcp(TcpSocket),
    Uds(UnixSocket),
}
