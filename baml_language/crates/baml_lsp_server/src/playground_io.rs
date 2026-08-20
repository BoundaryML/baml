//! Playground `baml.io` host: input resolution and stream output.
//!
//! When `baml.io.input(prompt)` is called during a playground function
//! execution, we send an `InputRequest` to all connected playground
//! clients and await the first `InputResponse`.
//!
//! If no playground client is connected (`receiver_count() == 0`),
//! we return an IO error immediately.
//!
//! `print` / `println` / `eprint` / `eprintln` become `Output` payloads on the
//! run, streamed to clients over the same channel. Process stdout is never
//! written: under `baml lsp` it carries the JSON-RPC frames, and under
//! `baml playground` the user is watching a browser tab, not the terminal.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use bex_events::run::{
    BoundaryId, HostCallId, InMemoryRunStore, OutputStream, RequestCommandOutcome, RunRequestState,
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
    pub fn resolve_for_run(&self, boundary_id: BoundaryId, id: u64, value: String) -> &'static str {
        let host_call_id = match self.pending.lock().get(&id) {
            Some(pending) => pending.host_call_id.clone(),
            None => {
                let outcome = self
                    .run_store
                    .input_request_outcome_for_run(boundary_id, id);
                return if outcome == RequestCommandOutcome::Accepted {
                    RequestCommandOutcome::Missing.as_wire_str()
                } else {
                    outcome.as_wire_str()
                };
            }
        };
        if self.run_store.boundary_id_for_host_call(&host_call_id) != Some(boundary_id) {
            return RequestCommandOutcome::RejectedStale.as_wire_str();
        }

        let result = self.run_store.resolve_input_request_for_run(
            boundary_id,
            id,
            RunRequestState::Resolved,
        );
        if result.outcome != RequestCommandOutcome::Accepted {
            return result.outcome.as_wire_str();
        }

        let Some(pending) = self.pending.lock().remove(&id) else {
            return self
                .run_store
                .input_request_outcome_for_run(boundary_id, id)
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

    /// Record a `baml.io` stream write and push it to connected clients.
    ///
    /// Process stdout is never touched. Under `baml lsp` it carries the
    /// JSON-RPC frames, so a stray byte desynchronizes the client; under
    /// `baml playground` the user is looking at a browser tab, not the
    /// terminal.
    ///
    /// A write that cannot be attributed to a live run is dropped rather than
    /// failed. Panicking a program over an unroutable debug print costs more
    /// than the lost line.
    pub fn write_output(&self, call_id: CallId, stream: OutputStream, text: String) {
        if text.is_empty() {
            return;
        }
        let host_call_id = HostCallId::Native(call_id);
        if let Some(patch) = self.run_store.ingest_output(&host_call_id, stream, text) {
            broadcast_run_patch(&self.broadcast_tx, &patch);
        }
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
        call_id: CallId,
        s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        self.0.write_output(call_id, OutputStream::Stdout, s);
        SysOpOutput::ok(())
    }
    fn println(
        &self,
        _heap: &Arc<BexHeap>,
        call_id: CallId,
        mut s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        s.push('\n');
        self.0.write_output(call_id, OutputStream::Stdout, s);
        SysOpOutput::ok(())
    }
    fn eprint(
        &self,
        _heap: &Arc<BexHeap>,
        call_id: CallId,
        s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        self.0.write_output(call_id, OutputStream::Stderr, s);
        SysOpOutput::ok(())
    }
    fn eprintln(
        &self,
        _heap: &Arc<BexHeap>,
        call_id: CallId,
        mut s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        s.push('\n');
        self.0.write_output(call_id, OutputStream::Stderr, s);
        SysOpOutput::ok(())
    }
}

#[cfg(test)]
mod tests {
    use bex_events::run::{
        BoundaryId, ExecutionRequest, ProjectGeneration, ProjectId, RequestId, RunTarget,
    };

    use super::*;

    #[test]
    fn run_scoped_input_response_requires_live_host_waiter() {
        let (broadcast_tx, _) = broadcast::channel(8);
        let run_store = Arc::new(InMemoryRunStore::default());
        let state = PlaygroundIoState::new(broadcast_tx, run_store.clone());
        let start = run_store.create_run(
            BoundaryId::new_random(),
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
        run_store.attach_host_call(start.boundary_id, host.clone());
        run_store
            .ingest_input_requested(&host, 1, Some("name?".to_string()))
            .unwrap();

        assert_eq!(
            state.resolve_for_run(start.boundary_id, 1, "answer".to_string()),
            RequestCommandOutcome::Missing.as_wire_str()
        );
    }

    fn start_run(run_store: &InMemoryRunStore, call_id: u64) -> (BoundaryId, HostCallId) {
        let start = run_store.create_run(
            BoundaryId::new_random(),
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
        let host = HostCallId::Native(CallId(call_id));
        run_store.attach_host_call(start.boundary_id, host.clone());
        (start.boundary_id, host)
    }

    fn output_payloads(
        run_store: &InMemoryRunStore,
        boundary_id: BoundaryId,
    ) -> Vec<(String, String)> {
        run_store
            .snapshot(boundary_id)
            .expect("run snapshot")
            .payloads
            .iter()
            .filter_map(|payload| match &payload.kind {
                bex_events::run::PayloadKind::Output(output) => {
                    Some((output.stream.as_wire_str().to_string(), output.text.clone()))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn print_writes_are_recorded_verbatim_on_the_run() {
        let (broadcast_tx, _rx) = broadcast::channel(8);
        let run_store = Arc::new(InMemoryRunStore::default());
        let state = Arc::new(PlaygroundIoState::new(broadcast_tx, run_store.clone()));
        let (boundary_id, _host) = start_run(&run_store, 7);

        // ANSI escapes must survive untouched, including a sequence split
        // across two calls the way a `print`-per-token program emits them.
        state.write_output(CallId(7), OutputStream::Stdout, "\u{1b}[3".to_string());
        state.write_output(
            CallId(7),
            OutputStream::Stdout,
            "1mred\u{1b}[0m".to_string(),
        );
        state.write_output(CallId(7), OutputStream::Stderr, "boom\n".to_string());

        assert_eq!(
            output_payloads(&run_store, boundary_id),
            vec![
                ("stdout".to_string(), "\u{1b}[3".to_string()),
                ("stdout".to_string(), "1mred\u{1b}[0m".to_string()),
                ("stderr".to_string(), "boom\n".to_string()),
            ]
        );
    }

    #[test]
    fn print_from_an_unattached_call_is_dropped_not_failed() {
        let (broadcast_tx, _rx) = broadcast::channel(8);
        let run_store = Arc::new(InMemoryRunStore::default());
        let state = Arc::new(PlaygroundIoState::new(broadcast_tx, run_store.clone()));
        let (boundary_id, _host) = start_run(&run_store, 7);

        // A call id that belongs to no run: the write disappears rather than
        // taking the program down with it.
        state.write_output(CallId(999), OutputStream::Stdout, "orphan".to_string());

        assert!(output_payloads(&run_store, boundary_id).is_empty());
    }
}
