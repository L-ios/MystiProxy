//! 代理模块
//!
//! 提供 TCP 代理服务器的核心功能，包括地址解析、监听和数据转发

mod address;
mod forward;
mod tcp;

#[cfg(unix)]
pub mod unix;

pub use address::Address;
pub use forward::{
    connect_to_target, forward_bidirectional, forward_bidirectional_with_timeout,
    forward_tcp_to_tcp, forward_to_target, forward_to_target_with_timeout,
    ForwardResult, TransferStats,
};

#[cfg(unix)]
pub use forward::forward_tcp_to_uds;

pub use tcp::TcpProxyListener;

use std::time::Duration;

use tracing::{error, info, warn};

use crate::config::{EngineConfig, ProxyType};
use crate::error::{MystiProxyError, Result};
use crate::io::StreamListener;

/// 代理服务器配置
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// 监听地址
    pub listen: Address,
    /// 目标地址
    pub target: Address,
    /// 代理类型
    pub proxy_type: ProxyType,
    /// 超时时间
    pub timeout: Option<Duration>,
}

impl ProxyConfig {
    /// 从 EngineConfig 创建 ProxyConfig
    pub fn from_engine_config(config: &EngineConfig) -> Result<Self> {
        let listen = Address::parse(&config.listen)?;
        let target = Address::parse(&config.target)?;

        Ok(Self {
            listen,
            target,
            proxy_type: config.proxy_type.clone(),
            timeout: config.timeout,
        })
    }
}

/// 代理服务器
pub struct ProxyServer {
    /// 配置
    config: ProxyConfig,
    /// 监听器
    listener: Option<StreamListener>,
}

impl ProxyServer {
    /// 创建新的代理服务器实例
    pub fn new(config: ProxyConfig) -> Self {
        Self {
            config,
            listener: None,
        }
    }

    /// 从 EngineConfig 创建代理服务器
    pub fn from_engine_config(config: &EngineConfig) -> Result<Self> {
        let proxy_config = ProxyConfig::from_engine_config(config)?;
        Ok(Self::new(proxy_config))
    }

    /// 启动代理服务器
    pub async fn start(&mut self) -> Result<()> {
        info!(
            "Starting proxy server: {} -> {} ({:?})",
            self.config.listen, self.config.target, self.config.proxy_type
        );

        let listener = StreamListener::new(self.config.listen.to_string()).await?;
        self.listener = Some(listener);

        info!("Proxy server started on {}", self.config.listen);

        Ok(())
    }

    /// 运行代理服务器的主循环
    ///
    /// 接受连接并处理转发
    pub async fn run(&self) -> Result<()> {
        let listener = self.listener.as_ref().ok_or_else(|| {
            MystiProxyError::Proxy("Server not started. Call start() first.".to_string())
        })?;

        info!("Proxy server is running, waiting for connections...");

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    info!("Accepted connection from {}", addr);

                    let target_addr = self.config.target.to_string();
                    let timeout_duration = self.config.timeout;

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, target_addr, timeout_duration).await {
                            error!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    /// 处理单个连接
    async fn handle_connection(
        stream: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + 'static,
        target_addr: String,
        timeout_duration: Option<Duration>,
    ) -> Result<()> {
        let result = if let Some(timeout) = timeout_duration {
            forward_to_target_with_timeout(stream, &target_addr, timeout).await
        } else {
            forward_to_target(stream, &target_addr).await
        };

        match result {
            Ok(forward_result) => {
                info!(
                    "Connection closed: sent {} bytes to target, {} bytes to client",
                    forward_result.stats.client_to_target,
                    forward_result.stats.target_to_client
                );
                Ok(())
            }
            Err(e) => {
                warn!("Forward error: {}", e);
                Err(e)
            }
        }
    }

    /// 获取监听地址
    pub fn listen_addr(&self) -> &Address {
        &self.config.listen
    }

    /// 获取目标地址
    pub fn target_addr(&self) -> &Address {
        &self.config.target
    }
}

/// TCP 代理服务（简化版本）
pub struct TcpProxy {
    /// 监听地址
    listen: String,
    /// 目标地址
    target: String,
    /// 超时时间
    timeout: Option<Duration>,
}

impl TcpProxy {
    /// 创建新的 TCP 代理实例
    pub fn new(listen: String, target: String, timeout: Option<Duration>) -> Self {
        Self {
            listen,
            target,
            timeout,
        }
    }

    /// 启动并运行代理服务器
    pub async fn run(&self) -> Result<()> {
        let listener = StreamListener::new(self.listen.clone()).await?;
        info!("TCP proxy listening on {}", self.listen);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    info!("Accepted connection from {}", addr);

                    let target = self.target.clone();
                    let timeout_duration = self.timeout;

                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, target, timeout_duration).await {
                            error!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }

    async fn handle_connection(
        stream: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + 'static,
        target_addr: String,
        timeout_duration: Option<Duration>,
    ) -> Result<()> {
        let result = if let Some(timeout) = timeout_duration {
            forward_to_target_with_timeout(stream, &target_addr, timeout).await
        } else {
            forward_to_target(stream, &target_addr).await
        };

        match result {
            Ok(forward_result) => {
                info!(
                    "Connection closed: {} bytes to target, {} bytes to client",
                    forward_result.stats.client_to_target,
                    forward_result.stats.target_to_client
                );
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

/// 旧的 Proxy 结构体（保持向后兼容）
pub struct Proxy;

impl Proxy {
    /// 创建新的代理实例
    pub fn new() -> Self {
        Proxy
    }
}

impl Default for Proxy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_parse() {
        let addr = Address::parse("tcp://0.0.0.0:3128").unwrap();
        assert!(addr.is_tcp());

        #[cfg(unix)]
        {
            let addr = Address::parse("unix:///var/run/docker.sock").unwrap();
            assert!(addr.is_unix());
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_proxy_config_from_engine_config() {
        let engine_config = EngineConfig {
            listen: "tcp://0.0.0.0:3128".to_string(),
            target: "unix:///var/run/docker.sock".to_string(),
            proxy_type: ProxyType::Tcp,
            timeout: Some(Duration::from_secs(10)),
            header: None,
            locations: None,
        };

        let proxy_config = ProxyConfig::from_engine_config(&engine_config).unwrap();
        assert!(proxy_config.listen.is_tcp());
        assert!(proxy_config.target.is_unix());
        assert_eq!(proxy_config.timeout, Some(Duration::from_secs(10)));
    }
}
