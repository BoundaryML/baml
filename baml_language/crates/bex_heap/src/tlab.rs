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
    HeapPtr, Object, Value,
    types::{Array, Instance, Map, Variant},
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
/// let ptr1 = tlab.alloc_string("hello".to_string());
/// let ptr2 = tlab.alloc_array(bex_vm_types::RealizedTy::int(), vec![Value::int(1), Value::int(2)]);
///
/// // When chunk exhausted, refill gets a new region
/// for _ in 0..2000 {
///     tlab.alloc_string("item".to_string()); // Auto-refills as needed
/// }
/// ```
pub struct Tlab {
    /// Next allocation index within current chunk.
    alloc_ptr: usize,

    /// End of current chunk (exclusive).
    alloc_limit: usize,

    /// Reference to the shared heap.
    heap: Arc<BexHeap>,
}

impl Tlab {
    /// Create a new TLAB with an initial chunk eagerly reserved from the
    /// heap.
    ///
    /// **Avoid in production code paths.** Reserving a chunk here happens
    /// outside any [`HeapPermitManager`](crate::HeapPermitManager) permit;
    /// if a concurrent GC clears Gen0 before the caller registers as a
    /// holder and acquires its permit, this TLAB's cursor is left pointing
    /// into a freed/cleared region. Subsequent allocations panic with
    /// `index N out of bounds (len=0)` (debug) or segfault (release).
    ///
    /// Permit-managed holders (`BexVm`, `FutureManagerInner`, …) must use
    /// [`Tlab::new_empty`] so the first refill happens under a held permit.
    /// `Tlab::new` is retained for standalone heap tests that exercise
    /// TLAB mechanics without the permit infrastructure (those tests
    /// guarantee single-threaded access).
    pub fn new(heap: Arc<BexHeap>) -> Self {
        let chunk = heap.alloc_tlab_chunk();
        Self {
            alloc_ptr: chunk.start,
            alloc_limit: chunk.end,
            heap,
        }
    }

    /// Create a TLAB without allocating an initial chunk.
    ///
    /// The first allocation will trigger a refill. This is the right
    /// constructor for permit-managed holders: it defers the
    /// `alloc_tlab_chunk` call until the holder has been registered with
    /// [`HeapPermitManager`](crate::HeapPermitManager) and acquired its
    /// permit, so a concurrent GC cannot strand the TLAB cursor.
    pub fn new_empty(heap: Arc<BexHeap>) -> Self {
        Self {
            alloc_ptr: 0,
            alloc_limit: 0,
            heap,
        }
    }

    /// Allocate an object, returning a HeapPtr to it.
    ///
    /// This is the fast path - just bump the pointer and write.
    /// If the current chunk is exhausted, refill from the heap.
    #[inline]
    pub fn alloc(&mut self, obj: Object) -> HeapPtr {
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

        // Get the pointer to the newly written object in Gen0.
        // SAFETY: We just wrote to runtime_idx in Gen0, so it's valid.
        let ptr = unsafe { (*self.heap.gen0.get()).get_ptr(runtime_idx) };

        // SAFETY: The pointer is valid and points to a valid object we just wrote
        unsafe { self.heap.make_heap_ptr(ptr) }
    }

    /// Allocate a float object.
    #[inline]
    pub fn alloc_float(&mut self, f: f64) -> HeapPtr {
        self.alloc(Object::Float(f))
    }

    /// Allocate a string object.
    #[inline]
    pub fn alloc_string(&mut self, s: impl Into<bex_str::BexStr>) -> HeapPtr {
        self.alloc(Object::String(s.into()))
    }

    /// Allocate an array object whose elements have static type `element_ty`.
    #[inline]
    pub fn alloc_array(
        &mut self,
        element_ty: bex_vm_types::RealizedTy,
        values: Vec<Value>,
    ) -> HeapPtr {
        self.alloc(Object::Array(Array::new(element_ty, values)))
    }

    /// Allocate a map object whose keys/values have static types `key_ty`/`value_ty`.
    #[inline]
    pub fn alloc_map(
        &mut self,
        key_ty: bex_vm_types::RealizedTy,
        value_ty: bex_vm_types::RealizedTy,
        values: IndexMap<bex_str::BexStr, Value>,
    ) -> HeapPtr {
        self.alloc(Object::Map(Map::new(key_ty, value_ty, values)))
    }

    /// Allocate a non-generic instance object (empty class type args).
    #[inline]
    pub fn alloc_instance(&mut self, class: HeapPtr, fields: Vec<Value>) -> HeapPtr {
        self.alloc_instance_with_type_args(class, Box::new([]), fields)
    }

    /// Allocate an instance object carrying its concrete class type args (De
    /// Bruijn order). Used by the inbound FFI path to land a generic instance's
    /// wire-supplied `type_args` into `Object::Instance::class_type_args`.
    #[inline]
    pub fn alloc_instance_with_type_args(
        &mut self,
        class: HeapPtr,
        type_args: Box<[bex_vm_types::RealizedTy]>,
        fields: Vec<Value>,
    ) -> HeapPtr {
        self.alloc(Object::Instance(Instance::new(class, type_args, fields)))
    }

    /// Allocate a variant object.
    #[inline]
    pub fn alloc_variant(&mut self, enm: HeapPtr, index: usize) -> HeapPtr {
        self.alloc(Object::Variant(Variant { enm, index }))
    }

    /// Allocate a uint8 array object.
    #[inline]
    pub fn alloc_uint8array(&mut self, data: Vec<u8>) -> HeapPtr {
        self.alloc(Object::Uint8Array(data.into()))
    }

    /// Allocate an arbitrary-precision integer on the heap. Wraps the value in
    /// an `Arc` so the digit slice can be shared by clones without a deep copy.
    #[inline]
    pub fn alloc_bigint(&mut self, value: num_bigint::BigInt) -> HeapPtr {
        self.alloc(Object::Bigint(Arc::new(value)))
    }

    /// Allocate opaque Rust data on the heap.
    #[inline]
    pub fn alloc_rust_data(&mut self, data: Arc<dyn std::any::Any + Send + Sync>) -> HeapPtr {
        self.alloc(Object::RustData(data))
    }

    /// Allocate a collector object on the heap.
    #[inline]
    pub fn alloc_collector(&mut self, collector: bex_vm_types::CollectorRef) -> HeapPtr {
        self.alloc(Object::Collector(collector))
    }

    /// Allocate a type descriptor object on the heap.
    ///
    /// Static materialization inside the VM should go through
    /// `BexVm::alloc_static_type`.
    #[inline]
    pub fn alloc_type(&mut self, tv: bex_vm_types::types::TypeValue) -> HeapPtr {
        self.alloc(Object::Type(Box::new(tv)))
    }

    /// Allocate a future object on the heap.
    #[inline]
    pub fn alloc_future(&mut self, future: bex_vm_types::Future) -> HeapPtr {
        self.alloc(Object::Future(future))
    }

    /// Allocate a bound method on the heap.
    #[inline]
    pub fn alloc_bound_method(&mut self, method: bex_vm_types::BoundMethod) -> HeapPtr {
        self.alloc(Object::BoundMethod(method))
    }

    /// Get a new chunk from the heap (cold path).
    #[cold]
    fn refill(&mut self) {
        let chunk = self.heap.alloc_tlab_chunk();
        self.alloc_ptr = chunk.start;
        self.alloc_limit = chunk.end;
    }
}

impl Tlab {
    /// Get the remaining capacity in the current chunk.
    pub fn remaining(&self) -> usize {
        self.alloc_limit.saturating_sub(self.alloc_ptr)
    }

    /// Get a reference to the heap.
    pub fn heap(&self) -> &Arc<BexHeap> {
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

    /// Read an object by HeapPtr.
    ///
    /// # Safety
    ///
    /// - The pointer must be valid (not collected by GC)
    /// - Caller must ensure no concurrent writes to this object
    pub unsafe fn get_object(&self, idx: HeapPtr) -> &Object {
        // SAFETY: Caller ensures no concurrent writes
        // Delegate to heap's get_object
        unsafe { self.heap.get_object(idx) }
    }

    /// Write an object by HeapPtr.
    ///
    /// # Safety
    ///
    /// - The pointer must be valid (not collected by GC)
    /// - Caller must ensure exclusive access to this object
    /// - Only runtime objects can be written (compile-time objects are immutable)
    ///
    /// # Write Barrier
    ///
    /// If `ptr` points to an older-generation object, callers must fire the
    /// generational write barrier for any `HeapPtr` references in `obj` that
    /// point into a younger generation. During normal execution this is called
    /// only for Gen0 objects (newly allocated), so no barrier is needed.
    pub unsafe fn set_object(&mut self, ptr: HeapPtr, obj: Object) {
        // SAFETY: Caller ensures exclusive access
        // Direct write to the object through the pointer
        unsafe {
            *ptr.get_mut() = obj;
        }
    }
}

impl std::fmt::Debug for Tlab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tlab")
            .field("alloc_ptr", &self.alloc_ptr)
            .field("alloc_limit", &self.alloc_limit)
            .field("remaining", &self.remaining())
            .finish()
    }
}

pub trait TlabHolder {
    fn tlab(&self) -> &Tlab;
    fn tlab_mut(&mut self) -> &mut Tlab;

    fn alloc(&mut self, obj: Object) -> HeapPtr {
        self.tlab_mut().alloc(obj)
    }

    fn alloc_float(&mut self, f: f64) -> HeapPtr {
        self.tlab_mut().alloc_float(f)
    }

    fn alloc_string(&mut self, s: impl Into<bex_str::BexStr>) -> HeapPtr {
        self.tlab_mut().alloc_string(s)
    }

    fn alloc_array(&mut self, element_ty: bex_vm_types::RealizedTy, values: Vec<Value>) -> HeapPtr {
        self.tlab_mut().alloc_array(element_ty, values)
    }

    fn alloc_map(
        &mut self,
        key_ty: bex_vm_types::RealizedTy,
        value_ty: bex_vm_types::RealizedTy,
        values: IndexMap<bex_str::BexStr, Value>,
    ) -> HeapPtr {
        self.tlab_mut().alloc_map(key_ty, value_ty, values)
    }

    fn alloc_instance(&mut self, class: HeapPtr, fields: Vec<Value>) -> HeapPtr {
        self.tlab_mut().alloc_instance(class, fields)
    }

    fn alloc_variant(&mut self, enm: HeapPtr, index: usize) -> HeapPtr {
        self.tlab_mut().alloc_variant(enm, index)
    }

    fn alloc_uint8array(&mut self, data: Vec<u8>) -> HeapPtr {
        self.tlab_mut().alloc_uint8array(data)
    }

    fn alloc_bigint(&mut self, value: num_bigint::BigInt) -> HeapPtr {
        self.tlab_mut().alloc_bigint(value)
    }

    fn alloc_rust_data(&mut self, data: Arc<dyn std::any::Any + Send + Sync>) -> HeapPtr {
        self.tlab_mut().alloc_rust_data(data)
    }

    fn alloc_collector(&mut self, collector: bex_vm_types::CollectorRef) -> HeapPtr {
        self.tlab_mut().alloc_collector(collector)
    }

    fn alloc_type(&mut self, tv: bex_vm_types::types::TypeValue) -> HeapPtr {
        self.tlab_mut().alloc_type(tv)
    }

    fn alloc_future(&mut self, future: bex_vm_types::Future) -> HeapPtr {
        self.tlab_mut().alloc_future(future)
    }

    fn alloc_bound_method(&mut self, method: bex_vm_types::BoundMethod) -> HeapPtr {
        self.tlab_mut().alloc_bound_method(method)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "heap_debug")]
    #[test]
    fn test_tlab_canary_panics_on_clobber() {
        use std::panic::{AssertUnwindSafe, catch_unwind};

        use crate::{HeapDebuggerConfig, heap_debugger::HeapVerifyMode};

        let debug = HeapDebuggerConfig {
            enabled: true,
            verify: HeapVerifyMode::Off,
        };
        let heap = BexHeap::with_tlab_size_and_debug(vec![], 4, debug);

        let _chunk = heap.alloc_tlab_chunk();

        let ct_len = heap.compile_time_len();
        let canary_idx = ct_len + heap.tlab_size();
        let runtime_idx = canary_idx - ct_len;
        unsafe {
            let gen0 = &*heap.gen0.get();
            gen0.set(
                runtime_idx,
                Object::String(bex_str::BexStr::from("clobbered")),
            );
        }

        let result = catch_unwind(AssertUnwindSafe(|| {
            let _ = heap.alloc_tlab_chunk();
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_tlab_alloc_single() {
        let heap = BexHeap::with_tlab_size(vec![], 100);
        let mut tlab = Tlab::new(heap);

        let _ptr = tlab.alloc(Object::String(bex_str::BexStr::from("hello")));
        assert_eq!(tlab.remaining(), 99);
    }

    #[test]
    fn test_tlab_alloc_multiple() {
        let heap = BexHeap::with_tlab_size(vec![], 100);
        let mut tlab = Tlab::new(heap);

        for i in 0..10 {
            let _ptr = tlab.alloc(Object::String(bex_str::BexStr::from(format!("obj{i}"))));
        }
        assert_eq!(tlab.remaining(), 90);
    }

    #[test]
    fn test_tlab_refill() {
        let heap = BexHeap::with_tlab_size(vec![], 5);
        let mut tlab = Tlab::new(heap);

        // Allocate 5 objects (fills first chunk)
        for i in 0..5 {
            let _ptr = tlab.alloc(Object::String(bex_str::BexStr::from(format!("obj{i}"))));
        }
        assert_eq!(tlab.remaining(), 0);

        // Next allocation triggers refill
        let _ptr = tlab.alloc(Object::String(bex_str::BexStr::from("obj5")));
        assert_eq!(tlab.remaining(), 4);
    }

    #[test]
    fn test_tlab_with_compile_time_objects() {
        let compile_time: Vec<Object> = vec![
            Object::String(bex_str::BexStr::from("builtin1")),
            Object::String(bex_str::BexStr::from("builtin2")),
        ];
        let heap = BexHeap::with_tlab_size(compile_time, 100);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // First runtime allocation must land outside the compile-time region.
        let ptr = tlab.alloc(Object::String(bex_str::BexStr::from("runtime")));
        assert!(
            !heap.is_compile_time_ptr(ptr),
            "runtime allocation must not overlap compile-time objects"
        );
    }

    #[test]
    fn test_multiple_tlabs_no_overlap() {
        let heap = BexHeap::with_tlab_size(vec![], 10);
        let heap2 = Arc::clone(&heap);

        let mut tlab1 = Tlab::new(Arc::clone(&heap));
        let mut tlab2 = Tlab::new(heap2);

        // Allocate from both TLABs
        let ptr1 = tlab1.alloc(Object::String(bex_str::BexStr::from("from_tlab1")));
        let ptr2 = tlab2.alloc(Object::String(bex_str::BexStr::from("from_tlab2")));

        // They should get different pointers (different TLAB regions).
        assert_ne!(ptr1, ptr2);
    }

    #[test]
    fn test_tlab_read_object() {
        let heap = BexHeap::with_tlab_size(vec![], 100);
        let mut tlab = Tlab::new(heap);

        let ptr = tlab.alloc(Object::String(bex_str::BexStr::from("test_value")));

        // SAFETY: Single-threaded test, no concurrent access
        unsafe {
            let obj = tlab.get_object(ptr);
            match obj {
                Object::String(s) => assert_eq!(s.as_str(), "test_value"),
                _ => panic!("Expected String object"),
            }
        }
    }

    #[test]
    fn test_alloc_string() {
        let heap = BexHeap::with_tlab_size(vec![], 100);
        let mut tlab = Tlab::new(heap);

        let ptr = tlab.alloc_string("hello world");

        unsafe {
            match ptr.get() {
                Object::String(s) => assert_eq!(s.as_str(), "hello world"),
                _ => panic!("Expected String"),
            }
        }
    }

    #[test]
    fn test_alloc_array() {
        let heap = BexHeap::with_tlab_size(vec![], 100);
        let mut tlab = Tlab::new(heap);

        let values = vec![Value::int(1), Value::int(2), Value::int(3)];
        let ptr = tlab.alloc_array(bex_vm_types::RealizedTy::int(), values);

        unsafe {
            match ptr.get() {
                Object::Array(arr) => {
                    assert_eq!(arr.len(), 3);
                    assert_eq!(arr.get(0), Some(Value::int(1)));
                }
                _ => panic!("Expected Array"),
            }
        }
    }

    #[test]
    fn test_alloc_map() {
        let heap = BexHeap::with_tlab_size(vec![], 100);
        let mut tlab = Tlab::new(heap);

        let mut map = IndexMap::new();
        map.insert(bex_str::BexStr::from("key"), Value::int(42));
        let ptr = tlab.alloc_map(
            bex_vm_types::RealizedTy::string(),
            bex_vm_types::RealizedTy::int(),
            map,
        );

        unsafe {
            match ptr.get() {
                Object::Map(m) => {
                    assert_eq!(m.get("key"), Some(Value::int(42)));
                }
                _ => panic!("Expected Map"),
            }
        }
    }

    #[test]
    fn test_alloc_instance() {
        use bex_vm_types::types::Class;

        // First allocate a class object
        let heap = BexHeap::with_tlab_size(vec![], 100);
        let mut tlab = Tlab::new(heap);

        // Simulate a class at index 0
        let class_ptr = tlab.alloc(Object::Class(Box::new(Class {
            name: baml_type::TypeName::local(baml_type::Name::new("TestClass")),
            fields: vec![
                bex_vm_types::ClassField {
                    name: "x".to_string(),
                    field_type: baml_type::RuntimeTy::Int {
                        attr: baml_type::TyAttr::default(),
                    },
                    field_template: baml_type::TyTemplate::from(baml_type::RealizedTy::Int {
                        attr: baml_type::TyAttr::default(),
                    }),
                    description: None,
                    alias: None,
                    docstring: None,
                    other: Default::default(),
                    skip: false,
                    runtime_type: None,
                },
                bex_vm_types::ClassField {
                    name: "y".to_string(),
                    field_type: baml_type::RuntimeTy::Int {
                        attr: baml_type::TyAttr::default(),
                    },
                    field_template: baml_type::TyTemplate::from(baml_type::RealizedTy::Int {
                        attr: baml_type::TyAttr::default(),
                    }),
                    description: None,
                    alias: None,
                    docstring: None,
                    other: Default::default(),
                    skip: false,
                    runtime_type: None,
                },
            ],
            description: None,
            alias: None,
            docstring: None,
            other: Default::default(),
            type_tag: baml_type::typetag::TypeTag::from_i64(100),
            ty_attr: baml_type::TyAttr::default(),
            has_cleanup: false,
            generic_param_count: 0,
            runtime_type: None,
        })));

        // Allocate an instance of that class
        let fields = vec![Value::int(10), Value::int(20)];
        let instance_ptr = tlab.alloc_instance(class_ptr, fields);

        unsafe {
            match instance_ptr.get() {
                Object::Instance(inst) => {
                    assert_eq!(inst.class, class_ptr);
                    assert_eq!(inst.fields.len(), 2);
                    assert_eq!(inst.load_field(0), Value::int(10));
                }
                _ => panic!("Expected Instance"),
            }
        }
    }

    #[test]
    fn test_alloc_variant() {
        use bex_vm_types::types::Enum;

        let heap = BexHeap::with_tlab_size(vec![], 100);
        let mut tlab = Tlab::new(heap);

        // Simulate an enum at index 0
        let enum_ptr = tlab.alloc(Object::Enum(Box::new(Enum {
            type_tag: baml_type::typetag::TypeTag::from_i64(200),
            name: baml_type::TypeName::local(baml_type::Name::new("Color")),
            variants: vec![
                bex_vm_types::EnumVariant {
                    name: "Red".to_string(),
                    description: None,
                    alias: None,
                    docstring: None,
                    other: Default::default(),
                    skip: false,
                },
                bex_vm_types::EnumVariant {
                    name: "Green".to_string(),
                    description: None,
                    alias: None,
                    docstring: None,
                    other: Default::default(),
                    skip: false,
                },
                bex_vm_types::EnumVariant {
                    name: "Blue".to_string(),
                    description: None,
                    alias: None,
                    docstring: None,
                    other: Default::default(),
                    skip: false,
                },
            ],
            description: None,
            alias: None,
            docstring: None,
            other: Default::default(),
            ty_attr: baml_type::TyAttr::default(),
            runtime_type: None,
        })));

        // Allocate a variant (Color::Green = index 1)
        let variant_ptr = tlab.alloc_variant(enum_ptr, 1);

        unsafe {
            match variant_ptr.get() {
                Object::Variant(v) => {
                    assert_eq!(v.enm, enum_ptr);
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
    ///
    #[test]
    fn test_miri_tlab_invalidation_and_refill() {
        let heap = BexHeap::with_tlab_size(vec![], 10);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // Allocate some objects before GC
        let obj1 = tlab.alloc_string("before_gc_1".to_string());
        let obj2 = tlab.alloc_string("before_gc_2".to_string());

        assert!(tlab.is_valid());

        let (stats, _remapped, _forwarding) = unsafe { heap.collect_garbage(&[obj1, obj2]) };
        assert_eq!(stats.live_count, 2);

        // Invalidate TLAB (what bex_engine does after GC)
        tlab.invalidate();

        assert!(!tlab.is_valid());
        assert_eq!(tlab.remaining(), 0);

        // Continue allocating - TLAB should refill from new space
        let obj3 = tlab.alloc_string("after_gc_1".to_string());
        let obj4 = tlab.alloc_string("after_gc_2".to_string());

        assert!(tlab.is_valid());

        // Verify new objects are accessible
        unsafe {
            match obj3.get() {
                Object::String(s) => assert_eq!(s, "after_gc_1"),
                _ => panic!("Expected String"),
            }
            match obj4.get() {
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
        let heap = BexHeap::with_tlab_size(vec![], 100);
        let mut tlab = Tlab::new(heap);

        // Allocate an object
        let ptr = tlab.alloc(Object::String(bex_str::BexStr::from("original")));

        // Verify original value
        unsafe {
            match tlab.get_object(ptr) {
                Object::String(s) => assert_eq!(s, "original"),
                _ => panic!("Expected String"),
            }
        }

        // Mutate the object using set_object
        unsafe {
            tlab.set_object(ptr, Object::String(bex_str::BexStr::from("mutated")));
        }

        // Verify mutation
        unsafe {
            match tlab.get_object(ptr) {
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
    /// This test previously failed under Miri due to a data race between
    /// TLAB writes and Vec resizing. The fix: replace Vec with ChunkedVec,
    /// which never moves existing data when growing.
    #[test]
    fn test_miri_concurrent_tlab_allocation() {
        use std::thread;

        let heap = BexHeap::with_tlab_size(vec![], 100);

        // Spawn threads that each get their own TLAB and allocate
        let handles: Vec<_> = (0..4)
            .map(|thread_id| {
                let heap = Arc::clone(&heap);
                thread::spawn(move || {
                    let mut tlab = Tlab::new(heap);

                    // Each thread allocates multiple objects
                    let mut pointers = Vec::new();
                    for i in 0..10 {
                        let ptr = tlab.alloc(Object::String(bex_str::BexStr::from(format!(
                            "thread_{thread_id}_obj_{i}"
                        ))));
                        pointers.push(ptr);
                    }

                    // Verify all objects are readable
                    for (i, ptr) in pointers.iter().enumerate() {
                        unsafe {
                            match tlab.get_object(*ptr) {
                                Object::String(s) => {
                                    assert_eq!(s, &format!("thread_{thread_id}_obj_{i}"));
                                }
                                _ => panic!("Expected String"),
                            }
                        }
                    }

                    pointers
                })
            })
            .collect();

        // Collect all pointers from all threads
        let all_pointers: Vec<Vec<HeapPtr>> =
            handles.into_iter().map(|h| h.join().unwrap()).collect();

        // Verify no overlapping pointers between threads
        let mut seen = std::collections::HashSet::new();
        for thread_pointers in &all_pointers {
            for ptr in thread_pointers {
                assert!(
                    seen.insert(ptr.as_ptr() as usize),
                    "Duplicate pointer {:?} allocated by multiple threads",
                    ptr.as_ptr()
                );
            }
        }

        // Verify all objects are still accessible from the heap
        for (thread_id, thread_pointers) in all_pointers.iter().enumerate() {
            for (i, ptr) in thread_pointers.iter().enumerate() {
                unsafe {
                    match heap.get_object(*ptr) {
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
        let heap = BexHeap::with_tlab_size(vec![], 5);

        let handles: Vec<_> = (0..3)
            .map(|thread_id| {
                let heap = Arc::clone(&heap);
                thread::spawn(move || {
                    let mut tlab = Tlab::new(heap);

                    // Allocate more objects than fit in one TLAB chunk
                    // to force multiple refills
                    let mut pointers = Vec::new();
                    for i in 0..20 {
                        let ptr = tlab.alloc(Object::String(bex_str::BexStr::from(format!(
                            "t{thread_id}_o{i}"
                        ))));
                        pointers.push(ptr);
                    }

                    pointers
                })
            })
            .collect();

        let all_pointers: Vec<Vec<HeapPtr>> =
            handles.into_iter().map(|h| h.join().unwrap()).collect();

        // Verify no overlaps - each pointer should be unique across all threads.
        let mut seen = std::collections::HashSet::new();
        for thread_pointers in &all_pointers {
            for ptr in thread_pointers {
                assert!(
                    seen.insert(*ptr),
                    "Duplicate pointer from concurrent refill"
                );
            }
        }

        // Verify all 60 objects (3 threads × 20 objects) are accessible
        assert_eq!(seen.len(), 60);
    }
}
