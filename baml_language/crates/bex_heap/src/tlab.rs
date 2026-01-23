//! Thread-Local Allocation Buffer (TLAB) for per-VM allocation.
//!
//! Each VM gets its own TLAB, which is a reserved chunk of the heap.
//! Allocation within a TLAB is a simple bump-pointer increment - no
//! locks, no atomics, no contention.
//!
//! When a TLAB is exhausted, the VM requests a new chunk from the heap.
//! This is the only point where synchronization is needed (an atomic
//! fetch_add on the heap's next_chunk counter).

use std::sync::Arc;

use bex_vm_types::{
    Object, ObjectIndex, Value,
    types::{Instance, Variant},
};
use indexmap::IndexMap;

use crate::BexHeap;

/// A reserved chunk of heap space for TLAB allocation.
#[derive(Clone, Copy, Debug)]
pub struct TlabChunk {
    /// Start index (inclusive).
    pub start: usize,
    /// End index (exclusive).
    pub end: usize,
}

impl TlabChunk {
    /// Get the size of this chunk.
    pub fn size(&self) -> usize {
        self.end - self.start
    }
}

/// Thread-Local Allocation Buffer for a BEX VM.
///
/// A TLAB provides fast, lock-free allocation within an exclusive heap region.
/// This is the same strategy used by the JVM, CLR, and Go runtime.
///
/// # Allocation Strategy
///
/// ```text
/// TLAB Memory Layout:
///
/// ┌────────────────────────────────────────────────────────────┐
/// │ [used] [used] [used] [free] [free] [free] ... [free]      │
/// │ ◄─── allocated ────► ◄────── available ─────────────►     │
/// │                      ▲                               ▲     │
/// │                 alloc_ptr                      alloc_limit │
/// └────────────────────────────────────────────────────────────┘
/// ```
///
/// # Performance
///
/// - **Fast path**: `alloc()` is a single pointer increment + write
/// - **No atomics**: Each VM owns its TLAB exclusively
/// - **No locks**: Direct memory access via `UnsafeCell`
/// - **Refill cost**: One `AtomicUsize::fetch_add` per ~1024 allocations
///
/// # Example
///
/// ```ignore
/// let heap = BexHeap::new(compile_time_objects);
/// let mut tlab = Tlab::new(Arc::clone(&heap));
///
/// // Fast allocation - just bumps pointer
/// let idx1 = tlab.alloc_string("hello".to_string());
/// let idx2 = tlab.alloc_array(vec![Value::Int(1), Value::Int(2)]);
///
/// // When chunk exhausted, refill gets a new region
/// for _ in 0..2000 {
///     tlab.alloc_string("item".to_string()); // Auto-refills as needed
/// }
/// ```
pub struct Tlab<F> {
    /// Next allocation index within current chunk.
    alloc_ptr: usize,

    /// End of current chunk (exclusive).
    alloc_limit: usize,

    /// Reference to the shared heap.
    heap: Arc<BexHeap<F>>,
}

impl<F> Tlab<F> {
    /// Create a new TLAB with an initial chunk from the heap.
    pub fn new(heap: Arc<BexHeap<F>>) -> Self {
        let chunk = heap.alloc_tlab_chunk();
        Self {
            alloc_ptr: chunk.start,
            alloc_limit: chunk.end,
            heap,
        }
    }

    /// Create a TLAB without allocating an initial chunk.
    ///
    /// The first allocation will trigger a refill.
    pub fn new_empty(heap: Arc<BexHeap<F>>) -> Self {
        Self {
            alloc_ptr: 0,
            alloc_limit: 0,
            heap,
        }
    }

    /// Allocate an object, returning its index.
    ///
    /// This is the fast path - just bump the pointer and write.
    /// If the current chunk is exhausted, refill from the heap.
    #[inline]
    pub fn alloc(&mut self, obj: Object<F>) -> ObjectIndex {
        if self.alloc_ptr >= self.alloc_limit {
            self.refill();
        }

        let global_idx = self.alloc_ptr;
        self.alloc_ptr += 1;

        // Convert global index to runtime-relative index for writing to active space
        let runtime_idx = global_idx - self.heap.compile_time_len();

        // SAFETY: This TLAB has exclusive access to indices in [chunk.start, chunk.end)
        // and we've ensured alloc_ptr < alloc_limit after potential refill.
        // ChunkedVec guarantees stable pointers during concurrent growth.
        unsafe {
            self.heap.write_runtime_object(runtime_idx, obj);
        }

        // Track allocation for GC heuristic
        self.heap.record_alloc();

        self.heap.make_object_index(global_idx)
    }

    /// Allocate a string object.
    #[inline]
    pub fn alloc_string(&mut self, s: String) -> ObjectIndex {
        self.alloc(Object::String(s))
    }

    /// Allocate an array object.
    #[inline]
    pub fn alloc_array(&mut self, values: Vec<Value>) -> ObjectIndex {
        self.alloc(Object::Array(values))
    }

    /// Allocate a map object.
    #[inline]
    pub fn alloc_map(&mut self, values: IndexMap<String, Value>) -> ObjectIndex {
        self.alloc(Object::Map(values))
    }

    /// Allocate an instance object.
    #[inline]
    pub fn alloc_instance(&mut self, class: ObjectIndex, fields: Vec<Value>) -> ObjectIndex {
        self.alloc(Object::Instance(Instance { class, fields }))
    }

    /// Allocate a variant object.
    #[inline]
    pub fn alloc_variant(&mut self, enm: ObjectIndex, index: usize) -> ObjectIndex {
        self.alloc(Object::Variant(Variant { enm, index }))
    }

    /// Get a new chunk from the heap (cold path).
    #[cold]
    fn refill(&mut self) {
        let chunk = self.heap.alloc_tlab_chunk();
        self.alloc_ptr = chunk.start;
        self.alloc_limit = chunk.end;
    }
}

impl<F> Tlab<F> {
    /// Get the remaining capacity in the current chunk.
    pub fn remaining(&self) -> usize {
        self.alloc_limit.saturating_sub(self.alloc_ptr)
    }

    /// Get a reference to the heap.
    pub fn heap(&self) -> &Arc<BexHeap<F>> {
        &self.heap
    }

    /// Invalidate this TLAB, forcing a refill on next allocation.
    /// Called by GC after swapping spaces.
    pub fn invalidate(&mut self) {
        self.alloc_limit = 0;
        self.alloc_ptr = 0;
    }

    /// Check if this TLAB is valid (has an allocated chunk).
    pub fn is_valid(&self) -> bool {
        self.alloc_limit > self.alloc_ptr
    }

    /// Read an object by index.
    ///
    /// # Safety
    ///
    /// Caller must ensure no concurrent writes to this index.
    pub unsafe fn get_object(&self, idx: ObjectIndex) -> &Object<F> {
        // SAFETY: Caller ensures no concurrent writes
        // Delegate to heap's get_object which handles compile-time vs runtime
        unsafe { self.heap.get_object(idx) }
    }

    /// Write an object by index.
    ///
    /// # Safety
    ///
    /// Caller must ensure exclusive access to this index.
    pub unsafe fn set_object(&mut self, idx: ObjectIndex, obj: Object<F>) {
        // SAFETY: Caller ensures exclusive access
        // Only runtime objects can be written (compile-time objects are immutable)
        let global_idx = idx.into_raw();
        let ct_len = self.heap.compile_time_len();
        if global_idx >= ct_len {
            let runtime_idx = global_idx - ct_len;
            unsafe {
                self.heap.write_runtime_object(runtime_idx, obj);
            }
        }
    }
}

impl<F> std::fmt::Debug for Tlab<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tlab")
            .field("alloc_ptr", &self.alloc_ptr)
            .field("alloc_limit", &self.alloc_limit)
            .field("remaining", &self.remaining())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "heap_debug")]
    #[test]
    fn test_tlab_canary_panics_on_clobber() {
        use std::panic::{AssertUnwindSafe, catch_unwind};

        use crate::{HeapDebuggerConfig, HeapVerifyMode};

        let debug = HeapDebuggerConfig {
            enabled: true,
            verify: HeapVerifyMode::Off,
        };
        let heap = BexHeap::<()>::with_tlab_size_and_debug(vec![], 4, debug);

        let _chunk = heap.alloc_tlab_chunk();

        let ct_len = heap.compile_time_len();
        let canary_idx = ct_len + heap.tlab_size();
        let runtime_idx = canary_idx - ct_len;
        unsafe {
            let space = &*heap.spaces[heap.active_space_index()].get();
            space.set(runtime_idx, Object::String("clobbered".to_string()));
        }

        let result = catch_unwind(AssertUnwindSafe(|| {
            let _ = heap.alloc_tlab_chunk();
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_tlab_alloc_single() {
        let heap = BexHeap::<()>::with_tlab_size(vec![], 100);
        let mut tlab = Tlab::new(heap);

        let idx = tlab.alloc(Object::String("hello".to_string()));
        assert_eq!(idx, ObjectIndex::from_raw(0));
        assert_eq!(tlab.remaining(), 99);
    }

    #[test]
    fn test_tlab_alloc_multiple() {
        let heap = BexHeap::<()>::with_tlab_size(vec![], 100);
        let mut tlab = Tlab::new(heap);

        for i in 0..10 {
            let idx = tlab.alloc(Object::String(format!("obj{i}")));
            assert_eq!(idx, ObjectIndex::from_raw(i));
        }
        assert_eq!(tlab.remaining(), 90);
    }

    #[test]
    fn test_tlab_refill() {
        let heap = BexHeap::<()>::with_tlab_size(vec![], 5);
        let mut tlab = Tlab::new(heap);

        // Allocate 5 objects (fills first chunk)
        for i in 0..5 {
            let idx = tlab.alloc(Object::String(format!("obj{i}")));
            assert_eq!(idx, ObjectIndex::from_raw(i));
        }
        assert_eq!(tlab.remaining(), 0);

        // Next allocation triggers refill
        let idx = tlab.alloc(Object::String("obj5".to_string()));
        assert_eq!(idx, ObjectIndex::from_raw(5));
        assert_eq!(tlab.remaining(), 4);
    }

    #[test]
    fn test_tlab_with_compile_time_objects() {
        let compile_time: Vec<Object<()>> = vec![
            Object::String("builtin1".to_string()),
            Object::String("builtin2".to_string()),
        ];
        let heap = BexHeap::with_tlab_size(compile_time, 100);
        let mut tlab = Tlab::new(heap);

        // First runtime allocation starts after compile-time objects
        let idx = tlab.alloc(Object::String("runtime".to_string()));
        assert_eq!(idx, ObjectIndex::from_raw(2));
    }

    #[test]
    fn test_multiple_tlabs_no_overlap() {
        let heap = BexHeap::<()>::with_tlab_size(vec![], 10);
        let heap2 = Arc::clone(&heap);

        let mut tlab1 = Tlab::new(Arc::clone(&heap));
        let mut tlab2 = Tlab::new(heap2);

        // Allocate from both TLABs
        let idx1 = tlab1.alloc(Object::String("from_tlab1".to_string()));
        let idx2 = tlab2.alloc(Object::String("from_tlab2".to_string()));

        // They should get different regions
        assert_eq!(idx1, ObjectIndex::from_raw(0));
        assert_eq!(idx2, ObjectIndex::from_raw(10));
    }

    #[test]
    fn test_tlab_read_object() {
        let heap = BexHeap::<()>::with_tlab_size(vec![], 100);
        let mut tlab = Tlab::new(heap);

        let idx = tlab.alloc(Object::String("test_value".to_string()));

        // SAFETY: Single-threaded test, no concurrent access
        unsafe {
            let obj = tlab.get_object(idx);
            match obj {
                Object::String(s) => assert_eq!(s, "test_value"),
                _ => panic!("Expected String object"),
            }
        }
    }

    #[test]
    fn test_alloc_string() {
        let heap = BexHeap::<()>::with_tlab_size(vec![], 100);
        let mut tlab = Tlab::new(heap);

        let idx = tlab.alloc_string("hello world".to_string());

        unsafe {
            match tlab.get_object(idx) {
                Object::String(s) => assert_eq!(s, "hello world"),
                _ => panic!("Expected String"),
            }
        }
    }

    #[test]
    fn test_alloc_array() {
        let heap = BexHeap::<()>::with_tlab_size(vec![], 100);
        let mut tlab = Tlab::new(heap);

        let values = vec![Value::Int(1), Value::Int(2), Value::Int(3)];
        let idx = tlab.alloc_array(values);

        unsafe {
            match tlab.get_object(idx) {
                Object::Array(arr) => {
                    assert_eq!(arr.len(), 3);
                    assert_eq!(arr[0], Value::Int(1));
                }
                _ => panic!("Expected Array"),
            }
        }
    }

    #[test]
    fn test_alloc_map() {
        let heap = BexHeap::<()>::with_tlab_size(vec![], 100);
        let mut tlab = Tlab::new(heap);

        let mut map = IndexMap::new();
        map.insert("key".to_string(), Value::Int(42));
        let idx = tlab.alloc_map(map);

        unsafe {
            match tlab.get_object(idx) {
                Object::Map(m) => {
                    assert_eq!(m.get("key"), Some(&Value::Int(42)));
                }
                _ => panic!("Expected Map"),
            }
        }
    }

    #[test]
    fn test_alloc_instance() {
        use bex_vm_types::types::Class;

        // First allocate a class object
        let heap = BexHeap::<()>::with_tlab_size(vec![], 100);
        let mut tlab = Tlab::new(heap);

        // Simulate a class at index 0
        let class_idx = tlab.alloc(Object::Class(Class {
            name: "TestClass".to_string(),
            field_names: vec!["x".to_string(), "y".to_string()],
            type_tag: 100,
        }));

        // Allocate an instance of that class
        let fields = vec![Value::Int(10), Value::Int(20)];
        let instance_idx = tlab.alloc_instance(class_idx, fields);

        unsafe {
            match tlab.get_object(instance_idx) {
                Object::Instance(inst) => {
                    assert_eq!(inst.class, class_idx);
                    assert_eq!(inst.fields.len(), 2);
                    assert_eq!(inst.fields[0], Value::Int(10));
                }
                _ => panic!("Expected Instance"),
            }
        }
    }

    #[test]
    fn test_alloc_variant() {
        use bex_vm_types::types::Enum;

        let heap = BexHeap::<()>::with_tlab_size(vec![], 100);
        let mut tlab = Tlab::new(heap);

        // Simulate an enum at index 0
        let enum_idx = tlab.alloc(Object::Enum(Enum {
            name: "Color".to_string(),
            variant_names: vec!["Red".to_string(), "Green".to_string(), "Blue".to_string()],
        }));

        // Allocate a variant (Color::Green = index 1)
        let variant_idx = tlab.alloc_variant(enum_idx, 1);

        unsafe {
            match tlab.get_object(variant_idx) {
                Object::Variant(v) => {
                    assert_eq!(v.enm, enum_idx);
                    assert_eq!(v.index, 1);
                }
                _ => panic!("Expected Variant"),
            }
        }
    }

    // ========================================================================
    // Miri-targeted tests
    //
    // These tests are specifically designed to exercise unsafe code paths
    // that Miri can verify for memory safety. They focus on:
    // - TLAB invalidation and refill after GC
    // - Concurrent TLAB allocation patterns
    // - Object mutation through set_object
    // ========================================================================

    /// Tests TLAB invalidation and refill after GC.
    ///
    /// This simulates what happens when GC runs and invalidates a VM's TLAB:
    /// 1. VM allocates objects via TLAB
    /// 2. GC runs, moves objects to new space, invalidates TLAB
    /// 3. VM continues allocating (TLAB refills from new space)
    #[test]
    fn test_miri_tlab_invalidation_and_refill() {
        let heap = BexHeap::<()>::with_tlab_size(vec![], 10);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Allocate some objects before GC
        let obj1 = tlab.alloc_string("before_gc_1".to_string());
        let obj2 = tlab.alloc_string("before_gc_2".to_string());

        assert!(tlab.is_valid());

        // Simulate GC: run collection and invalidate TLAB
        let (stats, _remapped, forwarding) =
            unsafe { heap.collect_garbage_with_forwarding(&[obj1, obj2]) };

        assert_eq!(stats.live_count, 2);

        // Invalidate TLAB (what bex_engine does after GC)
        tlab.invalidate();

        assert!(!tlab.is_valid());
        assert_eq!(tlab.remaining(), 0);

        // Get forwarded indices
        let new_obj1 = forwarding.get(&obj1).copied().unwrap_or(obj1);
        let new_obj2 = forwarding.get(&obj2).copied().unwrap_or(obj2);

        // Continue allocating - TLAB should refill from new space
        let obj3 = tlab.alloc_string("after_gc_1".to_string());
        let obj4 = tlab.alloc_string("after_gc_2".to_string());

        assert!(tlab.is_valid());

        // Verify all objects are accessible
        unsafe {
            // Objects from before GC (now in new space)
            match tlab.get_object(new_obj1) {
                Object::String(s) => assert_eq!(s, "before_gc_1"),
                _ => panic!("Expected String"),
            }
            match tlab.get_object(new_obj2) {
                Object::String(s) => assert_eq!(s, "before_gc_2"),
                _ => panic!("Expected String"),
            }

            // Objects from after GC (allocated in new space)
            match tlab.get_object(obj3) {
                Object::String(s) => assert_eq!(s, "after_gc_1"),
                _ => panic!("Expected String"),
            }
            match tlab.get_object(obj4) {
                Object::String(s) => assert_eq!(s, "after_gc_2"),
                _ => panic!("Expected String"),
            }
        }
    }

    /// Tests set_object for field mutation patterns.
    ///
    /// This exercises the unsafe write path used when VMs update object fields.
    #[test]
    fn test_miri_set_object_mutation() {
        let heap = BexHeap::<()>::with_tlab_size(vec![], 100);
        let mut tlab = Tlab::new(heap);

        // Allocate an object
        let idx = tlab.alloc_string("original".to_string());

        // Verify original value
        unsafe {
            match tlab.get_object(idx) {
                Object::String(s) => assert_eq!(s, "original"),
                _ => panic!("Expected String"),
            }
        }

        // Mutate the object using set_object
        unsafe {
            tlab.set_object(idx, Object::String("mutated".to_string()));
        }

        // Verify mutation
        unsafe {
            match tlab.get_object(idx) {
                Object::String(s) => assert_eq!(s, "mutated"),
                _ => panic!("Expected String"),
            }
        }
    }

    /// Tests concurrent TLAB allocation from multiple threads.
    ///
    /// This verifies that TLABs correctly provide non-overlapping regions
    /// when used from multiple threads simultaneously.
    ///
    /// Tests concurrent TLAB allocation from multiple threads.
    ///
    /// This verifies that TLABs correctly provide non-overlapping regions
    /// when used from multiple threads simultaneously.
    ///
    /// This test previously failed under Miri due to a data race between
    /// TLAB writes and Vec resizing. The fix: replace Vec with ChunkedVec,
    /// which never moves existing data when growing.
    #[test]
    fn test_miri_concurrent_tlab_allocation() {
        use std::thread;

        let heap = BexHeap::<()>::with_tlab_size(vec![], 100);

        // Spawn threads that each get their own TLAB and allocate
        let handles: Vec<_> = (0..4)
            .map(|thread_id| {
                let heap = Arc::clone(&heap);
                thread::spawn(move || {
                    let mut tlab = Tlab::new(heap);

                    // Each thread allocates multiple objects
                    let mut indices = Vec::new();
                    for i in 0..10 {
                        let idx = tlab.alloc_string(format!("thread_{thread_id}_obj_{i}"));
                        indices.push(idx);
                    }

                    // Verify all objects are readable
                    for (i, idx) in indices.iter().enumerate() {
                        unsafe {
                            match tlab.get_object(*idx) {
                                Object::String(s) => {
                                    assert_eq!(s, &format!("thread_{thread_id}_obj_{i}"));
                                }
                                _ => panic!("Expected String"),
                            }
                        }
                    }

                    indices
                })
            })
            .collect();

        // Collect all indices from all threads
        let all_indices: Vec<Vec<ObjectIndex>> =
            handles.into_iter().map(|h| h.join().unwrap()).collect();

        // Verify no overlapping indices between threads
        let mut seen = std::collections::HashSet::new();
        for thread_indices in &all_indices {
            for idx in thread_indices {
                assert!(
                    seen.insert(idx.into_raw()),
                    "Duplicate index {} allocated by multiple threads",
                    idx.into_raw()
                );
            }
        }

        // Verify all objects are still accessible from the heap
        for (thread_id, thread_indices) in all_indices.iter().enumerate() {
            for (i, idx) in thread_indices.iter().enumerate() {
                unsafe {
                    match heap.get_object(*idx) {
                        Object::String(s) => {
                            assert_eq!(s, &format!("thread_{thread_id}_obj_{i}"));
                        }
                        _ => panic!("Expected String"),
                    }
                }
            }
        }
    }

    /// Tests TLAB chunk exhaustion and refill under concurrent allocation.
    ///
    /// Multiple threads exhaust their TLAB chunks and refill, verifying
    /// the atomic chunk allocation doesn't cause races.
    ///
    /// This test previously failed under Miri due to a data race between
    /// TLAB writes and Vec resizing. The fix: replace Vec with ChunkedVec,
    /// which never moves existing data when growing.
    #[test]
    fn test_miri_concurrent_tlab_refill() {
        use std::thread;

        // Small TLAB size to force frequent refills
        let heap = BexHeap::<()>::with_tlab_size(vec![], 5);

        let handles: Vec<_> = (0..3)
            .map(|thread_id| {
                let heap = Arc::clone(&heap);
                thread::spawn(move || {
                    let mut tlab = Tlab::new(heap);

                    // Allocate more objects than fit in one TLAB chunk
                    // to force multiple refills
                    let mut indices = Vec::new();
                    for i in 0..20 {
                        let idx = tlab.alloc_string(format!("t{thread_id}_o{i}"));
                        indices.push(idx);
                    }

                    indices
                })
            })
            .collect();

        let all_indices: Vec<Vec<ObjectIndex>> =
            handles.into_iter().map(|h| h.join().unwrap()).collect();

        // Verify no overlaps
        let mut seen = std::collections::HashSet::new();
        for thread_indices in &all_indices {
            for idx in thread_indices {
                assert!(
                    seen.insert(idx.into_raw()),
                    "Duplicate index from concurrent refill"
                );
            }
        }

        // Verify all 60 objects (3 threads × 20 objects) are accessible
        assert_eq!(seen.len(), 60);
    }
}
