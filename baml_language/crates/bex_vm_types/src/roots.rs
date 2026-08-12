use ::std::{collections::HashMap, marker::PhantomData};

use crate::{HeapPtr, Value};

/// A trait for types that have heap roots.
///
/// Allows the GC to find all roots and update them when the GC moves objects.
///
/// `Send` is required because instances are tracked across tokio tasks via the heap
/// permit manager. `Sync` is intentionally not required — the permit/mutex enforces
/// that at most one thread accesses the inner value at a time.
pub trait RootHaver: Send {
    /// Collect all heap roots in this object.
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>);
    /// Forward the heap pointers of all roots in this object.
    fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>);
}

impl RootHaver for () {
    fn collect_roots(&self, _roots: &mut Vec<HeapPtr>) {}
    fn forward_roots(&mut self, _roots: &HashMap<HeapPtr, HeapPtr>) {}
}

/// Generational write-barrier hook for heap mutations that touch a [`Value`].
///
/// Implemented by `bex_heap::BexHeap` (`bex_heap` is downstream of this
/// crate, so we can't name it here directly). Lives in `bex_vm_types` so
/// types like [`crate::Future`] — which perform their own
/// `UnsafeCell<MaybeUninit<Value>>` writes via `unsafe` setters — can
/// require callers to fire the barrier through this trait without taking a
/// dependency on the heap crate.
///
/// # Why this exists
///
/// The generational GC tracks cross-generation references via a card table.
/// When an older-generation object is mutated to hold a younger-generation
/// reference, the container's card must be marked dirty so a partial
/// (Minor) GC can rediscover the reference. Heap-mutation sites in the VM
/// (`vm.rs`) call `BexHeap::write_barrier` for this. The `Future` heap
/// object is special: its terminal-state writes don't go through the VM —
/// they go through the engine's spawned `run_future` writeback path. That
/// path now must fire the barrier too, and `Future::set_ready` /
/// `set_error` enforces it via this trait at compile time.
pub trait WriteBarrier {
    /// Mark a card dirty for `container` if `value` is a younger-gen
    /// `Value::Object`. Called *before* the actual field write.
    fn write_barrier(&self, container: HeapPtr, value: Value);
}

/// A type-erased proof that an active heap permit is held in the current
/// scope (for at least lifetime `'a`).
///
/// `PermitProof<'a>` is a zero-sized lifetime witness. It carries no
/// runtime data — the GC-exclusion guarantee comes purely from the
/// lifetime, which is bound by the originating permit's borrow.
///
/// # Where it lives
///
/// Defined here in `bex_vm_types` (rather than `bex_heap`, where the
/// permit machinery lives) so that upstream types like
/// [`crate::SharedGlobals`] — which themselves need to gate their
/// `UnsafeCell`-backed reads on "an active permit is held" without taking
/// a dependency on `bex_heap` — can accept the proof safely.
/// `bex_heap::ActiveHeapPermit::proof` is the canonical safe constructor;
/// it re-exports this type for backward compatibility.
///
/// # Why the constructor is `unsafe`
///
/// The whole point of `PermitProof` is that *holding* one is a runtime
/// witness that a permit is active. Construction must therefore be
/// gated: only callers who genuinely hold a permit may produce one. The
/// only safe production caller is
/// `bex_heap::ActiveHeapPermit::proof`, which calls
/// [`Self::new`] inside an `unsafe` block whose safety argument is
/// "`&self` proves an active permit is held for this borrow's lifetime."
/// Test mocks may use [`Self::new`] directly with a documented safety
/// argument.
#[derive(Clone, Copy)]
pub struct PermitProof<'a> {
    _marker: PhantomData<&'a ()>,
}

impl PermitProof<'_> {
    /// Construct a `PermitProof` tied to lifetime `'a`.
    ///
    /// # Safety
    ///
    /// Caller must hold an active heap permit for at least lifetime `'a`.
    /// The only intended production caller is
    /// `bex_heap::ActiveHeapPermit::proof`, which justifies the
    /// invariant via its `&self` borrow.
    #[inline]
    #[must_use]
    #[allow(unsafe_code, reason = "construction is the unsafety boundary")]
    pub const unsafe fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}
