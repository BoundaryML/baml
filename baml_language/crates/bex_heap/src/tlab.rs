//! Per-mutator allocation: each mutator (a VM, the future manager) owns one
//! block per size class and takes cells from it, so the common path takes no
//! lock and touches no shared state.
//!
//! Every cell spends its size of the GC budget, and what each object keeps
//! alive outside its cell is measured as it is allocated. Both go into a
//! local balance, settled with the budget in quanta, whenever the allocator
//! takes a new block, when the owner asks, and when the allocator is dropped.

use std::{ptr::NonNull, sync::Arc};

use bex_vm_types::{
    AllocDebt, HeapPtr, Meter, Object, Value,
    types::{Array, Instance, Map, Variant},
};
use indexmap::IndexMap;

use crate::{
    BexHeap,
    descriptor::{
        DescriptorId, HEADER_BYTES, Header, OBJECT_CELL_BYTES, OBJECT_DESC, RUST_PAYLOAD_OFFSET,
    },
    space::{
        GRANULE_BYTES, LARGE_OBJECT_THRESHOLD,
        block::BlockHeader,
        granule_align, region_base,
        small::{SmallAllocator, cell_size},
    },
};

/// Payload bytes an allocator may hold unsettled before it settles with the
/// shared budget. Settling is one shared atomic update, so this bounds both
/// how often that happens and how far spending can lag behind allocation.
pub const SETTLE_QUANTUM: isize = 64 * 1024;

/// A mutator's allocation buffer.
///
/// Small cells come from the current block of their size class (see
/// [`crate::space::small`]); large cells are allocated individually.
///
/// Objects never move, so a collection leaves the buffer valid: the owned
/// blocks stay owned, their free lists rebuilt by the sweep.
pub struct Tlab {
    small: SmallAllocator,

    /// Reference to the shared heap.
    heap: Arc<BexHeap>,

    /// Payload bytes allocated (less released) since the last settlement.
    debt: AllocDebt,

    /// The heap's full-collection count when `debt` last started from zero.
    /// A full collection takes a census of everything, so a balance that
    /// predates one is already counted and must be discarded, not settled.
    debt_epoch: usize,

    /// Set when a settlement by this TLAB took spending over the budget, until
    /// the owner reads it with [`Self::take_budget_crossed`].
    budget_crossed: bool,
}

impl Tlab {
    /// Create an allocator and take its first block now.
    ///
    /// **Avoid in production code paths.** Taking a block here happens
    /// outside any [`HeapPermitManager`](crate::HeapPermitManager) permit.
    /// Permit-managed holders (`BexVm`, `FutureManagerInner`, …) must use
    /// [`Tlab::new_empty`] so the first block is taken under a held permit.
    /// `Tlab::new` is retained for standalone heap tests.
    pub fn new(heap: Arc<BexHeap>) -> Self {
        let mut tlab = Self::new_empty(heap);
        tlab.small.prepare(&tlab.heap.small, OBJECT_CELL_BYTES);
        tlab
    }

    /// Create an allocator that takes its first block on first allocation.
    pub fn new_empty(heap: Arc<BexHeap>) -> Self {
        Self {
            small: SmallAllocator::default(),
            debt: AllocDebt::new(),
            debt_epoch: heap.gc_budget().full_collections,
            budget_crossed: false,
            heap,
        }
    }

    /// Charge `obj`'s footprint, its cell included, to this allocator's
    /// account.
    fn charge(&mut self, obj: &mut Object) {
        let mut charge = Meter::charge();
        obj.measure(&mut charge);
        self.spend(OBJECT_CELL_BYTES + charge.total());
    }

    /// Add `bytes` to the balance, settling it once it reaches a quantum.
    #[inline]
    fn spend(&mut self, bytes: usize) {
        self.debt.grow(bytes);
        if self.debt.balance().unsigned_abs() >= SETTLE_QUANTUM.unsigned_abs() {
            self.flush_alloc_debt();
        }
    }

    /// Allocate an object, returning a HeapPtr to it.
    #[inline]
    pub fn alloc(&mut self, mut obj: Object) -> HeapPtr {
        self.charge(&mut obj);

        // BEP-042: an instance of a class with a finalizer joins the
        // collector's finalizable list. Checked before the write moves `obj`.
        // SAFETY: the allocating VM holds a heap permit, so an instance's
        // class object is live.
        let finalizable = unsafe { BexHeap::needs_finalizer_registration(&obj) };

        let (cell, _) = self.alloc_cell(OBJECT_DESC, OBJECT_CELL_BYTES);
        let payload = (cell + RUST_PAYLOAD_OFFSET) as *mut Object;
        // SAFETY: a fresh cell of this allocator's, sized and aligned for an
        // `Object` after its header.
        unsafe { payload.write(obj) };

        #[cfg(feature = "gc_profiling")]
        self.heap.record_alloc();

        // SAFETY: the pointer names the object just written.
        let ptr = unsafe { self.heap.make_heap_ptr(payload) };
        if finalizable {
            self.heap.register_finalizable(ptr);
        }
        ptr
    }

    /// Allocate a zeroed cell of `bytes` laid out by `descriptor`, with its
    /// header written. Returns the cell's address. The caller fills it in
    /// before the next safepoint.
    ///
    /// # Panics
    /// If `bytes` is smaller than the descriptor's size.
    pub fn alloc_zeroed(&mut self, descriptor: DescriptorId, bytes: usize) -> NonNull<u8> {
        let size = self.heap.descriptors().get(descriptor).size as usize;
        assert!(
            bytes >= size,
            "a cell of {bytes} bytes for a {size}-byte descriptor"
        );
        let (cell, cell_bytes) = self.alloc_cell(descriptor, granule_align(bytes));
        self.spend(cell_bytes);
        // SAFETY: a fresh cell of `cell_bytes`; the header stays intact.
        unsafe { ((cell + HEADER_BYTES) as *mut u8).write_bytes(0, cell_bytes - HEADER_BYTES) };
        NonNull::new(cell as *mut u8).expect("cells are never at address zero")
    }

    /// Place a cell for `bytes` (granule-aligned) and write its header.
    /// Returns the cell and its size, `bytes` rounded to its size class.
    #[inline]
    fn alloc_cell(&mut self, descriptor: DescriptorId, bytes: usize) -> (usize, usize) {
        debug_assert!(bytes.is_multiple_of(GRANULE_BYTES));
        if bytes > LARGE_OBJECT_THRESHOLD {
            return (self.alloc_large(descriptor, bytes), bytes);
        }
        let cell_bytes = cell_size(bytes);
        let cell = match self.small.try_alloc(cell_bytes) {
            Some(cell) => cell,
            None => self.alloc_small_slow(cell_bytes),
        };
        let block = region_base(cell);
        // SAFETY: a cell this allocator just took from a block it owns.
        unsafe {
            BlockHeader::at(block)
                .starts
                .set(BlockHeader::granule(block, cell));
            (cell as *mut Header).write(Header::new(descriptor, cell_bytes, false));
        }
        (cell, cell_bytes)
    }

    /// The class's current block is full: the size-class policy finds
    /// another. Taking a block is when the balance settles.
    #[cold]
    #[inline(never)]
    fn alloc_small_slow(&mut self, cell_bytes: usize) -> usize {
        let cell = self.small.alloc_slow(&self.heap.small, cell_bytes);
        self.flush_alloc_debt();
        cell
    }

    /// A cell above the large-object threshold, allocated on its own. The
    /// caller spends it.
    fn alloc_large(&mut self, descriptor: DescriptorId, cell_bytes: usize) -> usize {
        let cell = self.heap.large.lock().alloc(cell_bytes);
        // SAFETY: a fresh large cell of `cell_bytes`.
        unsafe { (cell as *mut Header).write(Header::new(descriptor, cell_bytes, true)) };
        cell
    }

    /// Assign a fresh telemetry identity without registering a weak reference.
    /// Call-path creation registers the completed definition on first observation.
    /// Like allocation, this requires the caller to exclude GC.
    pub fn alloc_function(
        &mut self,
        mut function: Box<bex_vm_types::Function>,
    ) -> Result<HeapPtr, btel_types::FunctionIdExhausted> {
        function.telemetry_function_id = Some(self.heap.functions.allocate_id()?);
        function.telemetry_registration = btel_types::FunctionRegistration::default();
        Ok(self.alloc(Object::Function(function)))
    }

    /// Reserve a slot: an address with no object behind it yet. See
    /// [`Reservation`].
    pub fn reserve(&mut self) -> Reservation {
        Reservation {
            ptr: self.alloc(Object::Tombstone),
        }
    }

    /// Fill a reserved slot with `obj`, charged like an allocation. Returns
    /// the address the reservation promised.
    ///
    /// # Safety
    ///
    /// No collection may have run since `reservation` was made: a collection
    /// frees or reuses the nursery slot it names. Holding the heap permit from
    /// `reserve` to `publish`, with no safepoint in between, guarantees that.
    /// `reservation` must come from a TLAB of this heap.
    pub unsafe fn publish(&mut self, reservation: Reservation, mut obj: Object) -> HeapPtr {
        self.charge(&mut obj);
        // SAFETY: the publishing VM holds a heap permit, so an instance's
        // class object is live.
        let finalizable = unsafe { crate::Finalizable::of(&obj) };
        let ptr = reservation.ptr;
        // SAFETY: the caller guarantees the slot is still the one reserved. It
        // holds a tombstone, which nothing reads: the only references to it are
        // addresses, so no borrow of its contents exists while it is replaced.
        let slot = unsafe { ptr.get_mut() };
        debug_assert!(
            matches!(slot, Object::Tombstone),
            "a reserved slot is published once"
        );
        *slot = obj;
        // Publishing rewrites the tombstone in place, so the finalizable list
        // hears of it as of any other rewrite.
        self.heap
            .note_rewrite(ptr, crate::Finalizable::Neither, finalizable);
        ptr
    }

    /// [`Self::publish`] for a function, which takes its telemetry identity
    /// here just as [`Self::alloc_function`] gives it at allocation.
    ///
    /// # Safety
    ///
    /// As for [`Self::publish`].
    pub unsafe fn publish_function(
        &mut self,
        reservation: Reservation,
        mut function: Box<bex_vm_types::Function>,
    ) -> Result<HeapPtr, btel_types::FunctionIdExhausted> {
        function.telemetry_function_id = Some(self.heap.functions.allocate_id()?);
        function.telemetry_registration = btel_types::FunctionRegistration::default();
        // SAFETY: the caller upholds `publish`'s contract.
        Ok(unsafe { self.publish(reservation, Object::Function(function)) })
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
        let values =
            bex_vm_types::MapData::from_hashed_entries(values.into_iter().map(|(key, value)| {
                let hash = bex_vm_types::map_string_hash(&key);
                (hash, Value::object(self.alloc_string(key)), value)
            }));
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
    pub fn alloc_rust_data(&mut self, data: Arc<dyn bex_vm_types::BexRustData>) -> HeapPtr {
        self.alloc(Object::RustData(data))
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
}

impl Tlab {
    /// Bytes of `Object` cells the current block of their class can still
    /// take. Walks that block's free list.
    pub fn remaining(&self) -> usize {
        self.small.room_for(OBJECT_CELL_BYTES) * OBJECT_CELL_BYTES
    }

    /// Get a reference to the heap.
    pub fn heap(&self) -> &Arc<BexHeap> {
        &self.heap
    }

    /// A collection ran. Objects do not move, so the owned blocks stay
    /// valid; only the balance can be stale.
    pub fn invalidate(&mut self) {
        // A full collection counted everything this balance covers.
        let full_collections = self.heap.gc_budget().full_collections;
        if full_collections == self.debt_epoch {
            return;
        }
        self.debt.take();
        self.debt_epoch = full_collections;
        self.budget_crossed = false;
    }

    /// The local balance of payload bytes, for callers that charge growth or
    /// release against it.
    pub fn alloc_debt(&self) -> &AllocDebt {
        &self.debt
    }

    /// Settle the local balance with the shared budget now.
    pub fn flush_alloc_debt(&mut self) {
        let balance = self.debt.take();
        self.budget_crossed |= self.heap.gc_policy.adjust(balance);
    }

    /// Whether a settlement by this TLAB took spending over the budget since
    /// the owner last asked. The owner should reach a safe point promptly.
    pub fn take_budget_crossed(&mut self) -> bool {
        std::mem::take(&mut self.budget_crossed)
    }

    /// Whether the current block of the `Object` class has room for another.
    pub fn is_valid(&self) -> bool {
        self.small.room_for(OBJECT_CELL_BYTES) > 0
    }

    /// Read an object by HeapPtr.
    ///
    /// # Safety
    ///
    /// - The pointer must be valid (not collected by GC)
    /// - Caller must ensure no concurrent writes to this object
    pub unsafe fn get_object(&self, idx: HeapPtr) -> &Object {
        // SAFETY: Caller ensures no concurrent writes
        unsafe { self.heap.get_object(idx) }
    }
}

impl Drop for Tlab {
    fn drop(&mut self) {
        self.small.release(&mut self.heap.small.lock());
        // What this allocator spent must not vanish with it.
        self.flush_alloc_debt();
    }
}

impl std::fmt::Debug for Tlab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tlab").field("small", &self.small).finish()
    }
}

/// A heap slot with an address but no object yet.
///
/// For a group of objects that name each other: reserve a slot per object so
/// every address is known, build each object complete, then
/// [publish](Tlab::publish) it, with no safepoint in between (which is why
/// publishing is `unsafe`). Until then the slot holds an
/// [`Object::Tombstone`], which nothing may read — the address is for storing,
/// not following. A reservation dropped unpublished leaves its tombstone
/// behind. That is harmless while nothing reachable stores its address, and a
/// use-after-free otherwise, which the collector `debug_assert!`s against.
#[must_use = "a reserved slot holds a tombstone until it is published"]
pub struct Reservation {
    ptr: HeapPtr,
}

impl Reservation {
    /// The address the published object will have.
    #[inline]
    pub fn ptr(&self) -> HeapPtr {
        self.ptr
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

    fn alloc_rust_data(&mut self, data: Arc<dyn bex_vm_types::BexRustData>) -> HeapPtr {
        self.tlab_mut().alloc_rust_data(data)
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
    use crate::descriptor::OBJECT_CELL_BYTES;

    #[test]
    fn test_tlab_alloc_single() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(heap);
        let before = tlab.remaining();

        let _ptr = tlab.alloc(Object::String(bex_str::BexStr::from("hello")));
        assert_eq!(tlab.remaining(), before - OBJECT_CELL_BYTES);
    }

    #[test]
    fn test_tlab_alloc_multiple() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(heap);
        let before = tlab.remaining();

        for i in 0..10 {
            let _ptr = tlab.alloc(Object::String(bex_str::BexStr::from(format!("obj{i}"))));
        }
        assert_eq!(tlab.remaining(), before - 10 * OBJECT_CELL_BYTES);
    }

    /// Filling the block moves allocation to another block.
    #[test]
    fn test_tlab_refill() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));
        let first = tlab.alloc_float(0.0);
        while tlab.remaining() >= OBJECT_CELL_BYTES {
            tlab.alloc_float(1.0);
        }
        let next = tlab.alloc_float(2.0);
        assert_ne!(
            crate::space::region_base(first.as_ptr() as usize),
            crate::space::region_base(next.as_ptr() as usize),
            "a full block hands allocation to the next one"
        );
        assert!(matches!(unsafe { next.get() }, Object::Float(f) if *f == 2.0));
    }

    #[test]
    fn test_tlab_with_compile_time_objects() {
        let compile_time: Vec<Object> = vec![
            Object::String(bex_str::BexStr::from("builtin1")),
            Object::String(bex_str::BexStr::from("builtin2")),
        ];
        let heap = BexHeap::new(compile_time);
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
        let heap = BexHeap::new(vec![]);
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
        let heap = BexHeap::new(vec![]);
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
        let heap = BexHeap::new(vec![]);
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
        let heap = BexHeap::new(vec![]);
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
        let heap = BexHeap::new(vec![]);
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
                    let entries = m.snapshot_entries();
                    assert_eq!(entries.len(), 1);
                    assert_eq!(entries[0].1, Value::int(42));
                    assert!(
                        matches!(entries[0].0.as_object_ptr().unwrap().get(), Object::String(key) if key == "key")
                    );
                }
                _ => panic!("Expected Map"),
            }
        }
    }

    #[test]
    fn test_alloc_instance() {
        use bex_vm_types::types::Class;

        // First allocate a class object
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(heap);

        // Simulate a class at index 0
        let class_ptr = tlab.alloc(Object::Class(Box::new(Class {
            path: bex_vm_types::types::LocalName::new(
                Vec::new(),
                baml_type::Name::new("TestClass"),
            ),
            fields: vec![
                bex_vm_types::ClassField {
                    name: "x".to_string(),
                    field_type: baml_type::RuntimeTy::Int,
                    field_template: baml_type::TyTemplate::from(baml_type::RealizedTy::Int),
                    description: None,
                    alias: None,
                    docstring: None,
                    other: Default::default(),
                    skip: false,
                    stream_done: false,
                    must_exist: false,
                    runtime_type: None,
                    json_fallback: false,
                    json_flatten: false,
                    json_rename: None,
                    json_omit_null: false,
                    redact: false,
                },
                bex_vm_types::ClassField {
                    name: "y".to_string(),
                    field_type: baml_type::RuntimeTy::Int,
                    field_template: baml_type::TyTemplate::from(baml_type::RealizedTy::Int),
                    description: None,
                    alias: None,
                    docstring: None,
                    other: Default::default(),
                    skip: false,
                    stream_done: false,
                    must_exist: false,
                    runtime_type: None,
                    json_fallback: false,
                    json_flatten: false,
                    json_rename: None,
                    json_omit_null: false,
                    redact: false,
                },
            ],
            description: None,
            alias: None,
            docstring: None,
            other: Default::default(),
            stream_done: false,
            declared_crossing: baml_type::ClassCrossing::Fields,
            type_tag: baml_type::typetag::TypeTag::from_i64(100),
            cleanup: None,
            methods: IndexMap::new(),
            generic_param_count: 0,
            owner: bex_vm_types::types::Owner::anonymous(),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
            redact: false,
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

        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(heap);

        // Simulate an enum at index 0
        let enum_ptr = tlab.alloc(Object::Enum(Box::new(Enum {
            type_tag: baml_type::typetag::TypeTag::from_i64(200),
            path: bex_vm_types::types::LocalName::new(Vec::new(), baml_type::Name::new("Color")),
            variants: vec![
                bex_vm_types::EnumVariant {
                    name: "Red".to_string(),
                    description: None,
                    alias: None,
                    docstring: None,
                    other: Default::default(),
                    skip: false,
                    json_rename: None,
                },
                bex_vm_types::EnumVariant {
                    name: "Green".to_string(),
                    description: None,
                    alias: None,
                    docstring: None,
                    other: Default::default(),
                    skip: false,
                    json_rename: None,
                },
                bex_vm_types::EnumVariant {
                    name: "Blue".to_string(),
                    description: None,
                    alias: None,
                    docstring: None,
                    other: Default::default(),
                    skip: false,
                    json_rename: None,
                },
            ],
            description: None,
            alias: None,
            docstring: None,
            other: Default::default(),
            owner: bex_vm_types::types::Owner::anonymous(),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
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
    // ========================================================================

    /// A collection leaves the allocator's blocks valid: objects do not
    /// move, so allocation continues where it was.
    #[test]
    fn test_miri_tlab_survives_collection() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let obj1 = tlab.alloc_string("before_gc_1".to_string());
        let obj2 = tlab.alloc_string("before_gc_2".to_string());
        let remaining = tlab.remaining();

        let (stats, _, _) = unsafe { heap.collect_garbage(&[obj1, obj2]) };
        assert_eq!(stats.live_count, 2);

        // What bex_engine does after GC.
        tlab.invalidate();
        assert!(tlab.is_valid());
        assert_eq!(tlab.remaining(), remaining);

        let obj3 = tlab.alloc_string("after_gc_1".to_string());
        assert!(matches!(unsafe { obj1.get() }, Object::String(s) if s == "before_gc_1"));
        assert!(matches!(unsafe { obj3.get() }, Object::String(s) if s == "after_gc_1"));
    }

    /// A reservation's address is where its object is published, and the
    /// object is charged when it is published, not when the slot is reserved.
    #[test]
    fn publish_lands_at_the_reserved_address() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(heap);
        let reservation = tlab.reserve();
        let ptr = reservation.ptr();
        let reserved = tlab.alloc_debt().balance();
        // SAFETY: no collection runs between `reserve` and `publish`.
        let published = unsafe {
            tlab.publish(
                reservation,
                Object::String(bex_str::BexStr::from("x".repeat(100))),
            )
        };
        assert_eq!(published, ptr);
        assert!(tlab.alloc_debt().balance() > reserved);
        // SAFETY: just published; no collection has run.
        let Object::String(text) = (unsafe { ptr.get() }) else {
            panic!("expected the published string")
        };
        assert_eq!(text.len(), 100);
    }

    /// A reservation dropped unpublished is garbage like any other slot.
    #[test]
    fn an_unpublished_reservation_is_reclaimed() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));
        let live = tlab.alloc_string("live".to_string());
        drop(tlab.reserve());
        // SAFETY: single-threaded heap; `live` is the only root.
        let (stats, _, _) = unsafe { heap.collect_garbage(&[live]) };
        assert_eq!(stats.live_count, 1);
    }

    /// Reaching a tombstone is a use-after-free: the collector refuses to
    /// carry an unpublished reservation's slot across a collection.
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "the collector reached a tombstone")]
    fn collecting_a_reachable_reservation_panics() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));
        let reservation = tlab.reserve();
        // SAFETY: deliberately breaks `publish`'s protocol by rooting the
        // reserved address across a collection.
        let _ = unsafe { heap.collect_garbage(&[reservation.ptr()]) };
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

        let heap = BexHeap::new(vec![]);

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
        let heap = BexHeap::new(vec![]);

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
    /// Several threads allocate through their own TLABs while the pool
    /// carves more than one slab of blocks: taking a block is the only
    /// shared step, and every object must read back intact afterwards.
    #[test]
    fn concurrent_allocation_while_the_pool_grows() {
        const THREADS: usize = 4;
        const PER_THREAD: usize = if cfg!(miri) { 1_000 } else { 6_000 };
        let heap = BexHeap::new(vec![]);
        let threads: Vec<_> = (0..THREADS)
            .map(|thread| {
                let heap = Arc::clone(&heap);
                std::thread::spawn(move || {
                    let mut tlab = Tlab::new(heap);
                    (0..PER_THREAD)
                        .map(|i| {
                            let key = u32::try_from(thread * PER_THREAD + i).unwrap();
                            (key, tlab.alloc_float(f64::from(key)))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let allocated: Vec<(u32, HeapPtr)> = threads
            .into_iter()
            .flat_map(|thread| thread.join().expect("an allocating thread panicked"))
            .collect();

        if !cfg!(miri) {
            assert!(heap.stats().blocks > 32, "more than one slab was carved");
        }
        for (key, ptr) in allocated {
            // SAFETY: the allocating thread wrote this cell and has joined.
            match unsafe { ptr.get() } {
                Object::Float(value) => assert_eq!(*value, f64::from(key)),
                other => panic!("cell for {key} holds {other:?}"),
            }
        }
    }
}
