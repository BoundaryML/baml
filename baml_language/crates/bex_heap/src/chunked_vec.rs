//! Chunked vector for stable-address storage.
//!
//! Note: This module allows `dangerous_implicit_autorefs` because we carefully
//! manage aliasing through atomics and an append-only chunk directory, and
//! the tree borrows model validates our access patterns.
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
//!   of `set()` / `get_ptr()` / `get()` / `contains_ptr()` callers.
//! - Multiple threads may call `resize_with()` / `push_with()` concurrently;
//!   they serialize on an internal mutex that readers never take.
//!
//! This is achieved by:
//! - Using `AtomicUsize` for the length and the chunk count, published with
//!   Release after the storage they cover, so readers (Acquire) see
//!   monotonically increasing valid ranges.
//! - An append-only chunk directory: a fixed array of slots, where slot `s`
//!   holds `2^s` atomic chunk pointers. Growth allocates a slot's array once
//!   and stores each chunk pointer once; neither ever moves. A reader
//!   resolves a chunk with two Acquire loads and no lock, so growth on one
//!   thread cannot pull storage out from under a reader on another.
//! - Freeing only through `&mut self` (`clear`, `Drop`).
//! - Using `UnsafeCell` for each element so per-slot writes are gated by
//!   the caller's own exclusivity (typically per-TLAB region).
//!
//! Inner-chunk access is never synchronized: each chunk is its own
//! heap allocation whose address is stable for the lifetime of the
//! `ChunkedVec` (until `clear`). Once a slot's address is resolved, that
//! pointer remains valid however the vector grows.
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
    ptr,
    sync::atomic::{AtomicPtr, AtomicUsize, Ordering},
};

use parking_lot::Mutex;

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

/// Number of slots in the chunk directory. Slot `s` holds `2^s` chunk
/// pointers, so the directory addresses `2^DIR_SLOTS - 1` chunks without
/// ever moving an entry it has published.
const DIR_SLOTS: usize = usize::BITS as usize;

/// Where chunk `chunk_idx` lives in the directory: `(slot, position)`.
/// Slot `s` covers chunks `2^s - 1 ..= 2^(s+1) - 2`.
#[inline]
fn dir_location(chunk_idx: usize) -> (usize, usize) {
    let n = chunk_idx + 1;
    let slot = n.ilog2() as usize;
    (slot, n - (1 << slot))
}

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
    /// The chunk directory. Slot `s` points to an array of `2^s` chunk
    /// pointers (null until the first chunk in that slot is added), and each
    /// chunk pointer points to `CHUNK_SIZE` cells.
    ///
    /// Append-only while shared: growth stores a slot array or chunk pointer
    /// once, then publishes it through `num_chunks`. Nothing it has published
    /// moves or is freed until `clear` or `Drop`, which take `&mut self`. So a
    /// reader resolves a slot with plain atomic loads and no lock, even while
    /// another thread grows the vector.
    dir: [AtomicPtr<AtomicPtr<UnsafeCell<T>>>; DIR_SLOTS],

    /// Number of published chunks. Written only while `grow_lock` is held,
    /// and by `clear` through `&mut self`.
    num_chunks: AtomicUsize,

    /// Serializes growth (`resize_with` / `push_with`). Readers never take it.
    grow_lock: Mutex<()>,

    /// Number of elements in the vec (not capacity). Written only while
    /// `grow_lock` is held, and by `clear` through `&mut self`.
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
            dir: [const { AtomicPtr::new(ptr::null_mut()) }; DIR_SLOTS],
            num_chunks: AtomicUsize::new(0),
            grow_lock: Mutex::new(()),
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
        self.num_chunks() * CHUNK_SIZE
    }

    /// Get the number of allocated chunks.
    ///
    /// Safe to call concurrently with growth: the count is published with
    /// Release after the chunks it covers, so every chunk below it can be
    /// resolved.
    #[inline]
    pub fn num_chunks(&self) -> usize {
        self.num_chunks.load(Ordering::Acquire)
    }

    /// Get a raw pointer to the start of a chunk.
    ///
    /// # Safety
    ///
    /// - `chunk_idx` must be `< num_chunks()`.
    ///
    /// The returned pointer remains valid as long as the chunk exists: a
    /// chunk is its own allocation and never moves.
    #[inline]
    pub unsafe fn chunk_start_ptr(&self, chunk_idx: usize) -> *const T {
        // SAFETY: the caller observed `chunk_idx < num_chunks()`.
        // `UnsafeCell<T>` has the layout of `T`.
        unsafe { self.chunk_ptr(chunk_idx).cast::<T>().cast_const() }
    }

    /// Check backing-storage membership, including unused capacity, without
    /// dereferencing `ptr`.
    ///
    /// Lock-free: walks the directory a slot at a time, so it is safe to call
    /// concurrently with growth.
    pub fn contains_ptr(&self, ptr: *const T) -> bool {
        let mut remaining = self.num_chunks();
        let mut slot = 0;
        while remaining > 0 {
            // ORDERING: the Acquire load of `num_chunks` above saw every
            // chunk counted in `remaining`, so their slot arrays and chunk
            // pointers are published; Acquire here pairs with the Release
            // store of this slot array in `grow_chunks`.
            let entries = self.dir[slot].load(Ordering::Acquire);
            let here = remaining.min(1 << slot);
            for position in 0..here {
                // SAFETY: `position < 2^slot`, the slot array's length, and
                // the entry holds a published chunk. Acquire pairs with the
                // Release store of that chunk pointer. A chunk has
                // `CHUNK_SIZE` cells, so `start + CHUNK_SIZE` is its
                // one-past-the-end address; we only compare addresses.
                let start = unsafe { (*entries.add(position)).load(Ordering::Acquire) }
                    .cast::<T>()
                    .cast_const();
                let end = unsafe { start.add(CHUNK_SIZE) };
                if ptr >= start && ptr < end {
                    return true;
                }
            }
            remaining -= here;
            slot += 1;
        }
        false
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

    /// The first cell of chunk `chunk_idx`.
    ///
    /// # Safety
    ///
    /// Chunk `chunk_idx` must be published: either this thread added it, or
    /// it observed a chunk count above `chunk_idx` (or a `len` covering an
    /// index in that chunk) with an Acquire load.
    #[inline]
    unsafe fn chunk_ptr(&self, chunk_idx: usize) -> *mut UnsafeCell<T> {
        let (slot, position) = dir_location(chunk_idx);
        // ORDERING: Acquire pairs with the Release store that published this
        // slot array in `grow_chunks`, so its entries are initialized.
        let entries = self.dir[slot].load(Ordering::Acquire);
        // SAFETY: the chunk is published, so its slot array is non-null and
        // `position < 2^slot`, the array's length. Nothing frees the array
        // while `&self` is held. Acquire pairs with the Release store of the
        // chunk pointer, so the chunk's initialized cells are visible.
        unsafe { (*entries.add(position)).load(Ordering::Acquire) }
    }

    /// Get a raw pointer to the element at (chunk_idx, offset).
    ///
    /// Lock-free: the directory never moves a published chunk pointer, and
    /// the chunk itself is a stable allocation, so the pointer stays valid
    /// after growth.
    ///
    /// # Safety
    ///
    /// - chunk `chunk_idx` must be published (see [`Self::chunk_ptr`])
    /// - offset must be < CHUNK_SIZE
    #[inline]
    unsafe fn element_ptr(&self, chunk_idx: usize, offset: usize) -> *mut T {
        // SAFETY: the caller upholds both preconditions; `raw_get` does not
        // create a reference to the cell.
        unsafe { UnsafeCell::raw_get(self.chunk_ptr(chunk_idx).add(offset)) }
    }

    /// Publish chunks until `needed` exist, filling their cells from
    /// `factory`. The caller holds `grow_lock`.
    fn grow_chunks(&self, needed: usize, factory: &mut impl FnMut() -> T) {
        // Relaxed: `num_chunks` only changes under `grow_lock`, which we hold.
        let mut count = self.num_chunks.load(Ordering::Relaxed);
        while count < needed {
            let (slot, position) = dir_location(count);
            let mut entries = self.dir[slot].load(Ordering::Relaxed);
            if entries.is_null() {
                let array: Box<[AtomicPtr<UnsafeCell<T>>]> = (0..1usize << slot)
                    .map(|_| AtomicPtr::new(ptr::null_mut()))
                    .collect();
                entries = Box::into_raw(array).cast::<AtomicPtr<UnsafeCell<T>>>();
                // ORDERING: Release publishes the initialized (null) entries
                // to readers that load this slot with Acquire.
                self.dir[slot].store(entries, Ordering::Release);
            }
            let chunk: Box<[UnsafeCell<T>]> = (0..CHUNK_SIZE)
                .map(|_| UnsafeCell::new(factory()))
                .collect();
            // SAFETY: `position < 2^slot`, the slot array's length.
            // ORDERING: Release publishes the chunk's initialized cells to
            // readers that load this entry with Acquire.
            unsafe {
                (*entries.add(position)).store(
                    Box::into_raw(chunk).cast::<UnsafeCell<T>>(),
                    Ordering::Release,
                );
            }
            count += 1;
            // ORDERING: Release, so a reader that loads the new count with
            // Acquire also sees the slot array and chunk pointer stored above.
            self.num_chunks.store(count, Ordering::Release);
        }
    }

    /// Clear all elements, dropping them.
    ///
    /// Takes `&mut self`, so no other thread is accessing the ChunkedVec.
    /// In practice, this is only called during GC when all VMs are at
    /// safepoints. Slot arrays are kept for the next growth.
    pub fn clear(&mut self) {
        let count = *self.num_chunks.get_mut();
        for chunk_idx in 0..count {
            let (slot, position) = dir_location(chunk_idx);
            let entries = *self.dir[slot].get_mut();
            // SAFETY: `&mut self` excludes every reader and grower. The
            // entry holds a chunk that `grow_chunks` boxed as `CHUNK_SIZE`
            // cells; nulling it before the free leaves no dangling entry.
            unsafe {
                let chunk = (*entries.add(position)).swap(ptr::null_mut(), Ordering::Relaxed);
                drop(Box::from_raw(ptr::slice_from_raw_parts_mut(
                    chunk, CHUNK_SIZE,
                )));
            }
        }
        *self.num_chunks.get_mut() = 0;
        *self.len.get_mut() = 0;
    }
}

impl<T, const CHUNK_SIZE: usize> Drop for ChunkedVec<T, CHUNK_SIZE> {
    fn drop(&mut self) {
        self.clear();
        for (slot, entry) in self.dir.iter_mut().enumerate() {
            let entries = *entry.get_mut();
            if !entries.is_null() {
                // SAFETY: `grow_chunks` boxed this slot array as `2^slot`
                // entries, and `&mut self` excludes every other access.
                drop(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(entries, 1 << slot)) });
            }
        }
    }
}

impl<T, const CHUNK_SIZE: usize> ChunkedVec<T, CHUNK_SIZE> {
    /// Ensure capacity for at least `min_len` elements, using a factory function.
    ///
    /// If the current capacity is insufficient, new chunks are allocated.
    /// Existing chunks are never moved (each is its own allocation).
    ///
    /// This also sets len to min_len, filling new slots with values from the factory.
    /// `len` never shrinks: a smaller `min_len` is a no-op.
    ///
    /// Takes `grow_lock`, so concurrent growers serialize; readers do not
    /// wait for it.
    pub fn resize_with<F2: FnMut() -> T>(&self, min_len: usize, mut factory: F2) {
        if min_len <= self.len.load(Ordering::Acquire) {
            return;
        }
        let _grow = self.grow_lock.lock();
        // Relaxed: `len` only changes under `grow_lock`, which we hold.
        if min_len <= self.len.load(Ordering::Relaxed) {
            return;
        }
        self.grow_chunks(min_len.div_ceil(CHUNK_SIZE), &mut factory);
        // ORDERING: Release, so a reader that sees the new len (Acquire)
        // sees the chunks published above.
        self.len.store(min_len, Ordering::Release);
    }

    /// Push an element, allocating a new chunk if needed.
    ///
    /// Returns the index of the pushed element.
    ///
    /// Takes `grow_lock`, so concurrent pushers and growers serialize.
    pub fn push_with<F2: FnMut() -> T>(&self, value: T, mut factory: F2) -> usize {
        let _grow = self.grow_lock.lock();
        // Relaxed: `len` only changes under `grow_lock`, which we hold.
        let index = self.len.load(Ordering::Relaxed);
        let (chunk_idx, offset) = self.chunk_location(index);
        self.grow_chunks(chunk_idx + 1, &mut factory);
        // SAFETY: this thread published chunk `chunk_idx` (now or earlier
        // under `grow_lock`), and `offset < CHUNK_SIZE`. `index` is not yet
        // below `len`, so no reader can reach the slot, and no other pusher
        // can claim it while we hold `grow_lock`.
        unsafe {
            let elem_ptr = self.element_ptr(chunk_idx, offset);
            // Drop the factory-created placeholder before writing the actual value
            ptr::drop_in_place(elem_ptr);
            ptr::write(elem_ptr, value);
        }
        // ORDERING: Release, so a reader that sees the new len (Acquire)
        // sees the element written above.
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
        // SAFETY: Index bounds checked above (Acquire len), so the chunk is published.
        unsafe { self.element_ptr(chunk_idx, offset) }
    }

    /// Set an element at the given index, dropping the previous value, and
    /// return a pointer to its storage (the pointer [`Self::get_ptr`] would
    /// return, without resolving the slot twice).
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
    pub unsafe fn set(&self, index: usize, value: T) -> *mut T {
        let current_len = self.len.load(Ordering::Acquire);
        assert!(
            index < current_len,
            "index {index} out of bounds (len={current_len})"
        );
        let (chunk_idx, offset) = self.chunk_location(index);
        // SAFETY: Caller ensures exclusive access to this index; the bounds
        // check above (Acquire len) shows the chunk is published.
        unsafe {
            let elem_ptr = self.element_ptr(chunk_idx, offset);
            // Drop the old value before writing the new one to prevent leaks
            ptr::drop_in_place(elem_ptr);
            ptr::write(elem_ptr, value);
            elem_ptr
        }
    }

    /// Iterate over all elements.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        let len = self.len.load(Ordering::Acquire);
        (0..len).map(move |i| self.get(i))
    }

    /// Iterate over all elements mutably.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        let len = *self.len.get_mut();
        let this: &Self = self;
        (0..len).map(move |i| {
            let (chunk_idx, offset) = this.chunk_location(i);
            // SAFETY: We have &mut self, `i < len` so the chunk is
            // published, and each index is yielded once.
            unsafe { &mut *this.element_ptr(chunk_idx, offset) }
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
    fn membership_covers_chunk_boundaries_and_survives_growth() {
        let mut values = ChunkedVec::<u64, 4>::new();
        assert!(!values.contains_ptr(std::ptr::null()));
        values.resize_with(5, || 0);
        let first = values.get_ptr(0);
        for index in [0, 3, 4] {
            assert!(values.contains_ptr(values.get_ptr(index)));
        }
        // Membership intentionally includes the unoccupied tail of a chunk.
        let second_chunk = unsafe { values.chunk_start_ptr(1) };
        assert!(values.contains_ptr(unsafe { second_chunk.add(3) }));
        // Use an isolated one-chunk allocation for its exclusive upper bound.
        let boundary = ChunkedVec::<u64, 4>::new();
        boundary.resize_with(1, || 0);
        let end = unsafe { boundary.chunk_start_ptr(0).add(4) };
        assert!(!boundary.contains_ptr(end));
        let other = Box::new(0_u64);
        assert!(!values.contains_ptr(&*other));
        values.resize_with(100, || 0);
        assert!(values.contains_ptr(first));
        assert!(values.contains_ptr(values.get_ptr(99)));
        values.clear();
        // Membership compares addresses only, including a now stale pointer.
        assert!(!values.contains_ptr(first));
    }

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

    /// Regression test for the outer-`Vec`-realloc / `set` race.
    ///
    /// **Original bug:** `set` (via `element_ptr`) read the buffer pointer of
    /// a reallocating outer `Vec` of chunks non-atomically. Concurrent
    /// `resize_with` callers pushed onto that `Vec`, which can reallocate the
    /// buffer and free the old one. A `set` caught mid-realloc dereferenced
    /// freed memory — UAF / SIGSEGV.
    ///
    /// **Now:** the chunk directory never moves an entry it has published,
    /// and readers load it with Acquire, so `set` needs no lock.
    ///
    /// This test launches one writer that grows the vector across several
    /// directory slots while several writers race `set` against the existing
    /// chunk's slots. Miri reports any data race or use-after-free.
    #[test]
    fn test_set_concurrent_with_resize_does_not_uaf() {
        use std::{sync::Arc, thread};

        const NUM_SETTERS: usize = 4;
        const SLOTS_PER_SETTER: usize = 2;
        // Chunk size of 2 so each `resize_to(N)` adds chunks, exercising
        // the growth path frequently.
        let vec: Arc<ChunkedVec<i32, 2>> = Arc::new(ChunkedVec::new());
        // Pre-allocate one dedicated slot range per setter so the
        // concurrent `set()` calls write **disjoint** indices — that's
        // `set`'s actual safety contract. Multiple writers to the same
        // index would be UB however the directory is synchronized.
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
                        // What we *are* racing is the directory read
                        // inside `set`/`element_ptr` against the grower
                        // thread's `resize_with` publishing new chunks.
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
            // Grow across several directory slots (slot `s` holds `2^s`
            // chunks): with chunk_size=2, every other resize_to bump
            // publishes another chunk.
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
    #[test]
    fn dir_location_maps_chunks_to_doubling_slots() {
        assert_eq!(dir_location(0), (0, 0));
        assert_eq!(dir_location(1), (1, 0));
        assert_eq!(dir_location(2), (1, 1));
        assert_eq!(dir_location(3), (2, 0));
        assert_eq!(dir_location(6), (2, 3));
        assert_eq!(dir_location(7), (3, 0));
        let mut seen = std::collections::HashSet::new();
        for chunk_idx in 0..1_000 {
            let (slot, position) = dir_location(chunk_idx);
            assert!(
                position < 1 << slot,
                "chunk {chunk_idx} overflows slot {slot}"
            );
            assert!(
                seen.insert((slot, position)),
                "chunk {chunk_idx} shares an entry"
            );
        }
    }

    /// `clear` frees every chunk and `Drop` every slot array; growth after a
    /// clear reuses the slot arrays. Miri reports a leak or a double free.
    #[test]
    fn clear_frees_chunks_and_regrowth_reuses_slots() {
        let mut vec: ChunkedVec<String, 2> = ChunkedVec::new();
        // 9 elements in 2-element chunks: 5 chunks, directory slots 0..=2.
        vec.resize_with(9, || "placeholder".to_string());
        // SAFETY: single-threaded; no other reference to the slot.
        unsafe {
            vec.set(8, "last".to_string());
        }
        assert_eq!(vec.num_chunks(), 5);
        vec.clear();
        assert_eq!(vec.len(), 0);
        assert_eq!(vec.num_chunks(), 0);
        assert!(!vec.contains_ptr(std::ptr::null()));
        vec.resize_with(3, String::new);
        assert_eq!(vec.num_chunks(), 2);
        assert_eq!(vec.get(2), "");
        assert_eq!(vec.push("pushed".to_string()), 3);
        assert_eq!(vec.get(3), "pushed");
    }

    /// Readers resolve published slots while two growers publish chunks
    /// across several directory slots (2-element chunks, 65 chunks: slots
    /// 0..=6). No reader takes a lock, so Miri checks the orderings: a
    /// reader that sees a length or a chunk count must also see the chunk
    /// pointers and the initialized cells it covers.
    #[test]
    fn readers_and_growers_race_across_directory_slots() {
        use std::{sync::Arc, thread};

        const TARGET_LEN: usize = 128;
        let vec: Arc<ChunkedVec<usize, 2>> = Arc::new(ChunkedVec::new());
        let growers: Vec<_> = (0..2)
            .map(|grower| {
                let vec = Arc::clone(&vec);
                thread::spawn(move || {
                    for len in (2..=TARGET_LEN).step_by(2) {
                        vec.resize_with(len + grower, || 7);
                    }
                })
            })
            .collect();
        let readers: Vec<_> = (0..2)
            .map(|_| {
                let vec = Arc::clone(&vec);
                thread::spawn(move || {
                    let mut seen = 0;
                    while seen < TARGET_LEN {
                        let len = vec.len();
                        // Nothing writes these slots after the factory, so
                        // reading them races only with growth.
                        for index in seen..len {
                            assert_eq!(*vec.get(index), 7);
                            assert!(vec.contains_ptr(vec.get_ptr(index)));
                        }
                        for chunk_idx in 0..vec.num_chunks() {
                            // SAFETY: `chunk_idx < num_chunks()`.
                            let start = unsafe { vec.chunk_start_ptr(chunk_idx) };
                            assert!(vec.contains_ptr(start));
                        }
                        seen = seen.max(len);
                        thread::yield_now();
                    }
                })
            })
            .collect();
        for handle in growers.into_iter().chain(readers) {
            handle.join().expect("a racing thread panicked");
        }
        assert_eq!(vec.len(), TARGET_LEN + 1);
        assert_eq!(vec.num_chunks(), (TARGET_LEN + 1).div_ceil(2));
    }
}
