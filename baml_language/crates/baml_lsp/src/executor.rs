//! Where read tasks run.
//!
//! The owner thread never runs a tracked query on its own handle: every read
//! is a job on an [`Executor`], holding a [`Snapshot`]. [`spawn_read`] is the
//! one place a snapshot is owned outside the owner — it lends the snapshot to
//! the job as `&Snapshot`, drops it, and only then reports the outcome, so a
//! finished job can never keep the owner's next `set_*` waiting.
//!
//! Two implementations: a fixed [`ThreadPool`] for native hosts (bounded
//! concurrency bounds the number of live snapshots, which bounds how long a
//! mutation can wait; rayon stays free for data-parallel fan-out *inside*
//! jobs), and [`Inline`] for single-threaded hosts (wasm), which runs the job
//! synchronously inside `spawn` — the snapshot is dropped before `spawn`
//! returns, so `set_*` never observes a live clone there either.

use std::sync::Arc;

use crate::{
    error::LspError,
    snapshot::{Snapshot, TaskFailure, run_guarded},
};

/// A unit of work handed to an executor.
pub type Job = Box<dyn FnOnce() + Send + 'static>;

/// Runs jobs. Object-safe; the snapshot discipline lives in [`spawn_read`].
pub trait Executor: Send + Sync {
    fn spawn_job(&self, job: Job);
}

/// The outcome a read job reports back to the owner.
pub type ReadOutcome<R> = Result<Result<R, LspError>, TaskFailure>;

/// Run `run` against `snap` on `executor`, then report through `done`.
///
/// This is the only function that owns a [`Snapshot`] off the owner thread,
/// and it drops the snapshot **before** calling `done`.
pub fn spawn_read<R: Send + 'static>(
    executor: &dyn Executor,
    snap: Snapshot,
    run: impl FnOnce(&Snapshot) -> Result<R, LspError> + Send + 'static,
    done: impl FnOnce(ReadOutcome<R>) + Send + 'static,
) {
    executor.spawn_job(Box::new(move || {
        let outcome = run_guarded(&snap, run);
        drop(snap);
        done(outcome);
    }));
}

/// A fixed-size pool of worker threads.
pub struct ThreadPool {
    sender: crossbeam_channel::Sender<Job>,
}

impl ThreadPool {
    /// `threads` is clamped to at least one.
    pub fn new(threads: usize) -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded::<Job>();
        for index in 0..threads.max(1) {
            let receiver = receiver.clone();
            std::thread::Builder::new()
                .name(format!("baml-lsp-pool-{index}"))
                .spawn(move || {
                    while let Ok(job) = receiver.recv() {
                        // A panic inside a job is caught by `run_guarded`;
                        // anything that escapes (a panicking `done`) must
                        // not take the worker down with it.
                        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
                    }
                })
                .unwrap_or_else(|e| panic!("failed to spawn LSP pool thread: {e}"));
        }
        Self { sender }
    }

    /// `available_parallelism()` clamped to `[2, 8]` — enough to overlap
    /// requests with a diagnostics sweep, few enough that a mutation waits on
    /// a handful of jobs at most.
    pub fn default_size() -> usize {
        std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(2)
            .clamp(2, 8)
    }
}

impl Executor for ThreadPool {
    fn spawn_job(&self, job: Job) {
        // The receivers live as long as the pool threads; a send only fails
        // after every worker has exited, which only happens on shutdown.
        if self.sender.send(job).is_err() {
            tracing::warn!("LSP pool has shut down; dropping job");
        }
    }
}

/// Runs each job synchronously inside `spawn_job` (single-threaded hosts).
#[derive(Debug, Default, Clone, Copy)]
pub struct Inline;

impl Executor for Inline {
    fn spawn_job(&self, job: Job) {
        job();
    }
}

/// The owner's three job lanes, separated by what can block them.
///
/// Snapshot-holding jobs are bounded by query granularity (a mutation cancels
/// them at the next query entry), but filesystem jobs are not bounded by
/// anything — a blocking read (a named pipe listed as `.baml`, a dead network
/// mount) can pin its worker forever. On a shared pool that pins the workers
/// a *queued* snapshot job needs, and the owner's next `set_*` then waits on
/// a snapshot that can never run: a permanent deadlock. Hence `io` is its own
/// lane, and no snapshot ever queues behind the filesystem.
///
/// `diagnostics` is split from `requests` for latency only: a whole-root
/// sweep occupies one worker for a while, and on a two-worker pool that is
/// half the interactive capacity.
pub struct Executors {
    /// Snapshot reads answering client requests.
    pub requests: Arc<dyn Executor>,
    /// Snapshot-holding diagnostics sweeps (at most one in flight per root).
    pub diagnostics: Arc<dyn Executor>,
    /// Filesystem jobs (discovery walks, disk reloads). Never hold a
    /// snapshot, so a stall here can delay freshness but never a mutation.
    pub io: Arc<dyn Executor>,
}

impl Executors {
    /// Every lane on one executor (tests, embedding).
    pub fn single(executor: Arc<dyn Executor>) -> Self {
        Self {
            requests: Arc::clone(&executor),
            diagnostics: Arc::clone(&executor),
            io: executor,
        }
    }

    /// Every lane inline (single-threaded hosts: wasm, unit tests).
    pub fn inline() -> Self {
        Self::single(Arc::new(Inline))
    }

    /// The native default: a request pool sized to the machine, one
    /// diagnostics worker, and two filesystem workers.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_default() -> Self {
        Self {
            requests: Arc::new(ThreadPool::new(ThreadPool::default_size())),
            diagnostics: Arc::new(ThreadPool::new(1)),
            io: Arc::new(ThreadPool::new(2)),
        }
    }
}
