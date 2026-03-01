//! MystiProxy - 灵活的 HTTP 代理服务器，支持 Mock 功能

pub mod config;
pub mod context;
pub mod error;
pub mod http;
pub mod io;
pub mod mock;
pub mod proxy;
pub mod router;
pub mod tls;

#[cfg(feature = "local-management")]
pub mod management;

pub mod gateway;
pub mod mocker;

// 重导出常用类型
pub use error::{MystiProxyError, Result};

// 重导出 Mock 相关类型
pub use mock::{MockBuilder, MockLocation, MockResponse, MockService};

// 重导出上下文相关类型
pub use context::{get_engine_name, get_thread_id, set_engine_name, thread_identity, with_engine};
