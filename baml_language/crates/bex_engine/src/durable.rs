//! Durable functions proof of concept: the embedder-facing hooks.
//!
//! An embedder (the `baml-cli worker` subcommand) installs a [`DurableHost`]
//! on a [`crate::BexEngine`] with [`crate::BexEngine::set_durable_host`].
//! Every root thread created while a host is installed, and every thread
//! spawned from such a thread, then:
//!
//! - reports its lifecycle through [`DurableHost::thread_started`] and
//!   [`DurableHost::thread_ended`],
//! - reports every VM yield through [`DurableHost::yielded`], with the
//!   position of the top user frame,
//! - hands calls to `remote_` user functions to [`DurableHost::remote_call`]
//!   instead of executing the callee (the VM yields
//!   `VmExecState::RemoteCall`; see `BexVm::remote_intercept`).
//!
//! The first root thread created after the host was installed, and every
//! thread spawned from it, form *the run*. The run can be paused into a
//! snapshot with `BexEngine::durable_snapshot` and continued in another
//! process with `BexEngine::durable_resume` (see the second half of this
//! file). Later root threads (helper calls of the embedder) get the
//! notifications and the remote intercept but are never paused.
//!
//! Without a host the engine behaves exactly as before. The per-yield cost of
//! the feature when off is one `Option` check on the thread.

use std::sync::Arc;

use bex_external_types::{BexExternalValue, RuntimeTy};
use futures::future::BoxFuture;

/// Identifies a BAML thread inside one engine. This is the VM's
/// `prof_thread_id`, minted per root call and per spawn.
pub type DurableThreadId = u64;

/// Why a thread handed control back to the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YieldReason {
    /// The thread is about to run a sys-op.
    SysOp,
    /// The thread is about to wait for one or more futures.
    Await,
    /// The thread yielded cooperatively (GC or park request).
    EarlyYield,
    /// The thread reached a call to a `remote_` function.
    RemoteCall,
}

impl YieldReason {
    /// The spelling used by the worker protocol.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SysOp => "sysop",
            Self::Await => "await",
            Self::EarlyYield => "early_yield",
            Self::RemoteCall => "remote_call",
        }
    }
}

/// Source position of the innermost frame that is not part of the standard
/// library.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YieldPosition {
    /// Function name as the VM records it, for example `user.durable_plan_trip`.
    pub function: String,
    /// Source file path as recorded at compile time.
    pub file: String,
    /// 1-based source line.
    pub line: usize,
}

/// One argument of a [`RemoteCallRequest`].
#[derive(Clone, Debug)]
pub struct RemoteCallArg {
    /// Parameter name.
    pub name: String,
    /// Declared parameter type.
    pub ty: RuntimeTy,
    /// The argument, copied out of the heap.
    pub value: BexExternalValue,
}

/// A call to a `remote_` function that the embedder must execute elsewhere.
#[derive(Clone, Debug)]
pub struct RemoteCallRequest {
    /// The thread that made the call and waits for the result.
    pub thread: DurableThreadId,
    /// Function name as the VM records it, for example
    /// `user.remote_fetch_weather`.
    pub function: String,
    /// Arguments in declaration order. Parameters the caller left to their
    /// default are absent.
    pub args: Vec<RemoteCallArg>,
    /// Declared return type of the callee. The engine converts the result
    /// against this type.
    pub return_type: RuntimeTy,
}

/// A thread that waits for the result of a remote call.
#[derive(Clone, Debug)]
pub struct RemoteResultWait {
    /// The identifier [`DurableHost::remote_call`] returned, possibly in an
    /// earlier process.
    pub call_id: String,
    /// The waiting thread (its id in this process).
    pub thread: DurableThreadId,
    /// Callee name as the VM records it.
    pub function: String,
    /// Declared return type of the callee.
    pub return_type: RuntimeTy,
}

/// Hooks an embedder installs to observe a run and to service `remote_` calls.
///
/// The notification methods are synchronous and are called on the engine's
/// executor threads while the reporting BAML thread holds its heap permit.
/// They must not block and must not call back into the engine.
pub trait DurableHost: Send + Sync + 'static {
    /// Announce `request` so that it is executed elsewhere, and resolve with
    /// the identifier of the call (or with an error message that the engine
    /// throws into the calling BAML thread).
    ///
    /// The engine releases the calling thread's heap permit while the returned
    /// future is pending and drops the future if the run is cancelled. A pause
    /// waits for this future: once it has resolved, the call is known outside
    /// the process, so the wait for its result can be re-issued after a resume.
    fn remote_call(&self, request: RemoteCallRequest)
    -> BoxFuture<'static, Result<String, String>>;

    /// Resolve with the result of the remote call `wait.call_id`, or with an
    /// error message that the engine throws into the waiting BAML thread.
    ///
    /// Called right after [`Self::remote_call`] resolves, and again by a
    /// resumed run whose snapshot was taken during the wait. The engine may
    /// stop polling the future while a snapshot is attempted and drops it when
    /// the run is suspended or cancelled; a result that the future has not
    /// returned yet must stay available to a later call for the same id.
    /// Because the future can sit unpolled for a while, it must not hold
    /// engine resources (a helper engine call in progress holds a heap permit)
    /// across its await points: spawn such work as its own task.
    fn remote_result(
        &self,
        wait: RemoteResultWait,
    ) -> BoxFuture<'static, Result<BexExternalValue, String>>;

    /// A thread is about to execute. `parent` is `None` for a root thread.
    fn thread_started(&self, thread: DurableThreadId, parent: Option<DurableThreadId>);

    /// A thread finished, on every exit path. A spawned thread that was
    /// cancelled before its body ran reports neither start nor end.
    fn thread_ended(&self, thread: DurableThreadId);

    /// A thread yielded. `op` is the sys-op path (for example
    /// `baml.sys.sleep`) when `reason` is [`YieldReason::SysOp`], and the
    /// callee name when it is [`YieldReason::RemoteCall`]. `position` is
    /// `None` when no user frame is on the stack.
    ///
    /// For a sys-op the engine calls this immediately before it dispatches the
    /// operation, on the same OS thread and with no await point in between, so
    /// an IO provider can attribute `baml.io.print*` output to `thread`.
    fn yielded(
        &self,
        thread: DurableThreadId,
        reason: YieldReason,
        op: Option<&str>,
        position: Option<&YieldPosition>,
    );
}

/// Shared handle to an installed host.
pub type SharedDurableHost = Arc<dyn DurableHost>;

// ============================================================================
// Pause, snapshot, and resume
// ============================================================================
//
// A run is paused in three steps.
//
// 1. The coordinator ([`crate::BexEngine::durable_snapshot`]) opens the pause
//    gate and sets the engine-wide `park_requested` flag, so that a VM in a
//    compute loop returns `EarlyYield` promptly.
// 2. Every thread of the run parks at the gate. The gate is checked at the top
//    of the engine loop, before each `exec()`. A thread that waits in a
//    re-issuable operation (`baml.sys.sleep`, the wait for a remote result, an
//    `await`) is woken by the gate and parks too, recording how to resume. A
//    parked thread hands its *released* heap permit to the coordinator and
//    waits for a reply.
// 3. When every live thread is parked the coordinator takes the exclusive
//    [`bex_heap::HeapGuard`] (the same stop-the-world barrier the collector
//    uses), so no mutator runs and no collection can move objects while it
//    reads the VMs, and writes the snapshot and the state dump.
//
// Parked threads hold no active permit on purpose. Tokio's semaphore is fair:
// a thread that held an active permit at the gate would block a queued
// collection, and the queued collection would block every other thread's
// `acquire()`, so the run could never finish parking.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};

use bex_heap::InactiveHeapPermit;

use crate::BexThread;

/// How a parked thread resumes. Stored in a snapshot as
/// `bex_snapshot::ParkedAt` (kind plus a JSON payload).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParkedKind {
    /// Call `exec()`.
    Runnable,
    /// Inside `baml.sys.sleep`: sleep until the deadline, push `null`, `exec()`.
    Sleep {
        /// Absolute wall-clock deadline in Unix milliseconds.
        deadline_unix_ms: u64,
    },
    /// Waiting for the result of a remote call: wait for the result of
    /// `call_id`, convert it against the callee's declared return type, push
    /// it (or inject the throw), `exec()`.
    RemoteCall {
        /// The host's identifier of the call.
        call_id: String,
        /// Callee name as the VM records it (`user.remote_fetch_weather`).
        function: String,
        /// Index of the callee in the heap's compile-time region.
        function_index: u64,
    },
}

impl ParkedKind {
    /// The `kind` string of the snapshot's `ParkedAt`.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Runnable => "runnable",
            Self::Sleep { .. } => "sleep",
            Self::RemoteCall { .. } => "remote_call",
        }
    }
}

/// What the coordinator tells a thread that parked at the gate.
pub(crate) enum GateReply {
    /// Carry on. The thread gets its (released) permit back.
    Continue(InactiveHeapPermit<BexThread>),
    /// The thread now lives in a snapshot. Its task ends without a result.
    Suspended,
}

// Read by the coordinator, which is native-only (snapshots link zstd).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) struct ParkedThread {
    pub(crate) thread_id: DurableThreadId,
    pub(crate) permit: InactiveHeapPermit<BexThread>,
    pub(crate) parked: ParkedKind,
    reply: tokio::sync::oneshot::Sender<GateReply>,
}

#[derive(Default)]
struct PauseState {
    /// Mirrors `PauseController::open`; the authoritative copy for `park`.
    open: bool,
    /// Threads of the run that exist: the root from the start of its engine
    /// loop, a spawned thread from the moment its parent spawns it.
    live: HashSet<DurableThreadId>,
    /// The run's root thread while it is live. A run whose root has returned
    /// cannot be snapshotted any more: the threads that are left settle
    /// futures nobody can resume.
    root: Option<DurableThreadId>,
    /// Threads that wait in an operation that cannot be re-issued.
    in_flight: HashMap<DurableThreadId, String>,
    parked: Vec<ParkedThread>,
}

/// Membership of one thread in the live set of the run. See
/// [`PauseController::register`].
pub(crate) struct LiveGuard {
    controller: Arc<PauseController>,
    thread: DurableThreadId,
}

impl Drop for LiveGuard {
    fn drop(&mut self) {
        self.controller.unregister(self.thread);
    }
}

/// Per-engine rendezvous between the threads of the durable run and the
/// snapshot coordinator.
pub(crate) struct PauseController {
    open: AtomicBool,
    gate: tokio::sync::watch::Sender<bool>,
    state: Mutex<PauseState>,
    changed: tokio::sync::Notify,
    /// Serializes coordinators (a pause and an automatic snapshot).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    coordinator: tokio::sync::Mutex<()>,
}

impl PauseController {
    pub(crate) fn new() -> Self {
        Self {
            open: AtomicBool::new(false),
            gate: tokio::sync::watch::Sender::new(false),
            state: Mutex::new(PauseState::default()),
            changed: tokio::sync::Notify::new(),
            coordinator: tokio::sync::Mutex::new(()),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, PauseState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Count `thread` as a live thread of the run until the returned guard
    /// drops. The guard is drop-safe: a thread task that is dropped or that
    /// panics still leaves the live set, so a later pause does not wait for it.
    pub(crate) fn register(self: &Arc<Self>, thread: DurableThreadId, is_root: bool) -> LiveGuard {
        {
            let mut state = self.state();
            state.live.insert(thread);
            if is_root {
                state.root = Some(thread);
            }
        }
        self.changed.notify_waiters();
        LiveGuard {
            controller: Arc::clone(self),
            thread,
        }
    }

    fn unregister(&self, thread: DurableThreadId) {
        {
            let mut state = self.state();
            state.live.remove(&thread);
            state.in_flight.remove(&thread);
            if state.root == Some(thread) {
                state.root = None;
            }
        }
        self.changed.notify_waiters();
    }

    /// The thread starts to wait in an operation that cannot be re-issued.
    pub(crate) fn enter_op(&self, thread: DurableThreadId, op: &str) {
        self.state().in_flight.insert(thread, op.to_string());
        self.changed.notify_waiters();
    }

    pub(crate) fn leave_op(&self, thread: DurableThreadId) {
        self.state().in_flight.remove(&thread);
    }

    /// One relaxed load: the per-`exec()` cost of the gate.
    pub(crate) fn gate_is_open(&self) -> bool {
        self.open.load(Ordering::Relaxed)
    }

    /// Resolves when the gate opens. Used to interrupt re-issuable waits.
    pub(crate) async fn opened(&self) {
        let mut gate = self.gate.subscribe();
        // The sender lives as long as `self`, so this cannot fail.
        let _ = gate.wait_for(|open| *open).await;
    }

    /// Park at the gate until the coordinator replies.
    pub(crate) async fn park(
        &self,
        thread_id: DurableThreadId,
        permit: InactiveHeapPermit<BexThread>,
        parked: ParkedKind,
    ) -> GateReply {
        let (reply, answer) = tokio::sync::oneshot::channel();
        {
            let mut state = self.state();
            if !state.open {
                // The coordinator closed the gate between the caller's check
                // and this call.
                return GateReply::Continue(permit);
            }
            state.parked.push(ParkedThread {
                thread_id,
                permit,
                parked,
                reply,
            });
        }
        self.changed.notify_waiters();
        // A dropped sender means the coordinator went away together with the
        // permit, so there is nothing to continue with.
        answer.await.unwrap_or(GateReply::Suspended)
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    fn set_open(&self, open: bool) {
        self.state().open = open;
        self.open.store(open, Ordering::Relaxed);
        self.gate.send_replace(open);
    }
}

pub(crate) fn unix_ms_now() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::{
        sync::{Arc, atomic::Ordering},
        time::{Duration, Instant},
    };

    use bex_external_types::BexExternalValue;
    use bex_heap::HeapPermit as _;
    use bex_snapshot::{ParkedAt, SnapshotError, ThreadInput, WriteOptions};
    /// The parts of the snapshot crate an embedder needs next to
    /// [`BexEngine::durable_snapshot`] and [`BexEngine::durable_resume`].
    pub use bex_snapshot::{
        SnapshotHeader, WriteStats, program_hash, program_hash_of_bytes, read_header,
        read_run_state, runtime_build,
    };

    use super::{GateReply, ParkedKind, ParkedThread, PauseController, unix_ms_now};
    use crate::{BexEngine, EngineError, FunctionCallContext};

    /// Minimum time between two snapshot attempts of one pause.
    const RETRY_INTERVAL: Duration = Duration::from_millis(100);
    /// How often a waiting coordinator re-asserts `park_requested`, which a
    /// finishing collection clears.
    const PARK_TICK: Duration = Duration::from_millis(20);

    impl ParkedKind {
        pub(crate) fn to_parked_at(&self) -> ParkedAt {
            let payload = match self {
                Self::Runnable => Vec::new(),
                Self::Sleep { deadline_unix_ms } => {
                    serde_json::json!({ "deadline_unix_ms": deadline_unix_ms })
                        .to_string()
                        .into_bytes()
                }
                Self::RemoteCall {
                    call_id,
                    function,
                    function_index,
                } => serde_json::json!({
                    "call_id": call_id,
                    "function": function,
                    "function_index": function_index,
                })
                .to_string()
                .into_bytes(),
            };
            ParkedAt {
                kind: self.kind().to_string(),
                payload,
            }
        }

        pub(crate) fn from_parked_at(parked: &ParkedAt) -> Result<Self, String> {
            let payload = || -> Result<serde_json::Value, String> {
                serde_json::from_slice(&parked.payload)
                    .map_err(|e| format!("the `{}` parked payload is not JSON: {e}", parked.kind))
            };
            let field = |value: &serde_json::Value, name: &str| {
                value
                    .get(name)
                    .cloned()
                    .ok_or_else(|| format!("the `{}` parked payload has no `{name}`", parked.kind))
            };
            match parked.kind.as_str() {
                "runnable" => Ok(Self::Runnable),
                "sleep" => {
                    let payload = payload()?;
                    let deadline_unix_ms = field(&payload, "deadline_unix_ms")?
                        .as_u64()
                        .ok_or("`deadline_unix_ms` is not a number")?;
                    Ok(Self::Sleep { deadline_unix_ms })
                }
                "remote_call" => {
                    let payload = payload()?;
                    Ok(Self::RemoteCall {
                        call_id: field(&payload, "call_id")?
                            .as_str()
                            .ok_or("`call_id` is not a string")?
                            .to_string(),
                        function: field(&payload, "function")?
                            .as_str()
                            .ok_or("`function` is not a string")?
                            .to_string(),
                        function_index: field(&payload, "function_index")?
                            .as_u64()
                            .ok_or("`function_index` is not a number")?,
                    })
                }
                other => Err(format!("unknown parked kind `{other}`")),
            }
        }
    }

    /// What a snapshot is for.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum SnapshotMode {
        /// Pause: on success the threads of the run are dropped. A blocked
        /// attempt is retried until it succeeds or the run ends.
        Suspend,
        /// Automatic snapshot: the threads continue afterwards. One attempt;
        /// it is abandoned when the run does not park within `park_timeout`.
        KeepRunning { park_timeout: Duration },
    }

    /// Identification of the snapshot to write.
    #[derive(Clone, Debug)]
    pub struct SnapshotRequest {
        pub run_id: String,
        pub segment: u32,
        pub seq: u64,
        /// `bex_snapshot::program_hash` of the program the engine was built from.
        pub program_hash: [u8; 32],
        /// Borsh bytes of the program, to make the snapshot self-contained.
        pub embed_program: Option<Vec<u8>>,
        pub compress: bool,
        pub mode: SnapshotMode,
    }

    /// Reported while a pause cannot complete yet.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum PauseProgress {
        /// Threads wait in operations that cannot be re-issued. Reported once.
        Pausing { waiting_on: Vec<String> },
        /// A snapshot attempt found a value that cannot be serialized.
        /// Reported when the reason or path changes; every attempt is counted
        /// in [`SnapshotReport::blocked_attempts`].
        Blocked { reason: String, path: Vec<String> },
    }

    /// A written snapshot, handed to the commit callback while the run is
    /// still held.
    pub struct SnapshotParts<'a> {
        pub bytes: &'a [u8],
        /// The `StateDump` JSON of the contract, section 2.5.
        pub state_dump: &'a serde_json::Value,
        pub stats: WriteStats,
    }

    #[derive(Debug)]
    pub struct SnapshotReport<T> {
        /// What the commit callback returned.
        pub committed: T,
        pub stats: WriteStats,
        /// From the request until every thread of the run was parked for the
        /// first time.
        pub pause_latency_ms: f64,
        pub blocked_attempts: u64,
        pub threads: usize,
    }

    #[derive(Debug, thiserror::Error)]
    pub enum SnapshotFailure {
        #[error("the run ended before a snapshot could be taken")]
        RunEnded,
        #[error("snapshot blocked: {reason}")]
        Blocked {
            reason: String,
            path: Vec<String>,
            blocked_attempts: u64,
        },
        #[error("the run did not reach a yield in time")]
        ParkTimeout,
        #[error("the snapshot could not be stored: {0}")]
        Commit(String),
        #[error("{0}")]
        Snapshot(String),
    }

    /// Parked threads taken out of the controller. Whatever is left when this
    /// drops (an error path, or a dropped coordinator future) continues.
    struct Held(Vec<ParkedThread>);

    impl Held {
        fn continue_all(&mut self) {
            for thread in self.0.drain(..) {
                let _ = thread.reply.send(GateReply::Continue(thread.permit));
            }
        }
    }

    impl Drop for Held {
        fn drop(&mut self) {
            self.continue_all();
        }
    }

    /// Closes the gate and clears the park request when an attempt ends,
    /// however it ends. The park request is cleared when the guard drops, not
    /// when the gate closes: the coordinator keeps its own request (a
    /// `ParkRequestGuard`) while it waits for the heap guard.
    struct GateGuard<'a> {
        controller: &'a PauseController,
        park_requested: &'a std::sync::atomic::AtomicBool,
    }

    impl GateGuard<'_> {
        /// Close the gate and take the threads that parked.
        fn close(&self) -> Held {
            let parked = {
                let mut state = self.controller.state();
                state.open = false;
                std::mem::take(&mut state.parked)
            };
            self.controller.open.store(false, Ordering::Relaxed);
            self.controller.gate.send_replace(false);
            Held(parked)
        }
    }

    impl Drop for GateGuard<'_> {
        fn drop(&mut self) {
            drop(self.close());
            self.park_requested.store(false, Ordering::Relaxed);
        }
    }

    enum Parked {
        All,
        RunEnded,
        TimedOut,
    }

    /// Facts about a restored run, reported before it continues.
    #[derive(Clone, Debug)]
    pub struct RestoredInfo {
        pub header: SnapshotHeader,
        pub run_state: Vec<u8>,
        /// `restore_into` plus the import into the VM.
        pub decode_ms: f64,
        /// Objects are not counted by the loader; the thread count is.
        pub threads: usize,
        /// Id of the restored root thread in this engine.
        pub root_thread: super::DurableThreadId,
        /// How the root thread resumes.
        pub root_parked: ParkedKind,
    }

    impl BexEngine {
        /// True when a durable run has threads inside the engine loop.
        #[must_use]
        pub fn durable_run_is_live(&self) -> bool {
            !self.durable_pause.state().live.is_empty()
        }

        /// Ask every VM of the engine to return to the engine loop at its
        /// next early-yield check (within 4096 control-flow instructions).
        ///
        /// A thread of the durable run looks at its cancel token at that
        /// yield, so an embedder calls this after it cancelled a run that may
        /// be in a compute loop. A finishing collection or snapshot attempt
        /// clears the request; call it again until the run has ended.
        pub fn durable_request_yield(&self) {
            self.park_requested.store(true, Ordering::Relaxed);
        }

        /// Wait until every thread of the run is parked at the gate.
        async fn durable_wait_parked(
            &self,
            deadline: Option<Instant>,
            reported_waiting: &mut bool,
            on_progress: &mut (dyn FnMut(PauseProgress) + Send),
        ) -> Parked {
            let controller = &self.durable_pause;
            loop {
                let changed = controller.changed.notified();
                tokio::pin!(changed);
                // Register before reading the state, so that no change between
                // the read and the await below is missed.
                changed.as_mut().enable();
                let waiting_on = {
                    let state = controller.state();
                    // Without its root the run is over, even when threads it
                    // spawned and never awaited are still running.
                    if state.live.is_empty() || state.root.is_none() {
                        return Parked::RunEnded;
                    }
                    if state.parked.len() >= state.live.len() {
                        return Parked::All;
                    }
                    let mut ops: Vec<String> = state.in_flight.values().cloned().collect();
                    ops.sort();
                    ops
                };
                if !waiting_on.is_empty() && !*reported_waiting {
                    *reported_waiting = true;
                    on_progress(PauseProgress::Pausing { waiting_on });
                }
                if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                    return Parked::TimedOut;
                }
                tokio::select! {
                    () = &mut changed => {}
                    () = tokio::time::sleep(PARK_TICK) => {
                        // A collection that finished in the meantime cleared
                        // the flag; a compute loop only yields while it is set.
                        self.park_requested.store(true, Ordering::Relaxed);
                    }
                }
            }
        }

        /// Pause the durable run at its next clean point and write a snapshot.
        ///
        /// `run_state` produces the opaque `WriteOptions::run_state` once the
        /// run is parked. `commit` stores the snapshot (the worker writes the
        /// files) while the run is still held: when it fails, the run
        /// continues and the error is returned.
        ///
        /// # Errors
        ///
        /// See [`SnapshotFailure`]. In [`SnapshotMode::Suspend`] a blocked
        /// attempt is not an error: it is reported through `on_progress` and
        /// retried, at most one attempt per 100 ms, until the run ends.
        pub async fn durable_snapshot<T: Send>(
            self: &Arc<Self>,
            request: SnapshotRequest,
            mut run_state: impl FnMut() -> Vec<u8> + Send,
            mut on_progress: impl FnMut(PauseProgress) + Send,
            commit: impl FnOnce(SnapshotParts<'_>) -> Result<T, String> + Send,
        ) -> Result<SnapshotReport<T>, SnapshotFailure> {
            let controller = &self.durable_pause;
            let _coordinator = controller.coordinator.lock().await;
            let requested = Instant::now();
            let mut commit = Some(commit);
            let mut pause_latency_ms = None;
            let mut blocked_attempts = 0u64;
            let mut reported_waiting = false;
            let mut last_blocked: Option<(String, Vec<String>)> = None;
            let park_deadline = match request.mode {
                SnapshotMode::Suspend => None,
                SnapshotMode::KeepRunning { park_timeout } => Some(requested + park_timeout),
            };

            loop {
                let gate = GateGuard {
                    controller,
                    park_requested: &self.park_requested,
                };
                controller.set_open(true);
                self.park_requested.store(true, Ordering::Relaxed);
                match self
                    .durable_wait_parked(park_deadline, &mut reported_waiting, &mut on_progress)
                    .await
                {
                    Parked::All => {}
                    Parked::RunEnded => return Err(SnapshotFailure::RunEnded),
                    Parked::TimedOut => return Err(SnapshotFailure::ParkTimeout),
                }
                pause_latency_ms.get_or_insert(requested.elapsed().as_secs_f64() * 1000.0);
                let mut held = gate.close();
                // Threads that are not part of the run (a helper call of the
                // embedder, a finalizer) hold active permits too. Keep asking
                // them to yield until the heap guard is granted, like the
                // collector does.
                drop(gate);
                if held.0.is_empty() {
                    return Err(SnapshotFailure::RunEnded);
                }
                let park_request = crate::ParkRequestGuard::new(Arc::clone(&self.park_requested));

                // Exclusive heap access: no mutator runs and no collection can
                // start until the guard drops.
                let heap_guard = self.heap_permit_manager.request_park().await;
                drop(park_request);
                let written = {
                    let threads: Vec<ThreadInput<'_>> = held
                        .0
                        .iter()
                        .map(|parked| {
                            // SAFETY: the heap guard excludes every mutator and
                            // the collector, and the thread's task is suspended
                            // in `PauseController::park`.
                            #[allow(unsafe_code)]
                            let thread = unsafe { parked.permit.holder() };
                            ThreadInput {
                                thread_id: parked.thread_id,
                                parent_thread: thread.durable.as_ref().and_then(|ctx| ctx.parent),
                                name: thread.name.clone().unwrap_or_default(),
                                vm: &thread.vm,
                                parked: parked.parked.to_parked_at(),
                                extra_roots: Vec::new(),
                            }
                        })
                        .collect();
                    // `durable_resume` restores one thread. A snapshot of
                    // several threads could never be resumed, and suspending
                    // into it would lose the run, so such an attempt counts as
                    // blocked until the extra threads have ended.
                    let unsupported = (threads.len() > 1
                        || threads.iter().any(|thread| thread.parent_thread.is_some()))
                    .then(|| SnapshotError::Blocked {
                        reason: format!(
                            "the run has {} live threads; snapshots of several threads are not \
                             supported yet",
                            threads.len()
                        ),
                        path: threads
                            .iter()
                            .map(|thread| match thread.name.as_str() {
                                "" => format!("thread {}", thread.thread_id),
                                name => format!("thread {} ({name})", thread.thread_id),
                            })
                            .collect(),
                    });
                    let header = SnapshotHeader {
                        format_version: bex_snapshot::FORMAT_VERSION,
                        runtime_build: bex_snapshot::runtime_build(),
                        program_hash: request.program_hash,
                        run_id: request.run_id.clone(),
                        segment: request.segment,
                        seq: request.seq,
                        created_unix_ms: unix_ms_now(),
                    };
                    // The writer runs first: when a value blocks the snapshot
                    // (the pending future of a spawned thread, for example),
                    // its path names the variable, which says more than the
                    // thread count.
                    bex_snapshot::write_snapshot(
                        &self.heap,
                        &threads,
                        WriteOptions {
                            header,
                            embed_program: request.embed_program.clone(),
                            compress: request.compress,
                            run_state: run_state(),
                        },
                    )
                    .and_then(|written| match unsupported {
                        Some(blocked) => Err(blocked),
                        None => Ok(written),
                    })
                    .map(|(bytes, stats)| {
                        let dump = bex_snapshot::state_dump(
                            &self.heap,
                            &threads,
                            &request.run_id,
                            request.segment,
                        );
                        (bytes, stats, dump)
                    })
                };

                match written {
                    Ok((bytes, stats, state_dump)) => {
                        let commit = commit.take().ok_or_else(|| {
                            SnapshotFailure::Snapshot("the snapshot was already committed".into())
                        })?;
                        let committed = commit(SnapshotParts {
                            bytes: &bytes,
                            state_dump: &state_dump,
                            stats,
                        });
                        drop(heap_guard);
                        let committed = committed.map_err(SnapshotFailure::Commit)?;
                        let threads = held.0.len();
                        if request.mode == SnapshotMode::Suspend {
                            for parked in held.0.drain(..) {
                                let ParkedThread { permit, reply, .. } = parked;
                                // A VM is dropped under its permit everywhere
                                // else in the engine; keep that invariant.
                                let mut active = permit.acquire().await;
                                // The thread ends without unwinding. Close its
                                // open profiler calls before its task emits
                                // the thread end, like every other path that
                                // ends a thread at a yield.
                                self.prof_refresh_vm_ring(&mut active.vm);
                                self.prof_drain_open_calls(
                                    &mut active.vm,
                                    bex_events::prof::record::FunctionEndStatus::Cancelled,
                                );
                                drop(active);
                                let _ = reply.send(GateReply::Suspended);
                            }
                        } else {
                            held.continue_all();
                        }
                        return Ok(SnapshotReport {
                            committed,
                            stats,
                            pause_latency_ms: pause_latency_ms.unwrap_or(0.0),
                            blocked_attempts,
                            threads,
                        });
                    }
                    Err(SnapshotError::Blocked { reason, path }) => {
                        drop(heap_guard);
                        held.continue_all();
                        blocked_attempts += 1;
                        if matches!(request.mode, SnapshotMode::KeepRunning { .. }) {
                            return Err(SnapshotFailure::Blocked {
                                reason,
                                path,
                                blocked_attempts,
                            });
                        }
                        let current = (reason, path);
                        if last_blocked.as_ref() != Some(&current) {
                            on_progress(PauseProgress::Blocked {
                                reason: current.0.clone(),
                                path: current.1.clone(),
                            });
                            last_blocked = Some(current);
                        }
                        tokio::time::sleep(RETRY_INTERVAL).await;
                    }
                    Err(other) => {
                        drop(heap_guard);
                        held.continue_all();
                        return Err(SnapshotFailure::Snapshot(other.to_string()));
                    }
                }
            }
        }

        /// Restore a single-thread snapshot into this engine and continue the
        /// run. The engine must have been built from the identical program
        /// (`program_hash` is checked against the snapshot header), and the
        /// durable host must already be installed.
        ///
        /// The root thread runs through the ordinary engine loop, so
        /// completion, failure, cancellation, positions and remote calls
        /// behave as in a fresh call of `function_name`. `on_restored` runs
        /// after the state is installed and before the thread continues.
        ///
        /// # Errors
        ///
        /// [`EngineError::Other`] when the snapshot does not belong to this
        /// build or program, is malformed, or has more than one thread;
        /// otherwise whatever the run itself produces.
        pub async fn durable_resume(
            self: &Arc<Self>,
            function_name: &str,
            bytes: &[u8],
            program_hash: [u8; 32],
            call_ctx: FunctionCallContext,
            copy_objects: bool,
            on_restored: impl FnOnce(&RestoredInfo) + Send,
        ) -> Result<BexExternalValue, EngineError> {
            let refuse = |message: String| EngineError::Other(message);
            let header = bex_snapshot::read_header(bytes).map_err(|e| refuse(e.to_string()))?;
            if header.runtime_build != bex_snapshot::runtime_build() {
                return Err(refuse(format!(
                    "the snapshot was written by runtime build `{}`, this is `{}`",
                    header.runtime_build,
                    bex_snapshot::runtime_build()
                )));
            }
            if header.program_hash != program_hash {
                return Err(refuse(
                    "the snapshot was taken from a different program (program hash differs)"
                        .to_string(),
                ));
            }
            self.durable_resume_root(
                function_name,
                bytes,
                program_hash,
                call_ctx,
                copy_objects,
                Box::new(on_restored),
            )
            .await
        }

        /// Steps of a restore that need the heap. The caller holds the new
        /// root thread's permit, which keeps the collector out from before
        /// `restore_into` until the state is rooted by the (registered) VM.
        pub(crate) fn durable_restore_into(
            &self,
            thread: &mut bex_heap::ActiveHeapPermit<crate::BexThread>,
            bytes: &[u8],
            program_hash: [u8; 32],
        ) -> Result<RestoredInfo, EngineError> {
            let refuse = |message: String| EngineError::Other(message);
            let started = Instant::now();
            let _proof = thread.proof();
            let mut restored = bex_snapshot::restore_into(&self.heap, bytes, program_hash)
                .map_err(|e| refuse(e.to_string()))?;
            if restored.threads.len() != 1 {
                return Err(refuse(format!(
                    "the snapshot has {} threads; resuming several threads is not supported yet",
                    restored.threads.len()
                )));
            }
            let restored_thread = restored.threads.remove(0);
            if !restored_thread.extra_roots.is_empty() {
                return Err(refuse(
                    "the snapshot holds values outside the VM, which this engine never writes"
                        .to_string(),
                ));
            }
            let root_parked =
                ParkedKind::from_parked_at(&restored_thread.parked).map_err(&refuse)?;
            thread.name = (!restored_thread.name.is_empty()).then_some(restored_thread.name);
            thread
                .vm
                .import_thread_state(restored_thread.state)
                .map_err(|e| refuse(format!("the snapshot does not fit this program: {e}")))?;
            Ok(RestoredInfo {
                header: restored.header,
                run_state: restored.run_state,
                decode_ms: started.elapsed().as_secs_f64() * 1000.0,
                threads: 1,
                root_thread: thread.vm.prof_thread_id,
                root_parked,
            })
        }
    }
}
