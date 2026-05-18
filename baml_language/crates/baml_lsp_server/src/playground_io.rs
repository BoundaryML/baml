//! Playground IO input resolution via WebSocket.
//!
//! When `baml.io.input(prompt)` is called during a playground function
//! execution, we send an `InputRequest` to all connected playground
//! clients and await the first `InputResponse`.
//!
//! If no playground client is connected (`receiver_count() == 0`),
//! we return an IO error immediately.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use bex_heap::BexHeap;
use sys_types::{CallId, OpErrorKind, SysOpContext, SysOpOutput};
use tokio::sync::{broadcast, oneshot};

use crate::playground_ws::WsOutMessage;

const INPUT_REQUEST_TIMEOUT_SECS: u64 = 300;
const INPUT_REQUEST_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(INPUT_REQUEST_TIMEOUT_SECS);

/// Shared state for resolving IO input requests from the playground UI.
pub struct PlaygroundIoState {
    pending: std::sync::Mutex<HashMap<u64, oneshot::Sender<String>>>,
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    next_id: AtomicU64,
}

impl PlaygroundIoState {
    pub fn new(broadcast_tx: broadcast::Sender<WsOutMessage>) -> Self {
        Self {
            pending: std::sync::Mutex::new(HashMap::new()),
            broadcast_tx,
            next_id: AtomicU64::new(1),
        }
    }

    /// Resolve a pending input request (called by WS handler on inputResponse).
    ///
    /// After fulfilling the oneshot, broadcasts `InputResolved` so other
    /// connected clients can dismiss the input prompt.
    pub fn resolve(&self, id: u64, call_id: u64, value: String) {
        let sender = self.pending.lock().unwrap().remove(&id);
        if let Some(sender) = sender {
            let _ = sender.send(value);
        }
        // Notify all clients that this input was resolved (so others dismiss it).
        let _ = self
            .broadcast_tx
            .send(WsOutMessage::InputResolved { id, call_id });
    }
}

/// `IoNamespaceIo` implementation that resolves `baml.io.input()` via
/// WebSocket roundtrip to the playground UI.
pub struct PlaygroundIo(pub Arc<PlaygroundIoState>);

impl sys_ops::io::IoNamespaceIo for PlaygroundIo {
    fn input(
        &self,
        _heap: &Arc<BexHeap>,
        call_id: CallId,
        prompt: Option<String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        // If no playground client is connected, return an IO error.
        if self.0.broadcast_tx.receiver_count() == 0 {
            return SysOpOutput::err(OpErrorKind::Io {
                message: "No playground connection to handle baml.io.input()".into(),
            });
        }

        let state = self.0.clone();
        let call_id_raw = call_id.0;
        SysOpOutput::async_op(async move {
            let (tx, rx) = oneshot::channel();
            let id = state.next_id.fetch_add(1, Ordering::Relaxed);
            state.pending.lock().unwrap().insert(id, tx);
            let _ = state.broadcast_tx.send(WsOutMessage::InputRequest {
                id,
                prompt,
                call_id: call_id_raw,
            });
            match tokio::time::timeout(INPUT_REQUEST_TIMEOUT, rx).await {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(_)) => {
                    // Channel closed (e.g. call cancelled) — clean up.
                    state.pending.lock().unwrap().remove(&id);
                    Err(OpErrorKind::Io {
                        message: "Input request was cancelled".into(),
                    })
                }
                Err(_) => {
                    state.pending.lock().unwrap().remove(&id);
                    Err(OpErrorKind::Timeout {
                        message: "No response to baml.io.input()".into(),
                        duration: INPUT_REQUEST_TIMEOUT,
                    })
                }
            }
        })
    }

    fn print(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        // Playground UI currently has no stdout channel; surface as Unsupported
        // so user code can `catch` and fall back.
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn println(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn eprint(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
    fn eprintln(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}
