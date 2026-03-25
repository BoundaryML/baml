//! Playground env var resolution via WebSocket.
//!
//! When the playground needs an env var, it broadcasts an `EnvVarRequest`
//! to all connected WebSocket clients. The first client to respond resolves
//! the pending oneshot.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use bex_heap::BexHeap;
use sys_types::{CallId, SysOpContext, SysOpOutput};
use tokio::sync::{broadcast, oneshot};

use crate::playground_ws::WsOutMessage;

const ENV_REQUEST_TIMEOUT_SECS: u64 = 120;
const ENV_REQUEST_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(ENV_REQUEST_TIMEOUT_SECS);

/// Shared state for resolving env var requests from the webview.
pub struct PlaygroundEnvState {
    pending: std::sync::Mutex<HashMap<u64, oneshot::Sender<Option<String>>>>,
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    next_id: AtomicU64,
}

impl PlaygroundEnvState {
    pub fn new(broadcast_tx: broadcast::Sender<WsOutMessage>) -> Self {
        Self {
            pending: std::sync::Mutex::new(HashMap::new()),
            broadcast_tx,
            next_id: AtomicU64::new(1),
        }
    }

    /// Resolve a pending env var request (called by WS handler on envVarResponse).
    pub fn resolve(&self, id: u64, value: Option<String>) {
        let sender = self.pending.lock().unwrap().remove(&id);
        if let Some(sender) = sender {
            let _ = sender.send(value);
        }
    }
}

/// `IoNamespaceEnv` implementation that asks the webview for every env var.
pub struct PlaygroundEnv(pub Arc<PlaygroundEnvState>);

impl sys_types::io::IoNamespaceEnv for PlaygroundEnv {
    fn get(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        key: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        let state = self.0.clone();
        SysOpOutput::async_op(async move {
            let (tx, rx) = oneshot::channel();
            let id = state.next_id.fetch_add(1, Ordering::Relaxed);
            state.pending.lock().unwrap().insert(id, tx);
            let _ = state
                .broadcast_tx
                .send(WsOutMessage::EnvVarRequest { id, variable: key });
            let value: Option<String> = match tokio::time::timeout(ENV_REQUEST_TIMEOUT, rx).await {
                Ok(Ok(value)) => value,
                Ok(Err(_)) | Err(_) => {
                    state.pending.lock().unwrap().remove(&id);
                    None
                }
            };
            Ok(value)
        })
    }
}
