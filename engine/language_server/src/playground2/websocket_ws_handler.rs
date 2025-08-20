use std::sync::Arc;

use axum::{
    extract::{ws::Message, State, WebSocketUpgrade},
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use tokio::sync::RwLock;

use crate::playground2::server::AppState;
use crate::Session;

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|ws| async move { start_client_connection(ws, state).await })
}

pub async fn start_client_connection(ws: axum::extract::ws::WebSocket, state: AppState) {
    tracing::info!("axum listening on /ws");
    let (mut ws_tx, mut ws_rx) = ws.split();
    let mut rx = state.broadcast_rx;

    // Send initial project state using the helper
    tracing::info!("send_all_projects_to_client BEGIN");
    // send_all_projects_to_client(&mut ws_tx, &session).await;
    tracing::info!("send_all_projects_to_client END");

    // --- SEND BUFFERED EVENTS (if any) ---
    // when the playground is loading, it sends a bunch of add_project events
    // the IDE sends a lot of add_project events, so we buffer them here
    // the language-server will receive these events before the playground is ready
    // so when the playground is open, it needs to connect to the language-server
    // and have the language-server replay them all
    // {
    //     let mut st = state.write().await;
    //     let buffered_events = st.drain_event_buffer();
    //     for event in buffered_events.clone() {
    //         let _ = ws_tx.send(Message::text(event)).await;
    //         // Add configurable delay between buffered events
    //         tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;
    //     }
    //     tracing::info!("Sent {} buffered events", buffered_events.len());
    //     st.mark_first_client_connected();
    // }
    // --- END BUFFERED EVENTS ---

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
