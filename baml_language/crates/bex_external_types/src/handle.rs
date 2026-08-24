//! Handle type for external/FFI boundary.
//!
//! Internal VM code uses `ObjectIndex` for fast access. External code
//! (Python bindings, JS bindings, etc.) uses opaque `Handle` values
//! that are validated before use.

use std::sync::Arc;

/// Trait for releasing handles back to the heap.
///
/// This is implemented by `BexHeap` to allow handles to clean up
/// when dropped, without creating a circular dependency.
pub trait WeakHeapRef: Send + Sync {
    /// Release a handle slot by its slab key.
    fn release_handle(&self, slab_key: usize);

    /// Resolve a handle to its current object pointer.
    /// Returns None if handle is invalid.
    fn resolve_handle_ptr(&self, slab_key: usize) -> Option<bex_vm_types::HeapPtr>;
}

/// Opaque handle to a heap object.
///
/// Handles are used at the FFI boundary to give external code safe
/// access to heap objects. Clone to share, drop to release.
///
/// # Thread Safety
///
/// Handles can be safely shared across threads. The underlying object
/// remains valid as long as at least one Handle clone exists.
///
/// # Example
///
/// ```ignore
/// // Get a handle from BexEngine
/// let handle = engine.call_function("create_user", &[]).await?;
///
/// // Clone to share
/// let handle2 = handle.clone();
///
/// // Both references keep the object alive
/// drop(handle);  // Object still alive via handle2
/// drop(handle2); // Object now eligible for GC
/// ```
#[derive(Clone)]
pub struct Handle {
    pub(crate) inner: Arc<HandleInner>,
}

/// Two handles are the same handle when they name the same slot **of the same
/// heap**. A slab key is only meaningful relative to the heap that issued it,
/// so comparing keys alone would equate unrelated objects across engines.
impl PartialEq for Handle {
    fn eq(&self, other: &Self) -> bool {
        self.slab_key() == other.slab_key() && self.same_heap_as(other)
    }
}

impl Eq for Handle {}

/// Internal handle data.
///
/// This is public for use by `bex_heap` but should not be constructed
/// directly by external code.
pub struct HandleInner {
    /// Key in the sharded_slab handle table.
    pub slab_key: usize,
    /// Weak reference to heap for cleanup on drop.
    /// Using trait object to avoid circular dependency with bex_heap.
    pub heap: Option<Arc<dyn WeakHeapRef>>,
}

impl Handle {
    /// Create a new handle.
    ///
    /// This is intended for use by `bex_heap` only.
    pub fn new(slab_key: usize, heap: Arc<dyn WeakHeapRef>) -> Self {
        Self {
            inner: Arc::new(HandleInner {
                slab_key,
                heap: Some(heap),
            }),
        }
    }

    /// Create a handle without a heap reference (for testing).
    #[cfg(test)]
    pub fn new_detached(slab_key: usize) -> Self {
        Self {
            inner: Arc::new(HandleInner {
                slab_key,
                heap: None,
            }),
        }
    }

    /// Rebuild a handle from shared inner state.
    ///
    /// The heap's one-key-per-object index hands back the `Arc` behind a live
    /// handle rather than minting a second key; sharing it is what counts the
    /// new reference, so the object is released only once the last holder
    /// drops.
    #[must_use]
    pub fn from_inner(inner: Arc<HandleInner>) -> Self {
        Self { inner }
    }

    /// A weak reference to this handle's inner state.
    ///
    /// For the heap's reverse index, which must observe a handle without
    /// keeping it alive: holding it strongly would prevent the very drop that
    /// releases the slab key.
    #[must_use]
    pub fn downgrade_inner(&self) -> std::sync::Weak<HandleInner> {
        Arc::downgrade(&self.inner)
    }

    /// Get the slab key for this handle.
    ///
    /// This is primarily for internal use by `bex_heap`.
    pub fn slab_key(&self) -> usize {
        self.inner.slab_key
    }

    /// Whether this handle was issued by `heap`.
    ///
    /// A slab key indexes one heap's handle table; the same key names an
    /// unrelated live object in any other engine. Resolving a foreign handle
    /// would therefore hand back an arbitrary object rather than failing, so
    /// every inbound resolution checks provenance first. Compares the heap
    /// reference by address — identity, not equality.
    #[must_use]
    pub fn is_of_heap(&self, heap: &Arc<dyn WeakHeapRef>) -> bool {
        self.inner
            .heap
            .as_ref()
            .is_some_and(|own| Arc::ptr_eq(own, heap))
    }

    /// Whether both handles were issued by the same heap. A detached handle
    /// (test-only, no heap) matches only another detached one.
    #[must_use]
    pub fn same_heap_as(&self, other: &Self) -> bool {
        match (&self.inner.heap, &other.inner.heap) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }
}

impl Drop for HandleInner {
    fn drop(&mut self) {
        // When the last Handle clone is dropped, remove from slab
        if let Some(ref heap) = self.heap {
            heap.release_handle(self.slab_key);
        }
    }
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handle")
            .field("slab_key", &self.inner.slab_key)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_clone() {
        let handle1 = Handle::new_detached(42);
        let handle2 = handle1.clone();

        assert_eq!(handle1.slab_key(), 42);
        assert_eq!(handle2.slab_key(), 42);
    }

    #[test]
    fn test_handle_debug() {
        let handle = Handle::new_detached(42);
        let debug_str = format!("{:?}", handle);
        assert!(debug_str.contains("42"));
    }
}
