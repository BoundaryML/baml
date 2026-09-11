//! Opportunistic cleanup after work stops. Scheduling state contains no heap
//! pointers and can be inspected without a heap permit. Moving GC cannot.
use std::sync::{Arc, Mutex, Weak};

use bex_heap::{BexHeap, CollectionLevel};
use tokio::sync::Notify;
#[cfg(not(target_arch = "wasm32"))]
use tokio::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::{BexEngine, GcCheckGuard};

const IDLE_DELAY: std::time::Duration = std::time::Duration::from_millis(100);
#[cfg(not(target_arch = "wasm32"))]
const PARK_WAIT: std::time::Duration = std::time::Duration::from_millis(5);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct CleanupVersion {
    work: u64,
    releases: usize,
}

struct State {
    work: usize,
    revision: u64,
    collected: CleanupVersion,
    observed_releases: usize,
    deadline: Option<Instant>,
    suspended: bool,
    closed: bool,
    #[cfg(not(target_arch = "wasm32"))]
    worker: Option<tokio::task::JoinHandle<()>>,
}

pub(crate) struct IdleGc {
    state: Mutex<State>,
    heap: Weak<BexHeap>,
    wake: Arc<Notify>,
}

/// Counts a root call, a registry entry, or a child producer's entire lifetime.
/// Registry and producer guards deliberately overlap: cancellation can settle
/// the future while its producer is still unwinding or waiting for admission.
pub(crate) struct WorkGuard {
    idle: Arc<IdleGc>,
    idle_due_on_entry: bool,
}

impl Drop for WorkGuard {
    fn drop(&mut self) {
        let mut s = self.idle.lock();
        s.work -= 1;
        s.revision = s.revision.wrapping_add(1);
        if s.work == 0 && !s.suspended && !s.closed {
            s.deadline = Some(Instant::now() + IDLE_DELAY);
        }
        drop(s);
        self.idle.wake.notify_one();
    }
}

impl IdleGc {
    pub(crate) fn new(heap: &Arc<BexHeap>) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                work: 0,
                revision: 0,
                collected: CleanupVersion {
                    work: 0,
                    releases: heap.root_release_epoch(),
                },
                observed_releases: heap.root_release_epoch(),
                deadline: None,
                suspended: false,
                closed: false,
                #[cfg(not(target_arch = "wasm32"))]
                worker: None,
            }),
            heap: Arc::downgrade(heap),
            wake: heap.gc_activity(),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn version(&self, s: &State) -> CleanupVersion {
        CleanupVersion {
            work: s.revision,
            releases: self
                .heap
                .upgrade()
                .map_or(s.observed_releases, |h| h.root_release_epoch()),
        }
    }

    fn refresh(&self, s: &mut State) {
        let current = self.version(s);
        if current.releases != s.observed_releases {
            s.observed_releases = current.releases;
            // A release while already waiting must not postpone the existing
            // deadline. On WASM it may only be observed at the next admission.
            if s.work == 0 && !s.suspended && !s.closed && current != s.collected {
                #[cfg(not(target_arch = "wasm32"))]
                let deadline = Instant::now() + IDLE_DELAY;
                // WASM has no observer while idle: service newly observed root
                // releases at this admission instead of delaying another call.
                #[cfg(target_arch = "wasm32")]
                let deadline = Instant::now();
                s.deadline.get_or_insert(deadline);
            }
        }
    }

    pub(crate) fn start_work(self: &Arc<Self>) -> WorkGuard {
        let mut s = self.lock();
        self.refresh(&mut s);
        let idle_due_on_entry = s.work == 0
            && !s.suspended
            && !s.closed
            && s.deadline.is_some_and(|d| d <= Instant::now())
            && self.version(&s) != s.collected;
        s.work += 1;
        s.deadline = None;
        drop(s);
        self.wake.notify_one();
        WorkGuard {
            idle: Arc::clone(self),
            idle_due_on_entry,
        }
    }

    pub(crate) fn cleanup_version(&self) -> CleanupVersion {
        self.version(&self.lock())
    }

    /// Called for every full GC while it still owns exclusive heap access.
    /// Never consume a root release or work completion newer than this token.
    pub(crate) fn collected(&self, version: CleanupVersion) {
        let mut s = self.lock();
        s.collected = version;
        if self.version(&s) == version {
            s.deadline = None;
        } else if s.work == 0 && !s.suspended && !s.closed {
            s.deadline.get_or_insert(Instant::now() + IDLE_DELAY);
        }
        drop(s);
        self.wake.notify_one();
    }

    pub(crate) fn suspend(&self, suspended: bool) {
        let mut s = self.lock();
        s.suspended = suspended;
        s.deadline = if !suspended && !s.closed && s.work == 0 && self.version(&s) != s.collected {
            Some(Instant::now() + IDLE_DELAY)
        } else {
            None
        };
        drop(s);
        self.wake.notify_one();
    }

    pub(crate) fn close(&self) {
        let mut s = self.lock();
        s.closed = true;
        s.deadline = None;
        drop(s);
        self.wake.notify_one();
    }

    pub(crate) async fn join_worker(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let worker = self.lock().worker.take();
            if let Some(worker) = worker {
                let _ = worker.await;
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn due(&self) -> bool {
        let mut s = self.lock();
        self.refresh(&mut s);
        !s.closed
            && !s.suspended
            && s.work == 0
            && s.deadline.is_some_and(|d| d <= Instant::now())
            && self.version(&s) != s.collected
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn run(self: Arc<Self>, engine: Weak<BexEngine>) {
        loop {
            // Arm the notification before reading state; notifications are
            // coalesced, state is authoritative. Drop this waiter before the
            // acquisition attempt creates its own waiter.
            let fired = {
                let notified = self.wake.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                let deadline = {
                    let mut s = self.lock();
                    if s.closed {
                        return;
                    }
                    self.refresh(&mut s);
                    s.deadline
                };
                match deadline {
                    Some(deadline) => tokio::select! {
                        biased;
                        () = &mut notified => false,
                        () = tokio::time::sleep_until(deadline) => true,
                    },
                    None => {
                        notified.await;
                        false
                    }
                }
            };
            if !fired {
                continue;
            }
            let Some(engine) = engine.upgrade() else {
                return;
            };
            Box::pin(engine.try_idle_gc()).await;
            drop(engine); // No strong engine reference across a timer/event wait.
            let mut s = self.lock();
            if s.deadline.is_some_and(|d| d <= Instant::now()) {
                // Keep cleanup pending after timeout/contention; one bounded
                // retry per idle delay, not a busy loop at an expired deadline.
                s.deadline = Some(Instant::now() + IDLE_DELAY);
            }
        }
    }
}

impl BexEngine {
    pub(crate) fn ensure_idle_gc_worker(self: &Arc<Self>) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut s = self.idle_gc.lock();
            if s.closed
                || s.worker
                    .as_ref()
                    .is_some_and(|worker| !worker.is_finished())
            {
                return;
            }
            // Embedders without a Tokio runtime still get entry/safe-point GC.
            // A later call on a runtime can start the single idle worker.
            let Ok(runtime) = tokio::runtime::Handle::try_current() else {
                return;
            };
            s.worker = Some(runtime.spawn(Arc::clone(&self.idle_gc).run(Arc::downgrade(self))));
        }
    }

    pub(crate) async fn collect_before_call(self: &Arc<Self>, work: &WorkGuard) {
        let idle_due = cfg!(target_arch = "wasm32") && work.idle_due_on_entry;
        if !idle_due && !self.heap.should_gc() {
            return;
        }
        if self
            .checking_gc
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::Acquire,
                std::sync::atomic::Ordering::Relaxed,
            )
            .is_err()
        {
            return;
        }
        let _checking = GcCheckGuard(&self.checking_gc);
        // No active heap permit yet. Incoming external values/handles own roots.
        if idle_due || self.heap.should_gc() {
            self.collect_garbage_with_reason(
                CollectionLevel::Major,
                if idle_due {
                    "idle_on_entry"
                } else {
                    "allocation_on_entry"
                },
            )
            .await;
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn try_idle_gc(self: &Arc<Self>) {
        let changed = self.idle_gc.wake.notified();
        tokio::pin!(changed);
        changed.as_mut().enable();
        if !self.idle_gc.due()
            || self
                .checking_gc
                .compare_exchange(
                    false,
                    true,
                    std::sync::atomic::Ordering::Acquire,
                    std::sync::atomic::Ordering::Relaxed,
                )
                .is_err()
        {
            return;
        }
        let _checking = GcCheckGuard(&self.checking_gc);
        let cycle = bex_heap::GcCycleProfiler::start();
        // Idle GC is passive: do not set the VM park-request flag. If an active
        // permit remains, cancel this request instead of forcing its VM to yield.
        let guard = tokio::select! {
            biased;
            () = &mut changed => return,
            acquired = tokio::time::timeout(PARK_WAIT, self.heap_permit_manager.request_park()) => {
                let Ok(guard) = acquired else { return; };
                guard
            }
        };
        // Admission updates the same lifecycle state before obtaining a permit.
        // Once this check succeeds, a later caller can wait for this collection.
        if !self.idle_gc.due() {
            return;
        }
        self.collect_garbage_parked(
            CollectionLevel::Major,
            "idle",
            guard,
            cycle,
        )
        .await;
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use bex_heap::HeapPermit;
    use bex_vm_types::{RealizedTy, Value};
    use sys_native::SysOpsExt;

    use super::*;
    use crate::{BexExternalValue as Ext, CancellationToken, FunctionCallContextBuilder};

    fn engine() -> Arc<BexEngine> {
        Arc::new(
            BexEngine::new(
                baml_db::testing::compile_source(
                    r#"
                    class Node { value int }
                    function Tiny() -> Node { Node { value: 7 } }
                    function Detached() -> int {
                        spawn with baml.spawn.options(detach = true) {
                            baml.sys.sleep(baml.time.Duration.from_milliseconds(1000n));
                            Tiny()
                        };
                        1
                    }
                    "#,
                ),
                Arc::new(sys_native::SysOps::native()),
                vec![],
            )
            .unwrap(),
        )
    }

    async fn tiny(engine: &Arc<BexEngine>) -> Ext {
        engine
            .call_function(
                "Tiny",
                vec![],
                FunctionCallContextBuilder::new(sys_types::CallId::next())
                    .suppress_internal_profile()
                    .build(),
                false,
            )
            .await
            .unwrap()
    }

    // Advance the paused clock only after spawned tasks have consumed queued
    // notifications. Yielding never waits for a timer or advances time itself.
    async fn settle() {
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    }

    async fn advance(duration: std::time::Duration) {
        settle().await;
        tokio::time::advance(duration).await;
        settle().await;
    }

    #[tokio::test(start_paused = true)]
    async fn burst_uses_one_worker_and_handle_release_rearms_cleanup() {
        let engine = engine();
        let kept = tiny(&engine).await;
        let worker = engine.idle_gc.lock().worker.as_ref().unwrap().id();
        for _ in 0..20 {
            drop(tiny(&engine).await);
            assert_eq!(engine.idle_gc.lock().worker.as_ref().unwrap().id(), worker);
        }
        advance(IDLE_DELAY / 2).await;
        assert_eq!(engine.heap.gc_budget().full_collections, 0);
        advance(IDLE_DELAY / 2).await;
        assert_eq!(engine.heap.gc_budget().full_collections, 1);
        assert!(engine.idle_gc.lock().deadline.is_none());
        assert_eq!(engine.heap.stats().active_handles, 1);
        advance(IDLE_DELAY * 10).await;
        assert_eq!(engine.heap.gc_budget().full_collections, 1);

        // No BAML call follows this bridge-handle release.
        drop(kept);
        advance(IDLE_DELAY).await;
        assert_eq!(engine.heap.gc_budget().full_collections, 2);
        assert_eq!(engine.heap.stats().active_handles, 0);
        assert!(engine.idle_gc.lock().deadline.is_none());
        engine.shutdown().await;
        assert!(engine.idle_gc.lock().worker.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn blocked_heap_times_out_without_parking_vms_and_retries() {
        let engine = engine();
        drop(tiny(&engine).await);
        let held = engine
            .heap_permit_manager
            .new_permit(())
            .await
            .acquire()
            .await;
        advance(IDLE_DELAY).await;
        assert!(
            engine
                .checking_gc
                .load(std::sync::atomic::Ordering::Acquire)
        );
        assert!(
            !engine
                .park_requested
                .load(std::sync::atomic::Ordering::Acquire)
        );
        advance(PARK_WAIT).await;
        assert!(
            !engine
                .checking_gc
                .load(std::sync::atomic::Ordering::Acquire)
        );
        assert_eq!(engine.heap.gc_budget().full_collections, 0);
        // Cancellation of acquire_many must release partial semaphore claims.
        let another = engine
            .heap_permit_manager
            .new_permit(())
            .await
            .acquire()
            .await;
        drop(another);
        drop(held);
        advance(IDLE_DELAY).await;
        assert_eq!(engine.heap.gc_budget().full_collections, 1);
        engine.shutdown().await;
    }

    #[tokio::test(start_paused = true)]
    async fn arriving_work_cancels_wait_and_restarts_quiet_interval() {
        let engine = engine();
        drop(tiny(&engine).await);
        let held = engine
            .heap_permit_manager
            .new_permit(())
            .await
            .acquire()
            .await;
        advance(IDLE_DELAY).await;
        assert!(
            engine
                .checking_gc
                .load(std::sync::atomic::Ordering::Acquire)
        );
        let work = engine.idle_gc.start_work();
        settle().await;
        assert!(
            !engine
                .checking_gc
                .load(std::sync::atomic::Ordering::Acquire)
        );
        drop(held);
        advance(IDLE_DELAY * 2).await;
        assert_eq!(engine.heap.gc_budget().full_collections, 0);
        drop(work);
        advance(IDLE_DELAY).await;
        assert_eq!(engine.heap.gc_budget().full_collections, 1);
        engine.shutdown().await;
    }

    #[tokio::test(start_paused = true)]
    async fn registry_and_producer_must_both_finish_including_cancel_and_error() {
        let engine = engine();
        drop(tiny(&engine).await);
        for outcome in 0..3 {
            let producer = engine.idle_gc.start_work();
            let permit = engine
                .heap_permit_manager
                .new_permit(())
                .await
                .acquire()
                .await;
            let id = {
                let mut futures = engine.futures.acquire(permit.proof()).await;
                let (id, _) = futures.new_future(
                    RealizedTy::int(),
                    RealizedTy::never(),
                    CancellationToken::new(),
                    "idle-gc-test".into(),
                );
                id
            };
            drop(permit);
            advance(IDLE_DELAY * 2).await;
            assert_eq!(engine.heap.gc_budget().full_collections, outcome);
            let permit = engine
                .heap_permit_manager
                .new_permit(())
                .await
                .acquire()
                .await;
            {
                let mut futures = engine.futures.acquire(permit.proof()).await;
                match outcome {
                    0 => futures.fulfill_future(id, Value::int(1)).unwrap(),
                    1 => futures.cancel_future(id).unwrap(),
                    _ => futures.err_future(id, Value::int(2), vec![]).unwrap(),
                }
            }
            drop(permit);
            // Settlement alone cannot claim a queued/cancelling producer exited.
            advance(IDLE_DELAY * 2).await;
            assert_eq!(engine.heap.gc_budget().full_collections, outcome);
            drop(producer);
            advance(IDLE_DELAY).await;
            assert_eq!(engine.heap.gc_budget().full_collections, outcome + 1);
        }
        engine.shutdown().await;
    }

    #[tokio::test(start_paused = true)]
    async fn detached_child_completion_rearms_after_root_has_returned() {
        let engine = engine();
        engine
            .call_function(
                "Detached",
                vec![],
                FunctionCallContextBuilder::new(sys_types::CallId::next())
                    .suppress_internal_profile()
                    .build(),
                true,
            )
            .await
            .unwrap();
        settle().await;
        assert!(engine.idle_gc.lock().work > 0);
        advance(IDLE_DELAY * 2).await;
        assert_eq!(engine.heap.gc_budget().full_collections, 0);
        advance(std::time::Duration::from_secs(1)).await;
        assert_eq!(engine.idle_gc.lock().work, 0);
        advance(IDLE_DELAY).await;
        assert_eq!(engine.heap.gc_budget().full_collections, 1);
        engine.shutdown().await;
    }

    #[tokio::test(start_paused = true)]
    async fn full_collection_consumes_only_the_version_it_observed() {
        let engine = engine();
        let kept = tiny(&engine).await;
        engine.collect_garbage(CollectionLevel::Major).await;
        advance(IDLE_DELAY * 2).await;
        assert_eq!(engine.heap.gc_budget().full_collections, 1);
        let collected = engine.idle_gc.cleanup_version();
        drop(kept);
        engine.idle_gc.collected(collected);
        advance(IDLE_DELAY).await;
        assert_eq!(engine.heap.gc_budget().full_collections, 2);
        engine.shutdown().await;
    }

    #[tokio::test(start_paused = true)]
    async fn cancelled_shutdown_resumes_worker_and_engine_drop_stops_it() {
        let engine = engine();
        drop(tiny(&engine).await);
        let worker = engine.idle_gc.lock().worker.as_ref().unwrap().id();
        // Exercise the same RAII transition as cancellation of shutdown().
        let shutdown = engine.begin_shutdown().await.unwrap();
        advance(IDLE_DELAY * 2).await;
        assert_eq!(engine.heap.gc_budget().full_collections, 0);
        drop(shutdown);
        advance(IDLE_DELAY).await;
        assert_eq!(engine.heap.gc_budget().full_collections, 1);
        assert_eq!(engine.idle_gc.lock().worker.as_ref().unwrap().id(), worker);
        let idle = Arc::clone(&engine.idle_gc);
        let weak = Arc::downgrade(&engine);
        drop(engine);
        settle().await;
        assert!(
            weak.upgrade().is_none(),
            "sleeping worker must not own engine"
        );
        assert!(idle.lock().closed);
        assert!(idle.lock().worker.as_ref().unwrap().is_finished());
    }

    #[tokio::test(start_paused = true)]
    async fn cooperative_entry_remembers_expired_idle_deadline() {
        // No worker: model WASM's bookkeeping and next-entry decision.
        let heap = BexHeap::new(vec![]);
        let idle = IdleGc::new(&heap);
        drop(idle.start_work());
        tokio::time::advance(IDLE_DELAY).await;
        let entry = idle.start_work();
        assert!(entry.idle_due_on_entry);
        let overlapping = idle.start_work();
        assert!(!overlapping.idle_due_on_entry);
    }
}
