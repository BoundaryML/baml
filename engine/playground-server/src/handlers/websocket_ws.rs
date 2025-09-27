use axum::{
    extract::{ws::Message, State, WebSocketUpgrade},
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};

use crate::server::AppState;

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|ws| async move { start_client_connection(ws, state).await })
}

pub async fn start_client_connection(ws: axum::extract::ws::WebSocket, state: AppState) {
    tracing::info!("EventListener client connected");
    let (mut ws_tx, mut ws_rx) = ws.split();
    let mut rx = state.webview_router_to_websocket_rx_provider.subscribe();

    // Handle incoming messages and broadcast updates
    tokio::spawn(async move {
        while let Some(msg) = ws_rx.next().await {
            match msg {
                Ok(msg) => {
                    tracing::warn!("Received message from EventListener, discarding it:\n{msg:?}");
                }
                Err(e) => {
                    tracing::error!("Error receiving message from EventListener: {e}");
                    break;
                }
            }
        }
        tracing::info!("WebSocket rx channel closed");
    });
    tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            // Convert the LSP message to a format suitable for WebSocket
            let message_text = serde_json::to_string(&msg).unwrap_or_else(|_| "{}".to_string());
            if let Err(e) = ws_tx.send(Message::Text(message_text.into())).await {
                tracing::error!("Failed to send message to EventListener: {e}");
                break;
            }
        }
        tracing::info!("WebSocket tx channel closed");
    });
}
