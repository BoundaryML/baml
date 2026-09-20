//! `BexThread`: a `BexVm` plus the metadata needed to participate in
//! BEP-034 spawn / await scheduling.
//!
//! Phase A only introduces the wrapper. The engine still runs a single
//! root thread per `call_function` and there is no behavior change. Phase
//! B adds child threads and routes child completions through
//! `settles_future`.

use std::collections::HashMap;

use ::bex_heap::{Tlab, TlabHolder};
use ::bex_vm_types::{HeapPtr, RootHaver, types::FutureId};
use bex_vm::BexVm;
use tokio_util::sync::CancellationToken;

/// A BEX virtual-machine instance plus the scheduling metadata that
/// distinguishes a root call from a spawned child.
pub struct BexThread {
    pub vm: BexVm,
    pub name: Option<String>,
    pub cancel: CancellationToken,
    pub settles_future: Option<FutureId>,
    /// Durable POC: set when the run this thread belongs to has a
    /// [`crate::durable::DurableHost`]. Spawned children inherit the host.
    pub durable: Option<DurableThreadCtx>,
    /// The thread's registration with a `baml.spawn.TaskGroup`, from the
    /// spawn until the thread ends. A durable snapshot records it.
    pub group: Option<ThreadGroupSeat>,
}

/// A thread's seat in a task group.
#[derive(Clone)]
pub struct ThreadGroupSeat {
    pub group: std::sync::Arc<::bex_vm_types::TaskGroupInner>,
    /// `TaskGroupTicket::member_id` of the registration.
    pub member_id: u64,
}

/// The durable host of a thread plus the id of the thread that spawned it.
pub struct DurableThreadCtx {
    pub host: crate::durable::SharedDurableHost,
    pub parent: Option<crate::durable::DurableThreadId>,
    /// The thread whose cancel token is the parent of this thread's token:
    /// `parent`, or `None` for a root and for a `detach = true` spawn.
    pub token_parent: Option<crate::durable::DurableThreadId>,
    /// User cancel tokens (`spawn with options(cancel = ...)`) linked into
    /// this thread's token.
    pub user_cancels: Vec<std::sync::Arc<::bex_vm_types::CancelTokenData>>,
    /// Set for the threads of the durable run (the first root thread after the
    /// host was installed, and its descendants): they park at the pause gate.
    /// `None` for helper calls the embedder makes while the host is installed.
    pub(crate) pause: Option<std::sync::Arc<crate::durable::PauseController>>,
    /// The thread's place in the spawn tree of the run: `"0"` for the root,
    /// and the parent's path plus `.<n>` for the parent's n-th spawn (from 0).
    /// It is the same in every execution of the program, so identifiers that
    /// must survive a recovery from an older snapshot are derived from it.
    pub path: String,
    /// How many threads this thread has spawned.
    pub spawned: u64,
    /// How many remote calls this thread has announced.
    pub remote_calls: u64,
}

impl DurableThreadCtx {
    /// The per-thread state a snapshot keeps (`ThreadInput::engine_state`).
    /// Snapshots are native-only, and so is the JSON crate they use.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn engine_state(&self) -> Vec<u8> {
        serde_json::json!({
            "path": self.path,
            "spawned": self.spawned,
            "remote_calls": self.remote_calls,
        })
        .to_string()
        .into_bytes()
    }

    /// Reads [`Self::engine_state`] back: `(path, spawned, remote_calls)`.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn parse_engine_state(bytes: &[u8]) -> Result<(String, u64, u64), String> {
        let value: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|e| format!("the engine state of a thread is not JSON: {e}"))?;
        let number = |name: &str| {
            value
                .get(name)
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| format!("the engine state of a thread has no `{name}`"))
        };
        let path = value
            .get("path")
            .and_then(serde_json::Value::as_str)
            .filter(|path| !path.is_empty())
            .ok_or("the engine state of a thread has no `path`")?
            .to_string();
        Ok((path, number("spawned")?, number("remote_calls")?))
    }
}

impl BexThread {
    /// Build a root thread with no future to settle.
    pub fn new_root(vm: BexVm, cancel: CancellationToken) -> Self {
        Self {
            vm,
            name: None,
            cancel,
            settles_future: None,
            durable: None,
            group: None,
        }
    }

    /// Build a child thread that will settle `future_id` when its body terminates.
    pub fn new_child(
        vm: BexVm,
        cancel: CancellationToken,
        name: Option<String>,
        settles_future: FutureId,
    ) -> Self {
        Self {
            vm,
            name,
            cancel,
            settles_future: Some(settles_future),
            durable: None,
            group: None,
        }
    }

    /// The future this thread settles on termination, if it is a spawned
    /// child. Named with the `vm_thread_` prefix so the call site reads
    /// clearly through an `ActiveHeapPermit<BexThread>` deref.
    pub fn vm_thread_settles_future(&self) -> Option<FutureId> {
        self.settles_future
    }

    /// This thread's own cancellation token.
    pub fn vm_thread_cancel(&self) -> &CancellationToken {
        &self.cancel
    }
}

impl RootHaver for BexThread {
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        self.vm.collect_roots(roots);
    }

    fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
        self.vm.forward_roots(roots);
    }
}

impl TlabHolder for BexThread {
    fn tlab(&self) -> &Tlab {
        self.vm.tlab()
    }

    fn tlab_mut(&mut self) -> &mut Tlab {
        self.vm.tlab_mut()
    }
}
