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
//! file). A snapshot holds every thread of the run with its futures, its
//! place in the cancellation tree, and its task group seat, and a resume
//! recreates each thread as its own task. Later root threads (helper calls of the embedder) get the
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
    /// The calling thread's place in the spawn tree of the run: `"0"` for the
    /// root, `"0.2"` for the root's third spawn, and so on. Unlike `thread`
    /// it does not depend on how the scheduler interleaved the threads.
    pub thread_path: String,
    /// The number of this call among the remote calls of the calling thread,
    /// from 1. `(thread_path, call_index)` names the same call in every
    /// execution of the program, also in one that starts again from an older
    /// snapshot, so a host derives its call identifier from the pair.
    pub call_index: u64,
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
    /// future is pending. A pause waits for this future: once it has resolved,
    /// the call is known outside the process, so the wait for its result can
    /// be re-issued after a resume. A cancellation of the calling thread also
    /// waits for it (for at most five seconds), so that the engine learns the
    /// identifier and can report the call through [`Self::remote_cancelled`].
    /// The future should therefore resolve promptly.
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

    /// `thread` was cancelled while it waited for the result of the remote
    /// call `call_id` (`Future.cancel`, a `race` loser, a cancel token, a
    /// timeout, the failure of a parent, or the cancellation of the run). The
    /// run no longer waits on the call: the engine has dropped the
    /// [`Self::remote_result`] future and will not ask for this id again, so
    /// the host can cancel the remote work and must discard a result that
    /// still arrives. Also called by a resumed run for a thread whose
    /// snapshot was taken in the wait and whose cancel token had fired by
    /// then; the host may therefore hear about one call in two processes.
    ///
    /// The default does nothing.
    fn remote_cancelled(&self, call_id: &str, thread: DurableThreadId) {
        let _ = (call_id, thread);
    }

    /// True when the host already holds the result of `call_id` and the
    /// waiting thread has not taken it yet
    /// ([`Self::remote_result_delivered`] has not been called for it). The
    /// thread is then about to continue, so the run is not idle: the
    /// self-suspend rule and the ordered replay of a restored run ask this.
    /// Called under an engine lock; it must not call back into the engine.
    ///
    /// The default answers `false`.
    fn remote_result_available(&self, call_id: &str) -> bool {
        let _ = call_id;
        false
    }

    /// The engine took the result that [`Self::remote_result`] resolved with:
    /// the wait of `thread` on `call_id` is over and the thread continues with
    /// the value. Until this call a snapshot may still park the thread in the
    /// wait, so the host keeps the result (and keeps answering
    /// [`Self::remote_result_available`] with `true`) until now.
    ///
    /// The default does nothing.
    fn remote_result_delivered(&self, call_id: &str, thread: DurableThreadId) {
        let _ = (call_id, thread);
    }

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
// Durable sleep (contract section 9.2) uses the same path. Every thread of
// the run records in the controller when it enters and leaves a wait that can
// be re-issued. [`crate::BexEngine::durable_idle_sleep`] resolves when every
// live thread is in such a wait, at least one of them sleeps, and the earliest
// sleep deadline is far enough away. The embedder then takes a snapshot in
// `SnapshotMode::SuspendIfIdle`. That mode runs the three steps above and
// checks the rule once more under the heap guard: a thread that the gate woke
// inside its wait still has its wait entry, and a thread whose wait ended by
// itself has removed it before it parked. When the rule no longer holds the
// threads continue and nothing is written.
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
use bex_vm_types::CancelTokenData;

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
    /// A spawned thread that has not started: it waits for a slot in its
    /// `baml.spawn.TaskGroup`. Its VM holds the entry frame of the spawn body.
    /// A resumed process queues it again in the recorded order.
    Queued,
}

impl ParkedKind {
    /// The `kind` string of the snapshot's `ParkedAt`.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Runnable => "runnable",
            Self::Sleep { .. } => "sleep",
            Self::RemoteCall { .. } => "remote_call",
            Self::Queued => "queued",
        }
    }
}

/// A wait that a snapshot can hold and a resumed process can finish: the
/// waits of the self-suspend rule (contract section 9.2).
///
/// Every variant carries what is needed to tell whether the wait is still
/// blocked. A thread whose wait has been satisfied stays in its wait until its
/// task is polled again; during that time it looks like a waiting thread, but
/// it is about to run. The rules that ask "is the run idle" look through the
/// probe and do not count such a thread as waiting.
#[derive(Clone, Debug)]
pub(crate) enum WaitKind {
    /// Inside `baml.sys.sleep`, until the absolute deadline.
    Sleep { deadline_unix_ms: u64 },
    /// Waiting for the result of the announced remote call `call_id`.
    RemoteCall { call_id: String },
    /// Inside `await` or `__await_any`, on futures of the run. `None` stands
    /// for a future that had settled before the wait began.
    Await {
        futures: Vec<Option<crate::future::PendingJoinHandle>>,
    },
    /// Waiting for a slot of a `baml.spawn.TaskGroup`, which another thread
    /// of the run holds.
    Queued {
        group: Arc<bex_vm_types::TaskGroupInner>,
        member_id: u64,
    },
}

impl WaitKind {
    /// The wait of a thread that parks in a sleep or in a remote wait.
    /// `Runnable` and `Queued` threads enter their waits elsewhere.
    pub(crate) fn of(parked: &ParkedKind) -> Self {
        match parked {
            ParkedKind::Sleep { deadline_unix_ms } => Self::Sleep {
                deadline_unix_ms: *deadline_unix_ms,
            },
            ParkedKind::RemoteCall { call_id, .. } => Self::RemoteCall {
                call_id: call_id.clone(),
            },
            ParkedKind::Runnable | ParkedKind::Queued => Self::Await {
                futures: Vec::new(),
            },
        }
    }
}

/// A completion that was recorded while the run had no process: a sleep whose
/// deadline passed, or a remote result that arrived. See [`Replay`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ReplayItem {
    at_unix_ms: u64,
    thread: DurableThreadId,
}

/// Ordered replay of what happened while the run had no process.
///
/// A restored run can hold several waits that are already satisfied: sleeps
/// whose deadline has passed and remote calls whose result has arrived. An
/// uninterrupted run would have seen these completions one at a time, in the
/// order of their times, and the effects of one (a `race` that cancels its
/// losers, a timeout that cancels its work) would have landed before the next.
/// A restore that let every satisfied thread continue at once would leave the
/// order to the task scheduler.
///
/// The restore therefore queues the recorded completions by time. A driver
/// task releases the first one when the run is quiescent (every live thread is
/// blocked in a wait and no operation is in flight), waits until the run is
/// quiescent again, and releases the next. While the replay is active no other
/// sleep and no other remote wait of the run completes: a completion that
/// happens now is later than every recorded one.
///
/// The replay keeps a virtual clock, the time of the last released item. A
/// `sleep` that a thread starts during the replay begins at that virtual time.
/// When its virtual deadline is not later than the last recorded completion,
/// the sleep joins the queue at that deadline, so that a thread that would
/// have slept between two recorded completions still wakes between them. A
/// sleep that ends after the last recorded completion is an ordinary sleep.
/// Once the queue is empty the clock is the real clock again, so a run that is
/// resumed late does not rush through the sleeps it starts afterwards.
///
/// A pause in the middle of a replay needs no extra state. A thread that was
/// not released is still parked in its sleep (with its past deadline) or in
/// its remote wait (the host still has the result with its time), and the next
/// restore builds the same queue again.
#[derive(Default)]
struct Replay {
    active: bool,
    /// Recorded completions that have not been released, earliest first.
    queue: std::collections::VecDeque<ReplayItem>,
    /// Threads whose turn has come and whose wait has not ended yet.
    released: HashSet<DurableThreadId>,
    /// Time of the last released item; the snapshot's time before the first.
    virtual_now_unix_ms: u64,
    /// Time of the last recorded completion.
    horizon_unix_ms: u64,
}

/// What [`PauseState::quiescent`] found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
enum Quiescence {
    /// Every live thread is blocked in a wait and nothing is in flight.
    Quiescent,
    Busy,
    /// The run has no root any more.
    RunEnded,
}

/// The sleep a run that is idle would suspend itself for: the sleep with the
/// earliest deadline among the threads of the run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdleSleep {
    /// The thread that sleeps.
    pub thread: DurableThreadId,
    /// Absolute wall-clock deadline of the sleep in Unix milliseconds.
    pub deadline_unix_ms: u64,
    /// Time left until the deadline when the value was computed.
    pub remaining_ms: u64,
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

/// Where a live thread hangs in the cancellation tree of the run.
///
/// The entries only name live threads. When a thread ends, the threads whose
/// tokens are children of its token move to its own parent and inherit its
/// user tokens: the ended thread's token can then only fire through its
/// ancestors or through a user token linked into it, and both still reach the
/// moved threads. A snapshot can therefore rebuild the tree from live threads
/// alone.
#[derive(Clone, Default)]
pub(crate) struct ThreadLinks {
    pub(crate) token_parent: Option<DurableThreadId>,
    pub(crate) user_cancels: Vec<Arc<CancelTokenData>>,
    /// The thread's own cancel token. A thread whose token has fired is about
    /// to leave whatever wait it is in.
    pub(crate) cancel: Option<tokio_util::sync::CancellationToken>,
}

impl ThreadLinks {
    /// True when the thread is cancelled, or will be as soon as the watcher
    /// task of a linked user token has run.
    fn cancel_pending(&self) -> bool {
        fn fired(token: &CancelTokenData) -> bool {
            token.is_cancelled() || token.sources().iter().any(|source| fired(source))
        }
        self.cancel
            .as_ref()
            .is_some_and(tokio_util::sync::CancellationToken::is_cancelled)
            || self.user_cancels.iter().any(|token| fired(token))
    }
}

#[derive(Default)]
struct PauseState {
    /// Mirrors `PauseController::open`; the authoritative copy for `park`.
    open: bool,
    /// Threads of the run that exist: the root from the start of its engine
    /// loop, a spawned thread from the moment its parent spawns it.
    live: HashSet<DurableThreadId>,
    /// Cancellation links of every live thread.
    links: HashMap<DurableThreadId, ThreadLinks>,
    /// The run's root thread while it is live. A run whose root has returned
    /// cannot be snapshotted any more: the threads that are left settle
    /// futures nobody can resume.
    root: Option<DurableThreadId>,
    /// Threads that wait in an operation that cannot be re-issued.
    in_flight: HashMap<DurableThreadId, String>,
    /// Threads that wait in an operation that can be re-issued. A thread that
    /// the gate woke keeps its entry while it is parked; a thread whose wait
    /// ended for any other reason removes it before it does anything else.
    waits: HashMap<DurableThreadId, WaitKind>,
    /// The sleep (thread and deadline) for which a self-suspend attempt could
    /// not write or store its snapshot. The run is not considered idle again
    /// while that sleep is its earliest one, so one sleep costs at most one
    /// failed attempt.
    // Read by the coordinator, which is native-only.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    suspend_refused: Option<(DurableThreadId, u64)>,
    parked: Vec<ParkedThread>,
    /// Ordered replay of a restored run. Inactive in a run that was started
    /// in this process.
    replay: Replay,
}

/// Answers whether the host already holds the result of a remote call
/// ([`DurableHost::remote_result_available`]).
pub(crate) type ResultProbe<'a> = &'a dyn Fn(&str) -> bool;

impl PauseState {
    /// The slack of the self-suspend threshold in milliseconds. The rule is
    /// evaluated a moment after the sleep began, so a sleep of exactly the
    /// threshold has a little less than the threshold left by then. Without
    /// the slack such a sleep would suspend the run only sometimes.
    const IDLE_SLACK_MS: u64 = 100;

    /// True when `thread`, which waits in `wait`, cannot continue before
    /// something else happens. False when the wait is already satisfied (the
    /// thread continues as soon as its task is polled).
    fn blocked(
        &self,
        thread: DurableThreadId,
        wait: &WaitKind,
        now_unix_ms: u64,
        result_available: ResultProbe<'_>,
    ) -> bool {
        if self
            .links
            .get(&thread)
            .is_some_and(ThreadLinks::cancel_pending)
        {
            return false;
        }
        match wait {
            // During a replay sleeps and remote waits complete only when the
            // driver releases them, and a released thread has no wait entry.
            WaitKind::Sleep { .. } | WaitKind::RemoteCall { .. } if self.replay.active => true,
            WaitKind::Sleep { deadline_unix_ms } => *deadline_unix_ms > now_unix_ms,
            WaitKind::RemoteCall { call_id } => !result_available(call_id),
            WaitKind::Await { futures } => futures
                .iter()
                .all(|future| future.as_ref().is_some_and(|f| !f.initialized())),
            WaitKind::Queued { group, member_id } => !group.member_is_active(*member_id),
        }
    }

    /// Every live thread waits in an operation that can be re-issued, and
    /// every one of these waits is still blocked.
    fn all_blocked(&self, now_unix_ms: u64, result_available: ResultProbe<'_>) -> bool {
        self.in_flight.is_empty()
            && self.live.iter().all(|thread| {
                self.waits
                    .get(thread)
                    .is_some_and(|wait| self.blocked(*thread, wait, now_unix_ms, result_available))
            })
    }

    /// The self-suspend rule: every live thread of the run is blocked in an
    /// operation that can be re-issued, at least one of them sleeps, and the
    /// earliest sleep deadline is at least `min_remaining_ms` after `now`
    /// (less [`Self::IDLE_SLACK_MS`]). `min_remaining_ms == 0` disables the
    /// rule. A run that still replays recorded completions is not idle.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    fn idle_sleep(
        &self,
        min_remaining_ms: u64,
        now_unix_ms: u64,
        result_available: ResultProbe<'_>,
    ) -> Option<IdleSleep> {
        if min_remaining_ms == 0 || self.root.is_none() || self.live.is_empty() {
            return None;
        }
        if self.replay.active || !self.all_blocked(now_unix_ms, result_available) {
            return None;
        }
        let (deadline_unix_ms, thread) = self
            .waits
            .iter()
            .filter_map(|(thread, wait)| match wait {
                WaitKind::Sleep { deadline_unix_ms } => Some((*deadline_unix_ms, *thread)),
                _ => None,
            })
            .min()?;
        let remaining_ms = deadline_unix_ms.saturating_sub(now_unix_ms);
        if remaining_ms == 0
            || remaining_ms.saturating_add(Self::IDLE_SLACK_MS) < min_remaining_ms
            || self.suspend_refused == Some((thread, deadline_unix_ms))
        {
            return None;
        }
        Some(IdleSleep {
            thread,
            deadline_unix_ms,
            remaining_ms,
        })
    }

    /// Whether the replay driver may release the next recorded completion.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    fn quiescent(&self, now_unix_ms: u64, result_available: ResultProbe<'_>) -> Quiescence {
        if self.root.is_none() || self.live.is_empty() {
            return Quiescence::RunEnded;
        }
        // While a snapshot attempt holds the gate open, threads are parked
        // and not blocked in their waits.
        if self.open || !self.parked.is_empty() {
            return Quiescence::Busy;
        }
        if self.all_blocked(now_unix_ms, result_available) {
            Quiescence::Quiescent
        } else {
            Quiescence::Busy
        }
    }

    /// Release the next recorded completion whose thread still waits for it.
    /// Returns false when the queue was empty: the replay is over.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    fn replay_release_next(&mut self) -> bool {
        while let Some(item) = self.replay.queue.pop_front() {
            self.replay.virtual_now_unix_ms = self.replay.virtual_now_unix_ms.max(item.at_unix_ms);
            // A thread that ended or was cancelled in the meantime has no
            // use for its turn.
            if !self.live.contains(&item.thread) || !self.waits.contains_key(&item.thread) {
                continue;
            }
            // From here on the thread is about to run. It has no wait entry
            // until it enters its next wait.
            self.waits.remove(&item.thread);
            self.replay.released.insert(item.thread);
            return true;
        }
        self.replay.active = false;
        self.replay.released.clear();
        false
    }
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
    /// Notified when the replay releases a thread or ends.
    replay_changed: tokio::sync::Notify,
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
            replay_changed: tokio::sync::Notify::new(),
            coordinator: tokio::sync::Mutex::new(()),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, PauseState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Count `thread` as a live thread of the run until the returned guard
    /// drops. The guard is drop-safe: a thread task that is dropped or that
    /// panics still leaves the live set, so a later pause does not wait for it.
    pub(crate) fn register(
        self: &Arc<Self>,
        thread: DurableThreadId,
        is_root: bool,
        links: ThreadLinks,
    ) -> LiveGuard {
        {
            let mut state = self.state();
            state.live.insert(thread);
            state.links.insert(thread, links);
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
            state.waits.remove(&thread);
            state.replay.released.remove(&thread);
            if let Some(ended) = state.links.remove(&thread) {
                for links in state.links.values_mut() {
                    if links.token_parent == Some(thread) {
                        links.token_parent = ended.token_parent;
                        for token in &ended.user_cancels {
                            if !links.user_cancels.iter().any(|own| Arc::ptr_eq(own, token)) {
                                links.user_cancels.push(Arc::clone(token));
                            }
                        }
                    }
                }
            }
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

    /// The thread starts to wait in an operation that can be re-issued. The
    /// self-suspend rule is evaluated again.
    pub(crate) fn enter_wait(&self, thread: DurableThreadId, wait: WaitKind) {
        self.state().waits.insert(thread, wait);
        self.changed.notify_waiters();
    }

    /// The wait of the thread is over (it has a result, it was cancelled, or
    /// a snapshot attempt that woke it has let it continue past the wait).
    /// Called before the thread takes its heap permit back, so that a
    /// coordinator that sees the thread parked later on knows that it is not
    /// waiting any more.
    pub(crate) fn leave_wait(&self, thread: DurableThreadId) {
        {
            let mut state = self.state();
            state.waits.remove(&thread);
            state.replay.released.remove(&thread);
        }
        self.changed.notify_waiters();
    }

    /// Resolves when `thread` may finish a sleep or a remote wait: at once in
    /// a run without a replay, when the replay driver releases the thread,
    /// or when the replay is over. See [`Replay`].
    pub(crate) async fn replay_turn(&self, thread: DurableThreadId) {
        loop {
            let changed = self.replay_changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let state = self.state();
                if !state.replay.active || state.replay.released.contains(&thread) {
                    return;
                }
            }
            changed.await;
        }
    }

    /// A thread starts a `sleep` of `duration_ms`. During a replay the sleep
    /// begins at the replay's virtual time; when it ends before the last
    /// recorded completion it joins the replay queue, and the virtual
    /// deadline is returned. `None` means an ordinary sleep.
    pub(crate) fn replay_sleep(&self, thread: DurableThreadId, duration_ms: u64) -> Option<u64> {
        let mut state = self.state();
        if !state.replay.active {
            return None;
        }
        let deadline = state.replay.virtual_now_unix_ms.saturating_add(duration_ms);
        if deadline > state.replay.horizon_unix_ms {
            return None;
        }
        let item = ReplayItem {
            at_unix_ms: deadline,
            thread,
        };
        let at = state.replay.queue.partition_point(|queued| *queued <= item);
        state.replay.queue.insert(at, item);
        Some(deadline)
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
                Self::Runnable | Self::Queued => Vec::new(),
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
                "queued" => Ok(Self::Queued),
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
        /// Self-suspend for a sleep (contract section 9.2): like
        /// [`Self::Suspend`], but the snapshot is only written when the
        /// self-suspend rule still holds once every thread is parked. One
        /// attempt. When the rule does not hold any more the threads continue
        /// and the attempt ends with [`SnapshotFailure::NotIdle`]. When the
        /// snapshot cannot be written or stored the threads keep waiting in
        /// process, the error is returned, and the run is not reported idle
        /// again for the same sleep. See [`BexEngine::durable_idle_sleep`].
        SuspendIfIdle {
            /// The threshold of the rule in milliseconds. `0` never suspends.
            min_remaining_ms: u64,
        },
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
        /// Set by [`SnapshotMode::SuspendIfIdle`]: the sleep the run suspended
        /// itself for, with the remaining time measured when the snapshot was
        /// written (the header's `created_unix_ms`).
        pub wake: Option<super::IdleSleep>,
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
        /// [`SnapshotMode::SuspendIfIdle`] only: a thread left its wait, or
        /// the earliest sleep came closer than the threshold, before the run
        /// was parked.
        #[error("the run is not idle in a long sleep any more")]
        NotIdle,
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
            // The replay driver does not release a thread while an attempt
            // holds the gate; it looks again now.
            self.controller.changed.notify_waiters();
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

    /// One thread of a restored run.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct RestoredThreadInfo {
        /// The id the thread had when the snapshot was written. It keeps it.
        pub thread: super::DurableThreadId,
        pub parent: Option<super::DurableThreadId>,
        pub name: String,
        pub parked: ParkedKind,
        /// The thread's cancel token had fired, or the resume cancelled it.
        pub cancelled: bool,
    }

    /// Facts about a restored run, reported before it continues.
    #[derive(Clone, Debug)]
    pub struct RestoredInfo {
        pub header: SnapshotHeader,
        pub run_state: Vec<u8>,
        /// `restore_into` plus the import into the VMs.
        pub decode_ms: f64,
        /// Objects are not counted by the loader; the thread count is.
        pub threads: usize,
        /// Id of the restored root thread.
        pub root_thread: super::DurableThreadId,
        /// How the root thread resumes.
        pub root_parked: ParkedKind,
        /// Every restored thread, the root first.
        pub thread_infos: Vec<RestoredThreadInfo>,
        /// Futures of the snapshot that had not settled.
        pub pending_futures: usize,
    }

    /// Options of [`BexEngine::durable_resume_with`].
    #[derive(Clone, Debug, Default)]
    pub struct ResumeOptions {
        /// Threads to cancel as part of the restore, before any thread runs.
        /// An embedder uses this to deliver a cancellation that was decided
        /// while the run had no process. Unknown ids are ignored.
        pub cancel_threads: Vec<super::DurableThreadId>,
        /// Remote results that the host holds for calls of the snapshot, with
        /// the time each one arrived. The restored run takes them one at a
        /// time, in the order of their times, merged with the sleeps whose
        /// deadline has passed (see the ordered replay in this module). A
        /// result that is not listed here is delivered after the replay.
        pub recorded_results: Vec<RecordedResult>,
    }

    /// A remote result that arrived while the run had no process.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct RecordedResult {
        pub call_id: String,
        /// When the result arrived, in Unix milliseconds. A time in the
        /// future is treated as the time of the restore.
        pub at_unix_ms: u64,
    }

    /// A restored spawned thread that waits to be started as its own task.
    pub(crate) struct RestoredChild {
        pub(crate) thread_id: super::DurableThreadId,
        pub(crate) parent: Option<super::DurableThreadId>,
        pub(crate) permit: bex_heap::InactiveHeapPermit<crate::BexThread>,
        pub(crate) parked: ParkedKind,
        pub(crate) settles_future: bex_vm_types::types::FutureId,
        pub(crate) cancel: tokio_util::sync::CancellationToken,
        pub(crate) ticket: Option<bex_vm_types::TaskGroupTicket>,
        pub(crate) live: super::LiveGuard,
    }

    /// What `durable_restore_into` hands to the root's entry point.
    pub(crate) struct RestoredRun {
        pub(crate) info: RestoredInfo,
        pub(crate) children: Vec<RestoredChild>,
    }

    impl BexEngine {
        /// True when a durable run has threads inside the engine loop.
        #[must_use]
        pub fn durable_run_is_live(&self) -> bool {
            !self.durable_pause.state().live.is_empty()
        }

        /// True while a snapshot coordinator waits for the threads of the run
        /// to park. An embedder can use it to order its own work relative to
        /// a pause it requested from another task.
        #[must_use]
        pub fn durable_pause_requested(&self) -> bool {
            self.durable_pause.gate_is_open()
        }

        /// Resolves when the self-suspend rule of the durable run holds
        /// (contract section 9.2): every live thread of the run waits in an
        /// operation that can be re-issued (a `sleep`, the wait for a remote
        /// result, an `await` on futures of the run, the queue of a task
        /// group), at least one of them sleeps, and the earliest sleep
        /// deadline is at least `min_remaining_ms` away. The rule is evaluated
        /// whenever a thread of the run starts, ends, or enters or leaves a
        /// wait. `min_remaining_ms == 0` disables the rule: the future never
        /// resolves.
        ///
        /// The embedder then calls [`Self::durable_snapshot`] with
        /// [`SnapshotMode::SuspendIfIdle`], which checks the rule again with
        /// the run parked. A sleep for which that attempt failed to write or
        /// store a snapshot is not reported again.
        pub async fn durable_idle_sleep(&self, min_remaining_ms: u64) -> super::IdleSleep {
            let controller = &self.durable_pause;
            loop {
                let changed = controller.changed.notified();
                tokio::pin!(changed);
                // Register before reading the state, so that no change between
                // the read and the await below is missed.
                changed.as_mut().enable();
                let host = self.durable_host();
                let idle =
                    controller
                        .state()
                        .idle_sleep(min_remaining_ms, unix_ms_now(), &|call_id| {
                            host.as_ref()
                                .is_some_and(|host| host.remote_result_available(call_id))
                        });
                if let Some(idle) = idle {
                    return idle;
                }
                changed.await;
            }
        }

        /// Drives the ordered replay of a restored run (see `Replay`):
        /// releases one recorded completion whenever the run is quiescent, and
        /// ends the replay when the queue is empty and the run is quiescent
        /// once more. Ends at once for a run without recorded completions.
        pub(crate) async fn durable_replay_driver(self: Arc<Self>) {
            let controller = &self.durable_pause;
            let end = |controller: &PauseController| {
                {
                    let mut state = controller.state();
                    state.replay.active = false;
                    state.replay.queue.clear();
                    state.replay.released.clear();
                }
                controller.replay_changed.notify_waiters();
                controller.changed.notify_waiters();
            };
            loop {
                if !controller.state().replay.active {
                    return;
                }
                loop {
                    let changed = controller.changed.notified();
                    tokio::pin!(changed);
                    changed.as_mut().enable();
                    let host = self.durable_host();
                    let quiescence = controller.state().quiescent(unix_ms_now(), &|call_id| {
                        host.as_ref()
                            .is_some_and(|host| host.remote_result_available(call_id))
                    });
                    match quiescence {
                        super::Quiescence::Quiescent => break,
                        super::Quiescence::RunEnded => {
                            end(controller);
                            return;
                        }
                        super::Quiescence::Busy => {}
                    }
                    // Every change towards quiescence notifies `changed`. The
                    // tick only bounds the damage of a missed notification.
                    tokio::select! {
                        () = &mut changed => {}
                        () = tokio::time::sleep(PARK_TICK) => {}
                    }
                }
                let released = controller.state().replay_release_next();
                controller.replay_changed.notify_waiters();
                controller.changed.notify_waiters();
                if !released {
                    return;
                }
            }
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
                SnapshotMode::Suspend | SnapshotMode::SuspendIfIdle { .. } => None,
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
                let registry = self.futures.registry_view(&heap_guard).await;
                let created_unix_ms = unix_ms_now();
                // The self-suspend rule is checked again now that nothing can
                // change: a thread that the gate woke in a wait still has its
                // wait entry, and a thread whose wait ended by itself does not.
                let wake = match request.mode {
                    SnapshotMode::SuspendIfIdle { min_remaining_ms } => {
                        let host = self.durable_host();
                        let idle = controller.state().idle_sleep(
                            min_remaining_ms,
                            created_unix_ms,
                            &|call_id| {
                                host.as_ref()
                                    .is_some_and(|host| host.remote_result_available(call_id))
                            },
                        );
                        if idle.is_none() {
                            drop(heap_guard);
                            held.continue_all();
                            return Err(SnapshotFailure::NotIdle);
                        }
                        idle
                    }
                    SnapshotMode::Suspend | SnapshotMode::KeepRunning { .. } => None,
                };
                // A self-suspend that fails is not repeated for the same sleep.
                let refuse_sleep = |controller: &PauseController| {
                    if let Some(idle) = wake {
                        controller.state().suspend_refused =
                            Some((idle.thread, idle.deadline_unix_ms));
                    }
                };
                let written = {
                    let links = controller.state().links.clone();
                    let threads: Vec<ThreadInput<'_>> = held
                        .0
                        .iter()
                        .map(|parked| {
                            // SAFETY: the heap guard excludes every mutator and
                            // the thread's task is suspended in
                            // `PauseController::park`.
                            #[allow(unsafe_code)]
                            let thread = unsafe { parked.permit.holder() };
                            let links = links.get(&parked.thread_id).cloned().unwrap_or_default();
                            ThreadInput {
                                thread_id: parked.thread_id,
                                parent_thread: thread.durable.as_ref().and_then(|ctx| ctx.parent),
                                name: thread.name.clone().unwrap_or_default(),
                                vm: &thread.vm,
                                parked: parked.parked.to_parked_at(),
                                extra_roots: Vec::new(),
                                settles_future: thread.settles_future.map(|id| {
                                    bex_snapshot::SettledFuture {
                                        id,
                                        object: registry.tracked.get(&id).map(|(ptr, _)| *ptr),
                                    }
                                }),
                                cancel: bex_snapshot::ThreadCancel {
                                    cancelled: thread.cancel.is_cancelled(),
                                    parent: links.token_parent,
                                },
                                user_cancels: links.user_cancels,
                                group: thread.group.as_ref().map(|seat| {
                                    bex_snapshot::GroupMembership {
                                        group: Arc::clone(&seat.group),
                                        member_id: seat.member_id,
                                    }
                                }),
                                engine_state: thread
                                    .durable
                                    .as_ref()
                                    .map(crate::thread::DurableThreadCtx::engine_state)
                                    .unwrap_or_default(),
                            }
                        })
                        .collect();
                    let header = SnapshotHeader {
                        format_version: bex_snapshot::FORMAT_VERSION,
                        runtime_build: bex_snapshot::runtime_build(),
                        program_hash: request.program_hash,
                        run_id: request.run_id.clone(),
                        segment: request.segment,
                        seq: request.seq,
                        created_unix_ms,
                    };
                    bex_snapshot::write_snapshot(
                        &self.heap,
                        &threads,
                        WriteOptions {
                            header,
                            embed_program: request.embed_program.clone(),
                            compress: request.compress,
                            run_state: run_state(),
                            future_id_span: registry.next_future_id as u64,
                        },
                    )
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
                        let committed = match committed {
                            Ok(committed) => committed,
                            Err(message) => {
                                refuse_sleep(controller);
                                return Err(SnapshotFailure::Commit(message));
                            }
                        };
                        let threads = held.0.len();
                        if !matches!(request.mode, SnapshotMode::KeepRunning { .. }) {
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
                            wake,
                        });
                    }
                    Err(SnapshotError::Blocked { reason, path }) => {
                        drop(heap_guard);
                        held.continue_all();
                        blocked_attempts += 1;
                        refuse_sleep(controller);
                        if !matches!(request.mode, SnapshotMode::Suspend) {
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
                        refuse_sleep(controller);
                        return Err(SnapshotFailure::Snapshot(other.to_string()));
                    }
                }
            }
        }

        /// Restore a snapshot into this engine and continue the run. The
        /// engine must have been built from the identical program
        /// (`program_hash` is checked against the snapshot header), and the
        /// durable host must already be installed.
        ///
        /// Every thread of the snapshot becomes a thread of this engine with
        /// the id, the parent, the cancellation links, and the task group seat
        /// it had. The root thread runs through the ordinary engine loop in
        /// this call, so completion, failure, cancellation, positions and
        /// remote calls behave as in a fresh call of `function_name`. Every
        /// other thread runs as its own task, like a freshly spawned thread,
        /// and first finishes the wait it was parked in. `on_restored` runs
        /// after the state is installed and before any thread continues.
        ///
        /// # Errors
        ///
        /// [`EngineError::Other`] when the snapshot does not belong to this
        /// build or program or is malformed; otherwise whatever the run itself
        /// produces.
        pub async fn durable_resume(
            self: &Arc<Self>,
            function_name: &str,
            bytes: &[u8],
            program_hash: [u8; 32],
            call_ctx: FunctionCallContext,
            copy_objects: bool,
            on_restored: impl FnOnce(&RestoredInfo) + Send,
        ) -> Result<BexExternalValue, EngineError> {
            self.durable_resume_with(
                function_name,
                bytes,
                program_hash,
                call_ctx,
                copy_objects,
                ResumeOptions::default(),
                on_restored,
            )
            .await
        }

        /// [`Self::durable_resume`] with [`ResumeOptions`].
        ///
        /// # Errors
        ///
        /// As for [`Self::durable_resume`].
        #[allow(clippy::too_many_arguments)]
        pub async fn durable_resume_with(
            self: &Arc<Self>,
            function_name: &str,
            bytes: &[u8],
            program_hash: [u8; 32],
            call_ctx: FunctionCallContext,
            copy_objects: bool,
            options: ResumeOptions,
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
                options,
                Box::new(on_restored),
            )
            .await
        }

        /// Steps of a restore that need the heap. The caller holds the new
        /// root thread's permit, which keeps the collector out from before
        /// `restore_into` until every restored value is rooted: the root's
        /// state by its registered VM, every other thread's state by the VM of
        /// its own registered (inactive) permit, and pending futures by the
        /// future registry.
        pub(crate) async fn durable_restore_into(
            self: &Arc<Self>,
            thread: &mut bex_heap::ActiveHeapPermit<crate::BexThread>,
            bytes: &[u8],
            program_hash: [u8; 32],
            options: &ResumeOptions,
        ) -> Result<RestoredRun, EngineError> {
            use std::collections::HashMap;

            use tokio_util::sync::CancellationToken;

            let refuse = |message: String| EngineError::Other(message);
            let started = Instant::now();
            let Some(root_ctx) = thread.durable.as_ref() else {
                return Err(refuse(
                    "a snapshot can only be resumed with a durable host installed".to_string(),
                ));
            };
            let host = Arc::clone(&root_ctx.host);
            let pause = root_ctx.pause.clone();

            // Reserve the future ids of the snapshot above the ids this engine
            // has issued, under one lock of the registry.
            let mut restored = {
                let mut registry = self.futures.acquire(thread.proof()).await;
                let future_id_base = registry.issued_future_ids();
                let restored = bex_snapshot::restore_into_with(
                    &self.heap,
                    bytes,
                    program_hash,
                    bex_snapshot::RestoreOptions { future_id_base },
                )
                .map_err(|e| refuse(e.to_string()))?;
                registry.reserve_future_ids(future_id_base + restored.future_id_span);
                restored
            };

            if restored
                .threads
                .iter()
                .any(|thread| !thread.extra_roots.is_empty())
            {
                return Err(refuse(
                    "the snapshot holds values outside the VM, which this engine never writes"
                        .to_string(),
                ));
            }
            let is_root = |thread: &bex_snapshot::RestoredThread| {
                thread.parent_thread.is_none() && thread.settles_future.is_none()
            };
            let roots = restored.threads.iter().filter(|t| is_root(t)).count();
            if roots != 1 {
                return Err(refuse(format!(
                    "the snapshot must have exactly one root thread; {roots} of its {} threads \
                     have no parent",
                    restored.threads.len()
                )));
            }
            if let Some(orphan) = restored
                .threads
                .iter()
                .find(|thread| !is_root(thread) && thread.settles_future.is_none())
            {
                return Err(refuse(format!(
                    "thread {} of the snapshot is spawned but settles no future",
                    orphan.thread_id
                )));
            }
            let Some(pause) = pause else {
                return Err(refuse(
                    "the restored root thread is not the root of the durable run".to_string(),
                ));
            };
            // The root first; `RestoredInfo::thread_infos` promises it.
            restored.threads.sort_by_key(|thread| !is_root(thread));
            let parked: Vec<ParkedKind> = restored
                .threads
                .iter()
                .map(|thread| ParkedKind::from_parked_at(&thread.parked).map_err(&refuse))
                .collect::<Result<_, _>>()?;
            if parked[0] == ParkedKind::Queued {
                return Err(refuse("the root thread cannot be queued".to_string()));
            }
            let engine_states: Vec<(String, u64, u64)> = restored
                .threads
                .iter()
                .map(|thread| {
                    crate::thread::DurableThreadCtx::parse_engine_state(&thread.engine_state)
                        .map_err(&refuse)
                })
                .collect::<Result<_, _>>()?;

            // Cancel tokens, parents before children. The root keeps the token
            // of this call, so that the embedder's cancel reaches the run.
            let mut tokens: HashMap<u64, CancellationToken> = HashMap::new();
            tokens.insert(restored.threads[0].thread_id, thread.cancel.clone());
            let mut remaining: Vec<&bex_snapshot::RestoredThread> =
                restored.threads.iter().skip(1).collect();
            while !remaining.is_empty() {
                let before = remaining.len();
                remaining.retain(|thread| {
                    let token = match thread.cancel.parent {
                        None => CancellationToken::new(),
                        Some(parent) => match tokens.get(&parent) {
                            Some(parent) => parent.child_token(),
                            None => return true,
                        },
                    };
                    tokens.insert(thread.thread_id, token);
                    false
                });
                if remaining.len() == before {
                    return Err(refuse(
                        "the cancel tokens of the snapshot's threads form a cycle".to_string(),
                    ));
                }
            }

            // Futures: every pending future needs the thread that settles it,
            // and an id names one future object and one producer. Checked
            // before the first side effect on the engine, so that the
            // registration below cannot fail half way.
            {
                let mut producers = std::collections::HashSet::new();
                for thread in &restored.threads {
                    if let Some(id) = thread.settles_future {
                        if !producers.insert(id) {
                            return Err(refuse(format!(
                                "two threads of the snapshot settle future {id}"
                            )));
                        }
                    }
                }
                let mut pending = std::collections::HashSet::new();
                for future in restored.futures.iter().filter(|future| future.pending) {
                    if !pending.insert(future.id) {
                        return Err(refuse(format!(
                            "two pending futures of the snapshot have the id {}",
                            future.id
                        )));
                    }
                    if !producers.contains(&future.id) {
                        return Err(refuse(format!(
                            "pending future {} of the snapshot has no thread that settles it",
                            future.id
                        )));
                    }
                }
            }

            // Everything that can fail comes before the first side effect on
            // the engine: the thread states are imported into their VMs here.
            let fits = |e: String| refuse(format!("the snapshot does not fit this program: {e}"));
            let mut vms = Vec::with_capacity(restored.threads.len().saturating_sub(1));
            for (index, restored_thread) in restored.threads.iter_mut().enumerate() {
                let state = std::mem::take(&mut restored_thread.state);
                if index == 0 {
                    thread.vm.import_thread_state(state).map_err(fits)?;
                    thread.vm.prof_thread_id = restored_thread.thread_id;
                    thread.name =
                        (!restored_thread.name.is_empty()).then(|| restored_thread.name.clone());
                } else {
                    let mut vm = self.durable_restored_child_vm(restored_thread.thread_id);
                    vm.import_thread_state(state).map_err(fits)?;
                    vms.push(vm);
                }
            }

            // What completed while the run had no process, by time: sleeps
            // whose deadline has passed and remote calls whose result the
            // host recorded. The queue is installed below, once nothing can
            // fail any more (see `Replay`).
            let now = unix_ms_now();
            let mut recorded: Vec<super::ReplayItem> = parked
                .iter()
                .zip(&restored.threads)
                .filter_map(|(parked, thread)| {
                    let at_unix_ms = match parked {
                        ParkedKind::Sleep { deadline_unix_ms } if *deadline_unix_ms <= now => {
                            *deadline_unix_ms
                        }
                        ParkedKind::RemoteCall { call_id, .. } => options
                            .recorded_results
                            .iter()
                            .find(|result| result.call_id == *call_id)?
                            .at_unix_ms
                            .min(now),
                        _ => return None,
                    };
                    Some(super::ReplayItem {
                        at_unix_ms,
                        thread: thread.thread_id,
                    })
                })
                .collect();
            recorded.sort_unstable();

            // Pending futures: observed by the token of the thread that
            // settles them, and rooted by the registry.
            let producers: HashMap<bex_vm_types::types::FutureId, &bex_snapshot::RestoredThread> =
                restored
                    .threads
                    .iter()
                    .filter_map(|thread| thread.settles_future.map(|id| (id, thread)))
                    .collect();
            let pending: Vec<&bex_snapshot::RestoredFuture> = restored
                .futures
                .iter()
                .filter(|future| future.pending)
                .collect();
            {
                let mut registry = self.futures.acquire(thread.proof()).await;
                let mut registered = Vec::with_capacity(pending.len());
                for future in &pending {
                    let producer = producers[&future.id];
                    // SAFETY: the object was allocated by the restore above,
                    // this thread holds its permit, and no VM runs with the
                    // object before this function returns.
                    #[allow(unsafe_code)]
                    if let bex_vm_types::Object::Future(object) = unsafe { future.object.get_mut() }
                    {
                        object.cancel = tokens[&producer.thread_id].clone();
                    }
                    let origin: Arc<str> = if producer.name.is_empty() {
                        "<restored spawn>".into()
                    } else {
                        producer.name.as_str().into()
                    };
                    if let Err(error) =
                        registry.register_restored_future(future.id, future.object, origin)
                    {
                        // Leave no pending future without a producer behind:
                        // it would keep the engine busy for ever.
                        for registered in &registered {
                            let _ = registry.cancel_future(*registered);
                        }
                        return Err(error);
                    }
                    registered.push(future.id);
                }
            }
            let pending_futures = pending.len();

            // A fired token stays fired. Children of a fired token are fired
            // by the token tree, which matches what the snapshot recorded.
            for restored_thread in &restored.threads {
                if restored_thread.cancel.cancelled
                    || options.cancel_threads.contains(&restored_thread.thread_id)
                {
                    tokens[&restored_thread.thread_id].cancel();
                }
            }

            // Links that live in tasks: composite user tokens, and user tokens
            // linked into thread tokens.
            for token in &restored.cancel_tokens {
                for watcher in token.watchers() {
                    tokio::spawn(watcher);
                }
            }
            for restored_thread in &restored.threads {
                let own = &tokens[&restored_thread.thread_id];
                for user in &restored_thread.user_cancels {
                    if !own.is_cancelled() {
                        tokio::spawn(bex_vm_types::cancel_token::link(
                            user.token().clone(),
                            own.clone(),
                        ));
                    }
                }
            }

            // Task group seats, in the recorded order of each group.
            let mut seats: Vec<(usize, &bex_snapshot::RestoredGroupMembership)> = restored
                .threads
                .iter()
                .enumerate()
                .filter_map(|(index, thread)| thread.group.as_ref().map(|seat| (index, seat)))
                .collect();
            seats.sort_by_key(|(_, seat)| (Arc::as_ptr(&seat.group) as usize, seat.order));
            let mut tickets = HashMap::new();
            for (index, seat) in &seats {
                let token = tokens[&restored.threads[*index].thread_id].clone();
                let ticket = seat.group.register_restored(token, seat.active);
                tickets.insert(*index, (Arc::clone(&seat.group), ticket));
            }
            for (_, seat) in &seats {
                seat.group.admit_waiters();
            }

            // Thread ids are part of the run: a thread keeps its id, and ids
            // minted from now on are higher than every restored id.
            let highest = restored
                .threads
                .iter()
                .map(|thread| thread.thread_id)
                .max()
                .unwrap_or(0);
            self.next_thread_id
                .fetch_max(highest.saturating_add(1), Ordering::Relaxed);

            let mut thread_infos = Vec::with_capacity(restored.threads.len());
            let mut children = Vec::new();
            let mut vms = vms.into_iter();
            for (index, ((restored_thread, parked), (path, spawned, remote_calls))) in restored
                .threads
                .into_iter()
                .zip(parked)
                .zip(engine_states)
                .enumerate()
            {
                let token = tokens[&restored_thread.thread_id].clone();
                thread_infos.push(RestoredThreadInfo {
                    thread: restored_thread.thread_id,
                    parent: restored_thread.parent_thread,
                    name: restored_thread.name.clone(),
                    parked: parked.clone(),
                    cancelled: token.is_cancelled(),
                });
                let (Some(settles_future), true) = (restored_thread.settles_future, index > 0)
                else {
                    if let Some(ctx) = thread.durable.as_mut() {
                        ctx.path = path;
                        ctx.spawned = spawned;
                        ctx.remote_calls = remote_calls;
                    }
                    continue;
                };
                let Some(vm) = vms.next() else {
                    continue;
                };
                let name = (!restored_thread.name.is_empty()).then_some(restored_thread.name);
                let mut child =
                    crate::BexThread::new_child(vm, token.clone(), name, settles_future);
                let links = super::ThreadLinks {
                    token_parent: restored_thread.cancel.parent,
                    user_cancels: restored_thread.user_cancels,
                    cancel: Some(token.clone()),
                };
                child.durable = Some(crate::thread::DurableThreadCtx {
                    host: Arc::clone(&host),
                    parent: restored_thread.parent_thread,
                    token_parent: links.token_parent,
                    user_cancels: links.user_cancels.clone(),
                    pause: Some(Arc::clone(&pause)),
                    path,
                    spawned,
                    remote_calls,
                });
                let ticket = tickets.remove(&index).map(|(group, ticket)| {
                    child.group = Some(crate::thread::ThreadGroupSeat {
                        group,
                        member_id: ticket.member_id(),
                    });
                    ticket
                });
                let live = pause.register(restored_thread.thread_id, false, links);
                // Registers the VM as a root holder. Like the spawn path, this
                // takes no second active permit on this task.
                let permit = self.heap_permit_manager.new_permit(child).await;
                children.push(RestoredChild {
                    thread_id: restored_thread.thread_id,
                    parent: restored_thread.parent_thread,
                    permit,
                    parked,
                    settles_future,
                    cancel: token,
                    ticket,
                    live,
                });
            }

            // The replay starts with the snapshot's clock. It is active from
            // here on, so no restored thread finishes a sleep or a remote
            // wait before the driver has released it.
            {
                let mut state = pause.state();
                state.replay = super::Replay {
                    active: !recorded.is_empty(),
                    horizon_unix_ms: recorded.last().map_or(0, |item| item.at_unix_ms),
                    virtual_now_unix_ms: restored.header.created_unix_ms,
                    queue: recorded.into(),
                    released: std::collections::HashSet::new(),
                };
            }

            let root = thread_infos[0].clone();
            Ok(RestoredRun {
                info: RestoredInfo {
                    header: restored.header,
                    run_state: restored.run_state,
                    decode_ms: started.elapsed().as_secs_f64() * 1000.0,
                    threads: thread_infos.len(),
                    root_thread: root.thread,
                    root_parked: root.parked,
                    thread_infos,
                    pending_futures,
                },
                children,
            })
        }
    }
}

#[cfg(test)]
mod self_suspend_rule_tests {
    use super::{IdleSleep, PauseState, Quiescence, Replay, ReplayItem, ThreadLinks, WaitKind};

    fn remote(call_id: &str) -> WaitKind {
        WaitKind::RemoteCall {
            call_id: call_id.to_string(),
        }
    }

    fn awaiting() -> WaitKind {
        WaitKind::Await {
            futures: Vec::new(),
        }
    }

    /// No result has arrived.
    fn none(_: &str) -> bool {
        false
    }

    const NOW: u64 = 1_000_000;

    /// A run whose root (thread 1) sleeps until `NOW + 5000` while threads 2
    /// and 3 wait for a remote result and in an `await`.
    fn idle_run() -> PauseState {
        let mut state = PauseState {
            root: Some(1),
            ..PauseState::default()
        };
        state.live.extend([1, 2, 3]);
        state.waits.insert(
            1,
            WaitKind::Sleep {
                deadline_unix_ms: NOW + 5000,
            },
        );
        state.waits.insert(2, remote("c1"));
        state.waits.insert(3, awaiting());
        state
    }

    #[test]
    fn the_threshold_is_inclusive() {
        let state = idle_run();
        assert_eq!(
            state.idle_sleep(5000, NOW, &none),
            Some(IdleSleep {
                thread: 1,
                deadline_unix_ms: NOW + 5000,
                remaining_ms: 5000,
            })
        );
        // A sleep of exactly the threshold still suspends the run when the
        // rule is evaluated a moment after the sleep began.
        assert_eq!(
            state
                .idle_sleep(5000, NOW + 1, &none)
                .map(|idle| idle.remaining_ms),
            Some(4999)
        );
        assert_eq!(
            state
                .idle_sleep(5000 + PauseState::IDLE_SLACK_MS, NOW, &none)
                .map(|idle| idle.remaining_ms),
            Some(5000)
        );
        assert_eq!(
            state.idle_sleep(5001 + PauseState::IDLE_SLACK_MS, NOW, &none),
            None
        );
        assert_eq!(
            state.idle_sleep(5000, NOW + PauseState::IDLE_SLACK_MS + 1, &none),
            None
        );
        assert_eq!(
            state
                .idle_sleep(1, NOW + 4999, &none)
                .map(|idle| idle.remaining_ms),
            Some(1)
        );
        // A deadline in the past leaves nothing to suspend for.
        assert_eq!(state.idle_sleep(1, NOW + 5000, &none), None);
        assert_eq!(state.idle_sleep(1, NOW + 9000, &none), None);
    }

    #[test]
    fn a_threshold_of_zero_disables_the_rule() {
        assert_eq!(idle_run().idle_sleep(0, NOW, &none), None);
    }

    #[test]
    fn every_live_thread_must_wait() {
        let mut state = idle_run();
        state.live.insert(4);
        assert_eq!(
            state.idle_sleep(1000, NOW, &none),
            None,
            "thread 4 is running"
        );
        state.in_flight.insert(4, "baml.http.send".to_string());
        assert_eq!(
            state.idle_sleep(1000, NOW, &none),
            None,
            "thread 4 is in an HTTP call"
        );
        state.in_flight.remove(&4);
        state.waits.insert(4, awaiting());
        assert!(state.idle_sleep(1000, NOW, &none).is_some());
    }

    #[test]
    fn one_thread_must_sleep_and_the_earliest_deadline_counts() {
        let mut state = idle_run();
        state.waits.insert(1, awaiting());
        assert_eq!(state.idle_sleep(1000, NOW, &none), None, "nobody sleeps");

        let mut state = idle_run();
        state.waits.insert(
            3,
            WaitKind::Sleep {
                deadline_unix_ms: NOW + 800,
            },
        );
        assert_eq!(
            state.idle_sleep(1000, NOW, &none),
            None,
            "thread 3 wakes in 800 ms"
        );
        assert_eq!(
            state.idle_sleep(800, NOW, &none),
            Some(IdleSleep {
                thread: 3,
                deadline_unix_ms: NOW + 800,
                remaining_ms: 800,
            })
        );
    }

    #[test]
    fn a_run_without_its_root_is_over() {
        let mut state = idle_run();
        state.root = None;
        assert_eq!(state.idle_sleep(1000, NOW, &none), None);
        assert_eq!(PauseState::default().idle_sleep(1000, NOW, &none), None);
    }

    #[test]
    fn a_refused_sleep_is_not_reported_again() {
        let mut state = idle_run();
        state.suspend_refused = Some((1, NOW + 5000));
        assert_eq!(state.idle_sleep(1000, NOW, &none), None);
        // Another sleep of the same thread is a new sleep.
        state.waits.insert(
            1,
            WaitKind::Sleep {
                deadline_unix_ms: NOW + 7000,
            },
        );
        assert_eq!(
            state
                .idle_sleep(1000, NOW, &none)
                .map(|idle| idle.deadline_unix_ms),
            Some(NOW + 7000)
        );
    }

    #[test]
    fn a_remote_wait_whose_result_has_arrived_is_not_idle() {
        let state = idle_run();
        assert!(state.idle_sleep(1000, NOW, &none).is_some());
        assert_eq!(
            state.idle_sleep(1000, NOW, &|call_id| call_id == "c1"),
            None,
            "thread 2 is about to take its result"
        );
        assert!(
            state
                .idle_sleep(1000, NOW, &|call_id| call_id == "c9")
                .is_some()
        );
    }

    #[test]
    fn an_await_on_a_settled_future_is_not_idle() {
        let mut state = idle_run();
        let signal = std::sync::Arc::new(tokio::sync::SetOnce::new());
        state.waits.insert(
            3,
            WaitKind::Await {
                futures: vec![Some(std::sync::Arc::clone(&signal))],
            },
        );
        assert!(state.idle_sleep(1000, NOW, &none).is_some());
        signal.set(Ok(())).expect("first set");
        assert_eq!(state.idle_sleep(1000, NOW, &none), None);
        assert_eq!(state.quiescent(NOW, &none), Quiescence::Busy);
        // A future that had settled before the wait began.
        state.waits.insert(
            3,
            WaitKind::Await {
                futures: vec![None],
            },
        );
        assert_eq!(state.idle_sleep(1000, NOW, &none), None);
    }

    #[test]
    fn a_cancelled_thread_is_not_idle() {
        let mut state = idle_run();
        let token = tokio_util::sync::CancellationToken::new();
        state.links.insert(
            2,
            ThreadLinks {
                cancel: Some(token.clone()),
                ..ThreadLinks::default()
            },
        );
        assert!(state.idle_sleep(1000, NOW, &none).is_some());
        token.cancel();
        assert_eq!(state.idle_sleep(1000, NOW, &none), None);

        // A user token that has fired cancels the thread once its watcher
        // task has run; a composite token fires with any of its sources.
        let mut state = idle_run();
        let source = bex_vm_types::CancelTokenData::new();
        let composite = bex_vm_types::CancelTokenData::composite(vec![source.clone()]);
        state.links.insert(
            3,
            ThreadLinks {
                user_cancels: vec![composite],
                ..ThreadLinks::default()
            },
        );
        assert!(state.idle_sleep(1000, NOW, &none).is_some());
        source.token().cancel();
        assert_eq!(state.idle_sleep(1000, NOW, &none), None);
    }

    #[test]
    fn a_replay_releases_recorded_completions_in_order_and_skips_gone_threads() {
        let mut state = idle_run();
        state.live.insert(4);
        state.waits.insert(4, remote("c2"));
        state.replay = Replay {
            active: true,
            queue: [
                ReplayItem {
                    at_unix_ms: 100,
                    thread: 2,
                },
                ReplayItem {
                    at_unix_ms: 200,
                    thread: 9,
                },
                ReplayItem {
                    at_unix_ms: 300,
                    thread: 4,
                },
            ]
            .into(),
            horizon_unix_ms: 300,
            virtual_now_unix_ms: 50,
            ..Replay::default()
        };
        // A run that replays is never idle, and its sleeps and remote waits
        // are blocked whatever the clock or the host says.
        assert_eq!(state.idle_sleep(1000, NOW, &none), None);
        assert_eq!(
            state.quiescent(NOW + 99_000, &|_| true),
            Quiescence::Quiescent
        );

        assert!(state.replay_release_next());
        assert!(state.replay.released.contains(&2));
        assert!(!state.waits.contains_key(&2), "thread 2 is about to run");
        assert_eq!(state.replay.virtual_now_unix_ms, 100);
        assert_eq!(state.quiescent(NOW, &none), Quiescence::Busy);

        // Thread 2 has taken its result and waits again; thread 9 is gone.
        state.waits.insert(2, awaiting());
        assert_eq!(state.quiescent(NOW, &none), Quiescence::Quiescent);
        assert!(state.replay_release_next());
        assert!(state.replay.released.contains(&4));
        assert_eq!(state.replay.virtual_now_unix_ms, 300);

        state.waits.insert(4, awaiting());
        assert!(!state.replay_release_next());
        assert!(!state.replay.active);
        assert!(state.idle_sleep(1000, NOW, &none).is_some());
    }

    #[test]
    fn quiescence_needs_every_thread_blocked_and_no_open_gate() {
        let mut state = idle_run();
        assert_eq!(state.quiescent(NOW, &none), Quiescence::Quiescent);
        assert_eq!(
            state.quiescent(NOW + 5000, &none),
            Quiescence::Busy,
            "the sleep is over"
        );
        state.open = true;
        assert_eq!(state.quiescent(NOW, &none), Quiescence::Busy);
        state.open = false;
        state.in_flight.insert(3, "baml.http.send".to_string());
        assert_eq!(state.quiescent(NOW, &none), Quiescence::Busy);
        state.in_flight.clear();
        state.root = None;
        assert_eq!(state.quiescent(NOW, &none), Quiescence::RunEnded);
    }
}
