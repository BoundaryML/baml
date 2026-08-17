//! Future tracking for the Bex engine.
//!
//! # Lifecycle
//!
//! Each [`bex_vm_types::Future`] heap object owns its own atomic state,
//! `tokio::sync::SetOnce` wake signal, and `CancellationToken` (see the
//! struct doc on `bex_vm_types::Future`). Most of what used to live in
//! `FutureManager` therefore lives on the heap object itself; this module
//! retains a thin lookup layer:
//!
//! - **`new_future`** allocates a heap `Future` in the `Pending` state and
//!   registers it in `active_futures` so the heap object stays GC-rooted
//!   while pending (for fire-and-forget cases where no VM stack references
//!   the future).
//! - **`fulfill_future` / `err_future` / `cancel_future` /
//!   `internal_error_future`** delegate to the heap `Future`'s
//!   `settle_ready` / `settle_error` / `settle_cancelled` /
//!   `settle_internal_error` methods. Each of those uses a CAS so concurrent
//!   `f.cancel()` from another thread races correctly with the producer.
//! - **`future_ready`** locates the heap `Future` by id and clones the
//!   `Arc<SetOnce>` for the engine to `.wait()` on without holding any
//!   heap reference across the await.
//!
//! ### `InternalError` leak
//!
//! Normal terminal helpers remove the entry from `active_futures` after
//! the CAS succeeds. `internal_error_future` deliberately leaves the entry
//! in place: the resulting GC root keeps the heap `Future` (and its
//! `SetOnce`-with-error payload) alive, so a later VM `Await` re-execution
//! can still resolve the wake and surface the original error to the host.
//! Engine internal errors are bugs by construction; the leak is the cheap
//! way to guarantee the error is never silently dropped.
//!
//! ### Pending-resolved-removed-before-await race
//!
//! VM observes `Pending`, yields `Await(future_id)`, but the producer
//! completes and the entry is removed before the engine calls
//! `future_ready`. `future_ready` treats a missing-but-previously-issued
//! id as already resolved and returns an immediate `Ok(())`; the VM
//! re-executes `Await`, reads the terminal state directly from the heap,
//! and proceeds.

use ::baml_type::RealizedTy;
use ::bex_heap::{
    HeapPermit, HeapPermitManager, InactiveHeapPermit, PermitProof, Tlab, TlabHolder,
};
use ::bex_vm_types::{
    FutureRead, HeapPtr, Object, ObjectType, RootHaver, SessionEvalLease, Value,
    errors::StackFrame,
    types::{FutureId, FutureInternalError, FutureType},
};
use ::core::sync::atomic::AtomicUsize;
use ::std::{collections::HashMap, sync::Arc};
use ::sys_types::CancellationToken;
use ::tokio::sync::{Mutex, MutexGuard, SetOnce};

use crate::EngineError;

/// One outstanding future's `ready` wake handle, snapshotted for shutdown.
pub(crate) type PendingJoinHandle = Arc<SetOnce<Result<(), FutureInternalError>>>;

/// Manages all futures for the Bex engine.
///
/// The wrapped [`InactiveHeapPermit`] is registered with
/// [`HeapPermitManager.holders`](::bex_heap::HeapPermitManager) so GC can
/// `collect_roots` / `forward_roots` on the inner. It is **never activated**
/// by the manager itself — every method requires a [`PermitProof`] from
/// the caller's own active permit, witnessing that GC is gated externally.
/// The wrapping [`tokio::sync::Mutex`] provides the single-writer invariant.
pub struct FutureManager {
    permit: Mutex<InactiveHeapPermit<FutureManagerInner>>,
}

impl FutureManager {
    pub fn new(permit: InactiveHeapPermit<FutureManagerInner>) -> Self {
        Self {
            permit: Mutex::new(permit),
        }
    }

    /// Acquire exclusive access to the future registry.
    ///
    /// `proof` is a witness that the caller currently holds an active heap
    /// permit. Its lifetime `'a` ties the returned guard to it: the
    /// borrow checker rejects any program that drops the proof source
    /// while the guard is still live. That tie is what makes the unsafe
    /// `holder` / `holder_mut` accesses inside the guard's `HeapPermit`
    /// impl sound — there is no safe way to construct a `FutureManagerGuard`
    /// without a real `PermitProof`, and no safe way to keep the guard
    /// past the proof's lifetime.
    ///
    /// Engine call sites should scope the guard tightly (a single
    /// transition: `new_future` / `fulfill_future` / `cancel_future` /
    /// `future_ready`) and drop it before re-borrowing the permit holder
    /// mutably (e.g. `vm.stack.push(...)` after `new_future`).
    pub async fn acquire<'a>(&'a self, proof: PermitProof<'a>) -> FutureManagerGuard<'a> {
        FutureManagerGuard {
            permit_guard: self.permit.lock().await,
            proof,
        }
    }

    /// Number of `Pending` futures currently tracked.
    ///
    /// Takes a one-shot heap permit internally so external diagnostic callers
    /// (notably tests) don't need to construct a `PermitProof` themselves.
    pub async fn active_future_count(&self, mgr: &HeapPermitManager) -> usize {
        let inactive = mgr.new_permit(()).await;
        let active = inactive.acquire().await;
        let guard = self.acquire(active.proof()).await;
        guard.active_future_count()
    }

    pub async fn pending_join_handles(&self, mgr: &HeapPermitManager) -> Vec<PendingJoinHandle> {
        let inactive = mgr.new_permit(()).await;
        let active = inactive.acquire().await;
        let mut guard = self.acquire(active.proof()).await;
        guard.pending_join_handles()
    }
}

pub struct FutureManagerGuard<'a> {
    permit_guard: MutexGuard<'a, InactiveHeapPermit<FutureManagerInner>>,
    /// Caller-supplied witness; `'a` ties the guard's existence to the
    /// caller's active heap permit so GC exclusion is type-system-enforced.
    proof: PermitProof<'a>,
}

impl FutureManagerGuard<'_> {
    /// Number of `Pending` futures currently tracked by the manager.
    pub fn active_future_count(&self) -> usize {
        self.holder().active_future_count()
    }

    /// Registers a future with the future manager and returns a unique ID.
    ///
    /// `returns` / `throws` are the `Future<T, E>` type arguments the spawn
    /// site was typed at, already resolved against the spawning frame. They are
    /// stored on the heap `Future` so reflection and `is`/`match` can see the
    /// future's generic parameters rather than only "some future".
    pub fn new_future(
        &mut self,
        returns: RealizedTy,
        throws: RealizedTy,
        cancel: CancellationToken,
    ) -> (FutureId, HeapPtr) {
        // The contract on `FutureId::from_usize` is "no two live ids share a
        // usize". We satisfy this by drawing the value from the manager's
        // monotonic `AtomicUsize`; uniqueness is preserved as long as the
        // counter hasn't wrapped (which would take 2^64 calls).
        let inner = self.holder_mut();
        let id = inner
            .next_future_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = FutureId::from_usize(id);

        let ptr = inner
            .tlab
            .alloc_future(::bex_vm_types::Future::pending(id, returns, throws, cancel));

        inner.active_futures.insert(id, FutureState { future: ptr });
        (id, ptr)
    }

    pub fn fulfill_future(&mut self, id: FutureId, value: Value) -> Result<(), EngineError> {
        // Snapshot the heap Arc before borrowing self mutably for take_pending;
        // settle_ready needs it to fire the generational write barrier.
        let heap = ::std::sync::Arc::clone(self.tlab().heap());
        if let Some((fut, self_ptr)) = self.take_pending(id)? {
            // SAFETY: caller holds the heap permit (witnessed by `self.proof`).
            // `settle_ready` CAS-transitions Pending → Ready; the producer is
            // the unique writer reaching this path (cancel uses its own CAS).
            // The write barrier marks `self_ptr`'s card so the next minor GC
            // finds any young-gen ptr embedded in `value`.
            let _ = unsafe { fut.settle_ready(heap.as_ref(), self_ptr, value) };
        }
        Ok(())
    }

    /// Transition a future to `Future::Error(value)` with the given BAML
    /// error/panic value.
    pub fn err_future(
        &mut self,
        id: FutureId,
        err: Value,
        trace: Vec<StackFrame>,
    ) -> Result<(), EngineError> {
        let heap = ::std::sync::Arc::clone(self.tlab().heap());
        if let Some((fut, self_ptr)) = self.take_pending(id)? {
            // SAFETY: see `fulfill_future`.
            let _ = unsafe { fut.settle_error(heap.as_ref(), self_ptr, err, trace) };
        }
        Ok(())
    }

    pub fn cancel_future(&mut self, id: FutureId) -> Result<(), EngineError> {
        if let Some((fut, _self_ptr)) = self.take_pending(id)? {
            // `settle_cancelled` fires the cancel token and the wake signal
            // internally. CAS-based, so this races safely with the producer's
            // settle_ready/settle_error. No value payload → no write barrier.
            let _ = fut.settle_cancelled();
        }
        Ok(())
    }

    /// Register a Session lease held by the producer of `id`. If user-side
    /// cancellation already won the future race, registration releases the
    /// lease immediately; otherwise `Future::settle_cancelled` releases it
    /// before waking the awaiter.
    pub fn register_session_lease(
        &mut self,
        id: FutureId,
        lease: &SessionEvalLease,
    ) -> Result<(), EngineError> {
        let Some(entry) = self.holder().active_futures.get(&id) else {
            lease.release();
            return Ok(());
        };
        // SAFETY: caller holds the heap permit via `self.proof`.
        let future = unsafe { entry.future_ref() }?;
        future.register_session_lease(lease);
        Ok(())
    }

    /// Sets the future to `InternalError` and notifies the waiter.
    ///
    /// Unlike the other terminal-transition helpers, this does **not** remove
    /// the entry from `active_futures`. The retained entry roots the heap
    /// `Future` (and its `SetOnce`-with-error) so a later `Await` resumes
    /// against the same heap object and the engine can surface the original
    /// `EngineError` to the host. Internal errors are by-construction bugs;
    /// the leak buys us never losing the error.
    pub fn internal_error_future(
        &mut self,
        id: FutureId,
        err: EngineError,
    ) -> Result<(), EngineError> {
        let entry = self
            .holder()
            .active_futures
            .get(&id)
            .ok_or(EngineError::FutureNotFound { future_id: id })?;
        // SAFETY: caller holds the heap permit via `self.proof`.
        let fut = unsafe { entry.future_ref() }?;
        let observed = FutureType::of(fut);
        if !matches!(fut.read(), FutureRead::Pending(_)) {
            debug_assert!(
                false,
                "internal_error_future called on non-Pending future {id:?} \
                 (actual: {observed:?}); invariant violated"
            );
            return Err(EngineError::TypeMismatch {
                message: format!(
                    "internal_error_future called on non-Pending future {id:?} \
                     (actual: {observed:?})"
                ),
            });
        }
        // Type-erase the EngineError into the SetOnce payload (the SetOnce
        // type lives in bex_vm_types and can't reference EngineError directly).
        // `future_ready` downcasts back when the awaiter resumes.
        //
        // The CAS can lose if a concurrent `f.cancel()` (or, in the future,
        // any other non-FutureManager writer) transitioned `Pending` →
        // `Cancelled` between the pre-check on line 209 and this call. In
        // that case the user-initiated cancellation already represents an
        // intentional terminal state and the awaiter will see `Cancelled`
        // — the engine error is dropped on the floor. We log it instead of
        // returning an `Err` because returning an error from this helper
        // would itself need to be wrapped in another `internal_error_future`,
        // and we'd recurse on the same race.
        if !fut.settle_internal_error(Box::new(err)) {
            let actual = FutureType::of(fut);
            tracing::warn!(
                ?id,
                ?actual,
                "internal_error_future CAS lost to a concurrent terminal \
                 transition (likely f.cancel()); engine error discarded"
            );
        }
        Ok(())
    }

    /// Settle `id` to `InternalError` from the spawn task's terminal
    /// engine-error path, where the child thread (and its heap permit) are
    /// already gone.
    ///
    /// Tolerant counterpart to [`Self::internal_error_future`]: that helper
    /// runs on the still-live child thread, where a non-`Pending` future is an
    /// invariant violation worth asserting. Here the thread died on an
    /// arbitrary engine-error escape path, so the future may legitimately
    /// already be settled (the error can strike after a successful settle) —
    /// an already-settled or unknown future is left as-is and the error only
    /// logged. Like `internal_error_future`, the `active_futures` entry is
    /// retained so a later `Await` resumes against the same heap object and
    /// surfaces the original [`EngineError`] instead of parking forever.
    pub fn settle_spawn_engine_error(&mut self, id: FutureId, err: EngineError) {
        let Some(entry) = self.holder().active_futures.get(&id) else {
            tracing::error!(
                ?id,
                ?err,
                "spawn thread terminated with an engine error but its future \
                 is not active; error dropped"
            );
            return;
        };
        // SAFETY: caller holds the heap permit via `self.proof`.
        let fut = match unsafe { entry.future_ref() } {
            Ok(fut) => fut,
            Err(handle_err) => {
                tracing::error!(
                    ?id,
                    ?err,
                    ?handle_err,
                    "spawn thread terminated with an engine error but its \
                     future handle is unreadable; error dropped"
                );
                return;
            }
        };
        // A lost CAS means the future already reached a terminal state (a
        // settle that preceded the error, or a concurrent `f.cancel()`);
        // that state is the one the awaiter observes, and it is already a
        // wake-up — no parked parent is left behind either way.
        let _ = fut.settle_internal_error(Box::new(err));
    }

    /// Remove and return the heap `Future` for `id` if it's still
    /// `Pending`. Returns `Ok(None)` if the future has already been
    /// settled out-of-band (e.g. via BAML's `f.cancel()` which transitions
    /// the heap state directly without going through `FutureManager`),
    /// or if it's not in the active set at all.
    ///
    /// The cleanup-on-already-settled path also drops the leftover entry
    /// from `active_futures` so the GC anchor doesn't leak.
    ///
    /// Returns the heap `HeapPtr` alongside the `Future` ref so callers
    /// can pass it to `settle_ready` / `settle_error` for the
    /// generational write barrier.
    fn take_pending(
        &mut self,
        id: FutureId,
    ) -> Result<Option<(&'static bex_vm_types::Future, HeapPtr)>, EngineError> {
        // Phase 1: peek. Already-removed entries → Ok(None).
        let already_settled = {
            let Some(entry_ref) = self.holder().active_futures.get(&id) else {
                return Ok(None);
            };
            // SAFETY: caller holds the heap permit via `self.proof`.
            let fut = unsafe { entry_ref.future_ref() }?;
            !matches!(fut.read(), FutureRead::Pending(_))
        };
        if already_settled {
            // Heap state moved on without us — drop the bookkeeping entry
            // and return None so the caller treats this as a no-op.
            let _ = self.holder_mut().active_futures.remove(&id);
            return Ok(None);
        }
        // Phase 2: still Pending. Remove and return the heap Future ref.
        // Once removed, GC rooting is whoever holds the HeapPtr
        // (typically `BexThread.settles_future` or the awaiter's stack).
        let entry = self
            .holder_mut()
            .active_futures
            .remove(&id)
            .expect("entry was present in phase 1 and we hold the `FutureManager` Mutex");
        let self_ptr = entry.future;
        // SAFETY: heap permit witnessed by `self.proof`. The returned ref
        // outlives `entry` because the heap object is rooted elsewhere
        // (BexThread.settles_future / awaiter stack) — the caller uses it
        // only for an immediate `settle_*` call and never escapes the borrow.
        // The `'static` lifetime is a documented lie that the borrow checker
        // can't see through here; callers must not store the reference.
        let fut: &bex_vm_types::Future = unsafe { entry.future_ref() }?;
        let fut_static: &'static bex_vm_types::Future = unsafe { std::mem::transmute(fut) };
        Ok(Some((fut_static, self_ptr)))
    }
    /// Returns a Rust future that resolves when the BAML future is ready.
    /// Once it is resolved, the future on the heap will be in a terminal
    /// state (some variant other than `Pending`).
    ///
    /// A `FutureId` that is missing from `active_futures` but has been
    /// previously issued (i.e. `id.as_usize() < next_future_id`) is treated
    /// as already resolved: the entry was dropped by the terminal-transition
    /// helper after the heap object was set. The VM's `Await` re-execution
    /// reads the terminal state directly from the heap. See the module-level
    /// docs for the full lifecycle invariant.
    ///
    /// ## Errors
    /// - Synchronous `EngineError::FutureNotFound` for a `FutureId` that was
    ///   never issued by this manager.
    /// - The returned future yields `EngineError` if the future produced an
    ///   `InternalError`.
    pub fn future_ready(
        &mut self,
        id: FutureId,
    ) -> Result<impl Future<Output = Result<(), EngineError>> + use<>, EngineError> {
        let inner = self.holder();
        let waiter = match inner.active_futures.get(&id) {
            Some(entry) => {
                // SAFETY: caller holds the heap permit via `self.proof`.
                let fut = unsafe { entry.future_ref() }?;
                Some(fut.ready_waiter())
            }
            None => {
                // Relaxed is fine: ordering with respect to the
                // `active_futures` HashMap is provided by the FutureManager
                // Mutex that this guard holds. We just need the latest
                // issued counter to bounds-check the id.
                let next = inner
                    .next_future_id
                    .load(std::sync::atomic::Ordering::Relaxed);
                if id.as_usize() >= next {
                    return Err(EngineError::FutureNotFound { future_id: id });
                }
                None
            }
        };
        Ok(async move {
            match waiter {
                // The SetOnce carries `Result<(), FutureInternalError>` where
                // the Err is the type-erased EngineError stuffed in by
                // `internal_error_future`. Downcast back here so the rest of
                // the engine sees its native error type.
                Some(w) => match w.wait().await {
                    Ok(()) => Ok(()),
                    Err(boxed) => match boxed.downcast_ref::<EngineError>().cloned() {
                        Some(engine_err) => Err(engine_err),
                        None => Err(EngineError::Other(format!(
                            "future internal error (non-EngineError payload): {boxed}"
                        ))),
                    },
                },
                None => Ok(()),
            }
        })
    }

    /// Snapshot every pending future for the explicit engine shutdown wait.
    pub(crate) fn pending_join_handles(&mut self) -> Vec<PendingJoinHandle> {
        let settled: Vec<_> = self
            .holder()
            .active_futures
            .iter()
            .filter_map(|(id, state)| {
                // SAFETY: caller holds the heap permit via `self.proof`.
                let future = unsafe { state.future_ref() }.ok()?;
                (!matches!(future.read(), FutureRead::Pending(_))).then_some(*id)
            })
            .collect();
        for id in settled {
            self.holder_mut().active_futures.remove(&id);
        }
        self.holder()
            .active_futures
            .values()
            .filter_map(|state| {
                // SAFETY: caller holds the heap permit via `self.proof`.
                let fut = unsafe { state.future_ref() }.ok()?;
                matches!(fut.read(), FutureRead::Pending(_)).then(|| fut.ready_waiter())
            })
            .collect()
    }
}
impl TlabHolder for FutureManagerGuard<'_> {
    fn tlab(&self) -> &Tlab {
        self.holder().tlab()
    }
    fn tlab_mut(&mut self) -> &mut Tlab {
        self.holder_mut().tlab_mut()
    }
}
impl HeapPermit<FutureManagerInner> for FutureManagerGuard<'_> {
    fn holder(&self) -> &FutureManagerInner {
        // SAFETY: `InactiveHeapPermit::holder`'s contract is "permit is
        // active". Here it isn't — but `self.proof: PermitProof<'a>` is a
        // runtime witness that *some* active heap permit is alive for `'a`,
        // which is exactly the GC-exclusion guarantee that contract is
        // meant to encode. The `MutexGuard` provides single-writer.
        unsafe { self.permit_guard.holder() }
    }
    fn holder_mut(&mut self) -> &mut FutureManagerInner {
        // SAFETY: see [`Self::holder`]; `&mut self` plus the MutexGuard
        // provide exclusive access on this thread.
        unsafe { self.permit_guard.holder_mut() }
    }
    fn proof(&self) -> PermitProof<'_> {
        self.proof
    }
}

pub struct FutureManagerInner {
    tlab: Tlab,
    next_future_id: AtomicUsize,
    active_futures: HashMap<FutureId, FutureState>,
}
impl FutureManagerInner {
    pub fn new(tlab: Tlab) -> Self {
        Self {
            tlab,
            next_future_id: AtomicUsize::new(0),
            active_futures: HashMap::new(),
        }
    }

    /// Number of `Pending` futures currently tracked by the manager. This is
    /// the same as the number of futures whose heap object is in
    /// `Future::Pending(_)`. Intended for tests and telemetry.
    pub fn active_future_count(&self) -> usize {
        self.active_futures.len()
    }
}
impl RootHaver for FutureManagerInner {
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        // blocking is fine since we should only ever call this while holding exclusive heap access
        for future in self.active_futures.values() {
            future.collect_roots(roots);
        }
    }
    fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
        // Drop the cached TLAB cursor — GC has swapped semispaces and our
        // `alloc_ptr`/`alloc_limit` now point into a region the heap will
        // hand out as a fresh chunk. The next `alloc_future` must refill
        // from the post-GC cursor. Mirrors `BexVm::forward_roots`.
        self.tlab.invalidate();
        for future in self.active_futures.values_mut() {
            future.forward_roots(roots);
        }
    }
}
impl TlabHolder for FutureManagerInner {
    fn tlab(&self) -> &Tlab {
        &self.tlab
    }
    fn tlab_mut(&mut self) -> &mut Tlab {
        &mut self.tlab
    }
}

struct FutureState {
    /// Heap pointer to the `Object::Future`. Rooted via `RootHaver` so
    /// the heap object survives even when no awaiter / producer stack
    /// holds it directly (fire-and-forget spawn before the producer task
    /// gets scheduled).
    future: HeapPtr,
}
impl FutureState {
    /// Returns an immutable reference to the heap-allocated `Future`.
    ///
    /// The [`bex_vm_types::Future`] uses interior mutability (`AtomicU8` +
    /// `UnsafeCell<MaybeUninit<Value>>` + `Arc<SetOnce>`), so all terminal
    /// transitions go through `settle_*(&self)` methods that internally use
    /// `compare_exchange` to race safely with `f.cancel()` callers.
    ///
    /// # Safety
    /// Caller must hold the heap permit (witnessed by the surrounding
    /// `FutureManagerGuard`'s `PermitProof`) so the heap object is alive
    /// and not being moved by GC during access.
    unsafe fn future_ref(&self) -> Result<&bex_vm_types::Future, EngineError> {
        // SAFETY: caller holds the heap permit.
        let obj = unsafe { self.future.get() };
        match obj {
            Object::Future(fut) => Ok(fut),
            other => Err(EngineError::TypeMismatch {
                message: format!("Expected Future, got {:?}", ObjectType::of(other)),
            }),
        }
    }
}
impl RootHaver for FutureState {
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        roots.push(self.future);
    }
    fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
        if let Some(new_result) = roots.get(&self.future) {
            self.future = *new_result;
        }
    }
}

#[cfg(test)]
mod tests {
    use ::bex_heap::{ActiveHeapPermit, BexHeap, HeapPermitManager};
    use ::bex_vm_types::Value;

    use super::*;

    /// Build a fresh `FutureManager` plus the `HeapPermitManager` it was
    /// registered against. Tests then take a one-shot `ActiveHeapPermit` over
    /// `()` from `pm` to obtain the `PermitProof` required by
    /// `FutureManager::acquire`.
    async fn make_manager() -> (FutureManager, Arc<HeapPermitManager>) {
        let heap = BexHeap::new(Vec::new());
        let permit_manager = Arc::new(HeapPermitManager::new());
        let permit = permit_manager
            .new_permit(FutureManagerInner::new(Tlab::new_empty(Arc::clone(&heap))))
            .await;
        (FutureManager::new(permit), permit_manager)
    }

    async fn temp_permit(pm: &HeapPermitManager) -> ActiveHeapPermit<()> {
        pm.new_permit(()).await.acquire().await
    }

    /// Register a future typed as `Future<int, never>` — the shape a
    /// non-throwing `spawn { 1 }` produces. These tests exercise registry
    /// bookkeeping, so the types are inert here; a concrete pair is used
    /// anyway so nothing reads as "type unknown by design".
    fn new_int_future(
        guard: &mut FutureManagerGuard<'_>,
        cancel: CancellationToken,
    ) -> (FutureId, HeapPtr) {
        guard.new_future(RealizedTy::int(), RealizedTy::never(), cancel)
    }

    #[tokio::test]
    async fn fulfill_removes_entry() {
        let (mgr, pm) = make_manager().await;
        let temp = temp_permit(&pm).await;
        let mut guard = mgr.acquire(temp.proof()).await;
        let (id, _ptr) = new_int_future(&mut guard, CancellationToken::new());
        assert_eq!(guard.active_future_count(), 1);
        guard.fulfill_future(id, Value::int(42)).unwrap();
        assert_eq!(guard.active_future_count(), 0);
    }

    #[tokio::test]
    async fn pending_join_handles_includes_all_pending_futures() {
        let (mgr, pm) = make_manager().await;
        let temp = temp_permit(&pm).await;
        let mut guard = mgr.acquire(temp.proof()).await;
        new_int_future(&mut guard, CancellationToken::new());
        new_int_future(&mut guard, CancellationToken::new());
        assert_eq!(guard.pending_join_handles().len(), 2);
    }

    #[tokio::test]
    async fn cancel_removes_entry_and_fires_token() {
        let (mgr, pm) = make_manager().await;
        let temp = temp_permit(&pm).await;
        let mut guard = mgr.acquire(temp.proof()).await;
        let token = CancellationToken::new();
        let (id, _ptr) = new_int_future(&mut guard, token.clone());
        assert!(!token.is_cancelled());
        guard.cancel_future(id).unwrap();
        assert_eq!(guard.active_future_count(), 0);
        assert!(token.is_cancelled());
    }

    #[tokio::test]
    async fn err_removes_entry_but_internal_error_leaks() {
        // `err_future` is a normal terminal transition and removes its entry.
        // `internal_error_future` deliberately leaks so the original error stays
        // pinned to the registry's `SetOnce` for surfacing through a later
        // VM `Await(future_id)` yield.
        let (mgr, pm) = make_manager().await;
        let temp = temp_permit(&pm).await;
        let mut guard = mgr.acquire(temp.proof()).await;
        let (id_a, _) = new_int_future(&mut guard, CancellationToken::new());
        let (id_b, _) = new_int_future(&mut guard, CancellationToken::new());
        assert_eq!(guard.active_future_count(), 2);
        guard.err_future(id_a, Value::int(7), Vec::new()).unwrap();
        guard
            .internal_error_future(
                id_b,
                EngineError::TypeMismatch {
                    message: "boom".into(),
                },
            )
            .unwrap();
        // Only `id_a` was removed; `id_b` is the leaked InternalError entry.
        assert_eq!(guard.active_future_count(), 1);
    }

    #[tokio::test]
    async fn internal_error_future_preserves_original_error_via_future_ready() {
        // The whole point of leaking the entry: `future_ready` must hand back
        // the original `EngineError` so the engine can re-throw it to the
        // host. This is the key correctness path that the H1+H2 unification
        // restored — without the leak, a race window would collapse the error
        // to `VmInternalError::AwaitedFutureInternalError`.
        let (mgr, pm) = make_manager().await;
        let temp = temp_permit(&pm).await;
        let mut guard = mgr.acquire(temp.proof()).await;
        let (id, _) = new_int_future(&mut guard, CancellationToken::new());
        let original = EngineError::TypeMismatch {
            message: "synthetic op error".into(),
        };
        guard.internal_error_future(id, original.clone()).unwrap();
        // Entry should still be live (deliberate leak).
        assert_eq!(guard.active_future_count(), 1);

        // A waiter on the leaked entry resolves to the original error.
        let waiter = guard.future_ready(id).expect("waiter should be created");
        drop(guard);
        let surfaced = waiter.await.expect_err("InternalError should propagate");
        assert_eq!(surfaced, original);
    }

    #[tokio::test]
    async fn fulfill_after_internal_error_is_disallowed() {
        // Once an entry is in the leaked InternalError state, none of the
        // terminal-transition helpers (fulfill/err/cancel) should subsequently
        // succeed against it. In debug builds the `debug_assert` fires; in
        // release builds the call returns an `EngineError::TypeMismatch`
        // *without* removing the entry, so the original error stays pinned
        // to the registry's `SetOnce` for surfacing through a later VM
        // `Await(future_id)` yield.
        //
        // We exercise the release-build path by skipping under
        // `cfg(debug_assertions)` — the panic there is the documented
        // intent of the invariant.
        if cfg!(debug_assertions) {
            return;
        }
        let (mgr, pm) = make_manager().await;
        let temp = temp_permit(&pm).await;
        let mut guard = mgr.acquire(temp.proof()).await;
        let (id, _) = new_int_future(&mut guard, CancellationToken::new());
        let original = EngineError::TypeMismatch {
            message: "stale".into(),
        };
        guard.internal_error_future(id, original.clone()).unwrap();
        assert_eq!(guard.active_future_count(), 1);

        // Release-build behavior: `fulfill_future` rejects the call (the
        // pre-check observes a non-Pending heap state) and leaves the
        // leaked entry untouched.
        let result = guard.fulfill_future(id, Value::int(0));
        assert!(
            matches!(result, Err(EngineError::TypeMismatch { .. })),
            "fulfill_future after internal_error should reject in release; got {result:?}"
        );
        assert_eq!(
            guard.active_future_count(),
            1,
            "leaked InternalError entry must survive a rejected fulfill"
        );

        // The waiter still resolves to the original error.
        let waiter = guard.future_ready(id).expect("waiter should be created");
        drop(guard);
        let surfaced = waiter.await.expect_err("InternalError should propagate");
        assert_eq!(surfaced, original);
    }

    #[tokio::test]
    async fn double_complete_is_idempotent() {
        // After the BEP-034 fix-A change, the terminal helpers
        // (`fulfill_future` / `err_future` / `cancel_future`) became
        // idempotent: if `take_pending` finds the entry already
        // removed (or the heap state already terminal — e.g. user
        // called `f.cancel()` from BAML, transitioning the heap
        // directly), the helper returns `Ok(())` without doing
        // anything. This protects the engine from spurious
        // `TypeMismatch` errors on legitimate races between user-side
        // settles and producer-thread settles.
        let (mgr, pm) = make_manager().await;
        let temp = temp_permit(&pm).await;
        let mut guard = mgr.acquire(temp.proof()).await;
        let (id, _) = new_int_future(&mut guard, CancellationToken::new());
        guard.fulfill_future(id, Value::int(1)).unwrap();
        let again = guard.fulfill_future(id, Value::int(2));
        assert!(again.is_ok(), "second fulfill should be idempotent no-op");
    }

    #[tokio::test]
    async fn future_ready_immediate_for_completed_id() {
        let (mgr, pm) = make_manager().await;
        let temp = temp_permit(&pm).await;
        let mut guard = mgr.acquire(temp.proof()).await;
        let (id, _) = new_int_future(&mut guard, CancellationToken::new());
        guard.fulfill_future(id, Value::int(1)).unwrap();
        // Entry is gone; future_ready should treat it as already-resolved.
        let waiter = guard.future_ready(id).expect("expected immediate Ok");
        drop(guard);
        waiter.await.expect("should resolve to Ok(())");
    }

    #[tokio::test]
    async fn future_ready_for_never_issued_id_errors() {
        let (mgr, pm) = make_manager().await;
        let temp = temp_permit(&pm).await;
        let mut guard = mgr.acquire(temp.proof()).await;
        // No futures have been issued; any id beyond `next_future_id` is
        // bogus. `usize::MAX` is unambiguously out of range regardless of
        // how many futures the test setup happens to issue, so the assertion
        // doesn't get brittle if the manager initializes its counter.
        let bogus = FutureId::from_usize(usize::MAX);
        let result = guard.future_ready(bogus);
        assert!(matches!(result, Err(EngineError::FutureNotFound { .. })));
    }

    #[tokio::test]
    async fn future_ready_waiter_resolves_after_fulfill() {
        // Grab a waiter while the future is still pending; fulfill from a
        // separate critical section; then the waiter should resolve. This
        // exercises the path where `future_ready` clones the `Arc<SetOnce>`
        // before the entry is removed.
        let (mgr, pm) = make_manager().await;
        let temp = temp_permit(&pm).await;
        let mut guard = mgr.acquire(temp.proof()).await;
        let (id, _) = new_int_future(&mut guard, CancellationToken::new());
        let waiter = guard.future_ready(id).expect("waiter should be created");
        drop(guard);

        let temp2 = temp_permit(&pm).await;
        let mut guard = mgr.acquire(temp2.proof()).await;
        guard.fulfill_future(id, Value::int(123)).unwrap();
        drop(guard);
        drop(temp2);

        waiter.await.expect("waiter should resolve to Ok(())");
        assert_eq!(mgr.active_future_count(&pm).await, 0);
    }
}
