//! TCP Listener 包装模块
//!
//! 提供统一的 TCP 监听器功能

use std::net::SocketAddr;

use tokio::net::TcpListener;

use crate::error::Result;

/// TCP 监听器包装
pub struct TcpProxyListener {
    /// 底层 TcpListener
    listener: TcpListener,
    /// 监听地址
    addr: SocketAddr,
}

impl TcpProxyListener {
    /// 绑定到指定地址创建新的监听器
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use mystiproxy::proxy::TcpProxyListener;
    ///
    /// #[tokio::main]
    /// async fn main() -> mystiproxy::Result<()> {
    ///     let listener = TcpProxyListener::bind("0.0.0.0:3128").await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn bind(addr: &str) -> Result<Self> {
        let socket_addr: SocketAddr = addr.parse()?;
        let listener = TcpListener::bind(socket_addr).await?;
        let local_addr = listener.local_addr()?;

        Ok(Self {
            listener,
            addr: local_addr,
        })
    }

    /// 从 SocketAddr 创建监听器
    pub async fn bind_addr(addr: SocketAddr) -> Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        let local_addr = listener.local_addr()?;

        Ok(Self {
            listener,
            addr: local_addr,
        })
    }

    /// 接受新的连接
    pub async fn accept(&self) -> Result<(tokio::net::TcpStream, SocketAddr)> {
        let (stream, addr) = self.listener.accept().await?;
        Ok((stream, addr))
    }

    /// 获取监听地址
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// 获取底层 TcpListener 的引用
    pub fn inner(&self) -> &TcpListener {
        &self.listener
    }
}

impl std::fmt::Debug for TcpProxyListener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TcpProxyListener")
            .field("addr", &self.addr)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tcp_listener_bind() {
        let listener = TcpProxyListener::bind("127.0.0.1:0").await;
        assert!(listener.is_ok());

        let listener = listener.unwrap();
        let addr = listener.local_addr();
        assert_eq!(addr.ip().to_string(), "127.0.0.1");
    }
}
