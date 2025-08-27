use axum::{
    extract::{ws::Message, State, WebSocketUpgrade},
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde_json::Value;

use crate::{definitions::PreLangServerToWasmMessage, server::AppState};

pub async fn ws_rpc_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|ws| async move { handle_rpc_websocket(ws, state).await })
}

/// Handles all playground RPC commands over the WebSocket connection.
pub async fn handle_rpc_websocket(ws: axum::extract::ws::WebSocket, state: AppState) {
    let (mut ws_tx, mut ws_rx) = ws.split();
    while let Some(Ok(msg)) = ws_rx.next().await {
        if let Ok(msg) = msg.to_text() {
            if let Ok(json) = serde_json::from_str::<Value>(msg) {
                let rpc_id = json["rpcId"].clone();
                let rpc_method = json["rpcMethod"].as_str().unwrap_or("");
                let _data = &json["data"];
                tracing::info!("Handling RPC request: {:?}", rpc_method);
                match rpc_method {
                    "INITIALIZED" => {
                        let response = serde_json::json!({
                            "rpcMethod": "INITIALIZED",
                            "rpcId": rpc_id,
                            "data": { "ok": true }
                        });
                        if let Err(e) = state
                            .playground_tx
                            .send(PreLangServerToWasmMessage::WasmIsInitialized)
                        {
                            tracing::error!(
                                "Failed to send INITIALIZED message to language-server: {e}"
                            );
                        }
                        tracing::info!("Sent INITIALIZED message to language-server");
                        let _ = ws_tx.send(Message::text(response.to_string())).await;
                    }
                    "GET_PLAYGROUND_PORT" => {
                        let response = serde_json::json!({
                            "rpcMethod": "GET_PLAYGROUND_PORT",
                            "rpcId": rpc_id,
                            // NB(sam): even though this seems like a misnomer, it is not!!! GET_PLAYGROUND_PORT is meant to fetch
                            // the proxy port for the playground and is just really poorly named.
                            "data": { "port": state.proxy_port }
                        });
                        let _ = ws_tx.send(Message::text(response.to_string())).await;
                    }
                    "SET_PROXY_SETTINGS" => {
                        let response = serde_json::json!({
                            "rpcMethod": "SET_PROXY_SETTINGS",
                            "rpcId": rpc_id,
                            "data": { "ok": true }
                        });
                        let _ = ws_tx.send(Message::text(response.to_string())).await;
                    }
                    "LOAD_AWS_CREDS" => {
                        let response = serde_json::json!({
                            "rpcMethod": "LOAD_AWS_CREDS",
                            "rpcId": rpc_id,
                            "data": { "ok": true }
                        });
                        let _ = ws_tx.send(Message::text(response.to_string())).await;
                    }
                    "LOAD_GCP_CREDS" => {
                        let response = serde_json::json!({
                            "rpcMethod": "LOAD_GCP_CREDS",
                            "rpcId": rpc_id,
                            "data": { "ok": true }
                        });
                        let _ = ws_tx.send(Message::text(response.to_string())).await;
                    }
                    _ => {
                        tracing::warn!("Unknown RPC method: {}", rpc_method);
                    }
                }
                tracing::info!("Handled RPC request: {:?}", rpc_method);
            }
        }
    }
}
