use ::std::collections::HashMap;

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
/// Implemented by [`bex_heap::BexHeap`] (`bex_heap` is downstream of this
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
