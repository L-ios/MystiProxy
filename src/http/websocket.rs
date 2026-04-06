//! WebSocket 模块
//! 
//! 提供 WebSocket 连接处理功能

use std::convert::Infallible;

use http_body_util::Empty;
use hyper::header;
use hyper::upgrade::Upgraded;
use hyper::{Request, Response, StatusCode};
use tokio_tungstenite::WebSocketStream;
use tracing::{debug, error, info};

use crate::error::{MystiProxyError, Result};

/// 检查是否为 WebSocket 升级请求
pub fn is_websocket_upgrade_request(req: &Request<hyper::body::Incoming>) -> bool {
    if let Some(upgrade) = req.headers().get(header::UPGRADE) {
        if let Ok(upgrade_str) = upgrade.to_str() {
            return upgrade_str.eq_ignore_ascii_case("websocket");
        }
    }
    false
}

/// 处理 WebSocket 升级请求
pub async fn handle_websocket_upgrade(req: Request<hyper::body::Incoming>) -> Result<Response<Empty<Infallible>>> {
    // 检查是否为 WebSocket 升级请求
    if !is_websocket_upgrade_request(&req) {
        return Err(MystiProxyError::Proxy("Not a WebSocket upgrade request".to_string()));
    }

    // 检查是否有 Sec-WebSocket-Key 头
    let key = req.headers().get(header::SEC_WEBSOCKET_KEY)
        .ok_or_else(|| MystiProxyError::Proxy("Missing Sec-WebSocket-Key header".to_string()))?
        .to_str()
        .map_err(|e| MystiProxyError::Proxy(format!("Invalid Sec-WebSocket-Key header: {}", e)))?;

    // 计算 WebSocket 接受密钥
    let accept = compute_websocket_accept(key);

    // 创建 WebSocket 响应
    let response = Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header(header::UPGRADE, "websocket")
        .header(header::CONNECTION, "upgrade")
        .header(header::SEC_WEBSOCKET_ACCEPT, accept)
        .body(Empty::new())
        .map_err(MystiProxyError::Http)?;

    Ok(response)
}

/// 计算 WebSocket 接受密钥
fn compute_websocket_accept(key: &str) -> String {
    use sha1::{Digest, Sha1};
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    
    let mut hasher = Sha1::new();
    hasher.update(key);
    hasher.update("258EAFA5-E914-47DA-95CA-C5AB0DC85B11"); // WebSocket 魔术字符串
    let hash = hasher.finalize();
    STANDARD.encode(hash)
}
