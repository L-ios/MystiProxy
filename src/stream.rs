use std::error::Error;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpStream, UnixStream};

enum NetworkStream {
    Tcp(TcpStream),
    Unix(UnixStream),
}


impl  NetworkStream {
    async fn try_from(addr: String) -> Result<Self, Box<dyn Error>> {
        if ! addr.contains("://") {
            todo!("invalid url")
        }

        let protocol = addr.split("://").nth(0).unwrap();
        let addr = addr.split("://").nth(1).unwrap();
        match protocol {
            "tcp" => {
                TcpStream::connect(addr)
                    .await
                    .map(NetworkStream::Tcp)
                    .map_err(Into::into)
            },
            "unix" => {
                UnixStream::connect(addr)
                    .await
                    .map(NetworkStream::Unix)
                    .map_err(Into::into)
            },
            _ => todo!("not for support {}", protocol)
        }
    }

}

impl AsyncRead for NetworkStream {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        match *self {
            NetworkStream::Tcp(ref mut stream) => Pin::new(stream).poll_read(cx, buf),
            NetworkStream::Unix(ref mut stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for NetworkStream {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, io::Error>> {
        match *self {
            NetworkStream::Tcp(ref mut stream) => Pin::new(stream).poll_write(cx, buf),
            NetworkStream::Unix(ref mut stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match *self {
            NetworkStream::Tcp(ref mut stream) => Pin::new(stream).poll_flush(cx),
            NetworkStream::Unix(ref mut stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match *self {
            NetworkStream::Tcp(ref mut stream) => Pin::new(stream).poll_shutdown(cx),
            NetworkStream::Unix(ref mut stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}

impl Unpin for NetworkStream {}

