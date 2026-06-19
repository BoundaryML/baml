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

use bex_events::run::{
    HostCallId, InMemoryRunStore, RequestCommandOutcome, RunId, RunRequestState,
};
use bex_heap::BexHeap;
use parking_lot::Mutex;
use sys_types::{CallId, SysOpContext, SysOpOutput, VmBamlError, VmRustFnError};
use tokio::sync::{broadcast, oneshot};

use crate::{playground_runs::broadcast_run_patch, playground_ws::WsOutMessage};

const INPUT_REQUEST_TIMEOUT_SECS: u64 = 300;
const INPUT_REQUEST_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(INPUT_REQUEST_TIMEOUT_SECS);

/// Shared state for resolving IO input requests from the playground UI.
pub struct PlaygroundIoState {
    pending: Mutex<HashMap<u64, PendingInputRequest>>,
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    run_store: Arc<InMemoryRunStore>,
    next_id: AtomicU64,
}

struct PendingInputRequest {
    sender: oneshot::Sender<String>,
    host_call_id: HostCallId,
}

impl PlaygroundIoState {
    pub fn new(
        broadcast_tx: broadcast::Sender<WsOutMessage>,
        run_store: Arc<InMemoryRunStore>,
    ) -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            broadcast_tx,
            run_store,
            next_id: AtomicU64::new(1),
        }
    }

    /// Resolve a pending input request (called by WS handler on inputResponse).
    ///
    /// After fulfilling the oneshot, broadcasts `InputResolved` so other
    /// connected clients can dismiss the input prompt.
    pub fn resolve(&self, id: u64, call_id: u64, value: String) {
        let pending = self.pending.lock().remove(&id);
        let host_call_id = pending
            .as_ref()
            .map(|pending| pending.host_call_id.clone())
            .unwrap_or(HostCallId::Native(CallId(call_id)));
        if let Some(pending) = pending {
            let _ = pending.sender.send(value);
        }
        if let Some(patch) =
            self.run_store
                .ingest_input_resolved(&host_call_id, id, RunRequestState::Resolved)
        {
            broadcast_run_patch(&self.broadcast_tx, &patch);
        }
        // Notify all clients that this input was resolved (so others dismiss it).
        let _ = self
            .broadcast_tx
            .send(WsOutMessage::InputResolved { id, call_id });
    }

    /// Resolve a pending input request through the run-scoped command path.
    pub fn resolve_for_run(&self, run_id: RunId, id: u64, value: String) -> &'static str {
        let host_call_id = match self.pending.lock().get(&id) {
            Some(pending) => pending.host_call_id.clone(),
            None => {
                let outcome = self.run_store.input_request_outcome_for_run(run_id, id);
                return if outcome == RequestCommandOutcome::Accepted {
                    RequestCommandOutcome::Missing.as_wire_str()
                } else {
                    outcome.as_wire_str()
                };
            }
        };
        if self.run_store.run_id_for_host_call(&host_call_id) != Some(run_id) {
            return RequestCommandOutcome::RejectedStale.as_wire_str();
        }

        let result =
            self.run_store
                .resolve_input_request_for_run(run_id, id, RunRequestState::Resolved);
        if result.outcome != RequestCommandOutcome::Accepted {
            return result.outcome.as_wire_str();
        }

        let Some(pending) = self.pending.lock().remove(&id) else {
            return self
                .run_store
                .input_request_outcome_for_run(run_id, id)
                .as_wire_str();
        };
        let call_id = match &pending.host_call_id {
            HostCallId::Native(call_id) => call_id.0,
            _ => 0,
        };
        let _ = pending.sender.send(value);
        if let Some(patch) = result.patch {
            broadcast_run_patch(&self.broadcast_tx, &patch);
        }
        let _ = self
            .broadcast_tx
            .send(WsOutMessage::InputResolved { id, call_id });
        RequestCommandOutcome::Accepted.as_wire_str()
    }

    /// Drop pending input waiters for a cancelled host call and notify
    /// connected clients to dismiss legacy prompt UI.
    pub fn cancel_for_host_call(&self, host_call_id: &HostCallId) {
        let cancelled = {
            let mut pending = self.pending.lock();
            let ids = pending
                .iter()
                .filter_map(|(id, pending)| (&pending.host_call_id == host_call_id).then_some(*id))
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|id| pending.remove(&id).map(|pending| (id, pending)))
                .collect::<Vec<_>>()
        };
        for (id, pending) in cancelled {
            let call_id = match pending.host_call_id {
                HostCallId::Native(call_id) => call_id.0,
                _ => 0,
            };
            drop(pending.sender);
            let _ = self
                .broadcast_tx
                .send(WsOutMessage::InputResolved { id, call_id });
        }
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
            return SysOpOutput::err(VmBamlError::Io {
                message: "No playground connection to handle baml.io.input()".into(),
            });
        }

        let state = self.0.clone();
        let call_id_raw = call_id.0;
        SysOpOutput::async_op(async move {
            let (tx, rx) = oneshot::channel();
            let id = state.next_id.fetch_add(1, Ordering::Relaxed);
            let host_call_id = HostCallId::Native(call_id);
            state.pending.lock().insert(
                id,
                PendingInputRequest {
                    sender: tx,
                    host_call_id: host_call_id.clone(),
                },
            );
            if let Some(patch) =
                state
                    .run_store
                    .ingest_input_requested(&host_call_id, id, prompt.clone())
            {
                broadcast_run_patch(&state.broadcast_tx, &patch);
            }
            let _ = state.broadcast_tx.send(WsOutMessage::InputRequest {
                id,
                prompt,
                call_id: call_id_raw,
            });
            match tokio::time::timeout(INPUT_REQUEST_TIMEOUT, rx).await {
                Ok(Ok(value)) => Ok(value),
                Ok(Err(_)) => {
                    // Channel closed (e.g. call cancelled) — clean up.
                    if let Some(pending) = state.pending.lock().remove(&id)
                        && let Some(patch) = state.run_store.ingest_input_resolved(
                            &pending.host_call_id,
                            id,
                            RunRequestState::Cancelled,
                        )
                    {
                        broadcast_run_patch(&state.broadcast_tx, &patch);
                    }
                    Err(VmRustFnError::from(VmBamlError::Io {
                        message: "Input request was cancelled".into(),
                    }))
                }
                Err(_) => {
                    if let Some(pending) = state.pending.lock().remove(&id)
                        && let Some(patch) = state.run_store.ingest_input_resolved(
                            &pending.host_call_id,
                            id,
                            RunRequestState::Expired,
                        )
                    {
                        broadcast_run_patch(&state.broadcast_tx, &patch);
                    }
                    // `INPUT_REQUEST_TIMEOUT_SECS` is a hard-coded
                    // constant (300s); the `i64::try_from` cannot fail in
                    // practice, but stays as a `Some` so a future bump to
                    // a much larger timeout surfaces the overflow loudly.
                    let duration_ms = i64::try_from(INPUT_REQUEST_TIMEOUT.as_millis())
                        .expect("INPUT_REQUEST_TIMEOUT must fit in i64 ms");
                    Err(VmRustFnError::from(VmBamlError::Timeout {
                        message: "No response to baml.io.input()".into(),
                        duration_ms: Some(duration_ms),
                    }))
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
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn println(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn eprint(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn eprintln(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use bex_events::run::{ExecutionRequest, ProjectGeneration, ProjectId, RequestId, RunTarget};

    use super::*;

    #[test]
    fn run_scoped_input_response_requires_live_host_waiter() {
        let (broadcast_tx, _) = broadcast::channel(8);
        let run_store = Arc::new(InMemoryRunStore::default());
        let state = PlaygroundIoState::new(broadcast_tx, run_store.clone());
        let start = run_store.create_run(
            ExecutionRequest {
                project_id: ProjectId("project".to_string()),
                project_generation: ProjectGeneration(1),
                target: RunTarget::Function {
                    function_name: "main".to_string(),
                },
                args_summary: None,
                options_summary: None,
            },
            RequestId(1),
        );
        let host = HostCallId::Native(CallId(42));
        run_store.attach_host_call(start.run_id, host.clone());
        run_store
            .ingest_input_requested(&host, 1, Some("name?".to_string()))
            .unwrap();

        assert_eq!(
            state.resolve_for_run(start.run_id, 1, "answer".to_string()),
            RequestCommandOutcome::Missing.as_wire_str()
        );
    }
}
