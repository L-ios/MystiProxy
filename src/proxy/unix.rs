//! Unix Domain Socket (UDS) 支持
//!
//! 该模块提供 UDS 监听和转发功能，仅在 Unix 系统上可用。

use std::path::Path;

use tokio::io::{self, AsyncWriteExt};
use tokio::net::{TcpStream, UnixListener, UnixStream};

use crate::error::{MystiProxyError, Result};

/// 绑定 Unix Domain Socket 监听器
///
/// # 参数
/// - `path`: socket 文件路径
///
/// # 返回
/// 成功返回 UnixListener
///
/// # 错误
/// - 如果文件已存在但无法删除
/// - 如果无法创建父目录
/// - 如果绑定失败
///
/// # 示例
/// ```no_run
/// use mystiproxy::proxy::unix::bind_unix;
/// use std::path::Path;
///
/// #[tokio::main]
/// async fn main() -> mystiproxy::Result<()> {
///     let listener = bind_unix(Path::new("/tmp/mysocket.sock")).await?;
///     Ok(())
/// }
/// ```
pub async fn bind_unix(path: &Path) -> Result<UnixListener> {
    // 如果文件已存在，先删除
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| {
            MystiProxyError::Proxy(format!("无法删除已存在的 socket 文件 {:?}: {}", path, e))
        })?;
    }

    // 创建父目录
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                MystiProxyError::Proxy(format!("无法创建 socket 父目录 {:?}: {}", parent, e))
            })?;
        }
    }

    // 绑定 Unix Domain Socket
    UnixListener::bind(path).map_err(|e| {
        MystiProxyError::Proxy(format!("无法绑定 Unix Domain Socket {:?}: {}", path, e))
    })
}

/// 连接到 Unix Domain Socket
///
/// # 参数
/// - `path`: socket 文件路径
///
/// # 返回
/// 成功返回 UnixStream
///
/// # 错误
/// - 如果连接失败
///
/// # 示例
/// ```no_run
/// use mystiproxy::proxy::unix::connect_unix;
/// use std::path::Path;
///
/// #[tokio::main]
/// async fn main() -> mystiproxy::Result<()> {
///     let stream = connect_unix(Path::new("/tmp/mysocket.sock")).await?;
///     Ok(())
/// }
/// ```
pub async fn connect_unix(path: &Path) -> Result<UnixStream> {
    UnixStream::connect(path).await.map_err(|e| {
        MystiProxyError::Proxy(format!("无法连接到 Unix Domain Socket {:?}: {}", path, e))
    })
}

/// UDS 到 TCP 的双向数据转发
///
/// 将 UDS 客户端连接转发到 TCP 目标地址
///
/// # 参数
/// - `uds_stream`: UDS 客户端连接
/// - `tcp_addr`: TCP 目标地址 (格式: "host:port")
///
/// # 返回
/// 成功返回转发的字节数统计
///
/// # 示例
/// ```no_run
/// use mystiproxy::proxy::unix::{bind_unix, forward_uds_to_tcp};
/// use std::path::Path;
///
/// #[tokio::main]
/// async fn main() -> mystiproxy::Result<()> {
///     let listener = bind_unix(Path::new("/tmp/proxy.sock")).await?;
///     
///     loop {
///         let (uds_stream, _) = listener.accept().await
///             .map_err(|e| mystiproxy::MystiProxyError::Io(e))?;
///         
///         tokio::spawn(async move {
///             let _ = forward_uds_to_tcp(uds_stream, "127.0.0.1:8080").await;
///         });
///     }
/// }
/// ```
pub async fn forward_uds_to_tcp(mut uds_stream: UnixStream, tcp_addr: &str) -> Result<(u64, u64)> {
    // 连接到 TCP 目标
    let mut tcp_stream = TcpStream::connect(tcp_addr)
        .await
        .map_err(|e| MystiProxyError::Proxy(format!("无法连接到 TCP 目标 {}: {}", tcp_addr, e)))?;

    // 双向转发
    let (mut uds_read, mut uds_write) = uds_stream.split();
    let (mut tcp_read, mut tcp_write) = tcp_stream.split();

    // UDS -> TCP 和 TCP -> UDS 同时进行
    let uds_to_tcp = async {
        let result = io::copy(&mut uds_read, &mut tcp_write).await;
        let _ = tcp_write.shutdown().await;
        result
    };

    let tcp_to_uds = async {
        let result = io::copy(&mut tcp_read, &mut uds_write).await;
        let _ = uds_write.shutdown().await;
        result
    };

    // 并发执行两个方向的转发
    let (bytes_uds_to_tcp, bytes_tcp_to_uds) = tokio::try_join!(uds_to_tcp, tcp_to_uds)
        .map_err(|e| MystiProxyError::Proxy(format!("转发过程中发生错误: {}", e)))?;

    Ok((bytes_uds_to_tcp, bytes_tcp_to_uds))
}

/// UDS 到 UDS 的双向数据转发
///
/// 将 UDS 客户端连接转发到另一个 UDS 目标
///
/// # 参数
/// - `client_stream`: UDS 客户端连接
/// - `target_path`: 目标 UDS 路径
///
/// # 返回
/// 成功返回转发的字节数统计
///
/// # 示例
/// ```no_run
/// use mystiproxy::proxy::unix::{bind_unix, connect_unix, forward_uds_to_uds};
/// use std::path::Path;
///
/// #[tokio::main]
/// async fn main() -> mystiproxy::Result<()> {
///     let listener = bind_unix(Path::new("/tmp/proxy.sock")).await?;
///     
///     loop {
///         let (client_stream, _) = listener.accept().await
///             .map_err(|e| mystiproxy::MystiProxyError::Io(e))?;
///         
///         tokio::spawn(async move {
///             let _ = forward_uds_to_uds(client_stream, Path::new("/tmp/target.sock")).await;
///         });
///     }
/// }
/// ```
pub async fn forward_uds_to_uds(
    mut client_stream: UnixStream,
    target_path: &Path,
) -> Result<(u64, u64)> {
    // 连接到目标 UDS
    let mut target_stream = connect_unix(target_path).await?;

    // 双向转发
    let (mut client_read, mut client_write) = client_stream.split();
    let (mut target_read, mut target_write) = target_stream.split();

    // Client -> Target 和 Target -> Client 同时进行
    let client_to_target = async {
        let result = io::copy(&mut client_read, &mut target_write).await;
        let _ = target_write.shutdown().await;
        result
    };

    let target_to_client = async {
        let result = io::copy(&mut target_read, &mut client_write).await;
        let _ = client_write.shutdown().await;
        result
    };

    // 并发执行两个方向的转发
    let (bytes_client_to_target, bytes_target_to_client) =
        tokio::try_join!(client_to_target, target_to_client)
            .map_err(|e| MystiProxyError::Proxy(format!("转发过程中发生错误: {}", e)))?;

    Ok((bytes_client_to_target, bytes_target_to_client))
}

/// Unix Domain Socket 代理服务器
///
/// 监听 UDS 并转发到指定目标
pub struct UnixProxy {
    /// 监听路径
    listen_path: std::path::PathBuf,
    /// 目标地址
    target: ProxyTarget,
}

/// 代理目标类型
pub enum ProxyTarget {
    /// TCP 目标
    Tcp(String),
    /// UDS 目标
    Uds(std::path::PathBuf),
}

impl UnixProxy {
    /// 创建新的 UDS 代理
    ///
    /// # 参数
    /// - `listen_path`: 监听的 socket 路径
    /// - `target`: 代理目标
    pub fn new(listen_path: impl Into<std::path::PathBuf>, target: ProxyTarget) -> Self {
        Self {
            listen_path: listen_path.into(),
            target,
        }
    }

    /// 启动代理服务器
    ///
    /// 该方法会阻塞，持续接受新连接
    pub async fn run(self) -> Result<()> {
        let listener = bind_unix(&self.listen_path).await?;

        loop {
            let (stream, _addr) = listener.accept().await.map_err(MystiProxyError::Io)?;

            let target = self.target.clone();

            tokio::spawn(async move {
                let result = match target {
                    ProxyTarget::Tcp(addr) => forward_uds_to_tcp(stream, &addr).await,
                    ProxyTarget::Uds(path) => forward_uds_to_uds(stream, &path).await,
                };

                if let Err(e) = result {
                    tracing::error!("代理转发错误: {}", e);
                }
            });
        }
    }
}

impl Clone for ProxyTarget {
    fn clone(&self) -> Self {
        match self {
            ProxyTarget::Tcp(addr) => ProxyTarget::Tcp(addr.clone()),
            ProxyTarget::Uds(path) => ProxyTarget::Uds(path.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_bind_and_connect_unix() {
        let path = std::env::temp_dir().join("test_mystiproxy_uds.sock");

        // 清理可能存在的旧文件
        let _ = std::fs::remove_file(&path);

        // 绑定监听器
        let listener = bind_unix(&path).await.expect("绑定失败");

        // 在另一个任务中连接
        let path_clone = path.clone();
        let connect_task = tokio::spawn(async move {
            // 稍微等待确保监听器已准备好
            tokio::time::sleep(Duration::from_millis(10)).await;
            connect_unix(&path_clone).await
        });

        // 接受连接
        let accept_result = timeout(Duration::from_secs(1), listener.accept()).await;
        assert!(accept_result.is_ok());

        // 验证连接成功
        let connect_result = timeout(Duration::from_secs(1), connect_task).await;
        assert!(connect_result.is_ok());
        assert!(connect_result.unwrap().is_ok());

        // 清理
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn test_forward_uds_to_uds() {
        let server_path = std::env::temp_dir().join("test_mystiproxy_server.sock");
        let proxy_path = std::env::temp_dir().join("test_mystiproxy_proxy.sock");

        // 清理
        let _ = std::fs::remove_file(&server_path);
        let _ = std::fs::remove_file(&proxy_path);

        // 创建目标服务器
        let server_listener = bind_unix(&server_path).await.expect("绑定服务器失败");

        // 目标服务器任务
        let server_task = tokio::spawn(async move {
            let (mut stream, _) = server_listener.accept().await.expect("接受连接失败");
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).await.expect("读取失败");
            stream.write_all(&buf[..n]).await.expect("写入失败");
        });

        // 等待服务器启动
        tokio::time::sleep(Duration::from_millis(10)).await;

        // 创建代理服务器
        let proxy_listener = bind_unix(&proxy_path).await.expect("绑定代理失败");

        // 克隆路径用于代理任务
        let server_path_clone = server_path.clone();
        let server_path_for_cleanup = server_path.clone();

        // 代理任务
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = proxy_listener.accept().await.expect("接受连接失败");
            let _ = forward_uds_to_uds(stream, &server_path_clone).await;
        });

        // 等待代理启动
        tokio::time::sleep(Duration::from_millis(10)).await;

        // 客户端连接并发送数据
        let mut client = connect_unix(&proxy_path).await.expect("连接代理失败");
        client.write_all(b"hello").await.expect("写入失败");

        let mut buf = [0u8; 1024];
        let n = client.read(&mut buf).await.expect("读取失败");
        assert_eq!(&buf[..n], b"hello");

        // 清理
        let _ = std::fs::remove_file(&server_path_for_cleanup);
        let _ = std::fs::remove_file(&proxy_path);

        // 等待任务完成
        let _ = timeout(Duration::from_secs(1), server_task).await;
        let _ = timeout(Duration::from_secs(1), proxy_task).await;
    }
}
