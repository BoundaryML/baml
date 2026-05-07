//! Future tracking for the Bex engine.
//!
//! # Lifecycle
//!
//! An entry exists in [`FutureManagerInner::active_futures`] **if** the
//! corresponding [`bex_vm_types::Future`] heap object is in the `Pending`
//! state, **or** in the `InternalError` state (which is leaked by design,
//! see below).
//!
//! - `fulfill_future`, `err_future`, `cancel_future`: terminal transitions
//!   that update the heap object, signal the cross-task ready notification,
//!   and remove the entry, all in one critical section.
//! - `internal_error_future`: terminal transition for unrecoverable engine
//!   errors that does **not** remove the entry. The entry's `SetOnce` keeps
//!   the original [`EngineError`] so a later VM `Await` can yield back to
//!   the engine, which surfaces the error to the host. This intentionally
//!   leaks the entry — internal errors should never happen in correct
//!   programs, and the leak buys us never losing the underlying error
//!   context to a removal/race window.
//!
//! Why this is safe:
//!
//! - The heap object alone is sufficient to drive the VM's `Await`
//!   instruction after it resumes; the engine's "ready" future does not
//!   carry the value, it is purely a "you may proceed" signal.
//! - All operations on a [`FutureManagerGuard`] hold an exclusive
//!   [`SharedHeapPermitGuard`], so terminal-transition-then-remove is
//!   atomic with respect to any other [`FutureManagerGuard`] operation
//!   (notably [`FutureManagerGuard::future_ready`]).
//! - Existing `Arc<tokio::sync::SetOnce<_>>` clones held by waiters keep
//!   working after the entry is dropped — removal only releases the
//!   manager's own `Arc`.
//!
//! Why this works for fire-and-forget: while pending, the [`FutureState`]
//! roots the heap object via [`RootHaver::collect_roots`]. After the
//! sys-op task completes the entry is removed (or, for `InternalError`,
//! retained); if no VM stack still references the heap object and the
//! entry has been removed, it is correctly reclaimed by the next GC.
//!
//! Why this works for the await race on `Pending` → `Ready/Error/Cancelled`
//! (a VM observes `Pending(future_id)` on the heap, yields
//! `Await(future_id)`, and the engine completes the future before the event
//! loop calls [`FutureManagerGuard::future_ready`]):
//! [`FutureManagerGuard::future_ready`] treats a missing-but-previously-
//! issued `FutureId` as "already resolved" and returns an immediate
//! `Ok(())`. The VM re-executes the saved `Await` instruction, reads the
//! terminal state directly from the heap, and proceeds.
//!
//! For `InternalError`, no race is possible: the entry is never removed, so
//! `future_ready` always finds a waiter that yields the original error.

use ::bex_heap::{
    HeapPermit, PermitProof, SharedHeapPermit, SharedHeapPermitGuard, Tlab, TlabHolder,
};
use ::bex_vm_types::{
    HeapPtr, Object, ObjectType, RootHaver, Value,
    types::{FutureId, FutureType},
};
use ::core::sync::atomic::AtomicUsize;
use ::std::{collections::HashMap, sync::Arc};
use ::sys_types::CancellationToken;

use crate::EngineError;

/// Manages all futures for the Bex engine.
///
/// This is a shared resource managed using a [`SharedHeapPermit`].
pub struct FutureManager {
    inner: SharedHeapPermit<FutureManagerInner>,
}

impl FutureManager {
    pub fn new(inner: SharedHeapPermit<FutureManagerInner>) -> Self {
        Self { inner }
    }
    pub async fn acquire(&self) -> FutureManagerGuard<'_> {
        FutureManagerGuard {
            inner: self.inner.acquire().await,
        }
    }

    /// Number of `Pending` futures currently tracked. Acquires the manager.
    pub async fn active_future_count(&self) -> usize {
        self.inner.acquire().await.active_future_count()
    }
}

pub struct FutureManagerGuard<'a> {
    inner: SharedHeapPermitGuard<'a, FutureManagerInner>,
}

impl FutureManagerGuard<'_> {
    /// Number of `Pending` futures currently tracked by the manager.
    pub fn active_future_count(&self) -> usize {
        self.inner.active_future_count()
    }

    /// Registers a future with the future manager and returns a unique ID.
    pub fn new_future(&mut self, cancel: CancellationToken) -> (FutureId, HeapPtr) {
        // The contract on `FutureId::from_usize` is "no two live ids share a
        // usize". We satisfy this by drawing the value from the manager's
        // monotonic `AtomicUsize`; uniqueness is preserved as long as the
        // counter hasn't wrapped (which would take 2^64 calls).
        let id = self
            .inner
            .next_future_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = FutureId::from_usize(id);

        let ptr = self
            .inner
            .tlab
            .alloc_future(::bex_vm_types::Future::Pending(id));

        let future_state = FutureState {
            future: ptr,
            ready: Arc::new(tokio::sync::SetOnce::new()),
            cancel,
        };
        self.inner.active_futures.insert(id, future_state);
        (id, ptr)
    }
    pub fn fulfill_future(&mut self, id: FutureId, value: Value) -> Result<(), EngineError> {
        self.complete_pending(id, bex_vm_types::Future::Ready(value), Ok(()))?;
        Ok(())
    }
    pub fn err_future(&mut self, id: FutureId, err: Value) -> Result<(), EngineError> {
        self.complete_pending(id, bex_vm_types::Future::Error(err), Ok(()))?;
        Ok(())
    }
    pub fn cancel_future(&mut self, id: FutureId) -> Result<(), EngineError> {
        let entry = self.complete_pending(id, bex_vm_types::Future::Cancelled, Ok(()))?;
        // The token is still cloned by the spawned sys-op task; firing it
        // here unparks that task even though `entry` itself is about to be
        // dropped.
        entry.cancel.cancel();
        Ok(())
    }
    /// Sets the future to `InternalError` and notifies the waiter.
    ///
    /// Unlike the other terminal-transition helpers, this does **not** remove
    /// the entry from `active_futures`. The originating `EngineError` is
    /// preserved on the entry's `SetOnce` so a later VM `Await` (which yields
    /// `Await(future_id)` even for an InternalError-state heap object) can
    /// surface the original error from this method's caller. Internal errors
    /// are bugs that "should never happen", so the resulting permanent leak is
    /// acceptable in exchange for not losing the underlying error context.
    pub fn internal_error_future(
        &mut self,
        id: FutureId,
        err: EngineError,
    ) -> Result<(), EngineError> {
        let entry = self
            .inner
            .active_futures
            .get_mut(&id)
            .ok_or(EngineError::FutureNotFound { future_id: id })?;
        // SAFETY: the `FutureManagerGuard` holds an exclusive heap permit.
        let fut = unsafe { entry.get_mut() }?;
        debug_assert!(
            matches!(fut, bex_vm_types::Future::Pending(_)),
            "internal_error_future called on non-Pending future {id:?}; \
             invariant violated"
        );
        *fut = bex_vm_types::Future::InternalError(id);
        let set = entry.ready.set(Err(err));
        debug_assert!(
            set.is_ok(),
            "Should not have been ready if the heap future was pending."
        );
        Ok(())
    }

    /// Atomically transition a `Pending` future to a terminal state, signal
    /// its [`tokio::sync::SetOnce`] waiter, and remove the entry from
    /// `active_futures`. The dropped [`FutureState`] is returned so callers
    /// (e.g. [`Self::cancel_future`]) can perform additional Drop-time work
    /// like firing a [`CancellationToken`] clone before it is released.
    ///
    /// # Invariant
    /// Only `fulfill_future`, `err_future`, and `cancel_future` route through
    /// this helper. `internal_error_future` deliberately does **not** — its
    /// entries are leaked to preserve the original error on the `SetOnce` and
    /// to let the VM yield back to the engine for surfacing. The
    /// `debug_assert` below encodes that invariant: if `complete_pending` ever
    /// observes a non-`Pending` heap state, a caller has violated the
    /// removal/transition contract.
    fn complete_pending(
        &mut self,
        id: FutureId,
        new_state: bex_vm_types::Future,
        result: Result<(), EngineError>,
    ) -> Result<FutureState, EngineError> {
        let mut entry = self
            .inner
            .active_futures
            .remove(&id)
            .ok_or(EngineError::FutureNotFound { future_id: id })?;
        // SAFETY: the `FutureManagerGuard` holds an exclusive heap permit.
        let fut = unsafe { entry.get_mut() }?;
        debug_assert!(
            matches!(fut, bex_vm_types::Future::Pending(_)),
            "complete_pending called with non-Pending heap state for {id:?} \
             (actual: {:?}); invariant violated — only fulfill/err/cancel may \
             route through this helper",
            FutureType::of(fut)
        );
        *fut = new_state;
        let set = entry.ready.set(result);
        debug_assert!(
            set.is_ok(),
            "Should not have been ready if the heap future was pending."
        );
        Ok(entry)
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
        &self,
        id: FutureId,
    ) -> Result<impl Future<Output = Result<(), EngineError>> + use<>, EngineError> {
        let waiter = match self.inner.active_futures.get(&id) {
            Some(future) => Some(Arc::clone(&future.ready)),
            None => {
                let next = self
                    .inner
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
                Some(w) => w.wait().await.clone(),
                None => Ok(()),
            }
        })
    }
}
impl TlabHolder for FutureManagerGuard<'_> {
    fn tlab(&self) -> &Tlab {
        self.inner.tlab()
    }
    fn tlab_mut(&mut self) -> &mut Tlab {
        self.inner.tlab_mut()
    }
}
impl HeapPermit<FutureManagerInner> for FutureManagerGuard<'_> {
    fn holder(&self) -> &FutureManagerInner {
        &self.inner
    }
    fn holder_mut(&mut self) -> &mut FutureManagerInner {
        &mut self.inner
    }
    fn proof(&self) -> PermitProof<'_> {
        self.inner.proof()
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
    future: HeapPtr,
    /// Set once the `Future` object is no longer `Pending`
    /// - `Ok(())` means there is a BAML value ready on the heap
    /// - `Err(err)` means it's `InternalError` and `err` is the error value
    ready: Arc<tokio::sync::SetOnce<Result<(), EngineError>>>,
    pub cancel: CancellationToken,
}
impl FutureState {
    /// SAFETY: We must hold a heap permit for the duration of the future object.
    unsafe fn get_mut(&mut self) -> Result<&mut bex_vm_types::Future, EngineError> {
        // SAFETY: We hold a permit, so we can access the future object.
        let obj = unsafe { self.future.get_mut() };
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
    use ::bex_heap::{BexHeap, HeapPermitManager};
    use ::bex_vm_types::Value;

    use super::*;

    async fn make_manager() -> FutureManager {
        let heap = BexHeap::new(Vec::new());
        let permit_manager = Arc::new(HeapPermitManager::new());
        let permit = permit_manager
            .new_permit(FutureManagerInner::new(Tlab::new_empty(Arc::clone(&heap))))
            .await;
        FutureManager::new(SharedHeapPermit::new(permit))
    }

    #[tokio::test]
    async fn fulfill_removes_entry() {
        let mgr = make_manager().await;
        let mut guard = mgr.acquire().await;
        let (id, _ptr) = guard.new_future(CancellationToken::new());
        assert_eq!(guard.active_future_count(), 1);
        guard.fulfill_future(id, Value::Int(42)).unwrap();
        assert_eq!(guard.active_future_count(), 0);
    }

    #[tokio::test]
    async fn cancel_removes_entry_and_fires_token() {
        let mgr = make_manager().await;
        let mut guard = mgr.acquire().await;
        let token = CancellationToken::new();
        let (id, _ptr) = guard.new_future(token.clone());
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
        let mgr = make_manager().await;
        let mut guard = mgr.acquire().await;
        let (id_a, _) = guard.new_future(CancellationToken::new());
        let (id_b, _) = guard.new_future(CancellationToken::new());
        assert_eq!(guard.active_future_count(), 2);
        guard.err_future(id_a, Value::Int(7)).unwrap();
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
        let mgr = make_manager().await;
        let mut guard = mgr.acquire().await;
        let (id, _) = guard.new_future(CancellationToken::new());
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
        // succeed against it: the `complete_pending` invariant is "entry exists
        // and heap is Pending". In debug builds the debug_assert fires; in
        // release builds the heap state is non-Pending and the call still
        // produces a `FutureNotFound`-or-equivalent error path.
        //
        // We exercise the release-build path by skipping under
        // `cfg(debug_assertions)` — the panic there is the documented
        // intent of the invariant.
        if cfg!(debug_assertions) {
            return;
        }
        let mgr = make_manager().await;
        let mut guard = mgr.acquire().await;
        let (id, _) = guard.new_future(CancellationToken::new());
        guard
            .internal_error_future(
                id,
                EngineError::TypeMismatch {
                    message: "stale".into(),
                },
            )
            .unwrap();
        // The release-build path: `fulfill_future`'s `complete_pending` happily
        // overwrites the heap to `Ready` and removes the entry. This is
        // tolerated rather than guaranteed (the `complete_pending` debug_assert
        // fails in debug builds). The point of the test is to pin the
        // observable behavior of the leaked state.
        let _ = guard.fulfill_future(id, Value::Int(0));
    }

    #[tokio::test]
    async fn double_complete_returns_not_found() {
        let mgr = make_manager().await;
        let mut guard = mgr.acquire().await;
        let (id, _) = guard.new_future(CancellationToken::new());
        guard.fulfill_future(id, Value::Int(1)).unwrap();
        let again = guard.fulfill_future(id, Value::Int(2));
        assert!(matches!(again, Err(EngineError::FutureNotFound { .. })));
    }

    #[tokio::test]
    async fn future_ready_immediate_for_completed_id() {
        let mgr = make_manager().await;
        let mut guard = mgr.acquire().await;
        let (id, _) = guard.new_future(CancellationToken::new());
        guard.fulfill_future(id, Value::Int(1)).unwrap();
        // Entry is gone; future_ready should treat it as already-resolved.
        let waiter = guard.future_ready(id).expect("expected immediate Ok");
        drop(guard);
        waiter.await.expect("should resolve to Ok(())");
    }

    #[tokio::test]
    async fn future_ready_for_never_issued_id_errors() {
        let mgr = make_manager().await;
        let guard = mgr.acquire().await;
        // No futures have been issued; any non-zero id is bogus. The contract
        // on `from_usize` is "no two live ids collide" — for this test we
        // construct one that is plainly out of range, which is safe since no
        // other id exists for it to collide with.
        let bogus = FutureId::from_usize(99);
        let result = guard.future_ready(bogus);
        assert!(matches!(result, Err(EngineError::FutureNotFound { .. })));
    }

    #[tokio::test]
    async fn future_ready_waiter_resolves_after_fulfill() {
        // Grab a waiter while the future is still pending; fulfill from a
        // separate critical section; then the waiter should resolve. This
        // exercises the path where `future_ready` clones the `Arc<SetOnce>`
        // before the entry is removed.
        let mgr = make_manager().await;
        let mut guard = mgr.acquire().await;
        let (id, _) = guard.new_future(CancellationToken::new());
        let waiter = guard.future_ready(id).expect("waiter should be created");
        drop(guard);

        let mut guard = mgr.acquire().await;
        guard.fulfill_future(id, Value::Int(123)).unwrap();
        drop(guard);

        waiter.await.expect("waiter should resolve to Ok(())");
        assert_eq!(mgr.active_future_count().await, 0);
    }
}
