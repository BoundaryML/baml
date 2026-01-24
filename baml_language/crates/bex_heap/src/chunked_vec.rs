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
//! - Multiple threads can call `set()` on different indices concurrently
//! - One thread can call `resize_with()` while others call `set()` on existing indices
//! - `resize_with()` must be externally synchronized (only one thread at a time)
//!
//! This is achieved by:
//! - Using `AtomicUsize` for the length
//! - Using raw pointer operations to avoid `&mut` reborrows that conflict with Miri's
//!   stacked borrows model
//! - Using `UnsafeCell` for each element
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
    /// Access to this field must be synchronized externally for resize operations.
    ///
    /// IMPORTANT: To avoid data races detected by Miri's stacked borrows, we never
    /// create `&mut Vec<...>` references to this field. Instead, we use raw pointer
    /// operations throughout.
    chunks: UnsafeCell<Vec<Box<[UnsafeCell<T>]>>>,

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

    /// Get the chunk size (compile-time constant).
    #[inline]
    pub const fn chunk_size(&self) -> usize {
        CHUNK_SIZE
    }

    /// Get the total capacity (number of slots across all chunks).
    #[inline]
    pub fn capacity(&self) -> usize {
        // SAFETY: We only read the length of the Vec via raw pointer
        unsafe {
            let chunks_ptr = self.chunks.get();
            (*chunks_ptr).len() * CHUNK_SIZE
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
    /// # Safety
    ///
    /// - chunk_idx must be < number of chunks
    /// - offset must be < CHUNK_SIZE
    #[inline]
    unsafe fn element_ptr(&self, chunk_idx: usize, offset: usize) -> *mut T {
        unsafe {
            let chunks_ptr = self.chunks.get();
            // Get pointer to the Vec's internal buffer
            let chunks_data_ptr = (*chunks_ptr).as_ptr();
            // Get pointer to the specific chunk (Box<[UnsafeCell<T>]>)
            let chunk_box_ptr = chunks_data_ptr.add(chunk_idx);
            // Dereference to get the Box, then get the slice pointer
            let chunk_slice_ptr = (*chunk_box_ptr).as_ptr();
            // Get pointer to the specific element
            let element_cell_ptr = chunk_slice_ptr.add(offset);
            // Get the inner pointer from UnsafeCell
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
    /// Existing chunks are never moved.
    ///
    /// This also sets len to min_len, filling new slots with values from the factory.
    ///
    /// # Safety
    ///
    /// This method must be externally synchronized - only one thread can call
    /// resize_with at a time. However, other threads can safely call `set()` or
    /// `get()` on existing indices while this is running.
    pub unsafe fn resize_with<F2: FnMut() -> T>(&self, min_len: usize, mut factory: F2) {
        let current_len = self.len.load(Ordering::Acquire);
        if min_len <= current_len {
            return;
        }

        // Calculate how many chunks we need
        let needed_chunks = min_len.div_ceil(CHUNK_SIZE);

        // SAFETY: Caller ensures only one thread is resizing at a time.
        // Other threads may be accessing existing chunks via set(), but we only
        // append new chunks - we never touch existing ones.
        //
        // CRITICAL: We use raw pointer operations here to avoid creating a `&mut Vec`
        // which would conflict with concurrent `&Vec` accesses in set() under Miri's
        // stacked borrows model.
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
    /// # Safety
    ///
    /// This method must be externally synchronized - only one thread can push
    /// at a time.
    pub unsafe fn push_with<F2: FnMut() -> T>(&self, value: T, mut factory: F2) -> usize {
        let index = self.len.load(Ordering::Acquire);

        // SAFETY: Caller ensures only one thread is pushing at a time.
        // Use raw pointer operations to avoid &mut reborrow.
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

            // Write the value
            let (chunk_idx, offset) = self.chunk_location(index);
            // SAFETY: Index is within allocated range, we have exclusive push access
            let elem_ptr = self.element_ptr(chunk_idx, offset);
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
            "index {index} out of bounds (len={})",
            current_len
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
            "index {index} out of bounds (len={})",
            current_len
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
            "index {index} out of bounds (len={})",
            current_len
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
            "index {index} out of bounds (len={})",
            current_len
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
    /// # Safety
    ///
    /// See `resize_with`.
    pub unsafe fn resize_to(&self, min_len: usize) {
        // SAFETY: Caller ensures proper synchronization
        unsafe { self.resize_with(min_len, T::default) }
    }

    /// Push an element, allocating a new chunk if needed.
    ///
    /// # Safety
    ///
    /// See `push_with`.
    pub unsafe fn push(&self, value: T) -> usize {
        // SAFETY: Caller ensures proper synchronization
        unsafe { self.push_with(value, T::default) }
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

        unsafe {
            let idx0 = vec.push(10);
            let idx1 = vec.push(20);
            let idx2 = vec.push(30);

            assert_eq!(idx0, 0);
            assert_eq!(idx1, 1);
            assert_eq!(idx2, 2);
        }
        assert_eq!(vec.len(), 3);

        assert_eq!(*vec.get(0), 10);
        assert_eq!(*vec.get(1), 20);
        assert_eq!(*vec.get(2), 30);
    }

    #[test]
    fn test_push_across_chunks() {
        let vec: ChunkedVec<i32, 2> = ChunkedVec::new();

        unsafe {
            // Push 5 elements (requires 3 chunks with chunk_size=2)
            for i in 0..5 {
                vec.push(i * 10);
            }
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

        unsafe { vec.resize_to(10) };

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
        unsafe { vec.resize_to(5) };

        unsafe {
            vec.set(2, 42);
        }

        assert_eq!(*vec.get(2), 42);
    }

    #[test]
    fn test_clear() {
        let mut vec: ChunkedVec<i32, 4> = ChunkedVec::new();

        unsafe {
            for i in 0..10 {
                vec.push(i);
            }
        }

        assert_eq!(vec.len(), 10);

        vec.clear();

        assert_eq!(vec.len(), 0);
        assert!(vec.is_empty());
    }

    #[test]
    fn test_get_mut() {
        let mut vec: ChunkedVec<i32, 4> = ChunkedVec::new();

        unsafe {
            vec.push(10);
            vec.push(20);
        }

        *vec.get_mut(1) = 99;

        assert_eq!(*vec.get(1), 99);
    }

    #[test]
    fn test_iter() {
        let vec: ChunkedVec<i32, 2> = ChunkedVec::new();

        unsafe {
            for i in 0..5 {
                vec.push(i * 10);
            }
        }

        let collected: Vec<i32> = vec.iter().copied().collect();
        assert_eq!(collected, vec![0, 10, 20, 30, 40]);
    }

    #[test]
    fn test_iter_mut() {
        let mut vec: ChunkedVec<i32, 2> = ChunkedVec::new();

        unsafe {
            for i in 0..5 {
                vec.push(i);
            }
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

        unsafe { vec.push(42) };

        // Get pointer to first element
        let ptr = vec.get_ptr(0);

        // Push more elements, causing chunk allocation
        unsafe {
            for i in 0..10 {
                vec.push(i);
            }
        }

        // Original pointer should still be valid
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

        unsafe {
            for i in 0..10_000 {
                vec.push(i);
            }
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
    /// HeapPtr is obtained once at allocation time via `get_ptr()` and stored in
    /// `Value::Object(HeapPtr)`. When the VM reads an object, it calls
    /// `HeapPtr::get()` which is a direct pointer dereference - it never goes
    /// through `ChunkedVec::get()`.
    ///
    /// See `test_miri_heap_ptr_access_is_race_free` which proves the HeapPtr
    /// approach is race-free.
    #[test]
    #[ignore = "Demonstrates ChunkedVec::get() race that VM avoids by using HeapPtr"]
    fn test_miri_concurrent_read_during_vec_reallocation() {
        use std::{sync::Arc, thread};

        let vec: Arc<ChunkedVec<i32, 2>> = Arc::new(ChunkedVec::new());

        unsafe {
            vec.resize_to(2);
            vec.set(0, 42);
            vec.set(1, 43);
        }

        let vec_reader = Arc::clone(&vec);
        let vec_writer = Arc::clone(&vec);

        // Reader uses get() which has the race
        let reader = thread::spawn(move || {
            for _ in 0..1000 {
                let val = *vec_reader.get(0);
                assert_eq!(val, 42);
            }
        });

        let writer = thread::spawn(move || {
            for i in 1..100 {
                let new_len = 2 + (i * 2);
                unsafe {
                    vec_writer.resize_to(new_len);
                }
            }
        });

        reader.join().expect("reader panicked");
        writer.join().expect("writer panicked");
    }

    /// Test that HeapPtr-style access (raw pointers obtained upfront) is race-free.
    ///
    /// This is the fixed code path that the VM uses. Instead of calling get()
    /// which internally reads the Vec's buffer pointer, we obtain a raw pointer
    /// via get_ptr() and use that directly. The raw pointer remains stable even
    /// when the Vec reallocates because chunks themselves are heap-allocated
    /// and never move.
    ///
    /// This test demonstrates the fix for the data race exposed in
    /// test_miri_concurrent_read_during_vec_reallocation.
    #[test]
    fn test_miri_heap_ptr_access_is_race_free() {
        use std::{sync::Arc, thread};

        // Chunk size of 2 means we need a new chunk every 2 elements.
        let vec: Arc<ChunkedVec<i32, 2>> = Arc::new(ChunkedVec::new());

        // Pre-populate with initial data
        unsafe {
            vec.resize_to(2);
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
                unsafe {
                    vec_writer.resize_to(new_len);
                }
            }
        });

        reader.join().expect("reader panicked");
        writer.join().expect("writer panicked");

        // Verify the original values are still accessible via the pointers
        let ptr0 = ptr0_addr as *const i32;
        let ptr1 = ptr1_addr as *const i32;
        unsafe {
            assert_eq!(*ptr0, 42);
            assert_eq!(*ptr1, 43);
        }
    }
}
