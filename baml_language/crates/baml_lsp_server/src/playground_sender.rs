//! Native playground notification sender.
//!
//! Implements `bex_project::PlaygroundSender` by broadcasting serialized
//! playground notifications through a `tokio::sync::broadcast` channel
//! that WebSocket clients subscribe to.
//!
//! `OpenPlayground` is special: instead of going over WebSocket it either
//! opens the system browser for `baml playground` or sends an LSP
//! notification to the editor client.

use std::sync::Arc;

use tokio::sync::broadcast;

use crate::playground_ws::WsOutMessage;

pub struct NativePlaygroundSender {
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    playground_port: u16,
    open_in_browser: bool,
    /// Browser-mode session token; carried on the opened URL so the page can
    /// authorize its /api requests.
    session_token: Option<Arc<str>>,
}

impl NativePlaygroundSender {
    pub fn new(
        broadcast_tx: broadcast::Sender<WsOutMessage>,
        playground_port: u16,
        open_in_browser: bool,
        session_token: Option<Arc<str>>,
    ) -> Self {
        Self {
            broadcast_tx,
            playground_port,
            open_in_browser,
            session_token,
        }
    }
}

impl bex_project::PlaygroundSender for NativePlaygroundSender {
    fn lsp_playground_port(&self) -> Option<u16> {
        (!self.open_in_browser).then_some(self.playground_port)
    }

    fn has_runtime_subscribers(&self) -> bool {
        self.broadcast_tx.receiver_count() > 0
    }

    fn send_playground_notification(&self, notification: bex_project::PlaygroundNotification) {
        if matches!(
            &notification,
            bex_project::PlaygroundNotification::OpenPlayground { .. }
        ) {
            if self.open_in_browser {
                let url = match &self.session_token {
                    Some(token) => {
                        format!("http://localhost:{}/?token={token}", self.playground_port)
                    }
                    None => format!("http://localhost:{}", self.playground_port),
                };
                // `webbrowser::open` can block until a text-mode browser (lynx/w3m)
                // exits on headless hosts; never hold up server startup on it.
                std::thread::spawn(move || {
                    if let Err(e) = webbrowser::open(&url) {
                        tracing::error!("Failed to open browser at {}: {}", url, e);
                    }
                });
            }
            return;
        }

        let json = serde_json::to_value(&notification).unwrap_or_default();
        let _ = self
            .broadcast_tx
            .send(WsOutMessage::PlaygroundNotification { notification: json });
    }
}
