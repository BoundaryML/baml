use ::std::collections::HashMap;

use crate::HeapPtr;

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
