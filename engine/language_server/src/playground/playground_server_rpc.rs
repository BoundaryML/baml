use std::sync::Arc;

use base64::{engine::general_purpose, Engine as _};
use futures_util::{SinkExt, StreamExt};
use mime_guess::from_path;
use serde_json::Value;
use warp::ws::{Message, WebSocket};
use webbrowser;

use crate::{playground::definitions::PlaygroundState, session::Session};

/// Handles all playground RPC commands over the WebSocket connection.
pub async fn handle_rpc_websocket(ws: WebSocket, session: Arc<Session>) {
    let (mut ws_tx, mut ws_rx) = ws.split();
    while let Some(Ok(msg)) = ws_rx.next().await {
        if msg.is_text() {
            if let Ok(json) = serde_json::from_str::<Value>(msg.to_str().unwrap()) {
                let rpc_id = json["rpcId"].clone();
                let rpc_method = json["rpcMethod"].as_str().unwrap_or("");
                let data = &json["data"];
                // tracing::info!("Handling RPC request!");
                // tracing::info!("RPC METHOD: {:?}", rpc_method);
                match rpc_method {
                    "INITIALIZED" => {
                        let response = serde_json::json!({
                            "rpcMethod": "INITIALIZED",
                            "rpcId": rpc_id,
                            "data": { "ok": true }
                        });
                        let _ = ws_tx.send(Message::text(response.to_string())).await;
                    }
                    "GET_WEBVIEW_URI" => {
                        let path = data["path"].as_str().unwrap_or("");
                        let port = session.get_session_playground_port().unwrap_or_else(|| {
                            session.baml_settings.playground_port.unwrap_or(3030)
                        });
                        let proxy_port = port + 1; // Proxy server runs on port + 1

                        tracing::info!(
                            "GET_WEBVIEW_URI: playground_port={}, proxy_port={}, path={}",
                            port,
                            proxy_port,
                            path
                        );

                        // Convert absolute path to relative path for /static/ URI
                        let rel_path = std::env::current_dir()
                            .ok()
                            .and_then(|cwd| std::path::Path::new(path).strip_prefix(&cwd).ok())
                            .map(|p| p.to_string_lossy().replace('\\', "/"))
                            .unwrap_or_else(|| path.to_string());

                        let uri = format!("http://localhost:{proxy_port}/static/{rel_path}");
                        let mut response_data = serde_json::json!({ "uri": uri });

                        // For non-image files, include contents as base64
                        let mime = from_path(path).first_or_octet_stream();
                        if !mime.type_().as_str().eq("image") {
                            if let Ok(contents) = std::fs::read(path) {
                                let base64 = general_purpose::STANDARD.encode(&contents);
                                response_data["contents"] = serde_json::Value::String(base64);
                            }
                        }

                        let response = serde_json::json!({
                            "rpcMethod": "GET_WEBVIEW_URI",
                            "rpcId": rpc_id,
                            "data": response_data
                        });
                        let _ = ws_tx.send(Message::text(response.to_string())).await;
                    }
                    "GET_PLAYGROUND_PORT" => {
                        let playground_port =
                            session.get_session_playground_port().unwrap_or_else(|| {
                                session.baml_settings.playground_port.unwrap_or(3030)
                            });

                        tracing::info!("GET_PLAYGROUND_PORT: returning port={}", playground_port);

                        let response = serde_json::json!({
                            "rpcMethod": "GET_PLAYGROUND_PORT",
                            "rpcId": rpc_id,
                            "data": { "port": playground_port }
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
                    "OPEN_PLAYGROUND" => {
                        // Get the actual playground port from session (determined by server after availability check)
                        // Fall back to configured port if actual port not set yet
                        let port = session.get_session_playground_port().unwrap_or_else(|| {
                            session.baml_settings.playground_port.unwrap_or(3030)
                        });

                        // Construct the URL
                        let url = format!("http://localhost:{port}");

                        // Open the browser
                        let success = match webbrowser::open(&url) {
                            Ok(_) => {
                                tracing::info!("Successfully opened playground at: {}", url);
                                true
                            }
                            Err(e) => {
                                tracing::error!("Failed to open browser: {}", e);
                                false
                            }
                        };

                        let response = serde_json::json!({
                            "rpcMethod": "OPEN_PLAYGROUND",
                            "rpcId": rpc_id,
                            "data": { "success": success, "url": url }
                        });
                        let _ = ws_tx.send(Message::text(response.to_string())).await;
                    }
                    _ => {
                        tracing::warn!("Unknown RPC method: {}", rpc_method);
                    }
                }
            }
        }
    }
}
