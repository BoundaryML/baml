//! Native playground notification sender.
//!
//! Implements `bex_project::PlaygroundSender` by broadcasting serialized
//! playground notifications through a `tokio::sync::broadcast` channel
//! that WebSocket clients subscribe to.
//!
//! `OpenPlayground` is special: instead of going over WebSocket it either
//! opens the system browser or sends an LSP notification to the client.

use std::sync::Arc;

use tokio::sync::broadcast;

use crate::playground_ws::WsOutMessage;

pub struct NativePlaygroundSender {
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    lsp_sender: Arc<dyn bex_project::LspClientSenderTrait + Send + Sync>,
    playground_port: u16,
    playground_via_browser: bool,
}

impl NativePlaygroundSender {
    pub fn new(
        broadcast_tx: broadcast::Sender<WsOutMessage>,
        lsp_sender: Arc<dyn bex_project::LspClientSenderTrait + Send + Sync>,
        playground_port: u16,
        playground_via_browser: bool,
    ) -> Self {
        Self {
            broadcast_tx,
            lsp_sender,
            playground_port,
            playground_via_browser,
        }
    }
}

impl bex_project::PlaygroundSender for NativePlaygroundSender {
    fn send_playground_notification(&self, notification: bex_project::PlaygroundNotification) {
        if let bex_project::PlaygroundNotification::OpenPlayground {
            ref project,
            ref function_name,
        } = notification
        {
            if self.playground_via_browser {
                let url = format!("http://localhost:{}", self.playground_port);
                if let Err(e) = webbrowser::open(&url) {
                    tracing::error!("Failed to open browser at {}: {}", url, e);
                }
            } else {
                let params = serde_json::json!({
                    "port": self.playground_port,
                    "projectPath": project,
                    "functionName": function_name,
                });
                let notif =
                    lsp_server::Notification::new("baml/openPlayground".to_string(), params);
                if let Err(e) = self.lsp_sender.send_notification(notif) {
                    tracing::error!("Failed to send baml/openPlayground notification: {}", e);
                }
            }
            return;
        }

        // Forward project list to the LSP client so the extension can
        // show per-project playground links in the status bar tooltip.
        if let bex_project::PlaygroundNotification::ListProjects { ref projects } = notification {
            let params = serde_json::json!({ "projects": projects });
            let notif = lsp_server::Notification::new("baml/listProjects".to_string(), params);
            if let Err(e) = self.lsp_sender.send_notification(notif) {
                tracing::error!("Failed to send baml/listProjects notification: {}", e);
            }
        }

        let json = serde_json::to_value(&notification).unwrap_or_default();
        let _ = self
            .broadcast_tx
            .send(WsOutMessage::PlaygroundNotification { notification: json });
    }
}
