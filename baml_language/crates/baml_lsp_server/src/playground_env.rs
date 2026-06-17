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

use bex_events::run::{
    EnvResolutionStatus, HostCallId, InMemoryRunStore, RequestCommandOutcome, RunId,
};
use bex_heap::BexHeap;
use parking_lot::Mutex;
use sys_types::{CallId, SysOpContext, SysOpOutput};
use tokio::sync::{broadcast, oneshot};

use crate::{
    playground_runs::broadcast_run_patch, playground_session::PlaygroundSessionStore,
    playground_ws::WsOutMessage,
};

const ENV_REQUEST_TIMEOUT_SECS: u64 = 120;
const ENV_REQUEST_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(ENV_REQUEST_TIMEOUT_SECS);

/// Shared state for resolving env var requests from the webview.
pub struct PlaygroundEnvState {
    pending: Mutex<HashMap<u64, PendingEnvRequest>>,
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    run_store: Arc<InMemoryRunStore>,
    session_store: Arc<PlaygroundSessionStore>,
    next_id: AtomicU64,
}

struct PendingEnvRequest {
    sender: oneshot::Sender<EnvResponse>,
    host_call_id: HostCallId,
}

struct EnvResponse {
    value: Option<String>,
    resolved_by_run_store: bool,
}

impl PlaygroundEnvState {
    pub fn new(
        broadcast_tx: broadcast::Sender<WsOutMessage>,
        run_store: Arc<InMemoryRunStore>,
        session_store: Arc<PlaygroundSessionStore>,
    ) -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            broadcast_tx,
            run_store,
            session_store,
            next_id: AtomicU64::new(1),
        }
    }

    /// Resolve a pending env var request (called by WS handler on envVarResponse).
    pub fn resolve(&self, id: u64, value: Option<String>) {
        let pending = self.pending.lock().remove(&id);
        if let Some(pending) = pending {
            let _ = pending.sender.send(EnvResponse {
                value,
                resolved_by_run_store: false,
            });
        }
    }

    /// Resolve a pending env request through the run-scoped command path.
    pub fn resolve_for_run(&self, run_id: RunId, id: u64, value: Option<String>) -> &'static str {
        let host_call_id = match self.pending.lock().get(&id) {
            Some(pending) => pending.host_call_id.clone(),
            None => {
                let outcome = self.run_store.env_request_outcome_for_run(run_id, id);
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

        let status = if value.is_some() {
            EnvResolutionStatus::ResolvedFromUser
        } else {
            EnvResolutionStatus::DeclinedMissing
        };
        let result = self
            .run_store
            .resolve_env_request_for_run(run_id, id, status, None);
        if result.outcome != RequestCommandOutcome::Accepted {
            return result.outcome.as_wire_str();
        }

        let Some(pending) = self.pending.lock().remove(&id) else {
            return self
                .run_store
                .env_request_outcome_for_run(run_id, id)
                .as_wire_str();
        };
        if let Some(patch) = result.patch {
            broadcast_run_patch(&self.broadcast_tx, &patch);
        }
        let _ = pending.sender.send(EnvResponse {
            value,
            resolved_by_run_store: true,
        });
        RequestCommandOutcome::Accepted.as_wire_str()
    }

    /// Resolve pending env waiters for a cancelled host call without adding
    /// another RunStore payload; `cancel_run` already recorded cancellation.
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
        for (_, pending) in cancelled {
            let _ = pending.sender.send(EnvResponse {
                value: None,
                resolved_by_run_store: true,
            });
        }
    }

    /// Mirror a SessionStore-owned env override into the native resolver.
    pub fn set_override(&self, key: String, value: String) {
        self.session_store.set_env_override(key, value);
    }

    /// Remove the native mirror for a SessionStore-owned env override.
    pub fn remove_override(&self, key: &str) {
        self.session_store.remove_env_override(key);
    }
}

/// `IoNamespaceEnv` implementation with three-tier resolution:
/// user overrides → process env → WebSocket roundtrip.
pub struct PlaygroundEnv(pub Arc<PlaygroundEnvState>);

impl sys_ops::io::IoNamespaceEnv for PlaygroundEnv {
    fn get(
        &self,
        _heap: &Arc<BexHeap>,
        call_id: CallId,
        key: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        let host_call_id = HostCallId::Native(call_id);
        let request_id = self.0.next_id.fetch_add(1, Ordering::Relaxed);
        if let Some(patch) =
            self.0
                .run_store
                .ingest_env_requested(&host_call_id, request_id, key.clone())
        {
            broadcast_run_patch(&self.0.broadcast_tx, &patch);
        }

        // 1. Check user overrides (manual entries from the playground dialog).
        if let Some(value) = self.0.session_store.env_override(&key) {
            if let Some(patch) = self.0.run_store.ingest_env_resolved(
                &host_call_id,
                request_id,
                key,
                EnvResolutionStatus::ResolvedFromOverride,
                None,
            ) {
                broadcast_run_patch(&self.0.broadcast_tx, &patch);
            }
            return SysOpOutput::ok(Some(value));
        }

        // 2. Check process environment (direnv, .envrc, .env, etc.).
        if let Ok(value) = std::env::var(&key) {
            // Notify the UI so it can display the shell-provided value.
            let _ = self.0.broadcast_tx.send(WsOutMessage::EnvVarFromShell {
                variable: key.clone(),
                value: value.clone(),
            });
            if let Some(patch) = self.0.run_store.ingest_env_resolved(
                &host_call_id,
                request_id,
                key,
                EnvResolutionStatus::ResolvedFromProcess,
                None,
            ) {
                broadcast_run_patch(&self.0.broadcast_tx, &patch);
            }
            return SysOpOutput::ok(Some(value));
        }

        // 3. Fall back to the WebSocket roundtrip (may show dialog in the UI).
        let state = self.0.clone();
        SysOpOutput::async_op(async move {
            let (tx, rx) = oneshot::channel();
            state.pending.lock().insert(
                request_id,
                PendingEnvRequest {
                    sender: tx,
                    host_call_id: host_call_id.clone(),
                },
            );
            let _ = state.broadcast_tx.send(WsOutMessage::EnvVarRequest {
                id: request_id,
                variable: key.clone(),
            });
            let response = match tokio::time::timeout(ENV_REQUEST_TIMEOUT, rx).await {
                Ok(Ok(response)) => response,
                Ok(Err(_)) | Err(_) => {
                    state.pending.lock().remove(&request_id);
                    EnvResponse {
                        value: None,
                        resolved_by_run_store: false,
                    }
                }
            };
            if response.resolved_by_run_store {
                return Ok(response.value);
            }
            let status = if response.value.is_some() {
                EnvResolutionStatus::ResolvedFromUser
            } else {
                EnvResolutionStatus::DeclinedMissing
            };
            if let Some(patch) =
                state
                    .run_store
                    .ingest_env_resolved(&host_call_id, request_id, key, status, None)
            {
                broadcast_run_patch(&state.broadcast_tx, &patch);
            }
            Ok(response.value)
        })
    }
}

#[cfg(test)]
mod tests {
    use bex_events::run::{ExecutionRequest, ProjectGeneration, ProjectId, RequestId, RunTarget};

    use super::*;

    #[test]
    fn run_scoped_env_response_requires_live_host_waiter() {
        let (broadcast_tx, _) = broadcast::channel(8);
        let run_store = Arc::new(InMemoryRunStore::default());
        let session_store = Arc::new(PlaygroundSessionStore::default());
        let state = PlaygroundEnvState::new(broadcast_tx, run_store.clone(), session_store);
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
            .ingest_env_requested(&host, 1, "API_KEY".to_string())
            .unwrap();

        assert_eq!(
            state.resolve_for_run(start.run_id, 1, Some("secret".to_string())),
            RequestCommandOutcome::Missing.as_wire_str()
        );
    }
}
