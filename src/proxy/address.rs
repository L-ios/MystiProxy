//! 地址解析模块
//!
//! 提供统一的地址解析功能，支持 TCP 和 Unix Domain Socket 地址

use std::net::SocketAddr;

#[cfg(unix)]
use std::path::PathBuf;

use crate::error::{MystiProxyError, Result};

/// 统一的地址类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    /// TCP 地址
    Tcp(SocketAddr),
    /// Unix Domain Socket 地址
    #[cfg(unix)]
    Unix(PathBuf),
}

impl Address {
    /// 从字符串解析地址
    ///
    /// 支持的格式:
    /// - `tcp://0.0.0.0:3128`
    /// - `tcp://127.0.0.1:8080`
    /// - `unix:///var/run/docker.sock` (仅 Unix 平台)
    /// - `unix:///tmp/proxy.sock` (仅 Unix 平台)
    ///
    /// # Examples
    ///
    /// ```
    /// use mystiproxy::proxy::Address;
    ///
    /// let addr = Address::parse("tcp://0.0.0.0:3128")?;
    /// # Ok::<(), mystiproxy::MystiProxyError>(())
    /// ```
    pub fn parse(s: &str) -> Result<Self> {
        if !s.contains("://") {
            return Err(MystiProxyError::Other(format!(
                "Invalid address format: missing protocol separator '://': {}",
                s
            )));
        }

        let parts: Vec<&str> = s.splitn(2, "://").collect();
        if parts.len() != 2 {
            return Err(MystiProxyError::Other(format!(
                "Invalid address format: {}",
                s
            )));
        }

        let protocol = parts[0];
        let address = parts[1];

        match protocol {
            "tcp" => {
                let socket_addr: SocketAddr = address.parse()?;
                Ok(Address::Tcp(socket_addr))
            }
            #[cfg(unix)]
            "unix" => {
                Ok(Address::Unix(PathBuf::from(address)))
            }
            #[cfg(not(unix))]
            "unix" => {
                Err(MystiProxyError::Other(
                    "Unix Domain Sockets are not supported on this platform".to_string(),
                ))
            }
            _ => Err(MystiProxyError::Other(format!(
                "Unsupported protocol: {}",
                protocol
            ))),
        }
    }

    /// 返回地址的协议类型
    pub fn protocol(&self) -> &'static str {
        match self {
            Address::Tcp(_) => "tcp",
            #[cfg(unix)]
            Address::Unix(_) => "unix",
        }
    }

    /// 检查是否为 TCP 地址
    pub fn is_tcp(&self) -> bool {
        matches!(self, Address::Tcp(_))
    }

    /// 检查是否为 Unix Domain Socket 地址
    #[cfg(unix)]
    pub fn is_unix(&self) -> bool {
        matches!(self, Address::Unix(_))
    }

    /// 获取 TCP 地址，如果不是 TCP 地址则返回 None
    pub fn as_tcp(&self) -> Option<&SocketAddr> {
        match self {
            Address::Tcp(addr) => Some(addr),
            #[cfg(unix)]
            _ => None,
        }
    }

    /// 获取 Unix 地址，如果不是 Unix 地址则返回 None
    #[cfg(unix)]
    pub fn as_unix(&self) -> Option<&PathBuf> {
        match self {
            Address::Unix(path) => Some(path),
            _ => None,
        }
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Address::Tcp(addr) => write!(f, "tcp://{}", addr),
            #[cfg(unix)]
            Address::Unix(path) => write!(f, "unix://{}", path.display()),
        }
    }
}

impl std::str::FromStr for Address {
    type Err = MystiProxyError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tcp_address() {
        let addr = Address::parse("tcp://0.0.0.0:3128").unwrap();
        assert!(addr.is_tcp());
        assert_eq!(addr.protocol(), "tcp");
        assert_eq!(addr.to_string(), "tcp://0.0.0.0:3128");
    }

    #[test]
    fn test_parse_tcp_localhost() {
        let addr = Address::parse("tcp://127.0.0.1:8080").unwrap();
        assert!(addr.is_tcp());
        let socket_addr = addr.as_tcp().unwrap();
        assert_eq!(socket_addr.ip().to_string(), "127.0.0.1");
        assert_eq!(socket_addr.port(), 8080);
    }

    #[cfg(unix)]
    #[test]
    fn test_parse_unix_address() {
        let addr = Address::parse("unix:///var/run/docker.sock").unwrap();
        assert!(addr.is_unix());
        assert_eq!(addr.protocol(), "unix");
        assert_eq!(addr.to_string(), "unix:///var/run/docker.sock");
    }

    #[cfg(unix)]
    #[test]
    fn test_parse_unix_temp() {
        let addr = Address::parse("unix:///tmp/proxy.sock").unwrap();
        assert!(addr.is_unix());
        let path = addr.as_unix().unwrap();
        assert_eq!(path.to_str().unwrap(), "/tmp/proxy.sock");
    }

    #[test]
    fn test_parse_invalid_no_protocol() {
        let result = Address::parse("0.0.0.0:3128");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_protocol() {
        let result = Address::parse("udp://0.0.0.0:3128");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_tcp_address() {
        let result = Address::parse("tcp://invalid:address");
        assert!(result.is_err());
    }

    #[test]
    fn test_from_str() {
        let addr: Address = "tcp://0.0.0.0:3128".parse().unwrap();
        assert!(addr.is_tcp());
    }
}
