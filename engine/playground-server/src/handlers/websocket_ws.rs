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
    tracing::info!("axum listening on /ws");
    let (mut ws_tx, mut ws_rx) = ws.split();
    let mut rx = state.webview_router_to_websocket_rx;

    // Send initial project state using the helper
    tracing::info!("send_all_projects_to_client BEGIN");
    // send_all_projects_to_client(&mut ws_tx, &session).await;
    tracing::info!("send_all_projects_to_client END");

    // Handle incoming messages and broadcast updates
    tokio::spawn(async move {
        loop {
            tokio::select! {
                // Handle incoming messages from the client
                Some(result) = ws_rx.next() => {
                    match result {
                        Ok(msg) => {
                            // Handle incoming WebSocket messages here
                            tracing::debug!("Received WebSocket message: {:?}", msg);
                        }
                        Err(e) => {
                            tracing::error!("WebSocket error: {}", e);
                            break;
                        }
                    }
                }
                // Handle broadcast messages
                Ok(msg) = rx.recv() => {
                    // Convert the LSP message to a format suitable for WebSocket
                    let message_text = serde_json::to_string(&msg).unwrap_or_else(|_| "{}".to_string());
                    if let Err(e) = ws_tx.send(Message::Text(message_text.into())).await {
                        tracing::error!("Failed to send broadcast message: {}", e);
                        break;
                    }
                }
            }
        }
    });
}
