use core::net;
use std::fmt::{Display, Formatter};
use std::io;

use tokio::net::TcpListener;

#[cfg(unix)]
use tokio::net::{unix, UnixListener};

use crate::io::stream::SocketStream;

pub enum StreamListener {
    TCP(TcpListener),
    #[cfg(unix)]
    UDS(UnixListener),
}

impl StreamListener {
    pub async fn new(listen: String) -> io::Result<Self> {
        if listen.starts_with("tcp://") {
            let listen = listen.replace("tcp://", "");
            let listener = TcpListener::bind(listen).await?;
            Ok(Self::TCP(listener))
        } else if listen.starts_with("unix://") {
            #[cfg(unix)]
            {
                let listen = listen.replace("unix://", "");
                let listener = UnixListener::bind(listen)?;
                Ok(Self::UDS(listener))
            }
            #[cfg(not(unix))]
            {
                Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Unix Domain Sockets are not supported on this platform",
                ))
            }
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid listen address format",
            ))
        }
    }

    pub async fn accept(&self) -> io::Result<(SocketStream, SocketAddr)> {
        match self {
            Self::TCP(listener) => {
                let (stream, _addr) = listener.accept().await?;
                Ok((SocketStream::Tcp(stream), SocketAddr::Tcp(_addr)))
            }
            #[cfg(unix)]
            Self::UDS(listener) => {
                let (stream, _addr) = listener.accept().await?;
                Ok((SocketStream::Uds(stream), SocketAddr::Uds(_addr)))
            }
        }
    }
}

pub enum SocketAddr {
    Tcp(net::SocketAddr),
    #[cfg(unix)]
    Uds(unix::SocketAddr),
}

impl Display for SocketAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tcp(addr) => write!(f, "tcp://{addr}"),
            #[cfg(unix)]
            Self::Uds(addr) => write!(f, "unix://{addr:?}"),
        }
    }
}
