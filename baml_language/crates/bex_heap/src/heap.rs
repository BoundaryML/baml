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
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use bex_external_types::{Handle, WeakHeapRef};
use bex_vm_types::{HeapPtr, Object, ObjectIndex};

use crate::{HeapDebuggerConfig, HeapDebuggerState, chunked_vec::ChunkedVec, tlab::TlabChunk};

/// Default TLAB chunk size (number of object slots).
///
/// This is the number of object slots each VM gets when it requests a new TLAB.
/// When a VM exhausts its TLAB, it atomically reserves the next `tlab_size` slots.
///
/// # Relationship to ChunkedVec chunk size
///
/// The underlying storage uses `ChunkedVec` with `DEFAULT_CHUNK_SIZE` (4096).
/// For optimal memory locality, TLAB size should divide evenly into the chunk size:
///
/// - `DEFAULT_CHUNK_SIZE = 4096` (storage chunks)
/// - `DEFAULT_TLAB_SIZE = 1024` (TLAB allocation unit)
/// - Result: 4 TLABs fit per storage chunk
///
/// This isn't strictly required (TLABs can span chunk boundaries), but aligned
/// TLABs have better cache behavior since all objects in a TLAB are contiguous.
pub const DEFAULT_TLAB_SIZE: usize = 1024;

// Compile-time assertion that default TLAB size divides evenly into chunk size
const _: () = assert!(
    crate::chunked_vec::DEFAULT_CHUNK_SIZE.is_multiple_of(DEFAULT_TLAB_SIZE),
    "DEFAULT_TLAB_SIZE should divide evenly into DEFAULT_CHUNK_SIZE for optimal alignment"
);

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
/// all VM instances via `Arc<BexHeap>`.
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
/// let heap: Arc<BexHeap> = BexHeap::new(compile_time_objects);
/// ```
pub struct BexHeap {
    /// Compile-time objects (never collected).
    /// These are permanent: functions, classes, enums, string literals.
    compile_time: Vec<Object>,

    /// Two runtime spaces - only one active at a time.
    /// Uses ChunkedVec for stable pointers during concurrent access.
    ///
    /// # Why ChunkedVec?
    ///
    /// With a regular Vec, if one VM is writing to an element while another
    /// VM triggers a resize (via TLAB chunk allocation), the Vec may reallocate
    /// and invalidate the first VM's pointer - that's undefined behavior.
    ///
    /// ChunkedVec stores objects in fixed-size chunks. Growing adds new chunks
    /// without moving existing data, so pointers remain stable even during
    /// concurrent growth.
    ///
    /// # Safety
    ///
    /// Access is safe because:
    /// - Each VM has exclusive write access to its TLAB region
    /// - BAML has no global mutable state
    /// - GC only runs at safepoints when no VMs are executing
    /// - ChunkedVec never moves existing elements during growth
    pub(crate) spaces: [UnsafeCell<ChunkedVec<Object>>; 2],

    /// Which space is currently active (0 or 1).
    pub(crate) active_space: AtomicUsize,

    /// Next TLAB chunk start index within active space.
    ///
    /// When a VM needs a new TLAB, it atomically increments this by the
    /// chunk size to reserve its region.
    next_chunk: AtomicUsize,

    /// Handle table for external/FFI boundary.
    ///
    /// Maps handle keys to HeapPtr values. Handles provide safe,
    /// validated access to heap objects from external code.
    ///
    /// Uses `RwLock<HashMap>` instead of sharded_slab to allow in-place
    /// updates after GC moves objects.
    pub(crate) handles: RwLock<HashMap<usize, HeapPtr>>,

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

    /// Debug instrumentation state and config.
    debug_state: HeapDebuggerState,
}

// SAFETY: BexHeap is Send + Sync because:
// - objects: UnsafeCell is accessed safely via TLAB exclusivity and growth_lock
// - compile_time_boundary: immutable after construction
// - next_chunk: AtomicUsize is thread-safe
// - handles: RwLock<HashMap> is thread-safe
// - tlab_size: immutable after construction
// - growth_lock: Mutex is thread-safe
unsafe impl Send for BexHeap {}
unsafe impl Sync for BexHeap {}

// Implement WeakHeapRef trait from bex_external_types
impl WeakHeapRef for BexHeap {
    fn release_handle(&self, handle_key: usize) {
        if let Ok(mut handles) = self.handles.write() {
            handles.remove(&handle_key);
        }
    }

    fn resolve_handle_ptr(&self, slab_key: usize) -> Option<HeapPtr> {
        self.handles
            .read()
            .ok()
            .and_then(|handles| handles.get(&slab_key).copied())
    }
}

impl BexHeap {
    /// Create a new heap with compile-time objects.
    ///
    /// The provided objects become permanent (never garbage collected).
    /// Runtime allocations will start after these objects.
    pub fn new(compile_time_objects: Vec<Object>) -> Arc<Self> {
        Self::with_tlab_size_and_debug(
            compile_time_objects,
            DEFAULT_TLAB_SIZE,
            HeapDebuggerConfig::from_env(),
        )
    }

    /// Create a new heap with custom TLAB size.
    pub fn with_tlab_size(compile_time_objects: Vec<Object>, tlab_size: usize) -> Arc<Self> {
        Self::with_tlab_size_and_debug(
            compile_time_objects,
            tlab_size,
            HeapDebuggerConfig::from_env(),
        )
    }

    /// Create a new heap with explicit debug configuration.
    pub fn with_tlab_size_and_debug(
        mut compile_time_objects: Vec<Object>,
        tlab_size: usize,
        debug: HeapDebuggerConfig,
    ) -> Arc<Self> {
        // Resolve bytecode constants for all Function objects before wrapping in Arc.
        // This converts ConstValue (with ObjectIndex) to Value (with HeapPtr).
        Self::resolve_function_constants(&mut compile_time_objects);

        Arc::new(Self {
            compile_time: compile_time_objects,
            spaces: [
                UnsafeCell::new(ChunkedVec::new()),
                UnsafeCell::new(ChunkedVec::new()),
            ],
            active_space: AtomicUsize::new(0),
            next_chunk: AtomicUsize::new(0), // Starts at 0 within active space
            handles: RwLock::new(HashMap::new()),
            next_handle_key: AtomicUsize::new(0),
            tlab_size,
            growth_lock: Mutex::new(()),
            allocs_since_gc: AtomicUsize::new(0),
            debug_state: HeapDebuggerState::new(debug),
        })
    }

    /// Resolve bytecode constants for all Function objects.
    ///
    /// Converts ConstValue (compile-time, with ObjectIndex) to Value (runtime, with HeapPtr).
    /// Must be called before wrapping in Arc since we need mutable access.
    fn resolve_function_constants(objects: &mut [Object]) {
        // First, compute pointers for all objects (they're at stable positions in the slice)
        let base_ptr = objects.as_ptr();

        for obj in objects.iter_mut() {
            if let Object::Function(func) = obj {
                // Resolve each constant, converting ObjectIndex to HeapPtr
                func.bytecode.resolved_constants = func
                    .bytecode
                    .constants
                    .iter()
                    .map(|cv| {
                        cv.to_value(|idx| {
                            // Get pointer to object at this index
                            let ptr = unsafe { base_ptr.add(idx.into_raw()) as *mut Object };
                            // Compile-time objects have epoch 0
                            #[cfg(feature = "heap_debug")]
                            unsafe {
                                bex_vm_types::HeapPtr::from_ptr(ptr, 0)
                            }
                            #[cfg(not(feature = "heap_debug"))]
                            unsafe {
                                bex_vm_types::HeapPtr::from_ptr(ptr)
                            }
                        })
                    })
                    .collect();
            }
        }
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

    /// Check if a pointer refers to a compile-time object.
    ///
    /// Returns true if the pointer falls within the compile_time Vec's memory range.
    #[inline]
    pub fn is_compile_time_ptr(&self, ptr: HeapPtr) -> bool {
        if self.compile_time.is_empty() {
            return false;
        }
        let raw_ptr = ptr.as_ptr() as *const Object;
        let start = self.compile_time.as_ptr();
        let end = unsafe { start.add(self.compile_time.len()) };
        raw_ptr >= start && raw_ptr < end
    }

    /// Get a HeapPtr to a compile-time object by index.
    ///
    /// # Panics
    ///
    /// Panics if the index is out of bounds.
    #[inline]
    pub fn compile_time_ptr(&self, index: usize) -> HeapPtr {
        assert!(
            index < self.compile_time.len(),
            "compile-time index {index} out of bounds (len={})",
            self.compile_time.len()
        );
        let raw_ptr = &self.compile_time[index] as *const Object as *mut Object;
        // SAFETY: The pointer is valid and points to a compile-time object
        // that will never be moved or deallocated.
        unsafe { self.make_heap_ptr(raw_ptr) }
    }

    /// Get the currently active runtime space index (0 or 1).
    #[inline]
    pub fn active_space_index(&self) -> usize {
        self.active_space.load(Ordering::Acquire)
    }

    /// Get a reference to the active runtime space.
    ///
    /// # Safety
    ///
    /// Caller must ensure no concurrent mutations to the space.
    #[inline]
    pub unsafe fn active_space(&self) -> &ChunkedVec<Object> {
        // SAFETY: Caller ensures no concurrent mutations
        unsafe { &*self.spaces[self.active_space_index()].get() }
    }

    /// Get a mutable reference to a specific space.
    ///
    /// # Safety
    ///
    /// Caller must ensure exclusive access to the space. This is safe during GC
    /// because all VMs are at safepoints (not executing).
    #[inline]
    #[allow(clippy::mut_from_ref)] // Interior mutability via UnsafeCell
    pub(crate) unsafe fn space_mut(&self, space_idx: usize) -> &mut ChunkedVec<Object> {
        // SAFETY: Caller ensures exclusive access
        unsafe { &mut *self.spaces[space_idx].get() }
    }

    /// Get a reference to a specific space.
    ///
    /// # Safety
    ///
    /// Caller must ensure no concurrent mutations to the space.
    #[inline]
    pub(crate) unsafe fn space_ref(&self, space_idx: usize) -> &ChunkedVec<Object> {
        // SAFETY: Caller ensures no concurrent mutations
        unsafe { &*self.spaces[space_idx].get() }
    }

    /// Convert a runtime space index to a global ObjectIndex.
    #[inline]
    pub fn runtime_to_global(&self, runtime_idx: usize) -> ObjectIndex {
        self.make_object_index(self.compile_time.len() + runtime_idx)
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

    /// Write an object at the given runtime index in the active space.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// 1. **Write exclusivity**: Only write to indices within your TLAB's
    ///    exclusive region (`tlab.alloc_ptr..tlab.alloc_limit`)
    /// 2. **Index validity**: The index must be < the space's current length
    ///
    /// # Why This API?
    ///
    /// ChunkedVec provides stable pointers - growing the storage never moves
    /// existing elements. This eliminates the data race that occurred with
    /// Vec, where one thread's pointer could be invalidated by another
    /// thread's resize operation.
    ///
    /// Production VMs (JVM, CLR, V8, Go) all use direct memory access for
    /// field writes. The "lock-free" property comes from:
    ///
    /// - **TLABs**: Each VM has exclusive write access to its allocation region
    /// - **No globals**: BAML has no global mutable state, preventing races
    /// - **Safepoint GC**: Collection only runs when no VMs are executing
    /// - **ChunkedVec**: Growing never moves existing elements
    #[inline]
    pub unsafe fn write_runtime_object(&self, runtime_idx: usize, obj: Object) {
        // SAFETY: Caller ensures exclusive access to this index
        // ChunkedVec's set() is internally safe for concurrent access to different indices
        unsafe {
            (*self.spaces[self.active_space_index()].get()).set(runtime_idx, obj);
        }
    }

    /// Get a mutable reference to a runtime object.
    ///
    /// # Safety
    ///
    /// Caller must ensure exclusive access to this object.
    #[inline]
    #[allow(clippy::mut_from_ref)] // Interior mutability via UnsafeCell
    pub unsafe fn get_runtime_object_mut(&self, runtime_idx: usize) -> &mut Object {
        // SAFETY: Caller ensures exclusive access
        unsafe { &mut *(*self.spaces[self.active_space_index()].get()).get_ptr(runtime_idx) }
    }

    /// Get the current number of objects in the heap.
    pub fn len(&self) -> usize {
        let active = self.active_space_index();
        // SAFETY: Reading len is safe, it's just a usize
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
    /// growth_lock protects the ChunkedVec resize operation.
    ///
    /// # Why This Is Now Safe
    ///
    /// With ChunkedVec, growing the storage adds new chunks without moving
    /// existing data. So even if one VM is writing to an existing element
    /// while another VM triggers growth here, there's no data race - the
    /// existing element's memory location doesn't change.
    ///
    /// Returns a `TlabChunk` describing the exclusive region for the VM.
    /// The VM can then allocate objects within this region without locks.
    pub fn alloc_tlab_chunk(&self) -> TlabChunk {
        self.debug_verify_tlab_canaries();

        let use_canary = self.debug_config().enabled;
        let canary_slots = if use_canary { 1 } else { 0 };

        // Atomically reserve a chunk range within active space
        let step = self.tlab_size + canary_slots;
        let runtime_start = self.next_chunk.fetch_add(step, Ordering::SeqCst);
        let runtime_end = runtime_start + self.tlab_size;
        let reserve_end = runtime_end + canary_slots;

        // Lock only for growth (serializes chunk allocation)
        let _guard = self.growth_lock.lock().unwrap();

        // Grow active space if needed - ChunkedVec never moves existing data
        let active = self.active_space_index();
        let ct_len = self.compile_time.len();
        // SAFETY: We hold the growth_lock, so no other thread is resizing.
        // ChunkedVec's resize_with never moves existing elements, and takes &self
        // so we don't need a mutable reference - which avoids the data race.
        let space = unsafe { &*self.spaces[active].get() };
        if space.len() < reserve_end {
            // SAFETY: We hold the growth_lock, ensuring only one thread resizes at a time.
            unsafe {
                space.resize_with(reserve_end, || {
                    // Placeholder object - will be overwritten by TLAB alloc
                    self.placeholder_object()
                });
            }
        }
        if use_canary {
            let chunk_start = ct_len + runtime_start;
            let chunk_end = ct_len + runtime_end;
            // SAFETY: We hold the growth_lock, index is within bounds
            unsafe {
                space.set(runtime_end, self.tlab_canary_object(chunk_start, chunk_end));
            }
        }

        // Return global indices (compile_time_len + runtime indices)
        if use_canary {
            let canary_idx = ct_len + runtime_end;
            self.record_tlab_canary(canary_idx);
        }
        TlabChunk {
            start: ct_len + runtime_start,
            end: ct_len + runtime_end,
        }
    }

    /// Read an object by HeapPtr (direct pointer dereference).
    ///
    /// # Safety
    ///
    /// - The pointer must be valid (not collected by GC)
    /// - Caller must ensure no concurrent writes to this object
    pub unsafe fn get_object(&self, idx: HeapPtr) -> &Object {
        self.debug_assert_valid_index(idx);

        // SAFETY: HeapPtr points directly to the object
        let obj = unsafe { idx.get() };

        self.debug_assert_not_sentinel(obj);
        obj
    }

    /// Get statistics about heap usage.
    pub fn stats(&self) -> HeapStats {
        let active = self.active_space_index();
        // SAFETY: Reading len is safe
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

    pub(crate) fn next_chunk_value(&self) -> usize {
        self.next_chunk.load(Ordering::Acquire)
    }

    pub(crate) fn debug_state(&self) -> &HeapDebuggerState {
        &self.debug_state
    }

    /// Update handle entries after GC.
    ///
    /// Updates handles to point to new object locations. Invalidates handles
    /// pointing to dead objects (runtime objects not found in forwarding map).
    /// Preserves handles to compile-time objects even if not traced.
    ///
    /// Returns the number of handles invalidated.
    pub fn update_handles(&self, forwarding: &HashMap<HeapPtr, HeapPtr>) -> usize {
        let mut invalidated_count = 0;
        if let Ok(mut handles) = self.handles.write() {
            handles.retain(|_, ptr| {
                if let Some(&new_ptr) = forwarding.get(ptr) {
                    *ptr = new_ptr;
                    true
                } else if self.is_compile_time_ptr(*ptr) {
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

    /// Create a handle to an object.
    ///
    /// Handles are used at the FFI boundary to give external code safe
    /// access to heap objects. Handles are GC roots - objects reachable
    /// from handles will not be collected.
    pub fn create_handle(self: &Arc<Self>, ptr: HeapPtr) -> Handle {
        // Get a unique key for this handle
        let handle_key = self.next_handle_key.fetch_add(1, Ordering::Relaxed);

        // Insert into the handle table
        if let Ok(mut handles) = self.handles.write() {
            handles.insert(handle_key, ptr);
        }

        // Handle no longer stores idx - always resolves through table
        Handle::new(handle_key, Arc::clone(self) as Arc<dyn WeakHeapRef>)
    }

    /// Collect all handle roots for garbage collection.
    ///
    /// Returns a Vec of HeapPtr values for all live handles.
    /// These should be treated as GC roots - objects reachable from
    /// handles must not be collected.
    pub fn collect_handle_roots(&self) -> Vec<HeapPtr> {
        self.handles
            .read()
            .map(|handles| handles.values().copied().collect())
            .unwrap_or_default()
    }
}

impl std::fmt::Debug for BexHeap {
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
    assert_send::<BexHeap>();
    assert_sync::<BexHeap>();
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_heap_empty() {
        let heap = BexHeap::new(vec![]);
        assert_eq!(heap.len(), 0);
        assert_eq!(heap.compile_time_boundary(), 0);
    }

    #[test]
    fn test_new_heap_with_objects() {
        let objects: Vec<Object> = vec![
            Object::String("hello".to_string()),
            Object::String("world".to_string()),
        ];
        let heap = BexHeap::new(objects);
        assert_eq!(heap.len(), 2);
        assert_eq!(heap.compile_time_boundary(), 2);
    }

    #[test]
    fn test_alloc_tlab_chunk() {
        let heap = BexHeap::with_tlab_size(vec![], 100);

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
        let compile_time: Vec<Object> = vec![
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
    fn test_heap_stats() {
        let compile_time: Vec<Object> = vec![Object::String("builtin".to_string())];
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

    // Note: Handle tests removed as they require HeapPtr creation which depends
    // on runtime allocation. Will be updated when full integration is complete.
}
