//! Handle type for external/FFI boundary.
//!
//! Internal VM code uses `ObjectIndex` for fast access. External code
//! (Python bindings, JS bindings, etc.) uses opaque `Handle` values
//! that are validated before use.

use std::sync::Arc;

use bex_vm_types::ObjectIndex;

use crate::EpochGuard;

/// Trait for releasing handles back to the heap.
///
/// This is implemented by `BexHeap` to allow handles to clean up
/// when dropped, without creating a circular dependency.
pub trait WeakHeapRef: Send + Sync {
    /// Release a handle slot by its slab key.
    fn release_handle(&self, slab_key: usize);

    /// Resolve a handle to its current ObjectIndex.
    /// Returns None if handle is invalid.
    fn resolve_handle(&self, slab_key: usize) -> Option<ObjectIndex>;
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

    /// Get the ObjectIndex this handle points to.
    ///
    /// Requires an `EpochGuard` to prove the caller is in epoch-protected code.
    /// This ensures GC cannot run and invalidate the returned index before
    /// the caller uses it.
    ///
    /// # When to use
    ///
    /// Use this method when you need the raw `ObjectIndex` for VM operations
    /// (e.g., pushing to stack, storing in objects). Only callable from
    /// epoch-protected code paths.
    ///
    /// # For external code
    ///
    /// External code (`baml_sys`) should use the heap accessor API instead:
    /// - `heap.read_string(handle)`
    /// - `heap.with_object(handle, |obj| ...)`
    ///
    /// Returns None if the handle has been invalidated.
    pub fn object_index(&self, _guard: &EpochGuard<'_>) -> Option<ObjectIndex> {
        self.inner
            .heap
            .as_ref()?
            .resolve_handle(self.inner.slab_key)
    }

    /// Get the slab key for this handle.
    ///
    /// This is primarily for internal use by `bex_heap`.
    pub fn slab_key(&self) -> usize {
        self.inner.slab_key
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
        // Note: object_index() requires EpochGuard and returns None for detached handles
    }

    #[test]
    fn test_handle_debug() {
        let handle = Handle::new_detached(42);
        let debug_str = format!("{:?}", handle);
        assert!(debug_str.contains("42"));
    }
}
