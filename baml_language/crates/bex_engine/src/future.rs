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
//! - All operations on a [`FutureManagerGuard`] hold the manager's state
//!   mutex, so terminal-transition-then-remove is atomic with respect to
//!   any other [`FutureManagerGuard`] operation (notably
//!   [`FutureManagerGuard::future_ready`]).
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
    HeapPermit, HeapPermitManager, InactiveHeapPermit, PermitProof, Tlab, TlabHolder,
};
use ::bex_vm_types::{
    FutureRead, HeapPtr, Object, ObjectType, RootHaver, Value,
    types::{FutureId, FutureType},
};
use ::core::sync::atomic::AtomicUsize;
use ::std::{collections::HashMap, sync::Arc};
use ::sys_types::CancellationToken;
use ::tokio::sync::{Mutex, MutexGuard};

use crate::EngineError;

/// Manages all futures for the Bex engine.
///
/// The wrapped [`InactiveHeapPermit`] is registered with
/// [`HeapPermitManager.holders`](::bex_heap::HeapPermitManager) so GC can
/// `collect_roots` / `forward_roots` on the inner. It is **never activated**
/// by the manager itself — every method requires a [`PermitProof`] from
/// the caller's own active permit, witnessing that GC is gated externally.
/// The wrapping [`tokio::sync::Mutex`] provides the single-writer invariant
/// that previous releases derived from `SharedHeapPermit`.
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
    pub fn new_future(&mut self, cancel: CancellationToken) -> (FutureId, HeapPtr) {
        // The contract on `FutureId::from_usize` is "no two live ids share a
        // usize". We satisfy this by drawing the value from the manager's
        // monotonic `AtomicUsize`; uniqueness is preserved as long as the
        // counter hasn't wrapped (which would take 2^64 calls).
        let inner = self.holder_mut();
        let id = inner
            .next_future_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = FutureId::from_usize(id);

        let ptr = inner.tlab.alloc_future(::bex_vm_types::Future::pending(id));

        let future_state = FutureState {
            future: ptr,
            ready: Arc::new(tokio::sync::SetOnce::new()),
            cancel,
        };
        inner.active_futures.insert(id, future_state);
        (id, ptr)
    }
    pub fn fulfill_future(&mut self, id: FutureId, value: Value) -> Result<(), EngineError> {
        // Snapshot the heap `Arc` before the borrow on `self` is taken by
        // `complete_pending`; the closure needs it to fire the write barrier.
        let heap = ::std::sync::Arc::clone(self.tlab().heap());
        self.complete_pending(id, |fut, self_ptr| {
            // SAFETY: complete_pending guarantees we hold the `future_permit`
            // (single-writer invariant) and that `fut` is currently Pending.
            // `self_ptr` is the heap location of `fut`, so the write barrier
            // marks the right card if `value` is a young heap pointer.
            unsafe { fut.set_ready(heap.as_ref(), self_ptr, value) };
        })?;
        Ok(())
    }
    /// Transition a future to `Future::Error(value)` with the given BAML
    /// error/panic value.
    ///
    /// **Currently unused by the engine in production.** All sys-op errors
    /// route through [`Self::internal_error_future`] (which preserves the
    /// original `EngineError` for surfacing). This API is reserved for a
    /// future capability where user-callable async functions can throw
    /// BAML values that the VM's `Await` opcode would re-throw via
    /// [`bex_vm_types::FutureRead::Error`]. The plumbing is kept in place
    /// (write here → variant in the heap object → throw in `Await`) so
    /// that wiring it up later is a one-call-site change.
    pub fn err_future(&mut self, id: FutureId, err: Value) -> Result<(), EngineError> {
        let heap = ::std::sync::Arc::clone(self.tlab().heap());
        self.complete_pending(id, |fut, self_ptr| {
            // SAFETY: see `fulfill_future`.
            unsafe { fut.set_error(heap.as_ref(), self_ptr, err) };
        })?;
        Ok(())
    }
    pub fn cancel_future(&mut self, id: FutureId) -> Result<(), EngineError> {
        let entry = self.complete_pending(id, |fut, _self_ptr| {
            // SAFETY: see `fulfill_future`. `set_cancelled` writes only the
            // discriminant tag, no `Value` payload, so no write barrier
            // needed.
            unsafe { fut.set_cancelled() };
        })?;
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
            .holder()
            .active_futures
            .get(&id)
            .ok_or(EngineError::FutureNotFound { future_id: id })?;
        // SAFETY: the `FutureManagerGuard` holds the `FutureManager` Mutex,
        // so we are the unique caller; the heap object is alive because
        // the entry roots it via `RootHaver::collect_roots`.
        let fut = unsafe { entry.future_ref() }?;
        let read = fut.read();
        let observed = FutureType::of(fut);
        debug_assert!(
            matches!(read, FutureRead::Pending(_)),
            "internal_error_future called on non-Pending future {id:?} \
             (actual: {observed:?}); invariant violated"
        );
        if !matches!(read, FutureRead::Pending(_)) {
            // Release-build invariant guard: a previously-resolved future
            // should never re-enter this path. Surfacing as an error
            // (rather than the legacy silent overwrite) makes the bug
            // observable to telemetry / `tracing::error!` in `run_future`.
            return Err(EngineError::TypeMismatch {
                message: format!(
                    "internal_error_future called on non-Pending future {id:?} \
                     (actual: {observed:?})"
                ),
            });
        }
        // SAFETY: single-writer via `FutureManager` Mutex.
        unsafe { fut.set_internal_error() };
        let set = entry.ready.set(Err(err));
        debug_assert!(
            set.is_ok(),
            "Should not have been ready if the heap future was pending."
        );
        if set.is_err() {
            return Err(EngineError::TypeMismatch {
                message: format!(
                    "internal_error_future: SetOnce already set for future {id:?}; \
                     invariant violated"
                ),
            });
        }
        Ok(())
    }

    /// Atomically transition a `Pending` future to a terminal state, signal
    /// its [`tokio::sync::SetOnce`] waiter, and remove the entry from
    /// `active_futures`. The `transition` closure is responsible for the
    /// actual `set_*` call and is passed an immutable reference to the
    /// `Future` heap object (which uses interior atomic mutation). The
    /// dropped [`FutureState`] is returned so callers (e.g.
    /// [`Self::cancel_future`]) can perform additional Drop-time work like
    /// firing a [`CancellationToken`] clone before it is released.
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
        transition: impl FnOnce(&bex_vm_types::Future, HeapPtr),
    ) -> Result<FutureState, EngineError> {
        // Phase 1: pre-check the state without removing. A non-Pending
        // heap state means the caller has already routed this id through
        // a terminal helper (or `internal_error_future`'s leak); in that
        // case we must not remove the entry, since doing so would discard
        // the leaked `SetOnce` payload and silently lose the error.
        {
            let entry_ref = self
                .holder()
                .active_futures
                .get(&id)
                .ok_or(EngineError::FutureNotFound { future_id: id })?;
            // SAFETY: the `FutureManagerGuard` holds the `FutureManager` Mutex.
            let fut = unsafe { entry_ref.future_ref() }?;
            let read = fut.read();
            let observed = FutureType::of(fut);
            debug_assert!(
                matches!(read, FutureRead::Pending(_)),
                "complete_pending called with non-Pending heap state for {id:?} \
                 (actual: {observed:?}); invariant violated — only fulfill/err/cancel may \
                 route through this helper"
            );
            if !matches!(read, FutureRead::Pending(_)) {
                return Err(EngineError::TypeMismatch {
                    message: format!(
                        "complete_pending called with non-Pending heap state for {id:?} \
                         (actual: {observed:?})"
                    ),
                });
            }
        }
        // Phase 2: state is Pending. Remove the entry and apply the
        // transition under the `FutureManager` Mutex (single-writer).
        let entry = self
            .holder_mut()
            .active_futures
            .remove(&id)
            .expect("entry was present in phase 1 and we hold the `FutureManager` Mutex");
        // SAFETY: the entry is still rooting the heap object via local
        // ownership; the `FutureManager` Mutex enforces single-writer.
        let fut = unsafe { entry.future_ref() }?;
        transition(fut, entry.future);
        let set = entry.ready.set(Ok(()));
        debug_assert!(
            set.is_ok(),
            "Should not have been ready if the heap future was pending."
        );
        if set.is_err() {
            return Err(EngineError::TypeMismatch {
                message: format!(
                    "complete_pending: SetOnce already set for future {id:?}; \
                     invariant violated"
                ),
            });
        }
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
        let inner = self.holder();
        let waiter = match inner.active_futures.get(&id) {
            Some(future) => Some(Arc::clone(&future.ready)),
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
                Some(w) => w.wait().await.clone(),
                None => Ok(()),
            }
        })
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
    future: HeapPtr,
    /// Set once the `Future` object is no longer `Pending`
    /// - `Ok(())` means there is a BAML value ready on the heap
    /// - `Err(err)` means it's `InternalError` and `err` is the error value
    ready: Arc<tokio::sync::SetOnce<Result<(), EngineError>>>,
    cancel: CancellationToken,
}
impl FutureState {
    /// Returns an immutable reference to the heap-allocated `Future`.
    ///
    /// The new [`bex_vm_types::Future`] uses interior mutability via an
    /// `AtomicU8` discriminant + `UnsafeCell<MaybeUninit<Value>>`, so all
    /// terminal-state writes go through the `set_*` methods that take
    /// `&self`. This lets the spawned async task and the VM read/write
    /// the same heap object concurrently without a data race, as long as
    /// the writer holds the `FutureManager` Mutex (single-writer invariant).
    ///
    /// # Safety
    /// Caller must hold the `FutureManager` Mutex (i.e., a
    /// [`FutureManagerGuard`]) for the duration of any subsequent access
    /// to the returned reference.
    unsafe fn future_ref(&self) -> Result<&bex_vm_types::Future, EngineError> {
        // SAFETY: caller holds the `FutureManager` Mutex; heap object is alive.
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

    #[tokio::test]
    async fn fulfill_removes_entry() {
        let (mgr, pm) = make_manager().await;
        let temp = temp_permit(&pm).await;
        let mut guard = mgr.acquire(temp.proof()).await;
        let (id, _ptr) = guard.new_future(CancellationToken::new());
        assert_eq!(guard.active_future_count(), 1);
        guard.fulfill_future(id, Value::Int(42)).unwrap();
        assert_eq!(guard.active_future_count(), 0);
    }

    #[tokio::test]
    async fn cancel_removes_entry_and_fires_token() {
        let (mgr, pm) = make_manager().await;
        let temp = temp_permit(&pm).await;
        let mut guard = mgr.acquire(temp.proof()).await;
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
        let (mgr, pm) = make_manager().await;
        let temp = temp_permit(&pm).await;
        let mut guard = mgr.acquire(temp.proof()).await;
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
        let (mgr, pm) = make_manager().await;
        let temp = temp_permit(&pm).await;
        let mut guard = mgr.acquire(temp.proof()).await;
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
        let (id, _) = guard.new_future(CancellationToken::new());
        let original = EngineError::TypeMismatch {
            message: "stale".into(),
        };
        guard.internal_error_future(id, original.clone()).unwrap();
        assert_eq!(guard.active_future_count(), 1);

        // Release-build behavior: `fulfill_future` rejects the call (the
        // pre-check observes a non-Pending heap state) and leaves the
        // leaked entry untouched.
        let result = guard.fulfill_future(id, Value::Int(0));
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
    async fn double_complete_returns_not_found() {
        let (mgr, pm) = make_manager().await;
        let temp = temp_permit(&pm).await;
        let mut guard = mgr.acquire(temp.proof()).await;
        let (id, _) = guard.new_future(CancellationToken::new());
        guard.fulfill_future(id, Value::Int(1)).unwrap();
        let again = guard.fulfill_future(id, Value::Int(2));
        assert!(matches!(again, Err(EngineError::FutureNotFound { .. })));
    }

    #[tokio::test]
    async fn future_ready_immediate_for_completed_id() {
        let (mgr, pm) = make_manager().await;
        let temp = temp_permit(&pm).await;
        let mut guard = mgr.acquire(temp.proof()).await;
        let (id, _) = guard.new_future(CancellationToken::new());
        guard.fulfill_future(id, Value::Int(1)).unwrap();
        // Entry is gone; future_ready should treat it as already-resolved.
        let waiter = guard.future_ready(id).expect("expected immediate Ok");
        drop(guard);
        waiter.await.expect("should resolve to Ok(())");
    }

    #[tokio::test]
    async fn future_ready_for_never_issued_id_errors() {
        let (mgr, pm) = make_manager().await;
        let temp = temp_permit(&pm).await;
        let guard = mgr.acquire(temp.proof()).await;
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
        let (id, _) = guard.new_future(CancellationToken::new());
        let waiter = guard.future_ready(id).expect("waiter should be created");
        drop(guard);

        let temp2 = temp_permit(&pm).await;
        let mut guard = mgr.acquire(temp2.proof()).await;
        guard.fulfill_future(id, Value::Int(123)).unwrap();
        drop(guard);
        drop(temp2);

        waiter.await.expect("waiter should resolve to Ok(())");
        assert_eq!(mgr.active_future_count(&pm).await, 0);
    }
}
