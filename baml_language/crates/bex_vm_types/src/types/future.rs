use std::{
    cell::UnsafeCell,
    fmt::Display,
    mem::MaybeUninit,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU8, Ordering},
    },
};

use borsh::{BorshDeserialize, BorshSerialize};
use tokio_util::sync::CancellationToken;

use crate::{
    HeapPtr, RealizedTy, SessionEvalLease, Value, errors::StackFrame,
    runtime_compile::WeakSessionEvalLease,
};

/// Error payload carried by a future's [`Future::ready_waiter`] `SetOnce` when the
/// underlying engine produced an unrecoverable internal error.
///
/// Type-erased so this crate doesn't have to pull in `bex_engine`'s
/// `EngineError` (which would form a cycle). The engine boxes its
/// `EngineError` into this shape when transitioning a future to the
/// `InternalError` terminal state (see `FutureRead::InternalError`);
/// consumers (on the await side) downcast when surfacing the error to
/// the host.
pub type FutureInternalError = Box<dyn std::error::Error + Send + Sync>;

struct FutureSettlement {
    ready: Arc<tokio::sync::SetOnce<Result<(), FutureInternalError>>>,
    cancelled_session_leases: Mutex<Vec<WeakSessionEvalLease>>,
}

/// A future heap object.
///
/// Holds the cross-thread state-machine for one `spawn { ... }` body:
/// atomic discriminant, optional result value, cancellation token, and a
/// `SetOnce` that wakes any consumer blocked in `await`. The engine registry
/// only roots pending futures and looks them up by id; producer and consumer
/// state is synchronized through this heap object.
///
/// Concretely:
///
/// - `state` is loaded with `Acquire` and stored with `Release`. When a
///   reader observes a terminal-state tag, all preceding payload writes by
///   the writer are visible to it.
/// - `id` is set at construction and never modified. It's purely for
///   debug/tracing; nothing keys lookups off it anymore.
/// - `value` is wrapped in [`UnsafeCell<MaybeUninit<Value>>`] and is written
///   *at most once* (during the unique transition from `Pending` to
///   `Ready` or `Error`). It is only readable when `state` indicates
///   `Ready` or `Error`.
/// - `cancel` is the producer-observable cancel token. Consumers fire it
///   via `f.cancel()`; the producer's next `await` checkpoint throws
///   `baml.panics.Cancelled`. Children spawned by the producer derive
///   their tokens from this one so cancellation cascades.
/// - `ready` is the cross-task wake mechanism. Producers set it after any
///   terminal state transition; the awaiter (via VM `Await` → engine)
///   awaits on a clone of this Arc.
///
/// # Safety
///
/// Writers (the producer thread, plus `f.cancel()` callers) coordinate via
/// the `state` atomic itself: terminal transitions are
/// `compare_exchange(Pending → terminal)`; the first CAS wins and is the
/// sole authority that writes `value`. The Acquire/Release pairing on
/// `state` provides the happens-before for cross-thread reads of `value`.
#[repr(C)]
pub struct Future {
    /// Atomic discriminant (one of [`FutureTag`]). Loaded with `Acquire`,
    /// stored with `Release`. The first thread to successfully transition
    /// `Pending → terminal` via `compare_exchange` is the unique writer.
    state: AtomicU8,
    /// Metadata orthogonal to the terminal state. `observed` is set when an
    /// await delivers an error. `cancel_requested` is set by `f.cancel()` even
    /// when the future already settled and cancellation cannot change it.
    /// `reported` is set when GC transfers an unreachable error to the
    /// engine-owned reporting queue.
    flags: AtomicU8,
    /// Set at construction; never modified. Purely for debug/tracing.
    id: FutureId,
    /// The return/throws types that the value will match.
    /// Used for reflection/pattern matching.
    /// Set once when the future is created; the GC repoints the heads inside
    /// through [`Future::visit_heads_mut`] when their declarations move.
    /// `Box`, not `Arc`: a shared allocation cannot be soundly head-walked
    /// (repointing would either fork it or mutate every holder), and the GC's
    /// relocation `Clone` must produce an independently-fixable copy.
    types: Box<FutureOutputTypes>,
    /// Written at most once by whichever writer wins the `state` CAS.
    /// Valid only when `state` indicates `Ready` or `Error`. For
    /// `Cancelled` / `InternalError`, this stays uninitialized.
    value: UnsafeCell<MaybeUninit<Value>>,
    /// Trace captured when this future settles to `Error`. Kept separately
    /// from `value` because stack frames contain no GC-managed pointers.
    error_trace: Arc<OnceLock<Arc<[StackFrame]>>>,
    /// Cancellation signal observed by the producer. Fired by
    /// `f.cancel()` or by parent-cascade when an ancestor is cancelled.
    pub cancel: CancellationToken,
    /// Cross-task settlement state: producer (or cancel) sets `ready` on terminal
    /// transition; awaiter clones the Arc and `.wait().await`s.
    /// `Ok(())` is "look at `state` for the actual outcome"; `Err(_)`
    /// carries an unrecoverable engine error for surfacing through the
    /// engine's `Await` resume path. It also carries weak Session eval leases
    /// that cancellation releases before waking the awaiter; keeping both in
    /// one Arc preserves `Future`'s interpreter-hot-loop size budget.
    settlement: Arc<FutureSettlement>,
}

// SAFETY: All access to `value` is gated by the Acquire/Release handshake
// on `state` and the single-writer invariant enforced by the
// `FutureManager`'s state mutex.
unsafe impl Send for Future {}
unsafe impl Sync for Future {}

// Futures are runtime-only; they never appear in a compiled Program. Reject
// serialization explicitly so a malformed program fails fast.
impl BorshSerialize for Future {
    fn serialize<W: std::io::Write>(&self, _writer: &mut W) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Future cannot be serialized",
        ))
    }
}

impl BorshDeserialize for Future {
    fn deserialize_reader<R: std::io::Read>(_reader: &mut R) -> std::io::Result<Self> {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Future cannot be deserialized",
        ))
    }
}

// `UnscheduledFuture` is a runtime spawn-request slot — same lifecycle
// shape as `Future`, never appears in a compiled `Program`. The pack
// envelope (`baml_exec::PackEnvelope`) serializes the bytecode + the
// constant heap; if an `UnscheduledFuture` ever reaches the serializer
// that's a malformed program and we want to fail fast.
impl BorshSerialize for UnscheduledFuture {
    fn serialize<W: std::io::Write>(&self, _writer: &mut W) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "UnscheduledFuture cannot be serialized",
        ))
    }
}

impl BorshDeserialize for UnscheduledFuture {
    fn deserialize_reader<R: std::io::Read>(_reader: &mut R) -> std::io::Result<Self> {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "UnscheduledFuture cannot be deserialized",
        ))
    }
}

// `Future::read` calls `MaybeUninit::<Value>::assume_init_read`, which is
// sound only because `Value: Copy`. If `Value` ever gains a non-trivial
// `Drop` (e.g. by holding an `Arc<…>` or `Box<…>`), `assume_init_read`
// becomes UB on the second read. Guard against that at compile time.
const _: () = {
    const fn assert_copy<T: Copy>() {}
    assert_copy::<Value>();
};

/// Discriminant byte for [`Future::state`].
#[repr(u8)]
enum FutureTag {
    Pending = 0,
    Ready = 1,
    Error = 2,
    Cancelled = 3,
    InternalError = 4,
}

const FUTURE_FLAG_OBSERVED: u8 = 1 << 0;
const FUTURE_FLAG_CANCEL_REQUESTED: u8 = 1 << 1;
const FUTURE_FLAG_REPORTED: u8 = 1 << 2;

/// Snapshot view of a [`Future`] used for pattern matching at read sites.
///
/// Returned by [`Future::read`] after an `Acquire`-load of the discriminant
/// and (for `Ready`/`Error`) a synchronized read of the payload.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FutureRead {
    /// Pending future.
    ///
    /// In terms of synchronization, this is "pending" from the heap's point of view.
    /// It will remain pending until set otherwise, but yielding back to the engine *could* see an immediate completion.
    Pending(FutureId),

    /// Ready value for the future.
    Ready(Value),

    /// A BAML error or panic occurred while executing the future.
    /// If awaited, the error/panic value will be thrown.
    ///
    /// Note: not currently produced by the engine. Reserved for future
    /// user-callable async functions that throw BAML values; the engine
    /// today routes all sys-op errors through `internal_error_future`.
    Error(Value),

    /// The future was cancelled before completion.
    /// If awaited, this will throw `baml.panics.Cancelled`.
    Cancelled,

    /// An unrecoverable internal error occurred while executing the future.
    /// The originating `FutureId` is preserved so the VM can yield control back
    /// to the engine on `Await`, allowing the engine to surface the underlying
    /// error from the `FutureManager`'s registry. Such entries are leaked from
    /// `FutureManager::active_futures` by design.
    InternalError(FutureId),
}

impl Display for FutureRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FutureRead::Pending(id) => {
                write!(f, "<pending: future #{}>", id.id)
            }
            FutureRead::Ready(value) => write!(f, "<ready: {value}>"),
            FutureRead::Error(value) => write!(f, "<error: {value}>"),
            FutureRead::Cancelled => write!(f, "<cancelled>"),
            FutureRead::InternalError(id) => {
                write!(f, "<internal error: future #{}>", id.id)
            }
        }
    }
}

impl Future {
    /// Construct a new [`Future`] in the `Pending` state.
    ///
    /// `cancel` is the future's own cancel token — fired by `f.cancel()`
    /// and observed by the producer. The caller is responsible for deriving
    /// it from the spawning thread's token so cascade cancellation works.
    pub fn pending(
        id: FutureId,
        returns: RealizedTy,
        throws: RealizedTy,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            state: AtomicU8::new(FutureTag::Pending as u8),
            flags: AtomicU8::new(0),
            id,
            types: Box::new(FutureOutputTypes { returns, throws }),
            value: UnsafeCell::new(MaybeUninit::uninit()),
            error_trace: Arc::new(OnceLock::new()),
            cancel,
            settlement: Arc::new(FutureSettlement {
                ready: Arc::new(tokio::sync::SetOnce::new()),
                cancelled_session_leases: Mutex::new(Vec::new()),
            }),
        }
    }

    /// Clone the wake signal used by an engine awaiter.
    pub fn ready_waiter(&self) -> Arc<tokio::sync::SetOnce<Result<(), FutureInternalError>>> {
        Arc::clone(&self.settlement.ready)
    }

    /// Associate a Session eval acquired by this future's producer with the
    /// future's cancellation boundary.
    ///
    /// Registration and cancellation share the mutex so a racing cancel
    /// either drains this lease before waking the awaiter or observes the
    /// already-cancelled state here and releases it immediately.
    pub fn register_session_lease(&self, lease: &SessionEvalLease) {
        let mut leases = self
            .settlement
            .cancelled_session_leases
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match self.read() {
            FutureRead::Pending(_) => leases.push(lease.downgrade()),
            FutureRead::Cancelled
            | FutureRead::Ready(_)
            | FutureRead::Error(_)
            | FutureRead::InternalError(_) => lease.release(),
        }
    }

    /// `FutureId` for debug/tracing purposes.
    pub fn id(&self) -> FutureId {
        self.id
    }

    pub fn returns(&self) -> &RealizedTy {
        &self.types.returns
    }

    /// Every head the output types reach — the `Future<T, E>` the spawn site
    /// was typed at. Named like the generated family walks so
    /// `visit_object_heads(_mut)` can call it uniformly: the declarations
    /// these heads name must stay live, and be repointed, for as long as the
    /// future itself — independent of the settled value.
    pub fn visit_heads(&self, f: &mut impl FnMut(&crate::TypeHead)) {
        self.types.returns.visit_heads(f);
        self.types.throws.visit_heads(f);
    }

    /// Mutable twin of [`Self::visit_heads`], for the GC's repoint pass.
    pub fn visit_heads_mut(&mut self, f: &mut impl FnMut(&mut crate::TypeHead)) {
        self.types.returns.visit_heads_mut(f);
        self.types.throws.visit_heads_mut(f);
    }

    pub fn throws(&self) -> &RealizedTy {
        &self.types.throws
    }

    /// Read the current state with appropriate atomic ordering.
    ///
    /// `Acquire`-loads the discriminant, then dispatches to the right
    /// payload field. For `Ready`/`Error`, reading `value` is synchronized
    /// against the writer's `Release`-store so the value is fully visible.
    pub fn read(&self) -> FutureRead {
        let tag = self.state.load(Ordering::Acquire);
        match tag {
            t if t == FutureTag::Pending as u8 => FutureRead::Pending(self.id),
            t if t == FutureTag::Ready as u8 => {
                // SAFETY: the Acquire-load above synchronized with the
                // writer's Release-store of `Ready`, so the preceding
                // `value` write is visible. `Value: Copy`, so a read here
                // does not move the underlying data.
                let v = unsafe { (*self.value.get()).assume_init_read() };
                FutureRead::Ready(v)
            }
            t if t == FutureTag::Error as u8 => {
                // SAFETY: as for `Ready`. See above.
                let v = unsafe { (*self.value.get()).assume_init_read() };
                FutureRead::Error(v)
            }
            t if t == FutureTag::Cancelled as u8 => FutureRead::Cancelled,
            t if t == FutureTag::InternalError as u8 => FutureRead::InternalError(self.id),
            other => unreachable!("invalid Future discriminant byte: {other}"),
        }
    }

    /// Mark this future's terminal error as delivered to an awaiter.
    pub fn mark_observed(&self) {
        self.flags.fetch_or(FUTURE_FLAG_OBSERVED, Ordering::AcqRel);
    }

    pub fn is_observed(&self) -> bool {
        self.flags.load(Ordering::Acquire) & FUTURE_FLAG_OBSERVED != 0
    }

    pub fn cancel_requested(&self) -> bool {
        self.flags.load(Ordering::Acquire) & FUTURE_FLAG_CANCEL_REQUESTED != 0
    }

    /// Mark this future's error as transferred to the engine reporting queue.
    /// Returns `true` exactly once.
    pub fn try_mark_reported(&self) -> bool {
        self.flags.fetch_or(FUTURE_FLAG_REPORTED, Ordering::AcqRel) & FUTURE_FLAG_REPORTED == 0
    }

    pub fn error_trace(&self) -> Vec<StackFrame> {
        self.error_trace
            .get()
            .map(Arc::as_ref)
            .unwrap_or_default()
            .to_vec()
    }

    /// Mutable access to the embedded `Value` for `Ready`/`Error` states.
    ///
    /// Used by the GC's fixup pass to update heap pointers after a move.
    /// Returns `Some(&mut Value)` only if the current state is `Ready` or
    /// `Error`. The GC runs with all permits parked, so synchronization is
    /// not needed — but a `Relaxed` load is used for clarity.
    ///
    /// # Safety
    ///
    /// The caller must hold exclusive access to the heap (e.g., a parked
    /// `HeapGuard`). Concurrent calls to `set_*` would race.
    pub unsafe fn value_mut_for_fixup(&mut self) -> Option<&mut Value> {
        let tag = *self.state.get_mut();
        if tag == FutureTag::Ready as u8 || tag == FutureTag::Error as u8 {
            // SAFETY: state indicates `Ready`/`Error`; the value is
            // initialized. `&mut self` proves no concurrent reader.
            Some(unsafe { (*self.value.get()).assume_init_mut() })
        } else {
            None
        }
    }

    /// Attempt to transition `Pending → Ready`, writing `value` and firing
    /// the wake signal. Returns `true` if the transition was performed.
    ///
    /// A `false` return means another writer (a concurrent `f.cancel()`,
    /// most likely) already settled the future to a different terminal
    /// state. The producer in that case discards `value` and exits.
    ///
    /// Cross-thread synchronization: the speculative `value` write happens
    /// before the CAS; the CAS uses `AcqRel` so a reader observing `Ready`
    /// also observes the value write. If the CAS fails, the value cell is
    /// reset back to uninitialized to keep GC honest (Ready/Error states
    /// are the only ones for which GC traces the cell, and our state is
    /// not Ready, so the cell shouldn't claim to hold a tracked Value).
    ///
    /// Fires the generational write barrier on `heap` for `self_ptr`
    /// before the value write. This is required because a `Future` can
    /// survive across GCs (rooted by `FutureManagerInner::active_futures`)
    /// and may end up in Gen2; if `value` carries a heap-object pointer
    /// (`value.is_object()`) to a younger-generation object, the next Minor GC's
    /// dirty-card scan must find this reference. Without the barrier the
    /// young object would be reclaimed and the `Future`'s `value` left
    /// dangling.
    ///
    /// # Safety
    ///
    /// Caller must hold the heap permit (to keep `value`'s embedded
    /// `HeapPtr`, if any, valid against concurrent GC moves). `self_ptr`
    /// must be the [`HeapPtr`] under which `self` lives, so the write
    /// barrier marks the correct card.
    pub unsafe fn settle_ready(
        &self,
        heap: &impl crate::WriteBarrier,
        self_ptr: HeapPtr,
        value: Value,
    ) -> bool {
        // Fire the generational write barrier BEFORE the speculative
        // value write. If `value` is a young heap pointer and our CAS
        // later wins, the card mark is what tells the next minor GC
        // to find this reference. (If the CAS loses, the rollback
        // below reverts the value cell but the spurious card mark is
        // benign — the GC will simply rescan it.)
        heap.write_barrier(self_ptr, value);
        // SAFETY: speculative write; observed by readers only if our CAS
        // wins (Release synchronizes the write to subsequent Acquire-loads).
        unsafe { (*self.value.get()).write(value) };
        match self.state.compare_exchange(
            FutureTag::Pending as u8,
            FutureTag::Ready as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                let _ = self.settlement.ready.set(Ok(()));
                true
            }
            Err(_) => {
                // CAS failed — another writer beat us. Roll back the
                // speculative write so GC's `value_mut_for_fixup` (which
                // gates on state) doesn't trip over stale contents.
                // SAFETY: state isn't Ready/Error, so no reader will look.
                unsafe { *self.value.get() = MaybeUninit::uninit() };
                false
            }
        }
    }

    /// Attempt to transition `Pending → Error`, writing the error value
    /// and firing the wake signal. Mirror of [`Self::settle_ready`].
    ///
    /// Fires the generational write barrier — see [`Self::settle_ready`].
    ///
    /// # Safety
    ///
    /// See [`Self::settle_ready`].
    pub unsafe fn settle_error(
        &self,
        heap: &impl crate::WriteBarrier,
        self_ptr: HeapPtr,
        value: Value,
        trace: Vec<StackFrame>,
    ) -> bool {
        heap.write_barrier(self_ptr, value);
        let _ = self.error_trace.set(Arc::from(trace));
        // SAFETY: see settle_ready.
        unsafe { (*self.value.get()).write(value) };
        let transitioned = self.state.compare_exchange(
            FutureTag::Pending as u8,
            FutureTag::Error as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        match transitioned {
            Ok(_) => {
                let _ = self.settlement.ready.set(Ok(()));
                true
            }
            Err(_) => {
                // SAFETY: see settle_ready.
                unsafe { *self.value.get() = MaybeUninit::uninit() };
                false
            }
        }
    }

    /// Attempt to transition `Pending → Cancelled`. Fires the cancel
    /// token (so the producer's next await checkpoint observes it) and
    /// the wake signal (so any current awaiter resumes).
    ///
    /// Returns `true` if the transition was performed. Idempotent in the
    /// sense that repeated calls all return `false` after the first
    /// successful one.
    pub fn settle_cancelled(&self) -> bool {
        match self.state.compare_exchange(
            FutureTag::Pending as u8,
            FutureTag::Cancelled as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                self.cancel.cancel();
                let leases = {
                    let mut leases = self
                        .settlement
                        .cancelled_session_leases
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    std::mem::take(&mut *leases)
                };
                for lease in leases {
                    lease.release();
                }
                let _ = self.settlement.ready.set(Ok(()));
                true
            }
            Err(_) => false,
        }
    }

    /// Record an explicit `f.cancel()` request, then attempt to cancel the
    /// future if it is still pending. The flag remains set when an existing
    /// terminal error wins the race.
    pub fn request_cancel(&self) -> bool {
        self.flags
            .fetch_or(FUTURE_FLAG_CANCEL_REQUESTED, Ordering::AcqRel);
        self.settle_cancelled()
    }

    /// Attempt to transition `Pending → InternalError`, carrying `err`
    /// on the wake signal for the engine to surface to the host on the
    /// awaiter's next `await` re-execution.
    ///
    /// Returns `true` if the transition was performed.
    pub fn settle_internal_error(&self, err: FutureInternalError) -> bool {
        match self.state.compare_exchange(
            FutureTag::Pending as u8,
            FutureTag::InternalError as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                let _ = self.settlement.ready.set(Err(err));
                true
            }
            Err(_) => false,
        }
    }
}

impl Clone for Future {
    fn clone(&self) -> Self {
        // Snapshot the current state and clone the corresponding payload.
        // The only legitimate caller is the GC's heap-relocation copy
        // (`gc.rs` `copy_object_to_inactive`), which clones the heap object
        // into the inactive space.
        //
        // The `cancel` token and `ready` SetOnce are reference-counted
        // (CancellationToken has internal `Arc`, `ready` is wrapped in an
        // explicit `Arc`), so the clone shares the same underlying sync
        // primitives. Producers that hold a clone of `ready` from before
        // the GC move continue to wake the same set of waiters, and the
        // moved heap copy observes the same `ready.set(...)` because both
        // copies share the same settlement allocation.
        //
        // Futures are conceptually *handles*, not values: there is no
        // "the same future, but a copy" at the user level. User-side
        // `deep_copy` reflects this by sharing the original `HeapPtr`
        // for any `Future` rather than calling this `Clone` impl. See
        // `crates/bex_vm/src/package_baml/root.rs::deep_copy_value_recursive`.
        let read = self.read();
        let cloned = Self {
            state: AtomicU8::new(0), // placeholder; rewritten below
            flags: AtomicU8::new(self.flags.load(Ordering::Acquire)),
            id: self.id,
            types: self.types.clone(),
            value: UnsafeCell::new(MaybeUninit::uninit()),
            error_trace: Arc::clone(&self.error_trace),
            cancel: self.cancel.clone(),
            settlement: Arc::clone(&self.settlement),
        };
        let tag: u8 = match read {
            FutureRead::Pending(_) => FutureTag::Pending as u8,
            FutureRead::Ready(v) => {
                // SAFETY: we just constructed `cloned` and have exclusive
                // access; no other observer exists yet.
                unsafe { (*cloned.value.get()).write(v) };
                FutureTag::Ready as u8
            }
            FutureRead::Error(v) => {
                // SAFETY: as above.
                unsafe { (*cloned.value.get()).write(v) };
                FutureTag::Error as u8
            }
            FutureRead::Cancelled => FutureTag::Cancelled as u8,
            FutureRead::InternalError(_) => FutureTag::InternalError as u8,
        };
        cloned.state.store(tag, Ordering::Release);
        cloned
    }
}

impl std::fmt::Debug for Future {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.read() {
            FutureRead::Pending(id) => f.debug_tuple("Pending").field(&id).finish(),
            FutureRead::Ready(v) => f.debug_tuple("Ready").field(&v).finish(),
            FutureRead::Error(v) => f.debug_tuple("Error").field(&v).finish(),
            FutureRead::Cancelled => f.write_str("Cancelled"),
            FutureRead::InternalError(id) => f.debug_tuple("InternalError").field(&id).finish(),
        }
    }
}

/// Runtime payload behind a `baml.spawn.SpawnConfig` instance's `_handle`
/// (`Object::RustData`). Produced by `baml.spawn.options(...)` and read by the
/// engine when dispatching a `spawn ... with` clause to derive the spawned
/// task's effective cancel token. BEP-034 "spawn options".
///
/// PR1 carries only the optional cancel token; `group` (rate limiting) and
/// `detach` are added as those features are wired, so the engine's downcast
/// target stays stable across PRs.
#[derive(Debug, Clone, Default)]
pub struct SpawnConfigData {
    /// User-provided cancel token from `options(cancel = ...)`, if any. Linked
    /// into the spawn's effective token by the engine.
    pub cancel: Option<CancellationToken>,
    /// `detach = true`: the spawn opts out of the parent→child cancel cascade
    /// (its effective token is independent of the parent's) and its unhandled
    /// errors route to the root task rather than the spawner.
    pub detach: bool,
    /// `TaskGroup` from `options(group = ...)`, if any. The engine acquires a
    /// concurrency slot from it before running the spawned body (BEP-034 rate
    /// limiting).
    pub group: Option<std::sync::Arc<crate::task_group::TaskGroupInner>>,
}

/// A pending user `spawn { body }` request that the engine still has to
/// dispatch on a fresh `BexThread`.
///
/// BEP-034 phase D′: this struct used to also carry sys-op invocations
/// (`kind: SysOp { ... }`), but sys-ops now go through the single-yield
/// `VmExecState::SysOp` path without allocating a heap object. Only the
/// spawn case survives.
#[derive(Clone, Debug)]
pub struct UnscheduledFuture {
    /// Pointer to an `Object::Closure` carrying the spawn body.
    pub closure: HeapPtr,
    /// Optional human-readable name attached at the spawn site. Surfaces in
    /// debug, stack traces, and the playground. Held here as a `HeapPtr` so
    /// the GC keeps the underlying string alive while the unscheduled
    /// future is on the heap.
    pub name: Option<HeapPtr>,
    /// Optional `baml.spawn.SpawnConfig` instance from a `spawn ... with
    /// baml.spawn.options(...)` clause (BEP-034 "spawn options"). Held as a
    /// `HeapPtr` — like `name` — so the GC keeps the config (and the
    /// `CancelToken`/`TaskGroup` it references) alive while this slot is on the
    /// heap. `None` when the spawn had no `with` clause. The engine reads the
    /// config's `_handle` (`SpawnConfigData`) when dispatching the spawn.
    pub config: Option<HeapPtr>,
    /// The `T` of the `Future<T, E>` this spawn yields, already resolved
    /// against the spawning frame's type args by `OpCode::Spawn`. Handed to
    /// [`Future::pending`] so the scheduled future can answer reflection and
    /// `is`/`match` on its generic parameters.
    pub returns: RealizedTy,
    /// The `E` of the `Future<T, E>` this spawn yields. See [`Self::returns`].
    pub throws: RealizedTy,
}

/// A unique identifier for a future.
///
/// Unlike `bex_engine::CallId`, these are created for every scheduled future (sys op or function call),
/// not just when there is a new call from the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct FutureId {
    id: usize,
}

impl std::fmt::Display for FutureId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Render as a bare number so error messages read as
        // "Future with ID 42 not found" instead of "FutureId { id: 42 }".
        self.id.fmt(f)
    }
}

impl FutureId {
    /// Construct a [`FutureId`] from a raw `usize`.
    ///
    /// # Contract
    ///
    /// Each `FutureId` constructed for a given engine **must** have a `usize`
    /// value distinct from every other live `FutureId` in that engine. The
    /// engine satisfies this by issuing values from a monotonic
    /// [`AtomicUsize`](::core::sync::atomic::AtomicUsize) counter inside its
    /// `FutureManager`.
    ///
    /// Violating this contract does **not** cause memory unsafety, but it
    /// causes `FutureManager` lookup collisions (two distinct futures sharing
    /// the same map key, with all the silent data corruption that implies).
    /// Outside of the engine and its tests, prefer calls that route through
    /// `FutureManagerGuard::new_future` instead of constructing ids by hand.
    pub fn from_usize(id: usize) -> Self {
        Self { id }
    }

    pub fn as_usize(self) -> usize {
        self.id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum FutureType {
    /// Top of future type lattice: represents all future types.
    Any,
    Pending,
    Ready,
    Error,
    Cancelled,
    InternalError,
}

impl FutureType {
    pub fn of(future: &Future) -> Self {
        match future.read() {
            FutureRead::Pending(_) => Self::Pending,
            FutureRead::Ready(_) => Self::Ready,
            FutureRead::Error(_) => Self::Error,
            FutureRead::Cancelled => Self::Cancelled,
            FutureRead::InternalError(_) => Self::InternalError,
        }
    }
}

impl std::fmt::Display for FutureType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FutureType::Any => write!(f, "any"),
            FutureType::Pending => write!(f, "pending"),
            FutureType::Ready => write!(f, "ready"),
            FutureType::Error => write!(f, "error"),
            FutureType::Cancelled => write!(f, "cancelled"),
            FutureType::InternalError => write!(f, "internal_error"),
        }
    }
}

impl From<&Future> for FutureType {
    fn from(value: &Future) -> Self {
        Self::of(value)
    }
}

#[derive(Clone)]
struct FutureOutputTypes {
    returns: RealizedTy,
    throws: RealizedTy,
}
