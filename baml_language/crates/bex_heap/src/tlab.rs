//! Per-mutator allocation: each mutator (a VM, the future manager) owns one
//! block per size class and takes cells from it, so the common path takes no
//! lock and touches no shared state.
//!
//! Every cell spends its size of the GC budget, and what each object keeps
//! alive outside its cell is measured as it is allocated. Both go into a
//! local balance, settled with the budget in quanta, whenever the allocator
//! takes a new block, when the owner asks, and when the allocator is dropped.

use std::{
    ops::Range,
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
        BackingAlloc, LIST_BYTES, MAP_BYTES, MapRef, ROPE_BYTES, ROPE_DESC, STR_DESC,
        STR_SHARE_THRESHOLD, StrRef, TypeValue, VIEW_BYTES, VIEW_DESC, Variant, backing_bytes,
        backing_descriptor, chars_in_bytes, init_backing, list_descriptor, map_descriptor,
        str_cell_bytes,
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

        let (cell, _) = self.alloc_cell(OBJECT_DESC, OBJECT_CELL_BYTES, true);
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
        let (cell, cell_bytes) = self.alloc_cell(descriptor, granule_align(bytes), hooked);
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
        let (cell, cell_bytes) = self.alloc_cell(descriptor, granule_align(bytes), false);
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
    /// drop and measure. Returns the cell and its size, `bytes` rounded to
    /// its size class.
    #[inline]
    fn alloc_cell(
        &mut self,
        descriptor: DescriptorId,
        bytes: usize,
        hooked: bool,
    ) -> (usize, usize) {
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
            let header = BlockHeader::at(block);
            let granule = BlockHeader::granule(block, cell);
            header.starts.set(granule);
            if hooked {
                header.hooked.set(granule);
            }
            (cell as *mut Header).write(Header::for_cell(descriptor, cell_bytes, false));
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

    /// Allocate a flat string cell holding a copy of `s`.
    #[inline]
    pub fn alloc_string(&mut self, s: impl AsRef<str>) -> HeapPtr {
        let text = s.as_ref();
        let bytes = str_cell_bytes(text.len());
        let (cell, cell_bytes) = self.alloc_cell(STR_DESC, bytes, false);
        self.spend(cell_bytes);
        let ptr = self
            .payload_ptr(NonNull::new(cell as *mut u8).expect("cells are never at address zero"));
        // SAFETY: a fresh cell sized for `text`, its header written.
        unsafe { StrRef::init(ptr, text) };
        #[cfg(feature = "gc_profiling")]
        self.heap.record_alloc();
        ptr
    }

    /// The payload of a fresh string cell of `descriptor` in `bytes`
    /// (granule-aligned), its header written, the rest unwritten.
    #[inline]
    fn alloc_string_cell(&mut self, descriptor: DescriptorId, bytes: usize) -> HeapPtr {
        let (cell, cell_bytes) = self.alloc_cell(descriptor, bytes, false);
        self.spend(cell_bytes);
        #[cfg(feature = "gc_profiling")]
        self.heap.record_alloc();
        self.payload_ptr(NonNull::new(cell as *mut u8).expect("cells are never at address zero"))
    }

    /// A flat holding a copy of `source`'s bytes, whatever its kind.
    fn alloc_flat_copy<'a>(&mut self, source: StrRef<'a>) -> StrRef<'a> {
        let ptr = self.alloc_string_cell(STR_DESC, str_cell_bytes(source.byte_len()));
        // SAFETY: a fresh flat cell sized for the source.
        unsafe { StrRef::init_flat_copy(ptr, source) }
    }

    /// The flat holding `string`'s bytes: itself, a view's base, or a
    /// rope's flattening, made now if nobody has published one. Flattening
    /// allocates one flat of the whole length and copies the leaves in
    /// order (iteratively, however deep the rope), then publishes it with
    /// a Release store; two racing flatteners both succeed and one pointer
    /// wins, the other's flat being garbage. A borrow of the text
    /// (`StrRef::as_str`) needs this first.
    pub fn flatten_string(&mut self, string: HeapPtr) -> HeapPtr {
        // SAFETY: the caller holds a live string.
        let source = unsafe { StrRef::new_unchecked(string) };
        if let Some((flat, _)) = source.flat() {
            return flat.ptr();
        }
        let flat = self.alloc_flat_copy(source);
        source.publish_flat(flat);
        flat.ptr()
    }

    /// `left` then `right`: a rope when the result is
    /// [`STR_SHARE_THRESHOLD`] bytes or more (both operands non-empty), else
    /// a flat copy. An empty operand yields the other operand itself.
    pub fn concat_str(&mut self, left: HeapPtr, right: HeapPtr) -> HeapPtr {
        // SAFETY: the caller holds two live strings.
        let (l, r) = unsafe { (StrRef::new_unchecked(left), StrRef::new_unchecked(right)) };
        if l.is_empty() {
            return right;
        }
        if r.is_empty() {
            return left;
        }
        let byte_len = l.byte_len() + r.byte_len();
        if byte_len >= STR_SHARE_THRESHOLD {
            let ptr = self.alloc_string_cell(ROPE_DESC, granule_align(ROPE_BYTES as usize));
            // SAFETY: a fresh rope cell; both operands are live.
            return unsafe { StrRef::init_rope(ptr, l, r) }.ptr();
        }
        let ptr = self.alloc_string_cell(STR_DESC, str_cell_bytes(byte_len));
        // SAFETY: a fresh flat cell sized for both texts.
        unsafe { StrRef::init_concat(ptr, l, r) }.ptr()
    }

    /// The bytes `bytes` (on char boundaries) of `string`: the string
    /// itself for its whole range, a view when the piece is
    /// [`STR_SHARE_THRESHOLD`] bytes or more, else a flat copy. A view's base
    /// is always a flat: a rope is flattened first, and a view of a view
    /// points at the base with the offsets added.
    pub fn substring(&mut self, string: HeapPtr, bytes: Range<usize>) -> HeapPtr {
        // SAFETY: the caller holds a live string.
        let source = unsafe { StrRef::new_unchecked(string) };
        assert!(
            bytes.start <= bytes.end && bytes.end <= source.byte_len(),
            "a substring lies inside its string"
        );
        if bytes.start == 0 && bytes.end == source.byte_len() {
            return string;
        }
        self.flatten_string(string);
        let (base, offset) = source.flat().expect("just flattened");
        let text = source.as_str();
        let char_len = chars_in_bytes(text, source.char_len(), bytes.clone());
        if bytes.len() < STR_SHARE_THRESHOLD {
            return self.alloc_string(&text[bytes]);
        }
        let ptr = self.alloc_string_cell(VIEW_DESC, granule_align(VIEW_BYTES as usize));
        let range = offset + bytes.start..offset + bytes.end;
        // SAFETY: a fresh view cell; `base` is the live flat holding the
        // bytes, and the range lies inside it on char boundaries.
        unsafe { StrRef::init_view(ptr, base, range, char_len) }.ptr()
    }

    /// Allocate a list whose elements have static type `element_ty`.
    pub fn alloc_array(&mut self, element_ty: RealizedTy, values: Vec<Value>) -> HeapPtr {
        let values: Vec<SlotValue> = values.into_iter().map(SlotValue::Value).collect();
        self.alloc_list_of(element_ty, &values).ptr()
    }

    /// [`Self::alloc_string_map`] keyed by the compiler's string, the shape
    /// the heap's own tests build fixtures in.
    pub fn alloc_map(
        &mut self,
        key_ty: RealizedTy,
        value_ty: RealizedTy,
        values: IndexMap<bex_str::BexStr, Value>,
    ) -> HeapPtr {
        self.alloc_string_map(key_ty, value_ty, values)
    }

    /// Allocate a string-keyed map whose keys/values have static types
    /// `key_ty`/`value_ty`, each key copied into a flat cell. A repeated key
    /// keeps its first position and takes its last value.
    pub fn alloc_string_map<K: AsRef<str>>(
        &mut self,
        key_ty: RealizedTy,
        value_ty: RealizedTy,
        values: impl IntoIterator<Item = (K, Value)>,
    ) -> HeapPtr {
        let map = self.alloc_empty_map(key_ty, value_ty);
        for (key, value) in values {
            let key = self.alloc_string(key);
            self.map_insert_str(map, key, value);
        }
        map.ptr()
    }

    /// Insert `key`, a string cell, into the string-keyed `map`: the value
    /// of the entry holding the same text is replaced in place, else the
    /// entry is appended. Hashes through the cell, so a key hashes once.
    pub fn map_insert_str(&mut self, map: MapRef, key: HeapPtr, value: Value) {
        // SAFETY: the caller holds a live string cell.
        let text = unsafe { StrRef::new_unchecked(key) };
        let hash = text.hash();
        let guard = map.lock(Access::ThreadLocal);
        let seen = guard.find(hash, |candidate| {
            candidate
                .as_object_ptr()
                // SAFETY: the map's keys are live string cells.
                .is_some_and(|ptr| unsafe { StrRef::new_unchecked(ptr) }.text_eq(text))
        });
        let result = match seen {
            Some(pos) => guard.set_value(pos, SlotValue::Value(value)),
            None => guard
                .insert(
                    self,
                    hash,
                    SlotValue::Value(Value::object(key)),
                    SlotValue::Value(value),
                )
                .map(|_| ()),
        };
        result.unwrap_or_else(|error| panic!("a map entry does not fit its slots: {error}"));
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
        let epoch = self.heap.gc_budget().full_collections;
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
        let bytes = granule_align(bytes);
        let cell_bytes = if bytes > LARGE_OBJECT_THRESHOLD {
            bytes
        } else {
            cell_size(bytes)
        };
        self.spend(cell_bytes);
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

    fn alloc_string(&mut self, s: impl AsRef<str>) -> HeapPtr {
        self.tlab_mut().alloc_string(s)
    }

    /// See [`Tlab::flatten_string`].
    fn flatten_string(&mut self, string: HeapPtr) -> HeapPtr {
        self.tlab_mut().flatten_string(string)
    }

    /// See [`Tlab::concat_str`].
    fn concat_str(&mut self, left: HeapPtr, right: HeapPtr) -> HeapPtr {
        self.tlab_mut().concat_str(left, right)
    }

    /// See [`Tlab::substring`].
    fn substring(&mut self, string: HeapPtr, bytes: Range<usize>) -> HeapPtr {
        self.tlab_mut().substring(string, bytes)
    }

    fn alloc_array(&mut self, element_ty: bex_vm_types::RealizedTy, values: Vec<Value>) -> HeapPtr {
        self.tlab_mut().alloc_array(element_ty, values)
    }

    /// Allocate a string-keyed map (see [`Tlab::alloc_string_map`]).
    fn alloc_map<K: AsRef<str>>(
        &mut self,
        key_ty: bex_vm_types::RealizedTy,
        value_ty: bex_vm_types::RealizedTy,
        values: impl IntoIterator<Item = (K, Value)>,
    ) -> HeapPtr {
        self.tlab_mut().alloc_string_map(key_ty, value_ty, values)
    }

    /// Allocate an empty string-keyed map (see [`Tlab::alloc_empty_map`]).
    fn alloc_empty_map(&mut self, key_ty: RealizedTy, value_ty: RealizedTy) -> MapRef {
        self.tlab_mut().alloc_empty_map(key_ty, value_ty)
    }

    /// Insert a string cell key (see [`Tlab::map_insert_str`]).
    fn map_insert_str(&mut self, map: MapRef, key: HeapPtr, value: Value) {
        self.tlab_mut().map_insert_str(map, key, value);
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
    use bex_vm_types::types::SharedKind;

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

        // SAFETY: just allocated; nothing collects.
        let string = unsafe { StrRef::of(ptr) }.expect("a flat string cell");
        assert_eq!(string.as_str(), "hello world");
        assert_eq!(string.byte_len(), 11);
        assert_eq!(string.char_len(), 11);
        assert!(matches!(
            unsafe { ptr.get() },
            Object::Shared(SharedKind::Str)
        ));
    }

    /// A flat string cell: lengths in bytes and chars, granule-sized, the
    /// hash computed once, and large text on the large-object path.
    #[test]
    fn flat_string_cells_hold_their_text_and_sizes() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let empty = tlab.alloc_string("");
        let small = tlab.alloc_string("héllo");
        let text = "x".repeat(LARGE_OBJECT_THRESHOLD + 1);
        let large = tlab.alloc_string(&text);

        // SAFETY: all just allocated; nothing collects.
        unsafe {
            let empty = StrRef::of(empty).unwrap();
            assert_eq!(
                (empty.as_str(), empty.byte_len(), empty.char_len()),
                ("", 0, 0)
            );
            assert!(empty.is_empty());

            let small = StrRef::of(small).unwrap();
            assert_eq!(small.as_str(), "héllo");
            assert_eq!((small.byte_len(), small.char_len()), (6, 5));
            let cell = small.ptr().header();
            assert_eq!(cell.descriptor, STR_DESC);
            assert_eq!(cell.cell_bytes(), 32, "a 6-byte string is one 32-byte cell");
            let hash = small.hash();
            assert_ne!(hash, 0);
            assert_eq!(small.hash(), hash, "the hash is cached");

            let large = StrRef::of(large).unwrap();
            assert_eq!(large.as_str(), text);
            assert_eq!(large.ptr().header().granules(), 0, "a large cell");
        }
        assert_eq!(heap.stats().large_objects, 1);
    }

    /// Flat string cells are collected like any cell: rooted ones keep their
    /// text, unrooted ones are freed, and a large one leaves the large space.
    #[test]
    fn flat_string_cells_are_collected() {
        let heap = BexHeap::new(vec![]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        let kept = tlab.alloc_string("kept");
        tlab.alloc_string("dead");
        let kept_large = tlab.alloc_string("k".repeat(LARGE_OBJECT_THRESHOLD + 1));
        tlab.alloc_string("d".repeat(LARGE_OBJECT_THRESHOLD + 1));
        let list = tlab.alloc_array(RealizedTy::string(), vec![Value::object(kept)]);
        assert_eq!(heap.stats().large_objects, 2);

        // SAFETY: single-threaded test; the roots are the only live pointers.
        let (stats, roots, _) = unsafe { heap.collect_garbage(&[kept_large, list]) };
        tlab.invalidate();
        assert_eq!(stats.collected_count, 2);
        assert_eq!(heap.stats().large_objects, 1);

        // SAFETY: survivors of the collection just run.
        unsafe {
            let large = StrRef::of(roots[0]).unwrap();
            assert_eq!(large.byte_len(), LARGE_OBJECT_THRESHOLD + 1);
            assert!(large.as_str().bytes().all(|b| b == b'k'));
            let list = ListRef::of(roots[1]).unwrap();
            let kept = list.get(0).unwrap().as_object_ptr().unwrap();
            assert_eq!(StrRef::of(kept).unwrap().as_str(), "kept");
        }

        // The dead cell's slot is reused by the next string of its class.
        let again = tlab.alloc_string("dead");
        // SAFETY: just allocated.
        assert_eq!(unsafe { StrRef::of(again) }.unwrap().as_str(), "dead");
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
            let key = StrRef::of(entries[0].0.as_object_ptr().unwrap()).expect("a string key");
            assert_eq!(key.as_str(), "key");
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
        assert_eq!(unsafe { StrRef::of(obj1) }.unwrap().as_str(), "before_gc_1");
        assert_eq!(unsafe { StrRef::of(obj3) }.unwrap().as_str(), "after_gc_1");
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

/// Ropes and views: the sharing kinds `concat_str` and `substring` make,
/// and their flattening.
#[cfg(test)]
mod string_kinds {
    use bex_vm_types::types::{STR_SHARE_THRESHOLD, StrKind, StrRef};

    use super::*;

    fn fresh() -> Tlab {
        Tlab::new(BexHeap::new(vec![]))
    }

    /// The string at `ptr`; nothing collects during these tests.
    fn str_at(ptr: HeapPtr) -> StrRef<'static> {
        // SAFETY: a string this test allocated and keeps.
        unsafe { StrRef::of(ptr) }.expect("a string")
    }

    #[test]
    fn concat_makes_a_rope_at_the_threshold_and_a_flat_below_it() {
        let mut tlab = fresh();
        let left = tlab.alloc_string("a".repeat(15));
        let short = tlab.alloc_string("b".repeat(16));
        let long = tlab.alloc_string("b".repeat(17));
        let below = str_at(tlab.concat_str(left, short));
        assert_eq!(below.byte_len(), 31);
        assert_eq!(below.kind(), StrKind::Flat, "31 bytes: copied");
        assert_eq!(below, format!("{}{}", "a".repeat(15), "b".repeat(16)).as_str());
        let at = str_at(tlab.concat_str(left, long));
        assert_eq!(at.byte_len(), STR_SHARE_THRESHOLD);
        assert_eq!(at.kind(), StrKind::Rope, "32 bytes: shared");
        assert!(!at.is_borrowable(), "a fresh rope is not flattened");
        assert_eq!(at.rope_halves().map(|(l, r)| (l.ptr(), r.ptr())), Some((left, long)));
        assert_eq!(at, format!("{}{}", "a".repeat(15), "b".repeat(17)).as_str());
    }

    #[test]
    fn concat_with_an_empty_operand_is_the_other_operand() {
        let mut tlab = fresh();
        let empty = tlab.alloc_string("");
        let long = tlab.alloc_string("x".repeat(40));
        assert_eq!(tlab.concat_str(empty, long), long);
        assert_eq!(tlab.concat_str(long, empty), long);
        assert_eq!(tlab.concat_str(empty, empty), empty);
    }

    #[test]
    fn a_rope_flattens_once_into_one_flat() {
        let mut tlab = fresh();
        let left = tlab.alloc_string("héllo, ".repeat(3));
        let right = tlab.alloc_string("wörld!".repeat(3));
        let rope = tlab.concat_str(left, right);
        let string = str_at(rope);
        assert_eq!(string.kind(), StrKind::Rope);
        assert_eq!(string.byte_len(), 8 * 3 + 7 * 3);
        assert_eq!(string.char_len(), 7 * 3 + 6 * 3, "char_len is summed, not scanned");
        let flat = tlab.flatten_string(rope);
        assert_ne!(flat, rope);
        assert_eq!(str_at(flat).kind(), StrKind::Flat);
        assert_eq!(string.rope_flat().map(StrRef::ptr), Some(flat), "published into the rope");
        assert!(string.is_borrowable());
        let expected = format!("{}{}", "héllo, ".repeat(3), "wörld!".repeat(3));
        assert_eq!(string.as_str(), expected);
        assert_eq!(str_at(flat).as_str(), expected);
        assert_eq!(str_at(flat).char_len(), string.char_len());
        assert_eq!(tlab.flatten_string(rope), flat, "flattened once");
        assert_eq!(tlab.flatten_string(flat), flat, "a flat is its own flat");
    }

    #[test]
    fn a_balanced_rope_flattens_in_order() {
        let mut tlab = fresh();
        let leaves: Vec<HeapPtr> = (0..64)
            .map(|i| tlab.alloc_string(format!("<{i:02}>")))
            .collect();
        let mut level = leaves;
        while level.len() > 1 {
            level = level
                .chunks(2)
                .map(|pair| tlab.concat_str(pair[0], pair[1]))
                .collect();
        }
        let root = level[0];
        let expected: String = (0..64).map(|i| format!("<{i:02}>")).collect();
        assert_eq!(str_at(root).kind(), StrKind::Rope);
        assert_eq!(str_at(root).to_owned_string(), expected, "walked without flattening");
        tlab.flatten_string(root);
        assert_eq!(str_at(root).as_str(), expected);
    }

    #[test]
    fn a_ten_thousand_deep_rope_flattens_without_recursion() {
        let mut tlab = fresh();
        let piece = tlab.alloc_string("0123456789abcdef");
        let mut acc = tlab.concat_str(piece, piece);
        for _ in 0..10_000 {
            acc = tlab.concat_str(acc, piece);
        }
        let string = str_at(acc);
        assert_eq!(string.kind(), StrKind::Rope);
        assert_eq!(string.byte_len(), 16 * 10_002);
        assert_eq!(string.hash(), bex_vm_types::map_string_hash(&"0123456789abcdef".repeat(10_002)).max(1), "hashed leaf by leaf");
        let flat = tlab.flatten_string(acc);
        assert_eq!(str_at(flat).byte_len(), 16 * 10_002);
        assert_eq!(string.as_str(), "0123456789abcdef".repeat(10_002));
        assert_eq!(str_at(flat).cached_hash(), string.cached_hash(), "the hash travels to the flat");
    }

    #[test]
    fn substring_makes_a_view_at_the_threshold_and_a_flat_below_it() {
        let mut tlab = fresh();
        let text = "abcdefghij".repeat(10);
        let base = tlab.alloc_string(&text);
        let below = str_at(tlab.substring(base, 5..36));
        assert_eq!(below.byte_len(), 31);
        assert_eq!(below.kind(), StrKind::Flat, "31 bytes: copied");
        assert_eq!(below.as_str(), &text[5..36]);
        let at = str_at(tlab.substring(base, 5..37));
        assert_eq!(at.byte_len(), STR_SHARE_THRESHOLD);
        assert_eq!(at.kind(), StrKind::View, "32 bytes: shared");
        assert_eq!(at.view_base().map(|(b, o)| (b.ptr(), o)), Some((base, 5)));
        assert_eq!(at.as_str(), &text[5..37]);
        assert_eq!(tlab.substring(base, 0..text.len()), base, "the whole string is itself");
    }

    #[test]
    fn a_view_of_a_view_or_a_rope_points_at_a_flat() {
        let mut tlab = fresh();
        let text = "abcdefghij".repeat(10);
        let base = tlab.alloc_string(&text);
        let outer = tlab.substring(base, 10..90);
        let inner = str_at(tlab.substring(outer, 20..60));
        assert_eq!(inner.kind(), StrKind::View);
        assert_eq!(inner.view_base().map(|(b, o)| (b.ptr(), o)), Some((base, 30)));
        assert_eq!(inner.as_str(), &text[30..70]);

        let left = tlab.alloc_string(&text[..50]);
        let right = tlab.alloc_string(&text[50..]);
        let rope = tlab.concat_str(left, right);
        let of_rope = str_at(tlab.substring(rope, 40..80));
        assert_eq!(of_rope.kind(), StrKind::View);
        let flat = str_at(rope).rope_flat().expect("slicing a rope flattens it");
        assert_eq!(of_rope.view_base().map(|(b, o)| (b.ptr(), o)), Some((flat.ptr(), 40)));
        assert_eq!(of_rope.as_str(), &text[40..80]);
    }

    #[test]
    fn hash_and_equality_agree_across_kinds() {
        let mut tlab = fresh();
        let text = "日本語テキスト, héllo wörld; ".repeat(4);
        let flat = tlab.alloc_string(&text);
        let (head, tail) = text.split_at(text.len() / 2 - 1);
        let (l, r) = (tlab.alloc_string(head), tlab.alloc_string(tail));
        let rope = tlab.concat_str(l, r);
        let padded = tlab.alloc_string(format!("<<{text}>>"));
        let view = tlab.substring(padded, 2..2 + text.len());
        let (flat, rope, view) = (str_at(flat), str_at(rope), str_at(view));
        assert_eq!(rope.kind(), StrKind::Rope);
        assert_eq!(view.kind(), StrKind::View);
        let expected = bex_vm_types::map_string_hash(&text).max(1);
        assert_eq!(flat.hash(), expected);
        assert_eq!(rope.hash(), expected, "an unflattened rope hashes its leaves");
        assert_eq!(view.hash(), expected, "a view hashes its sub-slice");
        assert_eq!(flat, rope);
        assert_eq!(rope, view);
        assert_eq!(view, flat);
        assert_eq!(rope.cmp(&flat), std::cmp::Ordering::Equal);
        let other = str_at(tlab.alloc_string(format!("{text}!")));
        assert_ne!(rope, other);
        assert!(rope < other, "a prefix orders first, leaf-wise");
        assert!(other > view);
        assert_eq!(rope.to_bex_str().as_str(), text);
        assert_eq!(rope.to_string(), text);
        assert_eq!(format!("{rope:?}"), format!("{text:?}"));
    }

    #[test]
    fn char_len_is_right_for_non_ascii_through_every_kind() {
        let mut tlab = fresh();
        let l = tlab.alloc_string("日本語テキスト日本語テキスト");
        let r = tlab.alloc_string("héllo wörld, héllo wörld");
        let rope = str_at(tlab.concat_str(l, r));
        assert_eq!(rope.kind(), StrKind::Rope);
        assert_eq!(rope.byte_len(), 14 * 3 + 28);
        assert_eq!(rope.char_len(), 14 + 24);
        assert!(!rope.is_ascii());
        let view = str_at(tlab.substring(rope.ptr(), 3..3 + 39 + 6));
        assert_eq!(view.kind(), StrKind::View);
        assert_eq!(view.as_str(), "本語テキスト日本語テキストhéllo");
        assert_eq!(view.char_len(), 13 + 5);
        assert_eq!(view.char_to_byte(13), 13 * 3);
        assert_eq!(view.char_slice(13..18), "héllo");
        let copy = str_at(tlab.substring(view.ptr(), 0..3 * 3));
        assert_eq!(copy.kind(), StrKind::Flat);
        assert_eq!(copy.as_str(), "本語テ");
        assert_eq!(copy.char_len(), 3);
    }
}
