//! Native playground notification sender.
//!
//! Implements `bex_project::PlaygroundSender` by broadcasting serialized
//! playground notifications through a `tokio::sync::broadcast` channel
//! that WebSocket clients subscribe to.
//!
//! `OpenPlayground` is special. In editor mode it sends an LSP notification to
//! the editor client. In browser mode (`baml playground`) it navigates the
//! open playground page over the WebSocket and only spawns a system browser
//! window when no page is connected, so repeated opens reuse one window
//! instead of stacking new ones.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tokio::sync::broadcast;

use crate::{playground_notify::PlaygroundNotification, playground_ws::WsOutMessage};

/// The playground target most recently requested via `OpenPlayground`.
///
/// Browser mode records this so a page that connects *after* the request
/// (a freshly spawned window, or a reconnect) can be navigated to the same
/// target when it asks for state. See the `RequestState` handler.
#[derive(Clone, Default)]
pub struct OpenPlaygroundTarget {
    pub project: String,
    pub function_name: Option<String>,
    pub test_name: Option<String>,
    pub testset_name: Option<String>,
}

/// Shared between the sender (writer) and the WS server (replays it on
/// connect). `None` until the first `OpenPlayground` in a browser session.
pub type SharedOpenTarget = Arc<Mutex<Option<OpenPlaygroundTarget>>>;

/// A second browser window must not be spawned while the first is still
/// loading (before it connects and bumps `receiver_count`).
const BROWSER_SPAWN_DEBOUNCE: Duration = Duration::from_secs(5);

pub struct NativePlaygroundSender {
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    lsp_sender: Arc<dyn baml_lsp::ClientSender>,
    playground_port: u16,
    open_in_browser: bool,
    /// Last requested open target, shared with the WS server for replay.
    current_open_target: SharedOpenTarget,
    /// When we last spawned a browser window, to debounce double-opens.
    last_browser_open: Mutex<Option<Instant>>,
}

impl NativePlaygroundSender {
    pub fn new(
        broadcast_tx: broadcast::Sender<WsOutMessage>,
        lsp_sender: Arc<dyn baml_lsp::ClientSender>,
        playground_port: u16,
        open_in_browser: bool,
        current_open_target: SharedOpenTarget,
    ) -> Self {
        Self {
            broadcast_tx,
            lsp_sender,
            playground_port,
            open_in_browser,
            current_open_target,
            last_browser_open: Mutex::new(None),
        }
    }

    /// Returns true if this call should spawn a browser window. Debounces
    /// rapid `OpenPlayground`s that arrive before the first window connects.
    fn claim_browser_open(&self) -> bool {
        let mut guard = self.last_browser_open.lock().unwrap();
        let now = Instant::now();
        if let Some(prev) = *guard
            && now.duration_since(prev) < BROWSER_SPAWN_DEBOUNCE
        {
            return false;
        }
        *guard = Some(now);
        true
    }
}

impl NativePlaygroundSender {
    /// Push one notification: `openPlayground` navigates (browser) or goes to
    /// the editor as `baml/openPlayground` (editor mode); `listProjects` is
    /// mirrored to the editor for its status-bar links; everything else is
    /// broadcast to connected playground pages.
    pub fn send_playground_notification(&self, notification: &PlaygroundNotification) {
        if let PlaygroundNotification::OpenPlayground {
            ref project,
            ref function_name,
            ref test_name,
            ref testset_name,
        } = *notification
        {
            if self.open_in_browser {
                // Remember where to navigate so a page that connects after this
                // request (fresh window or reconnect) can be sent here on
                // `RequestState`.
                *self.current_open_target.lock().unwrap() = Some(OpenPlaygroundTarget {
                    project: project.clone(),
                    function_name: function_name.clone(),
                    test_name: test_name.clone(),
                    testset_name: testset_name.clone(),
                });

                // Navigate any already-open page in place rather than spawning a
                // new window: pages listen on this broadcast and react to the
                // `openPlayground` notification.
                let json = serde_json::to_value(notification).unwrap_or_default();
                let _ = self
                    .broadcast_tx
                    .send(WsOutMessage::PlaygroundNotification { notification: json });

                // Only open a browser window when nothing is connected. The
                // debounce avoids a second window while the first is still
                // loading (before it connects and bumps `receiver_count`).
                if self.broadcast_tx.receiver_count() == 0 && self.claim_browser_open() {
                    let url = format!("http://localhost:{}", self.playground_port);
                    // `webbrowser::open` can block until a text-mode browser
                    // (lynx/w3m) exits on headless hosts; never hold up the
                    // server on it.
                    std::thread::spawn(move || {
                        if let Err(e) = webbrowser::open(&url) {
                            tracing::error!("Failed to open browser at {}: {}", url, e);
                        }
                    });
                }
            } else {
                let params = serde_json::json!({
                    "port": self.playground_port,
                    "projectPath": project,
                    "functionName": function_name,
                    "testName": test_name,
                    "testsetName": testset_name,
                });
                if let Err(e) = self
                    .lsp_sender
                    .send_notification("baml/openPlayground", params)
                {
                    tracing::error!("Failed to send baml/openPlayground notification: {}", e);
                }
            }
            return;
        }

        // Forward project list to the LSP client so the extension can
        // show per-project playground links in the status bar tooltip.
        if let PlaygroundNotification::ListProjects { ref projects } = *notification {
            let params = serde_json::json!({ "projects": projects });
            if let Err(e) = self
                .lsp_sender
                .send_notification("baml/listProjects", params)
            {
                tracing::error!("Failed to send baml/listProjects notification: {}", e);
            }
        }

        let json = serde_json::to_value(notification).unwrap_or_default();
        let _ = self
            .broadcast_tx
            .send(WsOutMessage::PlaygroundNotification { notification: json });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopLspSender;
    impl baml_lsp::ClientSender for NoopLspSender {
        fn send_notification(
            &self,
            _: &str,
            _: serde_json::Value,
        ) -> Result<(), baml_lsp::LspError> {
            Ok(())
        }
    }

    fn browser_sender(
        broadcast_tx: broadcast::Sender<WsOutMessage>,
        target: SharedOpenTarget,
    ) -> NativePlaygroundSender {
        NativePlaygroundSender::new(
            broadcast_tx,
            Arc::new(NoopLspSender),
            4265,
            true, // browser mode
            target,
        )
    }

    /// Browser-mode `OpenPlayground` navigates the already-open page in place and
    /// records the target for replay — the core of B-808. Holding a live
    /// receiver keeps `receiver_count() > 0`, which both exercises the reuse
    /// path and guarantees the test never launches a real browser.
    #[test]
    fn browser_open_playground_broadcasts_in_place_and_records_target() {
        let (tx, mut rx) = broadcast::channel(8);
        let target: SharedOpenTarget = Arc::new(Mutex::new(None));
        let sender = browser_sender(tx, target.clone());

        sender.send_playground_notification(&PlaygroundNotification::OpenPlayground {
            project: "/tmp/proj".to_string(),
            function_name: Some("Foo".to_string()),
            test_name: None,
            testset_name: None,
        });

        let WsOutMessage::PlaygroundNotification { notification } = rx
            .try_recv()
            .expect("open should broadcast to the connected page")
        else {
            panic!("expected a playground notification");
        };
        assert_eq!(notification["type"], "openPlayground");
        assert_eq!(notification["project"], "/tmp/proj");
        assert_eq!(notification["functionName"], "Foo");

        let recorded = target
            .lock()
            .unwrap()
            .clone()
            .expect("target recorded for RequestState replay");
        assert_eq!(recorded.project, "/tmp/proj");
        assert_eq!(recorded.function_name.as_deref(), Some("Foo"));
    }

    /// Two `OpenPlayground`s arriving before the first window connects must not
    /// spawn two browser windows.
    #[test]
    fn claim_browser_open_debounces_rapid_spawns() {
        let (tx, _rx) = broadcast::channel(8);
        let sender = browser_sender(tx, Arc::new(Mutex::new(None)));
        assert!(sender.claim_browser_open(), "first spawn allowed");
        assert!(
            !sender.claim_browser_open(),
            "second spawn within debounce suppressed"
        );
    }
}
