use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpStream, UnixStream};

pub enum SocketStream {
    Tcp(TcpStream),
    Uds(UnixStream),
}

impl SocketStream {
    pub async fn connect(addr: String) -> io::Result<Self> {
        if !addr.contains("://") {
            todo!("invalid url")
        }

        let protocol = addr.split("://").nth(0).unwrap();
        let addr = addr.split("://").nth(1).unwrap();
        match protocol {
            "tcp" => TcpStream::connect(addr).await.map(Self::Tcp),
            "unix" => UnixStream::connect(addr).await.map(Self::Uds),
            _ => todo!("not for support {}", protocol),
        }
    }
}

impl AsyncRead for SocketStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match *self {
            Self::Tcp(ref mut stream) => Pin::new(stream).poll_read(cx, buf),
            Self::Uds(ref mut stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for SocketStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        match *self {
            Self::Tcp(ref mut stream) => Pin::new(stream).poll_write(cx, buf),
            Self::Uds(ref mut stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match *self {
            Self::Tcp(ref mut stream) => Pin::new(stream).poll_flush(cx),
            Self::Uds(ref mut stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        match *self {
            Self::Tcp(ref mut stream) => Pin::new(stream).poll_shutdown(cx),
            Self::Uds(ref mut stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}

impl Unpin for SocketStream {}
