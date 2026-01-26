//! Safe accessor API for external code to read heap objects.
//!
//! External code (`baml_sys`) runs outside the epoch system and cannot
//! safely hold bare `HeapPtr` values. This module provides an API
//! that holds the handle table lock during access, preventing GC races.
//!
//! # Example
//!
//! ```ignore
//! // In baml_sys code (no EpochGuard available)
//! let content = heap.read_string(&handle)?;
//!
//! // Or for complex access with GC protection:
//! let result = heap.with_gc_protection(|protected| {
//!     let ptr = protected.resolve_handle(handle.slab_key())?;
//!     // ptr is safe to use - GC cannot run while we hold the guard
//!     Some(recursive_snapshot(protected, ptr))
//! });
//! ```

use std::sync::RwLockReadGuard;

use bex_external_types::Handle;
use bex_vm_types::{HeapPtr, Object, Value};

use crate::BexHeap;

/// Guard type proving the handles read lock is held.
///
/// This type can only be obtained from `BexHeap::with_gc_protection`.
/// Methods that return `HeapPtr` require this guard to ensure
/// the pointer remains valid (GC cannot run while the lock is held).
pub struct GcProtectedHeap<'a> {
    heap: &'a BexHeap,
    // Hold the read lock - prevents GC from updating handles
    _guard: RwLockReadGuard<'a, std::collections::HashMap<usize, HeapPtr>>,
}

impl<'a> GcProtectedHeap<'a> {
    /// Resolve a handle's slab key to a HeapPtr.
    ///
    /// Safe because we hold the handles read lock, preventing GC from
    /// moving objects and invalidating pointers.
    pub fn resolve_handle(&self, slab_key: usize) -> Option<HeapPtr> {
        self._guard.get(&slab_key).copied()
    }

    /// Get the underlying heap reference.
    ///
    /// Use this for operations that don't return HeapPtr
    /// (e.g., reading object contents).
    pub fn heap(&self) -> &'a BexHeap {
        self.heap
    }
}

/// Safe object access API for external code.
///
/// All methods hold the handle table read lock during access,
/// ensuring GC cannot run and invalidate pointers mid-operation.
impl BexHeap {
    /// Read a string object through a handle.
    ///
    /// Returns `None` if the handle is invalid or doesn't point to a string.
    pub fn read_string(&self, handle: &Handle) -> Option<String> {
        // Hold read lock on handles during entire operation
        let handles = self.handles.read().ok()?;
        let ptr = *handles.get(&handle.slab_key())?;

        // SAFETY: We hold the handles read lock, so GC cannot run
        // (GC needs write lock). The pointer is valid.
        let obj = unsafe { ptr.get() };
        match obj {
            Object::String(s) => Some(s.clone()),
            _ => None,
        }
    }

    /// Read an array object through a handle.
    ///
    /// Returns a clone of the array values.
    /// Returns `None` if the handle is invalid or doesn't point to an array.
    pub fn read_array(&self, handle: &Handle) -> Option<Vec<Value>> {
        let handles = self.handles.read().ok()?;
        let ptr = *handles.get(&handle.slab_key())?;

        let obj = unsafe { ptr.get() };
        match obj {
            Object::Array(arr) => Some(arr.clone()),
            _ => None,
        }
    }

    /// Read a map object through a handle.
    ///
    /// Returns a clone of the map.
    /// Returns `None` if the handle is invalid or doesn't point to a map.
    pub fn read_map(&self, handle: &Handle) -> Option<indexmap::IndexMap<String, Value>> {
        let handles = self.handles.read().ok()?;
        let ptr = *handles.get(&handle.slab_key())?;

        let obj = unsafe { ptr.get() };
        match obj {
            Object::Map(map) => Some(map.clone()),
            _ => None,
        }
    }

    /// Access an object through a handle with a closure.
    ///
    /// The closure receives a reference to the object. GC cannot run
    /// while the closure executes.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let len = heap.with_object(&handle, |obj| {
    ///     match obj {
    ///         Object::Array(arr) => arr.len(),
    ///         Object::String(s) => s.len(),
    ///         _ => 0,
    ///     }
    /// })?;
    /// ```
    pub fn with_object<R>(&self, handle: &Handle, f: impl FnOnce(&Object) -> R) -> Option<R> {
        let handles = self.handles.read().ok()?;
        let ptr = *handles.get(&handle.slab_key())?;

        // SAFETY: Handle table read lock held, GC cannot run
        let obj = unsafe { ptr.get() };
        Some(f(obj))
    }

    /// Execute a closure while holding the handles read lock.
    ///
    /// This prevents GC from updating handle pointers during the operation.
    /// The closure receives a `GcProtectedHeap` which provides safe access
    /// to `resolve_handle` - you can't accidentally call it without the lock.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Snapshot an entire object graph while protected from GC
    /// let snapshot = heap.with_gc_protection(|protected| {
    ///     let ptr = protected.resolve_handle(handle.slab_key())?;
    ///     recursive_snapshot(protected.heap(), ptr)
    /// });
    /// ```
    pub fn with_gc_protection<R>(&self, f: impl FnOnce(GcProtectedHeap<'_>) -> R) -> R {
        let guard = self.handles.read().expect("handles lock poisoned");
        f(GcProtectedHeap {
            heap: self,
            _guard: guard,
        })
    }
}
