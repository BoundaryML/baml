//! Raw pointer-based heap references.
//!
// This module is fundamentally about unsafe pointer operations - that's the whole point.
// The unsafe code here is intentional and necessary for the HeapPtr design.
#![allow(unsafe_code)]
//!
//! `HeapPtr` is a raw pointer to an `Object` in the heap. It replaces
//! the index-based `ObjectIndex` to eliminate data races during concurrent
//! access.
//!
//! # Why Raw Pointers?
//!
//! With index-based access, reading an object required traversing the
//! `ChunkedVec`'s internal `Vec` to find the chunk. If another thread
//! was resizing (adding chunks), the `Vec`'s buffer could reallocate,
//! causing the reader to access freed memory.
//!
//! With raw pointers, reading is just a dereference. The pointer points
//! directly into a chunk, and chunks never move (they're `Box<[T]>`).
//!
//! # Safety Invariants
//!
//! For `HeapPtr` to be sound:
//!
//! 1. **Pointer stability:** Points into a `ChunkedVec` chunk or compile-time
//!    Vec, neither of which ever relocate existing data.
//!
//! 2. **Lifetime:** Valid until GC collects or moves the object. After GC,
//!    use the forwarding table to get the new pointer.
//!
//! 3. **Thread safety:** The pointer can be copied across threads (it's just
//!    8 bytes). Dereferencing only happens within a single VM.

use crate::Object;

/// A pointer to an object in the heap.
///
/// # Safety
///
/// `HeapPtr` contains a raw pointer that is:
/// - Stable: points into a `ChunkedVec` chunk that never moves
/// - Valid: only created by heap allocation, always points to valid Object
/// - Synchronized: dereferencing only happens within a single VM
#[derive(Clone, Copy)]
pub struct HeapPtr {
    ptr: *mut Object,
    #[cfg(feature = "heap_debug")]
    epoch: u32,
}

// Raw pointers are !Send and !Sync by default.
// We implement them manually because:
// - The pointer is treated as data (8 bytes), not an active reference
// - Copying/storing the pointer across threads is safe
// - Dereferencing only happens in unsafe code within a single VM
// - The pointed-to memory is stable (chunks never move)
unsafe impl Send for HeapPtr {}
unsafe impl Sync for HeapPtr {}

impl HeapPtr {
    /// Create a new HeapPtr from a raw pointer.
    ///
    /// # Safety
    ///
    /// The pointer must point to a valid Object in the heap that will
    /// remain valid for the lifetime of this HeapPtr (until GC collects it).
    #[cfg(not(feature = "heap_debug"))]
    #[inline]
    pub unsafe fn from_ptr(ptr: *mut Object) -> Self {
        Self { ptr }
    }

    /// Create a new `HeapPtr` from a raw pointer with epoch tracking.
    ///
    /// # Safety
    ///
    /// The pointer must point to a valid Object in the heap that will
    /// remain valid for the lifetime of this `HeapPtr` (until GC collects it).
    #[cfg(feature = "heap_debug")]
    #[inline]
    pub unsafe fn from_ptr(ptr: *mut Object, epoch: u32) -> Self {
        Self { ptr, epoch }
    }

    /// Get the raw pointer.
    #[inline]
    pub fn as_ptr(self) -> *mut Object {
        self.ptr
    }

    /// Dereference to get a reference to the object.
    ///
    /// # Safety
    ///
    /// - The pointer must still be valid (object not collected by GC)
    /// - No other thread is writing to this object
    /// - With `heap_debug`: epoch must match current heap epoch
    #[inline]
    pub unsafe fn get(self) -> &'static Object {
        // SAFETY: Caller guarantees pointer validity and aliasing rules
        unsafe { &*self.ptr }
    }

    /// Dereference to get a mutable reference to the object.
    ///
    /// # Safety
    ///
    /// - The pointer must still be valid (object not collected by GC)
    /// - No other thread is accessing this object
    /// - With `heap_debug`: epoch must match current heap epoch
    #[inline]
    pub unsafe fn get_mut(self) -> &'static mut Object {
        // SAFETY: Caller guarantees pointer validity and exclusive access
        unsafe { &mut *self.ptr }
    }

    /// Get the epoch (for stale pointer detection in debug mode).
    #[cfg(feature = "heap_debug")]
    #[inline]
    pub fn epoch(self) -> u32 {
        self.epoch
    }

    /// Get the epoch (always 0 in non-debug mode).
    #[cfg(not(feature = "heap_debug"))]
    #[inline]
    pub fn epoch(self) -> u32 {
        0
    }
}

impl PartialEq for HeapPtr {
    fn eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }
}

impl Eq for HeapPtr {}

impl PartialOrd for HeapPtr {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HeapPtr {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.ptr as usize).cmp(&(other.ptr as usize))
    }
}

impl std::hash::Hash for HeapPtr {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.ptr.hash(state);
    }
}

impl std::fmt::Debug for HeapPtr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(feature = "heap_debug")]
        {
            write!(f, "HeapPtr({:p}@{})", self.ptr, self.epoch)
        }
        #[cfg(not(feature = "heap_debug"))]
        {
            write!(f, "HeapPtr({:p})", self.ptr)
        }
    }
}

impl std::fmt::Display for HeapPtr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:p}", self.ptr)
    }
}
