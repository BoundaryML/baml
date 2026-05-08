//! Playground env var resolution via WebSocket.
//!
//! Resolution order:
//! 1. User overrides (manual entries from the playground UI)
//! 2. Process environment (`std::env::var`) — e.g. from direnv / .envrc
//! 3. WebSocket roundtrip to the webview (shows dialog if needed)

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
    /// User overrides set from the playground UI (take priority over process env).
    overrides: std::sync::Mutex<HashMap<String, String>>,
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    next_id: AtomicU64,
}

impl PlaygroundEnvState {
    pub fn new(broadcast_tx: broadcast::Sender<WsOutMessage>) -> Self {
        Self {
            pending: std::sync::Mutex::new(HashMap::new()),
            overrides: std::sync::Mutex::new(HashMap::new()),
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

    /// Store a user override from the playground UI.
    pub fn set_override(&self, key: String, value: String) {
        self.overrides.lock().unwrap().insert(key, value);
    }

    /// Remove a user override (reverts to process env / WS fallback).
    pub fn remove_override(&self, key: &str) {
        self.overrides.lock().unwrap().remove(key);
    }
}

/// `IoNamespaceEnv` implementation with three-tier resolution:
/// user overrides → process env → WebSocket roundtrip.
pub struct PlaygroundEnv(pub Arc<PlaygroundEnvState>);

impl sys_ops::io::IoNamespaceEnv for PlaygroundEnv {
    fn get(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        key: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        // 1. Check user overrides (manual entries from the playground dialog).
        if let Some(value) = self.0.overrides.lock().unwrap().get(&key).cloned() {
            return SysOpOutput::ok(Some(value));
        }

        // 2. Check process environment (direnv, .envrc, .env, etc.).
        if let Ok(value) = std::env::var(&key) {
            // Notify the UI so it can display the shell-provided value.
            let _ = self.0.broadcast_tx.send(WsOutMessage::EnvVarFromShell {
                variable: key,
                value: value.clone(),
            });
            return SysOpOutput::ok(Some(value));
        }

        // 3. Fall back to the WebSocket roundtrip (may show dialog in the UI).
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
