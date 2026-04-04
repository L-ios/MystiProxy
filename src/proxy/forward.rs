//! 数据转发模块
//!
//! 提供双向数据转发功能，支持 TCP 到 TCP、TCP 到 UDS 的转发

use std::time::Duration;

use tokio::io::{self, AsyncRead, AsyncWrite};
use tokio::time::timeout;

use crate::error::{MystiProxyError, Result};
use crate::io::SocketStream;

/// 转发统计信息
#[derive(Debug, Clone, Default)]
pub struct TransferStats {
    /// 从客户端发送到目标的字节数
    pub client_to_target: u64,
    /// 从目标发送到客户端的字节数
    pub target_to_client: u64,
}

/// 双向转发结果
#[derive(Debug, Clone)]
pub struct ForwardResult {
    /// 传输统计
    pub stats: TransferStats,
}

/// 双向数据转发
///
/// 将客户端和目标之间的数据进行双向转发，直到任一端关闭连接
///
/// # Arguments
///
/// * `client` - 客户端连接（实现 AsyncRead + AsyncWrite）
/// * `target` - 目标连接（实现 AsyncRead + AsyncWrite）
///
/// # Returns
///
/// 返回转发统计信息
///
/// # Examples
///
/// ```ignore
/// use mystiproxy::proxy::forward_bidirectional;
///
/// let result = forward_bidirectional(client_stream, target_stream).await?;
/// println!("Transferred: {} bytes to target, {} bytes to client",
///     result.stats.client_to_target,
///     result.stats.target_to_client);
/// ```
pub async fn forward_bidirectional(
    client: impl AsyncRead + AsyncWrite,
    target: impl AsyncRead + AsyncWrite,
) -> Result<ForwardResult> {
    let (mut client_read, mut client_write) = io::split(client);
    let (mut target_read, mut target_write) = io::split(target);

    let client_to_target = io::copy(&mut client_read, &mut target_write);
    let target_to_client = io::copy(&mut target_read, &mut client_write);

    let (ct_result, tc_result) = tokio::try_join!(client_to_target, target_to_client)?;

    Ok(ForwardResult {
        stats: TransferStats {
            client_to_target: ct_result,
            target_to_client: tc_result,
        },
    })
}

/// 带超时的双向数据转发
///
/// 与 `forward_bidirectional` 类似，但支持超时控制
///
/// # Arguments
///
/// * `client` - 客户端连接
/// * `target` - 目标连接
/// * `timeout_duration` - 超时时间
///
/// # Returns
///
/// 如果超时则返回错误
pub async fn forward_bidirectional_with_timeout(
    client: impl AsyncRead + AsyncWrite,
    target: impl AsyncRead + AsyncWrite,
    timeout_duration: Duration,
) -> Result<ForwardResult> {
    let result = timeout(timeout_duration, forward_bidirectional(client, target)).await?;

    result
}

/// 连接到目标地址
///
/// 根据地址格式连接到目标服务器
///
/// # Arguments
///
/// * `target_addr` - 目标地址，格式如 "tcp://127.0.0.1:8080" 或 "unix:///var/run/docker.sock"
///
/// # Returns
///
/// 返回连接成功的 SocketStream
pub async fn connect_to_target(target_addr: &str) -> Result<SocketStream> {
    SocketStream::connect(target_addr.to_string())
        .await
        .map_err(MystiProxyError::Io)
}

/// TCP 到 TCP 的转发
///
/// 接受一个 TCP 连接，连接到目标 TCP 地址，并进行双向数据转发
///
/// # Arguments
///
/// * `client` - 客户端 TCP 连接
/// * `target_addr` - 目标 TCP 地址（如 "tcp://127.0.0.1:8080"）
pub async fn forward_tcp_to_tcp(
    client: tokio::net::TcpStream,
    target_addr: &str,
) -> Result<ForwardResult> {
    let target = connect_to_target(target_addr).await?;
    forward_bidirectional(client, target).await
}

/// TCP 到 UDS 的转发
///
/// 接受一个 TCP 连接，连接到目标 Unix Domain Socket，并进行双向数据转发
///
/// # Arguments
///
/// * `client` - 客户端 TCP 连接
/// * `uds_path` - 目标 UDS 路径（如 "unix:///var/run/docker.sock"）
#[cfg(unix)]
pub async fn forward_tcp_to_uds(
    client: tokio::net::TcpStream,
    uds_path: &str,
) -> Result<ForwardResult> {
    let target = connect_to_target(uds_path).await?;
    forward_bidirectional(client, target).await
}

/// 通用转发函数
///
/// 根据目标地址自动选择转发方式
///
/// # Arguments
///
/// * `client` - 客户端连接（可以是 TcpStream 或 UnixStream）
/// * `target_addr` - 目标地址（支持 tcp:// 和 unix:// 格式）
pub async fn forward_to_target(
    client: impl AsyncRead + AsyncWrite + Send + 'static,
    target_addr: &str,
) -> Result<ForwardResult> {
    let target = connect_to_target(target_addr).await?;
    forward_bidirectional(client, target).await
}

/// 带超时的通用转发函数
///
/// # Arguments
///
/// * `client` - 客户端连接
/// * `target_addr` - 目标地址
/// * `timeout_duration` - 超时时间
pub async fn forward_to_target_with_timeout(
    client: impl AsyncRead + AsyncWrite + Send + 'static,
    target_addr: &str,
    timeout_duration: Duration,
) -> Result<ForwardResult> {
    let target = connect_to_target(target_addr).await?;
    forward_bidirectional_with_timeout(client, target, timeout_duration).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    async fn setup_echo_server() -> (TcpListener, SocketAddr) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        (listener, addr)
    }

    #[tokio::test]
    async fn test_forward_bidirectional() {
        let (listener, addr) = setup_echo_server().await;

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = vec![0u8; 1024];
                while let Ok(n) = stream.read(&mut buf).await {
                    if n == 0 {
                        break;
                    }
                    if stream.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            }
        });

        tokio::time::sleep(Duration::from_millis(10)).await;

        let mut client = TcpStream::connect(addr).await.unwrap();
        let target = TcpStream::connect(addr).await.unwrap();

        client.write_all(b"hello").await.unwrap();

        // 关闭写入端
        client.shutdown().await.unwrap();

        let result = tokio::time::timeout(
            Duration::from_secs(2),
            forward_bidirectional(client, target),
        )
        .await;

        assert!(result.is_ok(), "forward_bidirectional should complete within timeout");
    }

    #[tokio::test]
    async fn test_connect_to_target_tcp() {
        let (listener, addr) = setup_echo_server().await;

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 10];
                let n = stream.read(&mut buf).await.unwrap();
                stream.write_all(&buf[..n]).await.unwrap();
            }
        });

        tokio::time::sleep(Duration::from_millis(10)).await;

        let target_addr = format!("tcp://{}", addr);
        let result = connect_to_target(&target_addr).await;
        assert!(result.is_ok());
    }
}
