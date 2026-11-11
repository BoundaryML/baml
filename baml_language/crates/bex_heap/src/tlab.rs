//! Per-mutator allocation: each mutator (a VM, the future manager) bumps
//! cells into a block of its own, so the common path takes no lock and
//! touches no shared state but the collection epoch.
//!
//! Every cell spends its size of the GC budget, and what each object keeps
//! alive outside its cell is measured as it is allocated. Both go into a
//! local balance, settled with the budget in quanta, whenever the allocator
//! takes a new block, when the owner asks, and when the allocator is dropped.

use std::{
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use std::collections::HashMap;

use bex_vm_types::{
    AllocDebt, Class, HeapPtr, InstanceRef, ListRef, Meter, Object, RealizedTy, TypeHead, Value,
    layout::{ElemKind, SlotKind},
    object_format::{Access, SlotValue},
    types::{
        BackingAlloc, LIST_BYTES, MAP_BYTES, MapRef, TypeValue, Variant, backing_bytes,
        backing_descriptor, init_backing, list_descriptor, map_descriptor,
    },
};
use indexmap::IndexMap;

use crate::{
    BexHeap,
    descriptor::{
        CellHeader, DescriptorId, HEADER_BYTES, Header, OBJECT_CELL_BYTES, OBJECT_DESC,
        RUST_PAYLOAD_OFFSET,
    },
    space::{
        GRANULE_BYTES, LARGE_OBJECT_THRESHOLD, block::BlockHeader, blocks::Bump, granule_align,
        region_base,
    },
};

/// Payload bytes an allocator may hold unsettled before it settles with the
/// shared budget. Settling is one shared atomic update, so this bounds both
/// how often that happens and how far spending can lag behind allocation.
pub const SETTLE_QUANTUM: isize = 64 * 1024;

/// A mutator's allocation buffer.
///
/// Small cells are bumped into the current block (see
/// [`crate::space::blocks`]); large cells are allocated individually.
///
/// The current block is from-space at a collection like any other: its
/// survivors are copied out and, with no pin in it, it is freed and may be
/// handed to another allocator. So the block is valid only under the
/// collection epoch it was taken in (`block_epoch`); the fast path compares
/// that with the heap's and the slow path takes a fresh block when it is
/// stale. Nothing has to be told that a collection ran.
pub struct Tlab {
    bump: Bump,
    /// The heap's full-collection count when `bump`'s block was taken.
    block_epoch: usize,

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

    /// Type objects this allocator made for list element types and generic
    /// instances, by type, so containers of one type share one. Valid while
    /// no collection has run since `types_epoch`: a collection may have freed
    /// any of them.
    types: HashMap<RealizedTy, HeapPtr>,
    types_epoch: usize,
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
        tlab.refill(tlab.heap.gc_policy.full_collections());
        tlab
    }

    /// Create an allocator that takes its first block on first allocation.
    pub fn new_empty(heap: Arc<BexHeap>) -> Self {
        Self {
            bump: Bump::default(),
            block_epoch: heap.gc_policy.full_collections(),
            debt: AllocDebt::new(),
            debt_epoch: heap.gc_budget().full_collections,
            budget_crossed: false,
            types: HashMap::new(),
            types_epoch: heap.gc_budget().full_collections,
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

        let cell = self.alloc_cell(OBJECT_DESC, OBJECT_CELL_BYTES, true);
        let payload = (cell + RUST_PAYLOAD_OFFSET) as *mut Object;
        // SAFETY: a fresh cell of this allocator's, sized and aligned for an
        // `Object` after its header.
        unsafe { payload.write(obj) };

        #[cfg(feature = "gc_profiling")]
        self.heap.record_alloc();

        // SAFETY: the pointer names the object just written.
        unsafe { self.heap.make_heap_ptr(payload) }
    }

    /// Allocate a zeroed cell of `bytes` laid out by `descriptor`, with its
    /// header written. Returns the cell's address. The caller fills it in
    /// before the next safepoint.
    ///
    /// # Panics
    /// If `bytes` is smaller than the descriptor's size.
    pub fn alloc_zeroed(&mut self, descriptor: DescriptorId, bytes: usize) -> NonNull<u8> {
        let entry = self.heap.descriptors().get(descriptor);
        let size = entry.size as usize;
        assert!(
            bytes >= size,
            "a cell of {bytes} bytes for a {size}-byte descriptor"
        );
        let hooked = matches!(entry.trace, crate::Trace::Rust(_));
        let cell_bytes = granule_align(bytes);
        let cell = self.alloc_cell(descriptor, cell_bytes, hooked);
        self.spend(cell_bytes);
        // SAFETY: a fresh cell of `cell_bytes`; the header stays intact.
        unsafe { ((cell + HEADER_BYTES) as *mut u8).write_bytes(0, cell_bytes - HEADER_BYTES) };
        NonNull::new(cell as *mut u8).expect("cells are never at address zero")
    }

    /// A zeroed instance of `class` laid out by `descriptor` in `bytes`, with
    /// only its class slot written, for compiled code that stores every
    /// field itself before its next safepoint (a zeroed tagged field reads
    /// as held until then). `descriptor` and `bytes` are the class's
    /// `InstanceLayout`'s, for a class without type arguments.
    #[inline]
    pub fn alloc_instance_unfilled(
        &mut self,
        descriptor: DescriptorId,
        bytes: usize,
        class: HeapPtr,
    ) -> HeapPtr {
        debug_assert_eq!(
            self.heap.descriptors().get(descriptor).size as usize,
            bytes,
            "an instance cell is its layout's size"
        );
        let cell_bytes = granule_align(bytes);
        let cell = self.alloc_cell(descriptor, cell_bytes, false);
        self.spend(cell_bytes);
        // SAFETY: a fresh cell of `cell_bytes`; the header stays intact, and
        // the class slot lies inside the instance.
        unsafe {
            ((cell + HEADER_BYTES) as *mut u8).write_bytes(0, cell_bytes - HEADER_BYTES);
            let slot = (cell + bex_vm_types::types::CLASS_OFFSET as usize) as *const AtomicU64;
            (*slot).store(Value::object(class).bits(), Ordering::Relaxed);
        }
        self.payload_ptr(NonNull::new(cell as *mut u8).expect("cells are never at address zero"))
    }

    /// Place a cell for `bytes` (granule-aligned) and write its header.
    /// `hooked`: the descriptor traces a Rust value, which the sweep must
    /// drop and measure. Returns the cell.
    #[inline]
    fn alloc_cell(&mut self, descriptor: DescriptorId, bytes: usize, hooked: bool) -> usize {
        debug_assert!(bytes.is_multiple_of(GRANULE_BYTES));
        if bytes > LARGE_OBJECT_THRESHOLD {
            return self.alloc_large(descriptor, bytes);
        }
        let epoch = self.heap.gc_policy.full_collections();
        let cell = if epoch == self.block_epoch
            && let Some(cell) = self.bump.try_alloc(bytes)
        {
            cell
        } else {
            self.alloc_small_slow(bytes, epoch)
        };
        let block = region_base(cell);
        // SAFETY: a cell this allocator just bumped in a block of its own.
        unsafe {
            let header = BlockHeader::at(block);
            let granule = BlockHeader::granule(block, cell);
            header.starts.set(granule);
            if hooked {
                header.hooked.set(granule);
            }
            (cell as *mut Header).write(Header::for_cell(descriptor, bytes, false));
        }
        cell
    }

    /// The current block is full, or a collection ran since it was taken:
    /// take a fresh one. Taking a block is when the balance settles.
    #[cold]
    #[inline(never)]
    fn alloc_small_slow(&mut self, bytes: usize, epoch: usize) -> usize {
        self.refill(epoch);
        self.flush_alloc_debt();
        self.bump
            .try_alloc(bytes)
            .expect("an empty block holds any small cell")
    }

    /// Take a fresh block under `epoch`, the heap's current collection
    /// count. The old block is left to the next collection: full, or from
    /// a stale epoch and so no longer this allocator's.
    fn refill(&mut self, epoch: usize) {
        self.bump.refill(&mut self.heap.blocks.lock());
        self.block_epoch = epoch;
    }

    /// Whether the current block was taken since the latest collection.
    #[inline]
    fn block_is_current(&self) -> bool {
        self.block_epoch == self.heap.gc_policy.full_collections()
    }

    /// A cell above the large-object threshold, allocated on its own. The
    /// caller spends it.
    fn alloc_large(&mut self, descriptor: DescriptorId, cell_bytes: usize) -> usize {
        let cell = self.heap.large.lock().alloc(cell_bytes);
        // SAFETY: a fresh large cell of `cell_bytes`.
        unsafe { (cell as *mut Header).write(Header::for_cell(descriptor, cell_bytes, true)) };
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
        let finalizable = crate::Finalizable::of(&obj);
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
            .note_rewrite(crate::Finalizable::Neither, finalizable);
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

    /// Allocate a list whose elements have static type `element_ty`.
    pub fn alloc_array(&mut self, element_ty: RealizedTy, values: Vec<Value>) -> HeapPtr {
        let values: Vec<SlotValue> = values.into_iter().map(SlotValue::Value).collect();
        self.alloc_list_of(element_ty, &values).ptr()
    }

    /// Allocate a string-keyed map whose keys/values have static types
    /// `key_ty`/`value_ty`.
    pub fn alloc_map(
        &mut self,
        key_ty: RealizedTy,
        value_ty: RealizedTy,
        values: IndexMap<bex_str::BexStr, Value>,
    ) -> HeapPtr {
        let entries: Vec<(u64, Value, Value)> = values
            .into_iter()
            .map(|(key, value)| {
                let hash = bex_vm_types::map_string_hash(&key);
                (hash, Value::object(self.alloc_string(key)), value)
            })
            .collect();
        self.alloc_map_hashed(key_ty, value_ty, entries).ptr()
    }

    /// Allocate an empty map of `key_ty` to `value_ty`.
    pub fn alloc_empty_map(&mut self, key_ty: RealizedTy, value_ty: RealizedTy) -> MapRef {
        let (key, value) = (SlotKind::of_type(&key_ty), SlotKind::of_type(&value_ty));
        let ty = self.type_object(RealizedTy::Map {
            key: Box::new(key_ty),
            value: Box::new(value_ty),
        });
        let cell = self.alloc_zeroed(map_descriptor(key, value), MAP_BYTES as usize);
        // SAFETY: the payload of a fresh map cell; the type object was just
        // made.
        unsafe { MapRef::init(self.payload_ptr(cell), ty) }
    }

    /// Allocate a map holding `entries`: distinct keys, each with the hash
    /// the interpreter's hashing gives it.
    pub fn alloc_map_hashed(
        &mut self,
        key_ty: RealizedTy,
        value_ty: RealizedTy,
        entries: Vec<(u64, Value, Value)>,
    ) -> MapRef {
        let map = self.alloc_empty_map(key_ty, value_ty);
        let guard = map.lock(Access::ThreadLocal);
        for (hash, key, value) in entries {
            guard
                .insert(self, hash, SlotValue::Value(key), SlotValue::Value(value))
                .unwrap_or_else(|error| panic!("a map entry does not fit its slots: {error}"));
        }
        map
    }

    /// Allocate a non-generic instance object (empty class type args).
    #[inline]
    pub fn alloc_instance(&mut self, class: HeapPtr, fields: Vec<Value>) -> HeapPtr {
        self.alloc_instance_with_type_args(class, Box::new([]), fields)
    }

    /// Allocate an instance carrying its concrete class type args (De Bruijn
    /// order), its fields stored in declaration order. Fields past the end of
    /// `fields` stay null.
    pub fn alloc_instance_with_type_args(
        &mut self,
        class: HeapPtr,
        type_args: Box<[RealizedTy]>,
        fields: Vec<Value>,
    ) -> HeapPtr {
        let instance = self.alloc_instance_cell(class, type_args);
        for (idx, value) in fields.into_iter().enumerate() {
            instance.store_with(idx, value, Access::ThreadLocal);
        }
        instance.ptr()
    }

    /// Allocate an instance of `class` whose fields are all null (zero), for
    /// the caller to store before it publishes the instance. The caller runs
    /// no user code before every field is stored, so the instance is
    /// registered for finalization at once.
    pub fn alloc_instance_cell(
        &mut self,
        class: HeapPtr,
        type_args: Box<[RealizedTy]>,
    ) -> InstanceRef {
        let instance = self.alloc_unsealed_instance_cell(class, type_args);
        self.seal_instance(instance);
        instance
    }

    /// [`Self::alloc_instance_cell`] for a construction that runs user code
    /// between field stores (a spread literal's `AllocInstance` and
    /// `InitField`/`InitSpread`), so it can throw with fields unset. The
    /// instance is not registered for finalization until
    /// [`Self::seal_instance`]: a `cleanup` never sees a half-built instance.
    pub fn alloc_unsealed_instance_cell(
        &mut self,
        class: HeapPtr,
        type_args: Box<[RealizedTy]>,
    ) -> InstanceRef {
        // SAFETY: the allocating mutator holds a heap permit, so the class is
        // live.
        let Object::Class(class_def) = (unsafe { class.get() }) else {
            panic!("an instance of a non-class object {class:?}");
        };
        let layout = self.heap.instance_layout(class_def);
        let type_object = layout
            .has_type_args()
            .then(|| self.alloc_type_args(class, class_def, type_args));
        let cell = self.alloc_zeroed(layout.descriptor(), layout.size());
        let ptr = self.payload_ptr(cell);
        // SAFETY: a fresh cell of the class's layout; the type object exists
        // exactly when the layout has type arguments.
        unsafe { InstanceRef::init(ptr, class, type_object) }
    }

    /// An instance's construction is complete: BEP-042, an instance of a
    /// class with a finalizer joins the collector's finalizable list.
    pub fn seal_instance(&self, instance: InstanceRef) {
        if BexHeap::needs_finalizer_registration(instance) {
            self.heap.register_finalizable(instance.ptr());
        }
    }

    /// The type object a generic instance points to: its class applied to
    /// `type_args`.
    fn alloc_type_args(
        &mut self,
        class: HeapPtr,
        class_def: &Class,
        type_args: Box<[RealizedTy]>,
    ) -> HeapPtr {
        debug_assert_eq!(type_args.len(), class_def.generic_param_count);
        let head = TypeHead::new(class, class_def.type_tag);
        self.type_object(RealizedTy::Class(head, type_args))
    }

    /// The `Object::Type` holding `ty`, shared by every container this
    /// allocator makes of that type until the next collection.
    pub fn type_object(&mut self, ty: RealizedTy) -> HeapPtr {
        let epoch = self.heap.gc_policy.full_collections();
        if epoch != self.types_epoch {
            self.types.clear();
            self.types_epoch = epoch;
        }
        if let Some(&ptr) = self.types.get(&ty) {
            return ptr;
        }
        let ptr = self.alloc_type(TypeValue::new(ty.clone()));
        self.types.insert(ty, ptr);
        ptr
    }

    /// Allocate an empty list of `element_ty`.
    pub fn alloc_list(&mut self, element_ty: RealizedTy) -> ListRef {
        let slot = SlotKind::of_type(&element_ty);
        let elem_ty = self.type_object(element_ty);
        let cell = self.alloc_zeroed(list_descriptor(slot), LIST_BYTES as usize);
        // SAFETY: the payload of a fresh list cell; the type object was just
        // made.
        unsafe { ListRef::init(self.payload_ptr(cell), elem_ty) }
    }

    /// Allocate a list of `element_ty` holding `values`, floats unboxed.
    pub fn alloc_list_of(&mut self, element_ty: RealizedTy, values: &[SlotValue]) -> ListRef {
        let list = self.alloc_list(element_ty);
        list.lock(Access::ThreadLocal)
            .replace_all(self, values)
            .unwrap_or_else(|error| panic!("a list element does not fit its slot: {error}"));
        list
    }

    /// The pointer to the object in `cell`, just after its header.
    fn payload_ptr(&self, cell: NonNull<u8>) -> HeapPtr {
        // SAFETY: the payload of a fresh cell of this heap.
        unsafe {
            self.heap
                .make_heap_ptr(cell.as_ptr().add(HEADER_BYTES).cast::<Object>())
        }
    }

    /// A copy of `source`, a fresh instance of its class with the same type
    /// arguments and field slots. Floats are copied unboxed.
    pub fn alloc_instance_copy(&mut self, source: InstanceRef) -> InstanceRef {
        let copy = self.alloc_instance_cell(source.class(), source.type_args().into());
        for idx in 0..source.field_len() {
            copy.store_slot_value(idx, source.load(idx), Access::ThreadLocal);
        }
        copy
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
    /// Bytes the current block can still take; zero when a collection ran
    /// since it was taken, so the next allocation takes a fresh block.
    pub fn remaining(&self) -> usize {
        if self.block_is_current() {
            self.bump.remaining()
        } else {
            0
        }
    }

    /// Get a reference to the heap.
    pub fn heap(&self) -> &Arc<BexHeap> {
        &self.heap
    }

    /// A collection ran. The block is handled by the epoch check on the
    /// allocation path; only the balance needs resetting here.
    pub fn invalidate(&mut self) {
        // A full collection counted everything this balance covers.
        let full_collections = self.heap.gc_policy.full_collections();
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

    /// Whether the current block has room for an `Object` under the current
    /// epoch.
    pub fn is_valid(&self) -> bool {
        self.remaining() >= OBJECT_CELL_BYTES
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
        // A block from a stale epoch was freed or retained by a collection
        // and is no longer this allocator's to give back.
        if self.block_is_current() {
            self.bump.release(&mut self.heap.blocks.lock());
        }
        // What this allocator spent must not vanish with it.
        self.flush_alloc_debt();
    }
}

impl std::fmt::Debug for Tlab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tlab").field("bump", &self.bump).finish()
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

impl BackingAlloc for Tlab {
    fn alloc_backing(&mut self, elem: ElemKind, cap: usize) -> HeapPtr {
        let cell = self.alloc_zeroed(backing_descriptor(elem), backing_bytes(elem, cap));
        let ptr = self.payload_ptr(cell);
        // SAFETY: a fresh backing cell of room for `cap` elements.
        unsafe { init_backing(ptr, cap, elem) };
        ptr
    }

    fn retire_backing(&mut self, bytes: usize) {
        // The cell's size as `alloc_cell` took it.
        self.spend(granule_align(bytes));
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

    fn alloc_instance_with_type_args(
        &mut self,
        class: HeapPtr,
        type_args: Box<[RealizedTy]>,
        fields: Vec<Value>,
    ) -> HeapPtr {
        self.tlab_mut()
            .alloc_instance_with_type_args(class, type_args, fields)
    }

    fn alloc_variant(&mut self, enm: HeapPtr, index: usize) -> HeapPtr {
        self.tlab_mut().alloc_variant(enm, index)
    }

    /// Allocate a map holding `entries` (see [`Tlab::alloc_map_hashed`]).
    fn alloc_map_hashed(
        &mut self,
        key_ty: RealizedTy,
        value_ty: RealizedTy,
        entries: Vec<(u64, Value, Value)>,
    ) -> HeapPtr {
        self.tlab_mut()
            .alloc_map_hashed(key_ty, value_ty, entries)
            .ptr()
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

        let list = unsafe { ListRef::of(ptr) }.expect("a list");
        assert_eq!(list.len(), 3);
        assert_eq!(list.get(0), Some(SlotValue::Value(Value::int(1))));
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
            let m = MapRef::of(ptr).expect("a map");
            let entries = m.snapshot_entries();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].1, SlotValue::Value(Value::int(42)));
            assert!(
                matches!(entries[0].0.as_object_ptr().unwrap().get(), Object::String(key) if key == "key")
            );
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
            layout: std::sync::OnceLock::default(),
            redact: false,
        })));

        // Allocate an instance of that class
        let fields = vec![Value::int(10), Value::int(20)];
        let instance_ptr = tlab.alloc_instance(class_ptr, fields);

        let inst = unsafe { InstanceRef::of(instance_ptr) }.expect("an instance");
        assert_eq!(inst.class(), class_ptr);
        assert_eq!(inst.field_len(), 2);
        assert_eq!(inst.load(0).expect_value(), Value::int(10));
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

    /// A collection makes the allocator's block stale: the survivors are
    /// copied out, the block is freed, and the next allocation goes into a
    /// fresh block.
    #[test]
    fn test_miri_tlab_takes_a_fresh_block_after_a_collection() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let obj1 = tlab.alloc_string("before_gc_1".to_string());
        let obj2 = tlab.alloc_string("before_gc_2".to_string());
        let block = crate::space::region_base(obj1.as_ptr() as usize);

        let (stats, roots, _) = unsafe { heap.collect_garbage(&[obj1, obj2]) };
        assert_eq!(stats.live_count, 2);

        // What bex_engine does after GC.
        tlab.invalidate();
        assert!(!tlab.is_valid(), "the block is from a stale epoch");
        assert_eq!(tlab.remaining(), 0);

        let obj3 = tlab.alloc_string("after_gc_1".to_string());
        assert!(tlab.is_valid());
        assert_ne!(roots[0], obj1, "the survivor moved out of the block");
        assert_ne!(
            crate::space::region_base(roots[0].as_ptr() as usize),
            block,
            "into a to-space block"
        );
        assert!(matches!(unsafe { roots[0].get() }, Object::String(s) if s == "before_gc_1"));
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
