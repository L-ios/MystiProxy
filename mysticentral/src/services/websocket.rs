//! WebSocket Support
//!
//! Real-time communication with MystiProxy instances.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::models::MockConfiguration;

/// Connected WebSocket clients
pub type ConnectedClients = Arc<RwLock<HashMap<Uuid, WebSocketClient>>>;

/// WebSocket client information
#[derive(Debug)]
#[allow(dead_code)]
pub struct WebSocketClient {
    pub instance_id: Uuid,
    pub tx: tokio::sync::mpsc::Sender<Message>,
}

/// WebSocket broadcaster for real-time updates
#[derive(Debug, Clone)]
pub struct WebSocketBroadcaster {
    #[allow(dead_code)]
    clients: ConnectedClients,
}

impl WebSocketBroadcaster {
    /// Create a new broadcaster
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a client
    #[allow(dead_code)]
    pub async fn add_client(&self, instance_id: Uuid, tx: tokio::sync::mpsc::Sender<Message>) {
        let mut clients = self.clients.write().await;
        clients.insert(instance_id, WebSocketClient { instance_id, tx });
        tracing::info!("WebSocket client connected: {}", instance_id);
    }

    /// Remove a client
    #[allow(dead_code)]
    pub async fn remove_client(&self, instance_id: &Uuid) {
        let mut clients = self.clients.write().await;
        clients.remove(instance_id);
        tracing::info!("WebSocket client disconnected: {}", instance_id);
    }

    /// Broadcast a configuration update to all clients
    #[allow(dead_code)]
    pub async fn broadcast_config_update(&self, config: &MockConfiguration) {
        let message = json!({
            "type": "config_update",
            "config": config
        });

        let msg_str = serde_json::to_string(&message).unwrap();
        self.broadcast_message(&msg_str).await;
    }

    /// Broadcast a configuration deletion to all clients
    #[allow(dead_code)]
    pub async fn broadcast_config_delete(&self, config_id: Uuid) {
        let message = json!({
            "type": "config_delete",
            "id": config_id
        });

        let msg_str = serde_json::to_string(&message).unwrap();
        self.broadcast_message(&msg_str).await;
    }

    /// Send a message to a specific client
    #[allow(dead_code)]
    pub async fn send_to_client(&self, instance_id: &Uuid, message: &str) -> bool {
        let clients = self.clients.read().await;
        if let Some(client) = clients.get(instance_id) {
            client
                .tx
                .send(Message::Text(message.to_string()))
                .await
                .is_ok()
        } else {
            false
        }
    }

    /// Broadcast a message to all clients
    #[allow(dead_code)]
    async fn broadcast_message(&self, message: &str) {
        let clients = self.clients.read().await;
        let mut failed = Vec::new();

        for (id, client) in clients.iter() {
            if client
                .tx
                .send(Message::Text(message.to_string()))
                .await
                .is_err()
            {
                failed.push(*id);
            }
        }

        // Don't modify the map while iterating - we'll log failures
        if !failed.is_empty() {
            tracing::warn!("Failed to send to {} clients", failed.len());
        }
    }

    /// Get connected client count
    #[allow(dead_code)]
    pub async fn client_count(&self) -> usize {
        self.clients.read().await.len()
    }
}

impl Default for WebSocketBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle WebSocket upgrade request
#[allow(dead_code)]
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State((pool, broadcaster)): State<(PgPool, Arc<WebSocketBroadcaster>)>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, pool, broadcaster))
}

/// Handle a WebSocket connection
#[allow(dead_code)]
async fn handle_socket(socket: WebSocket, _pool: PgPool, broadcaster: Arc<WebSocketBroadcaster>) {
    let (mut tx, mut rx) = socket.split();

    // Generate a temporary instance ID (in real app, this would come from auth)
    let instance_id = Uuid::new_v4();

    // Create a channel for outgoing messages
    let (outgoing_tx, mut outgoing_rx) = tokio::sync::mpsc::channel::<Message>(32);

    // Register the client
    broadcaster
        .add_client(instance_id, outgoing_tx.clone())
        .await;

    // Send welcome message
    let welcome = json!({
        "type": "connected",
        "instance_id": instance_id,
        "server_time": chrono::Utc::now().to_rfc3339()
    });
    let _ = tx
        .send(Message::Text(serde_json::to_string(&welcome).unwrap()))
        .await;

    // Spawn a task to handle outgoing messages
    let broadcast_clone = broadcaster.clone();
    let instance_id_clone = instance_id;
    tokio::spawn(async move {
        while let Some(msg) = outgoing_rx.recv().await {
            if tx.send(msg).await.is_err() {
                break;
            }
        }
        broadcast_clone.remove_client(&instance_id_clone).await;
    });

    // Handle incoming messages
    while let Some(msg) = rx.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                // Parse and handle the message
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    handle_incoming_message(&broadcaster, instance_id, json).await;
                }
            }
            Ok(Message::Ping(data)) => {
                let _ = outgoing_tx.send(Message::Pong(data)).await;
            }
            Ok(Message::Close(_)) => {
                break;
            }
            Err(e) => {
                tracing::error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    broadcaster.remove_client(&instance_id).await;
}

/// Handle an incoming WebSocket message
#[allow(dead_code)]
async fn handle_incoming_message(
    broadcaster: &WebSocketBroadcaster,
    instance_id: Uuid,
    message: serde_json::Value,
) {
    let msg_type = message.get("type").and_then(|v| v.as_str()).unwrap_or("");

    match msg_type {
        "heartbeat" => {
            // Handle heartbeat
            let response = json!({
                "type": "heartbeat_ack",
                "server_time": chrono::Utc::now().to_rfc3339()
            });
            broadcaster
                .send_to_client(&instance_id, &serde_json::to_string(&response).unwrap())
                .await;
        }
        "sync_request" => {
            // Handle sync request
            let response = json!({
                "type": "sync_required",
                "message": "Please use REST API for full sync"
            });
            broadcaster
                .send_to_client(&instance_id, &serde_json::to_string(&response).unwrap())
                .await;
        }
        _ => {
            tracing::warn!("Unknown WebSocket message type: {}", msg_type);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_broadcaster() {
        let broadcaster = WebSocketBroadcaster::new();

        let (tx, _rx) = tokio::sync::mpsc::channel(32);
        broadcaster.add_client(Uuid::nil(), tx).await;

        assert_eq!(broadcaster.client_count().await, 1);

        broadcaster.remove_client(&Uuid::nil()).await;
        assert_eq!(broadcaster.client_count().await, 0);
    }
}
