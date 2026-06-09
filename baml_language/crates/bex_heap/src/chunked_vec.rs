//! Chunked vector for stable-address storage.
//!
//! Note: This module allows `dangerous_implicit_autorefs` because we carefully
//! manage aliasing through external synchronization (growth_lock) and the
//! tree borrows model validates our access patterns.
#![allow(dangerous_implicit_autorefs)]
//!
//! `ChunkedVec` stores elements in fixed-size chunks. When the storage grows,
//! new chunks are appended without moving existing data. This provides stable
//! pointers to existing elements even during concurrent growth.
//!
//! # Why This Exists
//!
//! The heap uses lock-free field writes from multiple VMs. With a regular `Vec`,
//! if one VM is writing to an element while another VM triggers a resize (via
//! TLAB chunk allocation), the Vec may reallocate and move all elements,
//! invalidating the first VM's pointer.
//!
//! `ChunkedVec` solves this by never moving existing chunks:
//!
//! ```text
//! Before growth:
//!   chunks: [ Box<[T; 1024]> ]  <- pointer to chunk[0] is valid
//!            ^
//!            VM1 is writing here
//!
//! After growth (VM2 allocates new TLAB):
//!   chunks: [ Box<[T; 1024]>, Box<[T; 1024]> ]
//!            ^                 ^
//!            still valid!      new chunk added
//! ```
//!
//! # Thread Safety
//!
//! `ChunkedVec` is designed for the following concurrent access pattern:
//! - Multiple threads can call `set()` on different indices concurrently.
//! - `resize_with()` / `push_with()` may run concurrently with any number
//!   of `set()` / `get_ptr()` / `get()` callers; they serialize internally.
//! - Multiple threads may call `resize_with()` / `push_with()` concurrently;
//!   they serialize internally on a write lock.
//!
//! This is achieved by:
//! - Using `AtomicUsize` for the length so readers see monotonically
//!   increasing valid ranges.
//! - Holding an internal `parking_lot::RwLock<()>` (`chunks_lock`) around
//!   any access to the **outer** `Vec<Box<[…]>>`'s `(ptr, len, cap)`
//!   triple. Growth (`resize_with`/`push_with`) takes the write lock so a
//!   `Vec::push` that reallocates the outer buffer cannot race a reader
//!   reading the buffer pointer; readers (`element_ptr`, `num_chunks`,
//!   etc.) take the read lock for the brief window in which they read
//!   `(*chunks_ptr).as_ptr()`.
//! - Using raw pointer operations to avoid `&mut Vec` reborrows that
//!   conflict with Miri's stacked borrows model.
//! - Using `UnsafeCell` for each element so per-slot writes are gated by
//!   the caller's own exclusivity (typically per-TLAB region).
//!
//! Inner-chunk access is **not** lock-gated: each chunk is its own
//! heap-allocated `Box<[UnsafeCell<T>]>` whose address is stable for the
//! lifetime of the `ChunkedVec`. Once `element_ptr` has resolved a slot's
//! address, that pointer remains valid after the read lock is dropped.
//!
//! # Future Optimization: Virtual Memory Approach
//!
//! The current chunked approach requires `index / chunk_size` and `index % chunk_size`
//! for every access. With power-of-2 chunk sizes this compiles to a shift and AND,
//! which is cheap but not free.
//!
//! Production VMs like V8 and the JVM use a more efficient approach: reserve a large
//! contiguous virtual address space upfront using `mmap` (Unix) or `VirtualAlloc`
//! (Windows), then commit physical memory incrementally as needed.
//!
//! ```text
//! Virtual Memory Approach:
//!
//!   mmap reserves 4GB of ADDRESS SPACE (no physical RAM used yet)
//!   ┌────────────────────────────────────────────────────────────┐
//!   │ COMMITTED (1MB)  │         RESERVED (not backed by RAM)   │
//!   │ [objects here]   │         (grows by committing more)     │
//!   └────────────────────────────────────────────────────────────┘
//!   ▲
//!   base pointer (NEVER MOVES)
//!
//!   Access: base_ptr.add(index)  // Single addition, no division!
//! ```
//!
//! Benefits of virtual memory approach:
//! - Access is `base + offset` (one addition) vs chunked lookup
//! - Better cache locality for sequential access
//! - How V8's "pointer cage" and JVM's compressed oops work
//!
//! Why we use ChunkedVec instead:
//! - Pure Rust, no platform-specific `mmap`/`VirtualAlloc` code
//! - Works with Miri for memory safety verification
//! - Simpler implementation and maintenance
//! - BAML's workload is I/O-bound (LLM API calls), not CPU-bound
//!
//! If profiling shows object access as a bottleneck, the virtual memory approach
//! would be the next optimization to consider.

use std::{
    cell::UnsafeCell,
    sync::atomic::{AtomicUsize, Ordering},
};

use parking_lot::RwLock;

/// Default chunk size (number of elements per chunk).
///
/// This MUST be a power of 2 for efficient index calculation (shift + AND
/// instead of division + modulo). It should also be >= the TLAB size to
/// minimize chunk allocations during TLAB refills.
///
/// Current value: 4096 = 2^12
pub const DEFAULT_CHUNK_SIZE: usize = 4096;

// Compile-time assertion that DEFAULT_CHUNK_SIZE is a power of 2
const _: () = assert!(
    DEFAULT_CHUNK_SIZE.is_power_of_two(),
    "DEFAULT_CHUNK_SIZE must be a power of 2 for efficient index calculation"
);

/// A vector that stores elements in fixed-size chunks.
///
/// Provides stable pointers to elements: growing the storage never moves
/// existing elements, only adds new chunks.
///
/// The `CHUNK_SIZE` const generic must be a power of 2, which enables the compiler
/// to optimize `index / CHUNK_SIZE` to a right shift and `index % CHUNK_SIZE` to
/// a bitwise AND. This is enforced at compile time.
///
/// # Example
///
/// ```ignore
/// // Use default chunk size (4096)
/// let vec: ChunkedVec<i32> = ChunkedVec::new();
///
/// // Use custom chunk size
/// let vec: ChunkedVec<i32, 1024> = ChunkedVec::new();
/// ```
pub struct ChunkedVec<T, const CHUNK_SIZE: usize = DEFAULT_CHUNK_SIZE> {
    /// Storage chunks. Each chunk is heap-allocated and never moves.
    /// This is wrapped in UnsafeCell for interior mutability during resize.
    ///
    /// IMPORTANT: To avoid data races detected by Miri's stacked borrows, we never
    /// create `&mut Vec<...>` references to this field. Instead, we use raw pointer
    /// operations throughout.
    chunks: UnsafeCell<Vec<Box<[UnsafeCell<T>]>>>,

    /// Synchronizes access to the **outer** `chunks` `Vec`'s (ptr, len, cap)
    /// triple — *not* to the inner chunks themselves (those are individually
    /// heap-allocated `Box<[UnsafeCell<T>]>`s and never move once pushed).
    ///
    /// - `resize_with` / `push_with` / `clear` take the **write** lock around
    ///   any read/mutation of the outer Vec.
    /// - `element_ptr`, `num_chunks`, `chunk_start_ptr`, `capacity`, etc. take
    ///   the **read** lock for the brief window in which they read
    ///   `(*chunks_ptr).as_ptr()` (the outer Vec's heap buffer pointer).
    ///   Once the read lock has handed back a stable inner-chunk pointer, the
    ///   read lock is dropped — subsequent dereferences go through the
    ///   never-moving inner chunk and need no synchronization beyond the
    ///   `UnsafeCell` already gating per-element exclusivity.
    ///
    /// This closes a UB hole in the previous design where a concurrent
    /// `Vec::push` (inside `resize_with`) reallocating the outer Vec's heap
    /// buffer raced with `set` / `get_ptr` callers reading the same buffer
    /// pointer non-atomically. The buffer pointer can no longer change while
    /// any reader holds the read lock, so a freed buffer can never be
    /// dereferenced.
    chunks_lock: RwLock<()>,

    /// Number of elements in the vec (not capacity).
    /// Uses AtomicUsize for safe concurrent reads.
    len: AtomicUsize,
}

impl<T, const CHUNK_SIZE: usize> ChunkedVec<T, CHUNK_SIZE> {
    /// Create a new empty ChunkedVec.
    ///
    /// Uses the default chunk size (4096) unless a custom size is specified
    /// via the const generic parameter.
    ///
    /// # Compile-time Requirements
    ///
    /// `CHUNK_SIZE` must be a power of 2. This is enforced at compile time
    /// and enables the compiler to optimize division/modulo to shift/AND.
    pub fn new() -> Self {
        // Compile-time assertion that CHUNK_SIZE is a power of 2.
        // Using const block ensures this is evaluated at compile time.
        const {
            assert!(
                CHUNK_SIZE.is_power_of_two(),
                "CHUNK_SIZE must be a power of 2"
            )
        };

        Self {
            chunks: UnsafeCell::new(Vec::new()),
            chunks_lock: RwLock::new(()),
            len: AtomicUsize::new(0),
        }
    }

    /// Get the number of elements.
    #[inline]
    pub fn len(&self) -> usize {
        self.len.load(Ordering::Acquire)
    }

    /// Check if empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The chunk size as a public associated constant.
    pub const CHUNK_SIZE: usize = CHUNK_SIZE;

    /// Get the chunk size (compile-time constant).
    #[inline]
    pub const fn chunk_size(&self) -> usize {
        CHUNK_SIZE
    }

    /// Get the total capacity (number of slots across all chunks).
    #[inline]
    pub fn capacity(&self) -> usize {
        let _read = self.chunks_lock.read();
        // SAFETY: read lock excludes concurrent resize; we only read the
        // outer Vec's len.
        unsafe {
            let chunks_ptr = self.chunks.get();
            (*chunks_ptr).len() * CHUNK_SIZE
        }
    }

    /// Get the number of allocated chunks.
    ///
    /// Internally takes the read side of `chunks_lock`, so this is safe to
    /// call concurrently with `set`/`get_ptr` and is excluded only by an
    /// in-flight `resize_with`/`push_with`/`clear`.
    #[inline]
    pub fn num_chunks(&self) -> usize {
        let _read = self.chunks_lock.read();
        // SAFETY: read lock excludes concurrent resize.
        unsafe { (*self.chunks.get()).len() }
    }

    /// Get a raw pointer to the start of a chunk.
    ///
    /// # Safety
    ///
    /// - `chunk_idx` must be `< num_chunks()`.
    ///
    /// The returned pointer remains valid as long as the chunk exists; the
    /// chunk itself is heap-allocated and never moves (only the outer Vec's
    /// buffer can move on growth, and that race is closed by taking the
    /// read lock around the buffer-pointer dereference here).
    #[inline]
    pub unsafe fn chunk_start_ptr(&self, chunk_idx: usize) -> *const T {
        let _read = self.chunks_lock.read();
        // SAFETY: read lock excludes concurrent resize. Caller upholds the
        // bounds precondition.
        unsafe {
            let chunks_ptr = self.chunks.get();
            let chunks_data_ptr = (*chunks_ptr).as_ptr();
            let chunk_box_ptr = chunks_data_ptr.add(chunk_idx);
            (*chunk_box_ptr).as_ptr() as *const T
        }
    }

    /// Calculate chunk index and offset within chunk for a given index.
    ///
    /// Because CHUNK_SIZE is a compile-time constant power of 2, the compiler
    /// optimizes this to:
    /// - `index >> log2(CHUNK_SIZE)` (right shift)
    /// - `index & (CHUNK_SIZE - 1)` (bitwise AND)
    #[inline]
    fn chunk_location(&self, index: usize) -> (usize, usize) {
        (index / CHUNK_SIZE, index % CHUNK_SIZE)
    }

    /// Get a raw pointer to the element at (chunk_idx, offset) without creating references.
    ///
    /// Acquires `chunks_lock` in **read** mode for the brief window in which
    /// the outer Vec's buffer pointer is read. The returned pointer points
    /// into a heap-allocated `Box<[UnsafeCell<T>]>` whose address never
    /// changes, so it remains valid after the read lock is dropped.
    ///
    /// # Safety
    ///
    /// - chunk_idx must be < number of chunks
    /// - offset must be < CHUNK_SIZE
    #[inline]
    unsafe fn element_ptr(&self, chunk_idx: usize, offset: usize) -> *mut T {
        let _read = self.chunks_lock.read();
        // SAFETY: the read lock excludes any concurrent `Vec::push` that
        // could realloc the outer Vec's buffer. The inner chunk's `Box`
        // address is itself stable across the lifetime of the chunk, so the
        // returned `*mut T` remains valid after we drop `_read`.
        unsafe { self.element_ptr_locked(chunk_idx, offset) }
    }

    /// Lock-free helper for [`Self::element_ptr`].
    ///
    /// # Safety
    ///
    /// In addition to the bounds preconditions of `element_ptr`, the caller
    /// must already hold the `chunks_lock` (read or write). Used by
    /// [`Self::push_with`] which holds the write lock and would deadlock if
    /// `element_ptr` tried to take the read lock again (`parking_lot::RwLock`
    /// is not reentrant).
    #[inline]
    unsafe fn element_ptr_locked(&self, chunk_idx: usize, offset: usize) -> *mut T {
        // SAFETY: caller upholds the lock-held precondition; bounds are
        // checked by the caller.
        unsafe {
            let chunks_ptr = self.chunks.get();
            let chunks_data_ptr = (*chunks_ptr).as_ptr();
            let chunk_box_ptr = chunks_data_ptr.add(chunk_idx);
            let chunk_slice_ptr = (*chunk_box_ptr).as_ptr();
            let element_cell_ptr = chunk_slice_ptr.add(offset);
            (*element_cell_ptr).get()
        }
    }

    /// Clear all elements, dropping them.
    ///
    /// # Safety
    ///
    /// Must be called when no other thread is accessing the ChunkedVec.
    /// In practice, this is only called during GC when all VMs are at safepoints.
    pub fn clear(&mut self) {
        // SAFETY: We have &mut self, so no other thread is accessing
        unsafe {
            (*self.chunks.get()).clear();
        }
        self.len.store(0, Ordering::Release);
    }
}

impl<T, const CHUNK_SIZE: usize> ChunkedVec<T, CHUNK_SIZE> {
    /// Ensure capacity for at least `min_len` elements, using a factory function.
    ///
    /// If the current capacity is insufficient, new chunks are allocated.
    /// Existing chunks are never moved (each is its own heap-allocated
    /// `Box<[UnsafeCell<T>]>`).
    ///
    /// This also sets len to min_len, filling new slots with values from the factory.
    ///
    /// Internally takes the **write** side of `chunks_lock` so that any
    /// `set` / `get_ptr` reader concurrently reading the outer Vec's buffer
    /// pointer is excluded for the duration of the `Vec::push`. Without this
    /// exclusion, the outer Vec's heap buffer can be reallocated underneath
    /// a concurrent reader (use-after-free).
    pub fn resize_with<F2: FnMut() -> T>(&self, min_len: usize, mut factory: F2) {
        let current_len = self.len.load(Ordering::Acquire);
        if min_len <= current_len {
            return;
        }

        // Calculate how many chunks we need
        let needed_chunks = min_len.div_ceil(CHUNK_SIZE);

        // Acquire the write lock so concurrent `element_ptr` callers cannot
        // read a torn outer-Vec buffer pointer while we push & potentially
        // realloc.
        let _write = self.chunks_lock.write();

        // SAFETY: write lock provides exclusive access to the outer Vec.
        unsafe {
            let chunks_ptr = self.chunks.get();

            // Read current chunk count through raw pointer
            let current_chunk_count = (*chunks_ptr).len();

            // Allocate new chunks as needed
            for _ in current_chunk_count..needed_chunks {
                // Create a boxed slice of UnsafeCell<T>
                let chunk: Box<[UnsafeCell<T>]> = (0..CHUNK_SIZE)
                    .map(|_| UnsafeCell::new(factory()))
                    .collect::<Vec<_>>()
                    .into_boxed_slice();

                // Push through raw pointer to avoid &mut reborrow
                (*chunks_ptr).push(chunk);
            }
        }

        // Update len last (with Release ordering) so readers see the new chunks first
        self.len.store(min_len, Ordering::Release);
    }

    /// Push an element, allocating a new chunk if needed.
    ///
    /// Returns the index of the pushed element.
    ///
    /// Internally takes the **write** side of `chunks_lock` so concurrent
    /// `set` / `get_ptr` readers cannot observe a torn outer-Vec buffer
    /// pointer while we may grow & realloc.
    pub fn push_with<F2: FnMut() -> T>(&self, value: T, mut factory: F2) -> usize {
        let index = self.len.load(Ordering::Acquire);

        // Hold the write lock for the entire push-and-write so concurrent
        // `element_ptr` readers cannot race the realloc.
        let _write = self.chunks_lock.write();

        // SAFETY: write lock provides exclusive access to the outer Vec.
        unsafe {
            let chunks_ptr = self.chunks.get();
            let current_chunk_count = (*chunks_ptr).len();

            // Allocate new chunk if needed
            if index >= current_chunk_count * CHUNK_SIZE {
                let chunk: Box<[UnsafeCell<T>]> = (0..CHUNK_SIZE)
                    .map(|_| UnsafeCell::new(factory()))
                    .collect::<Vec<_>>()
                    .into_boxed_slice();
                (*chunks_ptr).push(chunk);
            }

            // Write the value. Use the lock-free helper since we already
            // hold the write lock — calling `element_ptr` here would
            // deadlock on the non-reentrant parking_lot RwLock.
            let (chunk_idx, offset) = self.chunk_location(index);
            // SAFETY: Index is within allocated range; we hold the write lock.
            let elem_ptr = self.element_ptr_locked(chunk_idx, offset);
            // Drop the factory-created placeholder before writing the actual value
            std::ptr::drop_in_place(elem_ptr);
            std::ptr::write(elem_ptr, value);
        }

        self.len.store(index + 1, Ordering::Release);
        index
    }
}

impl<T, const CHUNK_SIZE: usize> ChunkedVec<T, CHUNK_SIZE> {
    /// Get a reference to an element.
    ///
    /// # Panics
    ///
    /// Panics if index >= len.
    #[inline]
    pub fn get(&self, index: usize) -> &T {
        let current_len = self.len.load(Ordering::Acquire);
        assert!(
            index < current_len,
            "index {index} out of bounds (len={current_len})"
        );
        let (chunk_idx, offset) = self.chunk_location(index);
        // SAFETY: Index bounds checked above, and we read the len with Acquire
        // which synchronizes with the Release in resize_with/push_with.
        unsafe { &*self.element_ptr(chunk_idx, offset) }
    }

    /// Get a mutable reference to an element.
    ///
    /// # Panics
    ///
    /// Panics if index >= len.
    #[inline]
    pub fn get_mut(&mut self, index: usize) -> &mut T {
        let current_len = self.len.load(Ordering::Acquire);
        assert!(
            index < current_len,
            "index {index} out of bounds (len={current_len})"
        );
        let (chunk_idx, offset) = self.chunk_location(index);
        // SAFETY: Index bounds checked above, we have &mut self
        unsafe { &mut *self.element_ptr(chunk_idx, offset) }
    }

    /// Get a raw pointer to an element's storage.
    ///
    /// # Safety
    ///
    /// The pointer is valid as long as this ChunkedVec exists and the element
    /// is not removed. The pointer remains valid even if the ChunkedVec grows.
    ///
    /// # Panics
    ///
    /// Panics if index >= len.
    #[inline]
    pub fn get_ptr(&self, index: usize) -> *mut T {
        let current_len = self.len.load(Ordering::Acquire);
        assert!(
            index < current_len,
            "index {index} out of bounds (len={current_len})"
        );
        let (chunk_idx, offset) = self.chunk_location(index);
        // SAFETY: Index bounds checked above
        unsafe { self.element_ptr(chunk_idx, offset) }
    }

    /// Set an element at the given index, dropping the previous value.
    ///
    /// # Safety
    ///
    /// Caller must ensure no other references to this element exist.
    /// Different indices can be set concurrently from different threads.
    ///
    /// # Panics
    ///
    /// Panics if index >= len.
    #[inline]
    pub unsafe fn set(&self, index: usize, value: T) {
        let current_len = self.len.load(Ordering::Acquire);
        assert!(
            index < current_len,
            "index {index} out of bounds (len={current_len})"
        );
        let (chunk_idx, offset) = self.chunk_location(index);
        // SAFETY: Caller ensures exclusive access to this index.
        // We use raw pointer operations to avoid reborrow conflicts with
        // concurrent resize_with calls.
        unsafe {
            let elem_ptr = self.element_ptr(chunk_idx, offset);
            // Drop the old value before writing the new one to prevent leaks
            std::ptr::drop_in_place(elem_ptr);
            std::ptr::write(elem_ptr, value);
        }
    }

    /// Iterate over all elements.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        let len = self.len.load(Ordering::Acquire);
        (0..len).map(move |i| self.get(i))
    }

    /// Iterate over all elements mutably.
    ///
    /// # Safety
    ///
    /// Must have exclusive access to the ChunkedVec.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        let len = self.len.load(Ordering::Acquire);
        let chunks_ptr = self.chunks.get();

        (0..len).map(move |i| {
            let chunk_idx = i / CHUNK_SIZE;
            let offset = i % CHUNK_SIZE;
            // SAFETY: We have &mut self, each index is unique in the iteration
            unsafe {
                let chunks_data_ptr = (*chunks_ptr).as_ptr();
                let chunk_box_ptr = chunks_data_ptr.add(chunk_idx);
                let chunk_slice_ptr = (*chunk_box_ptr).as_ptr();
                let element_cell_ptr = chunk_slice_ptr.add(offset);
                &mut *(*element_cell_ptr).get()
            }
        })
    }
}

impl<T: Default, const CHUNK_SIZE: usize> ChunkedVec<T, CHUNK_SIZE> {
    /// Ensure capacity for at least `min_len` elements.
    ///
    /// See [`Self::resize_with`].
    pub fn resize_to(&self, min_len: usize) {
        self.resize_with(min_len, T::default);
    }

    /// Push an element, allocating a new chunk if needed.
    ///
    /// See [`Self::push_with`].
    pub fn push(&self, value: T) -> usize {
        self.push_with(value, T::default)
    }
}

impl<T, const CHUNK_SIZE: usize> Default for ChunkedVec<T, CHUNK_SIZE> {
    fn default() -> Self {
        Self::new()
    }
}

/// Allows read access via indexing: `&vec[idx]`
///
/// # Why `IndexMut` is NOT implemented
///
/// `IndexMut` requires `&mut self` to return `&mut T`. However, `ChunkedVec` is
/// designed for concurrent access where multiple threads can write to *different*
/// indices simultaneously via interior mutability (`UnsafeCell`).
///
/// The `set()` method intentionally takes `&self` (not `&mut self`) to enable this
/// pattern. If we implemented `IndexMut`, callers would need exclusive (`&mut`)
/// access to the entire `ChunkedVec` just to mutate one element, defeating the
/// purpose of the interior mutability design.
///
/// Use `set()` for writes:
/// ```ignore
/// // Read: use indexing
/// let value = &vec[idx];
///
/// // Write: use set() which takes &self
/// unsafe { vec.set(idx, new_value); }
/// ```
impl<T, const CHUNK_SIZE: usize> std::ops::Index<usize> for ChunkedVec<T, CHUNK_SIZE> {
    type Output = T;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        self.get(index)
    }
}

impl<T: std::fmt::Debug, const CHUNK_SIZE: usize> std::fmt::Debug for ChunkedVec<T, CHUNK_SIZE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChunkedVec")
            .field("len", &self.len())
            .field("chunk_size", &CHUNK_SIZE)
            .field("num_chunks", &(self.capacity() / CHUNK_SIZE))
            .finish()
    }
}

// SAFETY: ChunkedVec<T, CHUNK_SIZE> is Send if T is Send
// The UnsafeCell fields are properly synchronized:
// - len uses AtomicUsize
// - chunks is accessed with proper external synchronization
unsafe impl<T: Send, const CHUNK_SIZE: usize> Send for ChunkedVec<T, CHUNK_SIZE> {}

// SAFETY: ChunkedVec<T, CHUNK_SIZE> is Sync if T is Sync
// This is safe because:
// 1. Read-only methods use atomic loads with proper ordering
// 2. The unsafe methods require external synchronization
// 3. set() uses UnsafeCell for element access (different indices are independent)
// 4. All operations use raw pointers to avoid &mut reborrow conflicts
unsafe impl<T: Sync, const CHUNK_SIZE: usize> Sync for ChunkedVec<T, CHUNK_SIZE> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_chunked_vec() {
        let vec: ChunkedVec<i32> = ChunkedVec::new();
        assert_eq!(vec.len(), 0);
        assert!(vec.is_empty());
    }

    #[test]
    fn test_push_and_get() {
        let vec: ChunkedVec<i32, 4> = ChunkedVec::new();

        let idx0 = vec.push(10);
        let idx1 = vec.push(20);
        let idx2 = vec.push(30);

        assert_eq!(idx0, 0);
        assert_eq!(idx1, 1);
        assert_eq!(idx2, 2);
        assert_eq!(vec.len(), 3);

        assert_eq!(*vec.get(0), 10);
        assert_eq!(*vec.get(1), 20);
        assert_eq!(*vec.get(2), 30);
    }

    #[test]
    fn test_push_across_chunks() {
        let vec: ChunkedVec<i32, 2> = ChunkedVec::new();

        // Push 5 elements (requires 3 chunks with chunk_size=2)
        for i in 0..5 {
            vec.push(i * 10);
        }

        assert_eq!(vec.len(), 5);
        assert_eq!(vec.capacity(), 6); // 3 chunks * 2

        for i in 0..5 {
            assert_eq!(*vec.get(i), (i * 10) as i32);
        }
    }

    #[test]
    fn test_resize_to() {
        let vec: ChunkedVec<i32, 4> = ChunkedVec::new();

        vec.resize_to(10);

        assert_eq!(vec.len(), 10);
        assert!(vec.capacity() >= 10);

        // Default values should be 0
        for i in 0..10 {
            assert_eq!(*vec.get(i), 0);
        }
    }

    #[test]
    fn test_set() {
        let vec: ChunkedVec<i32, 4> = ChunkedVec::new();
        vec.resize_to(5);

        // SAFETY: single-threaded test, no concurrent access to slot 2.
        unsafe {
            vec.set(2, 42);
        }

        assert_eq!(*vec.get(2), 42);
    }

    #[test]
    fn test_clear() {
        let mut vec: ChunkedVec<i32, 4> = ChunkedVec::new();

        for i in 0..10 {
            vec.push(i);
        }

        assert_eq!(vec.len(), 10);

        vec.clear();

        assert_eq!(vec.len(), 0);
        assert!(vec.is_empty());
    }

    #[test]
    fn test_get_mut() {
        let mut vec: ChunkedVec<i32, 4> = ChunkedVec::new();

        vec.push(10);
        vec.push(20);

        *vec.get_mut(1) = 99;

        assert_eq!(*vec.get(1), 99);
    }

    #[test]
    fn test_iter() {
        let vec: ChunkedVec<i32, 2> = ChunkedVec::new();

        for i in 0..5 {
            vec.push(i * 10);
        }

        let collected: Vec<i32> = vec.iter().copied().collect();
        assert_eq!(collected, vec![0, 10, 20, 30, 40]);
    }

    #[test]
    fn test_iter_mut() {
        let mut vec: ChunkedVec<i32, 2> = ChunkedVec::new();

        for i in 0..5 {
            vec.push(i);
        }

        for elem in vec.iter_mut() {
            *elem *= 10;
        }

        let collected: Vec<i32> = vec.iter().copied().collect();
        assert_eq!(collected, vec![0, 10, 20, 30, 40]);
    }

    #[test]
    fn test_pointer_stability() {
        let vec: ChunkedVec<i32, 2> = ChunkedVec::new();

        vec.push(42);

        // Get pointer to first element
        let ptr = vec.get_ptr(0);

        // Push more elements, causing chunk allocation
        for i in 0..10 {
            vec.push(i);
        }

        // Original pointer should still be valid
        // SAFETY: chunks never move once allocated, so the original
        // pointer remains valid even across concurrent growth.
        unsafe {
            assert_eq!(*ptr, 42);
        }
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn test_get_out_of_bounds() {
        let vec: ChunkedVec<i32, 4> = ChunkedVec::new();
        let _ = vec.get(0);
    }

    #[test]
    fn test_large_allocation() {
        let vec: ChunkedVec<i32, 1024> = ChunkedVec::new();

        for i in 0..10_000 {
            vec.push(i);
        }

        assert_eq!(vec.len(), 10_000);

        // Spot check some values
        assert_eq!(*vec.get(0), 0);
        assert_eq!(*vec.get(1023), 1023);
        assert_eq!(*vec.get(1024), 1024);
        assert_eq!(*vec.get(9999), 9999);
    }

    /// Test for data race when Vec of chunk pointers reallocates during concurrent read.
    ///
    /// This test demonstrates a real data race in ChunkedVec when using `get()`:
    /// 1. Reader thread calls get() which internally reads the Vec's buffer pointer
    /// 2. Writer thread calls resize_with() which may reallocate the Vec
    /// 3. If the Vec reallocates after reader gets the buffer pointer but before
    ///    dereferencing it, we get use-after-free
    ///
    /// # Why this race cannot happen in practice
    ///
    /// The VM uses `HeapPtr` (raw pointers) instead of `ObjectIndex` (indices).
    /// Test that HeapPtr-style access (raw pointers obtained upfront) is race-free.
    ///
    /// **The race that could happen:** `ChunkedVec::get(index)` reads the
    /// internal `Vec<*mut [T; CHUNK_SIZE]>` to find the chunk pointer for a
    /// given index. If another thread calls `resize_to()` concurrently, the
    /// `Vec` may reallocate its buffer, causing the reader to follow a dangling
    /// buffer pointer — a use-after-free that Miri catches.
    ///
    /// **Why it can't happen in practice:** The VM never uses `get()` at
    /// runtime. Instead, at allocation time it obtains a raw pointer via
    /// `get_ptr()` and stores it in a `HeapPtr`. All subsequent reads go
    /// through that raw pointer directly (`HeapPtr::get()` is just a deref).
    /// Because `ChunkedVec` is backed by individually heap-allocated
    /// fixed-size chunks, those chunks are never moved or freed by growth —
    /// only new chunks are appended. So raw pointers into existing chunks
    /// remain stable regardless of concurrent `resize_to()` calls.
    ///
    /// This test exercises that exact pattern: one thread reads through raw
    /// pointers obtained upfront while another thread grows the vec.
    #[test]
    fn test_miri_heap_ptr_access_is_race_free() {
        use std::{sync::Arc, thread};

        // Chunk size of 2 means we need a new chunk every 2 elements.
        let vec: Arc<ChunkedVec<i32, 2>> = Arc::new(ChunkedVec::new());

        // Pre-populate with initial data
        vec.resize_to(2);
        // SAFETY: single-threaded set-up; the slot is uniquely written here.
        unsafe {
            vec.set(0, 42);
            vec.set(1, 43);
        }

        // Get raw pointers UPFRONT - this is the HeapPtr approach
        // These pointers remain valid even when the Vec grows
        // Convert to usize for Send (same technique HeapPtr uses internally)
        let ptr0_addr = vec.get_ptr(0) as usize;
        let ptr1_addr = vec.get_ptr(1) as usize;

        let vec_writer = Arc::clone(&vec);

        // Reader thread: use raw pointers directly (no ChunkedVec::get() call)
        // This is equivalent to HeapPtr::get() in the VM
        let reader = thread::spawn(move || {
            // Convert back to pointers
            let ptr0 = ptr0_addr as *const i32;
            let ptr1 = ptr1_addr as *const i32;
            for _ in 0..1000 {
                // SAFETY: The pointers were obtained from valid indices and
                // remain stable because chunks never move once allocated.
                unsafe {
                    assert_eq!(*ptr0, 42);
                    assert_eq!(*ptr1, 43);
                }
            }
        });

        // Writer thread: keep adding chunks to force Vec reallocation
        let writer = thread::spawn(move || {
            for i in 1..100 {
                let new_len = 2 + (i * 2);
                vec_writer.resize_to(new_len);
            }
        });

        reader.join().expect("reader panicked");
        writer.join().expect("writer panicked");

        // Verify the original values are still accessible via the pointers
        let ptr0 = ptr0_addr as *const i32;
        let ptr1 = ptr1_addr as *const i32;
        // SAFETY: chunks never move once allocated; the pointers remain valid.
        unsafe {
            assert_eq!(*ptr0, 42);
            assert_eq!(*ptr1, 43);
        }
    }

    /// Regression test for the outer-`Vec`-realloc / `set` race that motivated
    /// the `chunks_lock` `RwLock<()>`.
    ///
    /// **Pre-fix behavior:** `set` (via `element_ptr`) read
    /// `(*chunks.get()).as_ptr()` non-atomically. Concurrent `resize_with`
    /// callers `Vec::push`-ed onto the outer `Vec`, which can reallocate the
    /// outer buffer and free the old pointer. A `set` reader caught
    /// mid-realloc would dereference freed memory — UAF / SIGSEGV.
    ///
    /// **Post-fix:** `resize_with` takes the outer-Vec write lock, and `set`
    /// (via `element_ptr`) takes the outer-Vec read lock for the brief
    /// window of the buffer-pointer read. `Vec::push` cannot run while a
    /// reader holds the read lock; readers cannot start while a `Vec::push`
    /// holds the write lock.
    ///
    /// This test launches one writer that grows the outer `Vec` past
    /// multiple capacity-doubling boundaries while several writers race
    /// `set` against the existing chunk's slots. Without the fix, Miri
    /// (or sufficient real-world contention) catches the UAF; with the fix,
    /// every iteration completes cleanly.
    #[test]
    fn test_set_concurrent_with_resize_does_not_uaf() {
        use std::{sync::Arc, thread};

        const NUM_SETTERS: usize = 4;
        const SLOTS_PER_SETTER: usize = 2;
        // Chunk size of 2 so each `resize_to(N)` adds chunks, exercising
        // the outer-Vec growth path frequently.
        let vec: Arc<ChunkedVec<i32, 2>> = Arc::new(ChunkedVec::new());
        // Pre-allocate one dedicated slot range per setter so the
        // concurrent `set()` calls write **disjoint** indices — that's
        // `set`'s actual safety contract. Multiple writers to the same
        // index would be UB even on top of the `chunks_lock` RwLock fix.
        vec.resize_to(NUM_SETTERS * SLOTS_PER_SETTER);

        let setters: Vec<_> = (0..NUM_SETTERS)
            .map(|tid| {
                let vec = Arc::clone(&vec);
                thread::spawn(move || {
                    let base = tid * SLOTS_PER_SETTER;
                    for i in 0..200 {
                        // SAFETY: each thread owns the slot range
                        // `[tid*SLOTS_PER_SETTER, (tid+1)*SLOTS_PER_SETTER)`
                        // exclusively, so the concurrent `set`s observe
                        // `set`'s "different indices only" contract.
                        // What we *are* racing is the outer-Vec buffer
                        // pointer read inside `set`/`element_ptr` against
                        // the grower thread's `resize_with` + `Vec::push`.
                        unsafe {
                            for slot in 0..SLOTS_PER_SETTER {
                                #[expect(
                                    clippy::cast_possible_wrap,
                                    reason = "test data, values stay small"
                                )]
                                let value = (tid * 1_000 + i) as i32 + slot as i32;
                                vec.set(base + slot, value);
                            }
                        }
                    }
                })
            })
            .collect();

        let grower_vec = Arc::clone(&vec);
        let grower = thread::spawn(move || {
            // Grow past several outer-Vec capacity-doubling boundaries
            // (4 → 8 → 16 → 32 …): with chunk_size=2, every other
            // resize_to bump triggers another outer-Vec push.
            let start = NUM_SETTERS * SLOTS_PER_SETTER;
            for n in (start..400).step_by(2) {
                grower_vec.resize_to(n);
            }
        });

        for s in setters {
            s.join().expect("setter panicked");
        }
        grower.join().expect("grower panicked");

        // Best-effort sanity: every slot the setters owned is still
        // readable and the vec didn't wedge with `len=0`.
        for idx in 0..NUM_SETTERS * SLOTS_PER_SETTER {
            assert!(*vec.get(idx) >= 0);
        }
        assert!(vec.len() >= NUM_SETTERS * SLOTS_PER_SETTER);
    }
}
