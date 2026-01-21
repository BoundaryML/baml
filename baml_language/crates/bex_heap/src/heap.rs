//! Unified heap for BEX virtual machine.
//!
//! The heap stores all objects in a single `Vec<Object>` with:
//! - Compile-time objects at indices 0..compile_time_boundary (permanent)
//! - Runtime objects at indices compile_time_boundary.. (collectible)
//!
//! # Thread Safety
//!
//! The heap uses `UnsafeCell<Vec<Object>>` for lock-free field writes.
//! Safety is ensured by:
//! - TLABs give each VM exclusive write access to its allocation region
//! - BAML has no global mutable variables, so independent calls can't race
//! - GC only runs when all VMs are at safepoints (yielded)

use std::{
    cell::UnsafeCell,
    collections::HashMap,
    marker::PhantomData,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use bex_external_types::{Handle, WeakHeapRef};
use bex_vm_types::{Object, ObjectIndex};

use crate::tlab::TlabChunk;

/// Default TLAB chunk size (number of object slots).
pub const DEFAULT_TLAB_SIZE: usize = 1024;

/// Statistics about heap usage.
#[derive(Clone, Copy, Debug, Default)]
pub struct HeapStats {
    /// Total objects allocated (including compile-time).
    pub total_objects: usize,
    /// Compile-time objects (permanent).
    pub compile_time_objects: usize,
    /// Runtime objects (collectible).
    pub runtime_objects: usize,
    /// Number of active handles.
    pub active_handles: usize,
    /// Number of TLAB chunks allocated.
    pub tlab_chunks: usize,
}

/// Unified heap for the BEX virtual machine.
///
/// All heap-allocated objects live here. The heap is shared across
/// all VM instances via `Arc<BexHeap<F>>`.
///
/// The type parameter `F` represents the native function type stored
/// in `Object::Function` variants. This allows the heap to be generic
/// over the native function implementation without creating circular
/// dependencies between crates.
///
/// # Semi-Space Layout
///
/// The heap uses a semi-space structure for copy collection:
/// - `compile_time`: Permanent objects (functions, classes, enums) - never collected
/// - `spaces[0]` and `spaces[1]`: Two runtime spaces - only one active at a time
///
/// During GC, live objects are copied from the active space to the other,
/// then the spaces are swapped. This reclaims memory from dead objects.
///
/// # Example
///
/// ```ignore
/// // In bex_engine, instantiate with concrete NativeFunction type:
/// let heap: Arc<BexHeap<NativeFunction>> = BexHeap::new(compile_time_objects);
///
/// // In tests or when native functions aren't needed:
/// let heap: Arc<BexHeap<()>> = BexHeap::new(vec![]);
/// ```
pub struct BexHeap<F> {
    /// Compile-time objects (never collected).
    /// These are permanent: functions, classes, enums, string literals.
    compile_time: Vec<Object<F>>,

    /// Two runtime spaces - only one active at a time.
    /// Objects stored in UnsafeCell for lock-free field writes.
    ///
    /// # Safety
    ///
    /// Access is safe because:
    /// - Each VM has exclusive write access to its TLAB region
    /// - BAML has no global mutable state
    /// - GC only runs at safepoints when no VMs are executing
    pub(crate) spaces: [UnsafeCell<Vec<Object<F>>>; 2],

    /// Which space is currently active (0 or 1).
    pub(crate) active_space: AtomicUsize,

    /// Next TLAB chunk start index within active space.
    ///
    /// When a VM needs a new TLAB, it atomically increments this by the
    /// chunk size to reserve its region.
    next_chunk: AtomicUsize,

    /// Handle table for external/FFI boundary.
    ///
    /// Maps handle keys to ObjectIndex values. Handles provide safe,
    /// validated access to heap objects from external code.
    ///
    /// Uses `RwLock<HashMap>` instead of sharded_slab to allow in-place
    /// updates after GC moves objects.
    pub(crate) handles: RwLock<HashMap<usize, ObjectIndex>>,

    /// Next handle key to allocate.
    next_handle_key: AtomicUsize,

    /// TLAB chunk size for new allocations.
    tlab_size: usize,

    /// Lock for growing the objects vector (rare operation).
    ///
    /// Only held during Vec resizing when a TLAB chunk allocation needs to grow
    /// the backing storage. This doesn't affect fast-path allocation which is
    /// lock-free within a TLAB.
    growth_lock: Mutex<()>,

    /// Allocations since last GC (for triggering heuristic).
    allocs_since_gc: AtomicUsize,

    /// Phantom data to hold the type parameter.
    _marker: PhantomData<F>,
}

// SAFETY: BexHeap<F> is Send + Sync when F is Send + Sync because:
// - objects: UnsafeCell is accessed safely via TLAB exclusivity and growth_lock
// - compile_time_boundary: immutable after construction
// - next_chunk: AtomicUsize is thread-safe
// - handles: sharded_slab::Slab is thread-safe
// - handle_keys: RwLock<HashSet> is thread-safe
// - tlab_size: immutable after construction
// - growth_lock: Mutex is thread-safe
unsafe impl<F: Send + Sync> Send for BexHeap<F> {}
unsafe impl<F: Send + Sync> Sync for BexHeap<F> {}

// Implement WeakHeapRef trait from bex_external_types
impl<F> WeakHeapRef for BexHeap<F>
where
    F: Send + Sync,
{
    fn release_handle(&self, handle_key: usize) {
        if let Ok(mut handles) = self.handles.write() {
            handles.remove(&handle_key);
        }
    }

    fn resolve_handle(&self, slab_key: usize) -> Option<ObjectIndex> {
        self.handles
            .read()
            .ok()
            .and_then(|handles| handles.get(&slab_key).copied())
    }
}

impl<F> BexHeap<F> {
    /// Create a new heap with compile-time objects.
    ///
    /// The provided objects become permanent (never garbage collected).
    /// Runtime allocations will start after these objects.
    pub fn new(compile_time_objects: Vec<Object<F>>) -> Arc<Self> {
        Self::with_tlab_size(compile_time_objects, DEFAULT_TLAB_SIZE)
    }

    /// Create a new heap with custom TLAB size.
    pub fn with_tlab_size(compile_time_objects: Vec<Object<F>>, tlab_size: usize) -> Arc<Self> {
        Arc::new(Self {
            compile_time: compile_time_objects,
            spaces: [UnsafeCell::new(Vec::new()), UnsafeCell::new(Vec::new())],
            active_space: AtomicUsize::new(0),
            next_chunk: AtomicUsize::new(0), // Starts at 0 within active space
            handles: RwLock::new(HashMap::new()),
            next_handle_key: AtomicUsize::new(0),
            tlab_size,
            growth_lock: Mutex::new(()),
            allocs_since_gc: AtomicUsize::new(0),
            _marker: PhantomData,
        })
    }

    /// Get the number of compile-time objects.
    pub fn compile_time_len(&self) -> usize {
        self.compile_time.len()
    }

    /// Get the compile-time boundary index (alias for compile_time_len).
    ///
    /// Objects before this index are permanent. Objects at or after
    /// this index are runtime allocations that can be garbage collected.
    pub fn compile_time_boundary(&self) -> usize {
        self.compile_time.len()
    }

    /// Check if an index refers to a compile-time object.
    #[inline]
    pub fn is_compile_time(&self, idx: ObjectIndex) -> bool {
        idx.into_raw() < self.compile_time.len()
    }

    /// Get the currently active runtime space index (0 or 1).
    #[inline]
    pub fn active_space_index(&self) -> usize {
        self.active_space.load(Ordering::Acquire)
    }

    /// Get a pointer to the active runtime space.
    ///
    /// # Safety
    /// Same safety requirements as objects_ptr()
    #[inline]
    pub unsafe fn active_space_ptr(&self) -> *mut Vec<Object<F>> {
        self.spaces[self.active_space_index()].get()
    }

    /// Convert a runtime space index to a global ObjectIndex.
    #[inline]
    pub fn runtime_to_global(&self, runtime_idx: usize) -> ObjectIndex {
        ObjectIndex::from_raw(self.compile_time.len() + runtime_idx)
    }

    /// Convert a global ObjectIndex to a runtime space index.
    /// Returns None if this is a compile-time object.
    #[inline]
    pub fn global_to_runtime(&self, idx: ObjectIndex) -> Option<usize> {
        let raw = idx.into_raw();
        let ct_len = self.compile_time.len();
        if raw >= ct_len {
            Some(raw - ct_len)
        } else {
            None
        }
    }

    /// Get the TLAB chunk size.
    pub fn tlab_size(&self) -> usize {
        self.tlab_size
    }

    /// Get a raw pointer to the active runtime space.
    ///
    /// # Safety
    ///
    /// This method is `unsafe` to call because the returned pointer gives
    /// mutable access to runtime heap objects. Callers must ensure:
    ///
    /// 1. **Write exclusivity**: Only write to indices within your TLAB's
    ///    exclusive region (`tlab.alloc_ptr..tlab.alloc_limit`)
    ///
    /// 2. **Read consistency**: Only read compile-time objects (always safe)
    ///    or objects allocated by your own TLAB (no concurrent writes)
    ///
    /// 3. **No reallocation during access**: Do not hold the pointer across
    ///    operations that might grow the heap (TLAB refills)
    ///
    /// # Why UnsafeCell?
    ///
    /// Production VMs (JVM, CLR, V8, Go) all use direct memory access for
    /// field writes. The "lock-free" property comes from:
    ///
    /// - **TLABs**: Each VM has exclusive write access to its allocation region
    /// - **No globals**: BAML has no global mutable state, preventing races
    /// - **Safepoint GC**: Collection only runs when no VMs are executing
    ///
    /// Using `RwLock` or `Mutex` for field writes would make BEX unacceptably
    /// slow - every `x.field = value` would require lock acquisition.
    ///
    /// Note: This now returns the active space pointer. For compile-time objects,
    /// use `get_object()` which handles both compile-time and runtime objects.
    pub unsafe fn objects_ptr(&self) -> *mut Vec<Object<F>> {
        self.spaces[self.active_space_index()].get()
    }

    /// Get the current number of objects in the heap.
    pub fn len(&self) -> usize {
        let active = self.active_space_index();
        let runtime_len = unsafe { (*self.spaces[active].get()).len() };
        self.compile_time.len() + runtime_len
    }

    /// Check if the heap is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Allocate a new TLAB chunk.
    ///
    /// This method is thread-safe. Multiple VMs can request chunks
    /// concurrently - each gets a unique, non-overlapping region.
    ///
    /// # Thread Safety
    ///
    /// Uses `fetch_add` with `SeqCst` ordering to ensure each caller
    /// gets a unique chunk range, even under concurrent access. The
    /// growth_lock protects the rare Vec resize operation.
    ///
    /// Returns a `TlabChunk` describing the exclusive region for the VM.
    /// The VM can then allocate objects within this region without locks.
    pub fn alloc_tlab_chunk(&self) -> TlabChunk {
        // Atomically reserve a chunk range within active space
        let runtime_start = self.next_chunk.fetch_add(self.tlab_size, Ordering::SeqCst);
        let runtime_end = runtime_start + self.tlab_size;

        // Lock only for growth (rare operation)
        let _guard = self.growth_lock.lock().unwrap();

        // Grow active space if needed
        let active = self.active_space_index();
        unsafe {
            let space = &mut *self.spaces[active].get();
            if space.len() < runtime_end {
                space.resize_with(runtime_end, || {
                    // Placeholder object - will be overwritten by TLAB alloc
                    Object::String(String::new())
                });
            }
        }

        // Return global indices (compile_time_len + runtime indices)
        let ct_len = self.compile_time.len();
        TlabChunk {
            start: ct_len + runtime_start,
            end: ct_len + runtime_end,
        }
    }

    /// Read an object by index (handles compile-time vs runtime).
    ///
    /// # Safety
    ///
    /// For runtime objects, caller must ensure no concurrent writes.
    pub unsafe fn get_object(&self, idx: ObjectIndex) -> &Object<F> {
        let raw = idx.into_raw();
        let ct_len = self.compile_time.len();

        if raw < ct_len {
            // Compile-time object - always safe to read
            &self.compile_time[raw]
        } else {
            // Runtime object - index relative to active space
            let runtime_idx = raw - ct_len;
            // SAFETY: Caller ensures no concurrent writes to runtime objects
            unsafe {
                let space = &*self.spaces[self.active_space_index()].get();
                &space[runtime_idx]
            }
        }
    }

    /// Get statistics about heap usage.
    pub fn stats(&self) -> HeapStats {
        let active = self.active_space_index();
        let runtime_len = unsafe { (*self.spaces[active].get()).len() };
        let ct_len = self.compile_time.len();
        let total = ct_len + runtime_len;

        let tlab_chunks = self
            .next_chunk
            .load(Ordering::Relaxed)
            .div_ceil(self.tlab_size);

        HeapStats {
            total_objects: total,
            compile_time_objects: ct_len,
            runtime_objects: runtime_len,
            active_handles: self.handles.read().map(|h| h.len()).unwrap_or(0),
            tlab_chunks,
        }
    }

    /// Check if GC should run based on allocation pressure.
    ///
    /// Simple heuristic: trigger GC after N allocations since last collection.
    /// This can be tuned based on workload characteristics.
    pub fn should_gc(&self) -> bool {
        const GC_THRESHOLD: usize = 10_000; // Tune based on profiling
        self.allocs_since_gc.load(Ordering::Relaxed) >= GC_THRESHOLD
    }

    /// Reset the allocation counter after GC.
    pub fn reset_gc_counter(&self) {
        self.allocs_since_gc.store(0, Ordering::Relaxed);
    }

    /// Increment allocation counter (called by TLAB on alloc).
    pub(crate) fn record_alloc(&self) {
        self.allocs_since_gc.fetch_add(1, Ordering::Relaxed);
    }

    /// Reset the TLAB allocation pointer (called by GC after collection).
    pub(crate) fn reset_next_chunk(&self, new_value: usize) {
        self.next_chunk.store(new_value, Ordering::Release);
    }

    /// Update handle entries after GC with new object indices.
    ///
    /// Called by GC after copying objects to update handle table entries
    /// to point to the new locations.
    /// Update handle entries after GC.
    ///
    /// Updates handles to point to new object locations. Invalidates handles
    /// pointing to dead objects (runtime objects not found in forwarding map).
    /// Preserves handles to compile-time objects even if not traced.
    ///
    /// Returns the number of handles invalidated.
    pub fn update_handles(&self, forwarding: &HashMap<ObjectIndex, ObjectIndex>) -> usize {
        let mut invalidated_count = 0;
        if let Ok(mut handles) = self.handles.write() {
            handles.retain(|_, idx| {
                if let Some(&new_idx) = forwarding.get(idx) {
                    *idx = new_idx;
                    true
                } else if self.is_compile_time(*idx) {
                    // Compile-time objects are always valid
                    true
                } else {
                    // Object dead (not forwarded and not compile-time)
                    invalidated_count += 1;
                    false
                }
            });
        }
        invalidated_count
    }
}

impl<F> BexHeap<F>
where
    F: Send + Sync + 'static,
{
    /// Create a handle to an object.
    ///
    /// Handles are used at the FFI boundary to give external code safe
    /// access to heap objects. Handles are GC roots - objects reachable
    /// from handles will not be collected.
    pub fn create_handle(self: &Arc<Self>, idx: ObjectIndex) -> Handle {
        // Get a unique key for this handle
        let handle_key = self.next_handle_key.fetch_add(1, Ordering::Relaxed);

        // Insert into the handle table
        if let Ok(mut handles) = self.handles.write() {
            handles.insert(handle_key, idx);
        }

        // Handle no longer stores idx - always resolves through table
        Handle::new(handle_key, Arc::clone(self) as Arc<dyn WeakHeapRef>)
    }

    /// Collect all handle roots for garbage collection.
    ///
    /// Returns a Vec of ObjectIndex values for all live handles.
    /// These should be treated as GC roots - objects reachable from
    /// handles must not be collected.
    pub fn collect_handle_roots(&self) -> Vec<ObjectIndex> {
        self.handles
            .read()
            .map(|handles| handles.values().copied().collect())
            .unwrap_or_default()
    }
}

impl<F> std::fmt::Debug for BexHeap<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BexHeap")
            .field("len", &self.len())
            .field("compile_time_len", &self.compile_time.len())
            .field("active_space", &self.active_space_index())
            .field("tlab_size", &self.tlab_size)
            .finish()
    }
}

// Static assertions to verify thread safety
const _: () = {
    const fn assert_send<T: Send>() {}
    const fn assert_sync<T: Sync>() {}

    // BexHeap must be Send + Sync for Arc<BexHeap> to work across threads
    assert_send::<BexHeap<()>>();
    assert_sync::<BexHeap<()>>();
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_heap_empty() {
        let heap = BexHeap::<()>::new(vec![]);
        assert_eq!(heap.len(), 0);
        assert_eq!(heap.compile_time_boundary(), 0);
    }

    #[test]
    fn test_new_heap_with_objects() {
        let objects: Vec<Object<()>> = vec![
            Object::String("hello".to_string()),
            Object::String("world".to_string()),
        ];
        let heap = BexHeap::new(objects);
        assert_eq!(heap.len(), 2);
        assert_eq!(heap.compile_time_boundary(), 2);
    }

    #[test]
    fn test_alloc_tlab_chunk() {
        let heap = BexHeap::<()>::with_tlab_size(vec![], 100);

        // With no compile-time objects, global indices start at 0
        let chunk1 = heap.alloc_tlab_chunk();
        assert_eq!(chunk1.start, 0); // compile_time_len(0) + runtime_start(0)
        assert_eq!(chunk1.end, 100); // compile_time_len(0) + runtime_end(100)

        let chunk2 = heap.alloc_tlab_chunk();
        assert_eq!(chunk2.start, 100); // compile_time_len(0) + runtime_start(100)
        assert_eq!(chunk2.end, 200);

        // Heap should have grown to accommodate chunks
        assert!(heap.len() >= 200);
    }

    #[test]
    fn test_alloc_tlab_chunk_with_compile_time() {
        let compile_time: Vec<Object<()>> = vec![
            Object::String("ct1".to_string()),
            Object::String("ct2".to_string()),
        ];
        let heap = BexHeap::with_tlab_size(compile_time, 100);

        // With 2 compile-time objects, global indices start at 2
        let chunk1 = heap.alloc_tlab_chunk();
        assert_eq!(chunk1.start, 2); // compile_time_len(2) + runtime_start(0)
        assert_eq!(chunk1.end, 102); // compile_time_len(2) + runtime_end(100)

        let chunk2 = heap.alloc_tlab_chunk();
        assert_eq!(chunk2.start, 102); // compile_time_len(2) + runtime_start(100)
        assert_eq!(chunk2.end, 202);
    }

    #[test]
    fn test_handle_create_resolve() {
        use bex_external_types::WeakHeapRef;

        let objects: Vec<Object<()>> = vec![Object::String("test".to_string())];
        let heap = BexHeap::new(objects);

        let handle = heap.create_handle(ObjectIndex::from_raw(0));
        let resolved = heap.resolve_handle(handle.slab_key());
        assert_eq!(resolved, Some(ObjectIndex::from_raw(0)));
    }

    #[test]
    fn test_handle_clone_and_drop() {
        use bex_external_types::WeakHeapRef;

        let objects: Vec<Object<()>> = vec![Object::String("test".to_string())];
        let heap = BexHeap::new(objects);

        let handle1 = heap.create_handle(ObjectIndex::from_raw(0));
        let handle2 = handle1.clone();

        // Both handles resolve
        assert!(heap.resolve_handle(handle1.slab_key()).is_some());
        assert!(heap.resolve_handle(handle2.slab_key()).is_some());

        // Drop first clone
        drop(handle1);

        // Second clone still resolves
        assert!(heap.resolve_handle(handle2.slab_key()).is_some());

        // Drop second clone - this should release the slab entry
        drop(handle2);

        // Heap is still valid after all handles dropped
        assert_eq!(heap.len(), 1);
    }

    #[test]
    fn test_heap_stats() {
        let compile_time: Vec<Object<()>> = vec![Object::String("builtin".to_string())];
        let heap = BexHeap::with_tlab_size(compile_time, 50);

        let stats = heap.stats();
        assert_eq!(stats.compile_time_objects, 1);
        assert_eq!(stats.total_objects, 1);
        assert_eq!(stats.runtime_objects, 0);

        // Allocate a TLAB chunk
        let _chunk = heap.alloc_tlab_chunk();

        let stats = heap.stats();
        assert_eq!(stats.tlab_chunks, 1);
        assert!(stats.total_objects >= 51); // Expanded for TLAB
    }
}
