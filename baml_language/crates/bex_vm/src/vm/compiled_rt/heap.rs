//! Heap access and allocation for compiled code.

use bex_heap::TlabHolder;
use std::sync::atomic::{AtomicU64, Ordering};

use bex_vm_types::{
    HeapPtr, Object, ObjectType, RealizedTy, Value,
    object_format::{
        Access, FIRST_INSTANCE_DESC, HEADER_BYTES, read_tagged_inline, write_tagged_inline,
    },
    types::{
        BACKING_CAP_OFFSET, BACKING_ELEMS_OFFSET, CLASS_OFFSET, Closure, LIST_BACKING_OFFSET,
        LIST_LEN_OFFSET, Type,
    },
};

use crate::{
    errors::{VmError, VmInternalError, VmPanic},
    vm::BexVm,
};
use bex_vm_types::object_format::SlotValue;
use bex_vm_types::types::SharedKind;

impl BexVm {
    /// Return the length of a container, measuring strings in bytes.
    #[inline]
    pub fn container_len(&self, container: Value) -> Result<Value, Box<VmError>> {
        self.container_len_in::<false>(container)
    }

    /// [`Self::container_len`] on a container escape analysis proved
    /// thread-local, without its lock.
    #[inline]
    pub fn container_len_local(&self, container: Value) -> Result<Value, Box<VmError>> {
        self.container_len_in::<true>(container)
    }

    #[inline]
    fn container_len_in<const THREAD_LOCAL: bool>(
        &self,
        container: Value,
    ) -> Result<Value, Box<VmError>> {
        let Some(ptr) = container.as_object_ptr() else {
            return Err(self.type_error(ObjectType::Array, container));
        };
        // SAFETY (`THREAD_LOCAL`): codegen calls the `_local` entry points
        // only where escape analysis proved no other thread can reach the
        // container.
        #[allow(clippy::cast_possible_wrap, unsafe_code)]
        let len = match self.get_object(ptr) {
            Object::Uint8Array(bytes) if THREAD_LOCAL => {
                unsafe { bytes.lock_thread_local() }.len() as i64
            }
            Object::Shared(SharedKind::List) => self.expect_list(ptr).len() as i64,
            Object::Uint8Array(bytes) => bytes.len() as i64,
            Object::Shared(SharedKind::Map) => self.expect_map(ptr).len() as i64,
            Object::Shared(SharedKind::Str) => self.expect_str(ptr).byte_len() as i64,
            other => return Err(object_type_error(ObjectType::Array, other)),
        };
        Ok(Value::int(len))
    }

    /// Load an array or byte array element, counting negative indices from the end.
    #[inline]
    pub fn load_array_element(
        &mut self,
        array: Value,
        index: Value,
    ) -> Result<Value, Box<VmError>> {
        self.load_array_element_in::<false>(array, index)
    }

    /// [`Self::load_array_element`] on a thread-local array, without its lock.
    #[inline]
    pub fn load_array_element_local(
        &mut self,
        array: Value,
        index: Value,
    ) -> Result<Value, Box<VmError>> {
        self.load_array_element_in::<true>(array, index)
    }

    #[inline]
    fn load_array_element_in<const THREAD_LOCAL: bool>(
        &mut self,
        array: Value,
        index: Value,
    ) -> Result<Value, Box<VmError>> {
        let Some(array_ptr) = array.as_object_ptr() else {
            return Err(self.type_error(ObjectType::Array, array));
        };
        let Some(i) = index.as_int() else {
            return Err(self.type_error(Type::Int, index));
        };
        // Hold the lock across bounds checking and loading to prevent races with growth.
        let load_result: Result<SlotValue, (i64, usize)> = {
            match self.get_object(array_ptr) {
                Object::Shared(SharedKind::List) => Self::load_list_element(
                    self.expect_list(array_ptr),
                    i,
                    access::<THREAD_LOCAL>(),
                ),
                Object::Uint8Array(bytes) => {
                    // SAFETY (`THREAD_LOCAL`): see `container_len_in`.
                    #[allow(unsafe_code)]
                    let guard = if THREAD_LOCAL {
                        unsafe { bytes.lock_thread_local() }
                    } else {
                        bytes.lock()
                    };
                    crate::array_index::resolve_index_inline(i, guard.len())
                        .map(|idx| SlotValue::Value(Value::int(i64::from(guard[idx]))))
                        .ok_or((i, guard.len()))
                }
                other => return Err(object_type_error(ObjectType::Array, other)),
            }
        };
        match load_result {
            Ok(v) => Ok(self.slot_value(v)),
            Err((index, length)) => Err(self.index_out_of_bounds_boxed(index, length)),
        }
    }

    #[inline]
    pub fn store_array_element(
        &mut self,
        array: Value,
        index: Value,
        new_value: Value,
    ) -> Result<(), Box<VmError>> {
        self.store_array_element_in::<false>(array, index, new_value)
    }

    /// [`Self::store_array_element`] on a thread-local array, without its lock.
    #[inline]
    pub fn store_array_element_local(
        &mut self,
        array: Value,
        index: Value,
        new_value: Value,
    ) -> Result<(), Box<VmError>> {
        self.store_array_element_in::<true>(array, index, new_value)
    }

    #[inline]
    fn store_array_element_in<const THREAD_LOCAL: bool>(
        &mut self,
        array: Value,
        index: Value,
        new_value: Value,
    ) -> Result<(), Box<VmError>> {
        let Some(array_ptr) = array.as_object_ptr() else {
            return Err(self.type_error(ObjectType::Array, array));
        };
        let Some(i) = index.as_int() else {
            return Err(self.type_error(Type::Int, index));
        };
        let new_value_u8: Option<u8> = new_value.as_int().map(|v| (v.cast_unsigned() & 0xFF) as u8);
        let store_result: Result<(), (i64, usize)> = {
            match self.get_object(array_ptr) {
                Object::Shared(SharedKind::List) => Self::store_list_element(
                    self.expect_list(array_ptr),
                    i,
                    new_value,
                    access::<THREAD_LOCAL>(),
                ),
                Object::Uint8Array(bytes) => {
                    let Some(byte_v) = new_value_u8 else {
                        return Err(self.type_error(Type::Int, new_value));
                    };
                    let debt = self.tlab.alloc_debt();
                    // SAFETY (`THREAD_LOCAL`): see `container_len_in`.
                    #[allow(unsafe_code)]
                    let mut guard = if THREAD_LOCAL {
                        unsafe { bytes.lock_mut_thread_local(debt) }
                    } else {
                        bytes.lock_mut(debt)
                    };
                    crate::array_index::resolve_index_inline(i, guard.len())
                        .ok_or((i, guard.len()))
                        .map(|idx| guard[idx] = byte_v)
                }
                other => return Err(object_type_error(ObjectType::Array, other)),
            }
        };
        if let Err((index, length)) = store_result {
            return Err(self.index_out_of_bounds_boxed(index, length));
        }
        Ok(())
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn index_out_of_bounds(&mut self, index: i64, length: usize) -> VmError {
        VmError::thrown_fresh(
            self.panic_to_exception_value(VmPanic::IndexOutOfBounds { index, length }),
        )
    }

    #[cold]
    #[inline(never)]
    fn index_out_of_bounds_boxed(&mut self, index: i64, length: usize) -> Box<VmError> {
        Box::new(self.index_out_of_bounds(index, length))
    }

    #[cold]
    #[inline(never)]
    fn invalid_field_access_boxed(&mut self, idx: usize, length: usize) -> Box<VmError> {
        Box::new(self.invalid_field_access_error(idx, length))
    }

    /// `expected`, got `value`: the `TypeError` an accessor raises for an
    /// operand of the wrong type. Out of line, so the accessors' fast paths
    /// stay small enough to inline into compiled bodies.
    #[allow(
        clippy::unnecessary_box_returns,
        reason = "boxing out of line keeps the allocation off the inlined fast path"
    )]
    #[cold]
    #[inline(never)]
    fn type_error(&self, expected: impl Into<Type>, value: Value) -> Box<VmError> {
        VmInternalError::TypeError {
            expected: expected.into(),
            got: self.type_of(&value),
        }
        .into()
    }

    #[inline]
    pub fn load_field(&mut self, instance: Value, idx: usize) -> Result<Value, Box<VmError>> {
        self.load_field_in::<false>(instance, idx)
    }

    /// [`Self::load_field`] on an instance escape analysis proved
    /// thread-local, with no ordering on the load.
    #[inline]
    pub fn load_field_local(&mut self, instance: Value, idx: usize) -> Result<Value, Box<VmError>> {
        self.load_field_in::<true>(instance, idx)
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn load_field_in<const THREAD_LOCAL: bool>(
        &mut self,
        instance: Value,
        idx: usize,
    ) -> Result<Value, Box<VmError>> {
        // The fast path: a tag check, the object's kind, a bounds check and
        // the field load. Every failure leaves through a cold helper.
        let Some(ptr) = instance.as_object_ptr() else {
            return Err(self.type_error(ObjectType::Instance, instance));
        };
        let Some(object) = self.instance_at(ptr) else {
            return Err(self.type_error(ObjectType::Instance, instance));
        };
        let access = if THREAD_LOCAL {
            Access::ThreadLocal
        } else {
            Access::Shared
        };
        match object.try_load_with(idx, access) {
            Some(value) => Ok(self.slot_value(value)),
            None => {
                let length = object.field_len();
                Err(self.invalid_field_access_boxed(idx, length))
            }
        }
    }

    #[inline]
    pub fn store_field(
        &mut self,
        instance: Value,
        idx: usize,
        new_value: Value,
    ) -> Result<(), Box<VmError>> {
        self.store_field_in::<false>(instance, idx, new_value)
    }

    /// [`Self::store_field`] on a thread-local instance, with no ordering on
    /// the store.
    #[inline]
    pub fn store_field_local(
        &mut self,
        instance: Value,
        idx: usize,
        new_value: Value,
    ) -> Result<(), Box<VmError>> {
        self.store_field_in::<true>(instance, idx, new_value)
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn store_field_in<const THREAD_LOCAL: bool>(
        &mut self,
        instance: Value,
        idx: usize,
        new_value: Value,
    ) -> Result<(), Box<VmError>> {
        let Some(obj_ptr) = instance.as_object_ptr() else {
            return Err(self.type_error(ObjectType::Instance, instance));
        };
        let Some(object) = self.instance_at(obj_ptr) else {
            return Err(self.type_error(ObjectType::Instance, instance));
        };
        let length = object.field_len();
        if idx >= length {
            return Err(self.invalid_field_access_boxed(idx, length));
        }
        let access = if THREAD_LOCAL {
            Access::ThreadLocal
        } else {
            Access::Shared
        };
        object.store_with(idx, new_value, access);
        Ok(())
    }

    pub fn make_closure(
        &mut self,
        function: HeapPtr,
        captures: Vec<Value>,
        captured_type_args: Box<[RealizedTy]>,
    ) -> Value {
        let closure = Object::Closure(Closure {
            function,
            captures: captures.into_boxed_slice(),
            captured_type_args,
        });
        Value::object(self.tlab.alloc(closure))
    }

    /// `receiver.function` as a bound method value, carrying the frame the
    /// receiver's type supplies (see `BexVm::receiver_type_args`).
    pub fn make_bound_method(
        &mut self,
        function: HeapPtr,
        receiver: Value,
    ) -> Result<Value, Box<VmError>> {
        let type_args = self.receiver_type_args(receiver).map_err(VmError::from)?;
        let bound = Object::BoundMethod(bex_vm_types::BoundMethod {
            function,
            receiver,
            type_args,
        });
        Ok(Value::object(self.tlab.alloc(bound)))
    }

    #[inline]
    pub fn discriminant(&self, value: Value) -> Result<i64, Box<VmError>> {
        let Some(ptr) = value.as_object_ptr() else {
            return Err(self.type_error(ObjectType::Variant, value));
        };
        match self.get_object(ptr) {
            #[allow(clippy::cast_possible_wrap)]
            Object::Variant(variant) => Ok(variant.index as i64),
            other => Err(object_type_error(ObjectType::Variant, other)),
        }
    }

    #[inline]
    #[must_use]
    pub fn is_instance_of(&self, value: Value, class: HeapPtr) -> bool {
        self.instance_of(&value)
            .is_some_and(|instance| instance.class() == class)
    }

    #[inline]
    #[must_use]
    pub fn is_variant_of(&self, value: Value, enm: HeapPtr) -> bool {
        match value.as_object_ptr().map(|ptr| self.get_object(ptr)) {
            Some(Object::Variant(variant)) => variant.enm == enm,
            _ => false,
        }
    }

    #[must_use]
    pub fn class_type(&self, class: HeapPtr) -> RealizedTy {
        self.class_type_at(class, Box::new([]))
    }

    #[inline]
    pub fn new_instance(&mut self, class: HeapPtr, fields: Vec<Value>) -> Value {
        Value::object(self.alloc_instance(class, fields))
    }

    /// How compiled code allocates `class`'s instances in place, or `None`
    /// when an instance needs more than its fields: type arguments, or a
    /// finalizer, which the generic path registers once construction ends.
    #[inline]
    #[must_use]
    pub fn instance_shape(&self, class: HeapPtr) -> Option<InstanceShape> {
        // SAFETY: the caller's frame roots the class.
        #[allow(unsafe_code)]
        let Object::Class(def) = (unsafe { class.get() }) else {
            return None;
        };
        if def.generic_param_count != 0 || def.cleanup.is_some() {
            return None;
        }
        let layout = self.heap.instance_layout(def);
        Some(InstanceShape {
            descriptor: layout.descriptor(),
            bytes: layout.size(),
            class,
        })
    }

    /// [`Self::instance_shape`] of the compile-time class at object index
    /// `index`, looked up once per VM: the class's layout sits behind a
    /// `OnceLock`, whose Acquire load waits for the Release stores of the
    /// fields just written, on every literal.
    #[inline]
    pub fn instance_shape_at(&mut self, index: usize, class: HeapPtr) -> Option<InstanceShape> {
        match self.compiled.shapes.get(index) {
            Some(Some(shape)) => *shape,
            _ => self.find_instance_shape(index, class),
        }
    }

    #[cold]
    #[inline(never)]
    fn find_instance_shape(&mut self, index: usize, class: HeapPtr) -> Option<InstanceShape> {
        let shape = self.instance_shape(class);
        let shapes = &mut self.compiled.shapes;
        if shapes.len() <= index {
            shapes.resize(index + 1, None);
        }
        shapes[index] = Some(shape);
        shape
    }

    /// `list.push(value)` for a list with descriptor `descriptor`, whose
    /// element slot kind the emitter knows: the shared list's own push, with
    /// this thread's allocator for growth. `None` for any other value or a
    /// value the slot does not take; the builtin's own path then decides.
    ///
    /// Replaces, for such a list, the `Array.push` adapter call, which
    /// boxed its arguments and re-derived the slot kind from the element type
    /// on every push.
    #[inline]
    pub fn push_known(&mut self, list: Value, descriptor: u32, value: SlotValue) -> Option<i64> {
        ListBase::of(list, descriptor)?;
        let list = self.expect_list(list.as_object_ptr()?);
        let len = list.lock(Access::Shared).push(&mut self.tlab, value).ok()?;
        i64::try_from(len).ok()
    }

    /// `map.get(key)` for a `map<string, V>` with descriptor `descriptor`,
    /// which the emitter knows: the shared map read in place, with the key's
    /// hash and the entry's slot kinds taken as known. `Some(null)` for an
    /// absent key; `None` for any other value, for the builtin's own path.
    ///
    /// Replaces, for such a map, the `Map.get` adapter call, which
    /// re-derived the map's shape and slot kinds from its type on every call.
    #[inline]
    pub fn map_get_known(&mut self, map: Value, descriptor: u32, key: Value) -> Option<Value> {
        let (map, _, pos) = self.locate_string_key(map, descriptor, key)?;
        let Some(pos) = pos else {
            return Some(Value::NULL);
        };
        let value = map.lock(Access::Shared).value(pos);
        Some(self.slot_value(value))
    }

    /// `map.set(key, value)` for a `map<string, V>` with descriptor
    /// `descriptor`: the previous value, or null when the key was absent and
    /// is inserted. `None` for any other map or a value its slot does not
    /// take, for the builtin's own path. Replaces the `Map.set` adapter
    /// call for such a map, as [`Self::map_get_known`] does `Map.get`.
    #[inline]
    pub fn map_set_known(
        &mut self,
        map: Value,
        descriptor: u32,
        key: Value,
        value: SlotValue,
    ) -> Option<Value> {
        let (map, hash, pos) = self.locate_string_key(map, descriptor, key)?;
        let guard = map.lock(Access::Shared);
        let previous = match pos {
            Some(pos) => {
                let previous = guard.value(pos);
                guard.set_value(pos, value).ok()?;
                Some(previous)
            }
            None => {
                guard
                    .insert(&mut self.tlab, hash, SlotValue::Value(key), value)
                    .ok()?;
                None
            }
        };
        drop(guard);
        Some(previous.map_or(Value::NULL, |previous| self.slot_value(previous)))
    }

    /// The map behind `map` when its descriptor is `descriptor`, the hash
    /// of `key` (a string), and the position of its entry if present.
    #[inline]
    fn locate_string_key(
        &self,
        map: Value,
        descriptor: u32,
        key: Value,
    ) -> Option<(bex_vm_types::types::MapRef, u64, Option<usize>)> {
        let ptr = map.as_object_ptr()?;
        // SAFETY: a `Value` names a live object, and every object has a header.
        if unsafe { ptr.header() }.descriptor.0 != descriptor {
            return None;
        }
        let map = self.map_at(ptr)?;
        let text = self.as_string(&key).ok()?.as_str();
        let hash = bex_vm_types::map_string_hash(text);
        let pos = map.lock(Access::Shared).find(hash, |candidate| {
            let candidate = candidate.expect_value();
            candidate == key
                || candidate
                    .as_object_ptr()
                    .is_some_and(|ptr| self.str_at(ptr).is_some_and(|other| other.as_str() == text))
        });
        Some((map, hash, pos))
    }

    /// A new instance of `shape`, zeroed but for its class slot. The caller
    /// stores every field through the returned base before its next
    /// safepoint: a zeroed tagged field reads as held until stored.
    #[allow(unsafe_code)]
    #[inline]
    pub fn alloc_shaped(&mut self, shape: InstanceShape) -> (Value, FieldBase) {
        let ptr = self
            .tlab
            .alloc_instance_unfilled(shape.descriptor, shape.bytes, shape.class);
        // SAFETY: the instance's cell starts at its header.
        let base = unsafe { ptr.as_ptr().cast::<u8>().sub(HEADER_BYTES) };
        (Value::object(ptr), FieldBase(base))
    }

    #[inline]
    pub fn new_array(&mut self, element_ty: RealizedTy, values: Vec<Value>) -> Value {
        Value::object(self.alloc_array(element_ty, values))
    }

    #[inline]
    pub fn new_map(&mut self, key_ty: RealizedTy, value_ty: RealizedTy) -> Value {
        Value::object(
            self.tlab
                .alloc_map(key_ty, value_ty, indexmap::IndexMap::new()),
        )
    }

    #[inline]
    pub fn new_variant(&mut self, enm: HeapPtr, index: usize) -> Value {
        Value::object(self.tlab.alloc_variant(enm, index))
    }

    #[inline]
    pub fn string_value(&mut self, text: &str) -> Value {
        Value::object(self.alloc_string(text))
    }
}

/// `expected`, got an object of another kind. Cold for the reason
/// [`BexVm::type_error`] is.
#[allow(
    clippy::unnecessary_box_returns,
    reason = "boxing out of line keeps the allocation off the inlined fast path"
)]
#[cold]
#[inline(never)]
fn object_type_error(expected: ObjectType, got: &Object) -> Box<VmError> {
    VmInternalError::TypeError {
        expected: expected.into(),
        got: ObjectType::of(got).into(),
    }
    .into()
}

/// The slot access a `THREAD_LOCAL` entry point uses.
const fn access<const THREAD_LOCAL: bool>() -> Access {
    if THREAD_LOCAL {
        Access::ThreadLocal
    } else {
        Access::Shared
    }
}

#[cfg(test)]
mod tests {
    use bex_vm_types::{
        Object, ObjectIndex, RealizedTy, Value,
        types::{Enum, EnumVariant, Owner},
    };
    use indexmap::IndexMap;

    use crate::vm::tests::test_vm;

    fn color() -> Object {
        let variant = |name: &str| EnumVariant {
            name: name.to_string(),
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::default(),
            skip: false,
            json_rename: None,
        };
        Object::Enum(Box::new(Enum {
            path: bex_vm_types::types::LocalName::new(Vec::new(), baml_type::Name::new("Color")),
            type_tag: baml_type::typetag::TypeTag::of_static_index(0),
            variants: vec![variant("Red"), variant("Green")],
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::default(),
            owner: Owner::default(),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
        }))
    }

    #[test]
    fn a_new_map_is_empty_and_typed() {
        let mut vm = test_vm(Vec::new());
        let map = vm.new_map(RealizedTy::String, RealizedTy::Int);
        assert_eq!(vm.container_len(map).unwrap(), Value::int(0));
        let Some(object) = vm.map_at(map.as_object_ptr().unwrap()) else {
            panic!("a map object");
        };
        assert_eq!(*object.key_ty(), RealizedTy::String);
        assert_eq!(*object.value_ty(), RealizedTy::Int);
    }

    #[test]
    fn a_new_variant_is_of_its_enum_at_its_index() {
        let mut vm = test_vm(vec![color()]);
        let color = vm.idx_to_ptr(ObjectIndex::from_raw(0));
        let green = vm.new_variant(color, 1);
        assert!(vm.is_variant_of(green, color));
        assert_eq!(vm.discriminant(green).unwrap(), 1);
        let red = vm.new_variant(color, 0);
        assert_eq!(vm.discriminant(red).unwrap(), 0);
        assert_ne!(red, green, "each literal is a fresh object");
    }
}

/// A class instance compiled code reads and writes at fixed offsets.
///
/// Replaces, for a base whose class the emitter knows, the per-access layout
/// lookup of [`BexVm::load_field`] and [`BexVm::store_field`]: the emitter
/// takes each field's offset and slot kind from the same
/// [`InstanceLayout`](bex_vm_types::types::InstanceLayout) the interpreter
/// lays instances out with, and [`FieldBase::of`] checks the value is an
/// instance of that class before any slot is touched. Slot access follows
/// `documents/runtime-v2-shared-design.md` section 1: raw and pointer loads
/// are Relaxed, raw stores Relaxed, pointer stores Release. Tagged slots go
/// through the interpreter's own [`read_slot`]/[`write_slot`], so both sides
/// use one tagged-slot protocol.
#[derive(Clone, Copy)]
pub struct FieldBase(*const u8);

/// The class-slot word of an instance of `class`: what [`FieldBase::of`]
/// compares an instance's class slot with. Bound once per compiled body.
#[inline]
#[must_use]
pub fn class_word(class: HeapPtr) -> u64 {
    Value::object(class).bits()
}

/// How compiled code allocates one class's instances in place; see
/// [`BexVm::instance_shape`].
#[derive(Clone, Copy)]
pub struct InstanceShape {
    descriptor: bex_vm_types::layout::DescriptorId,
    bytes: usize,
    class: HeapPtr,
}

/// The class-slot word of `value` when it is an instance, else zero, which no
/// [`class_word`] is: what a dispatch on the receiver's class compares.
#[allow(clippy::inline_always, unsafe_code)]
#[inline(always)]
#[must_use]
pub fn instance_class_word(value: Value) -> u64 {
    let Some(ptr) = value.as_object_ptr() else {
        return 0;
    };
    // SAFETY: as in `FieldBase::of`.
    unsafe {
        if ptr.header().descriptor.0 < FIRST_INSTANCE_DESC.0 {
            return 0;
        }
        let base = ptr.as_ptr().cast::<u8>().sub(HEADER_BYTES);
        (*base.add(CLASS_OFFSET as usize).cast::<AtomicU64>()).load(Ordering::Relaxed)
    }
}

impl BexVm {
    /// `value` entering compiled code from a dynamic source (a call the VM
    /// made, a builtin's result, a boxed entry's argument) into a local typed
    /// by the class whose [`class_word`] is `class`. Compiled field access
    /// trusts that type ([`FieldBase::typed`]), so this is where it is
    /// checked, once. Null passes: the field access rejects it as the
    /// interpreter's does. So does an omitted argument, which the body
    /// replaces with the parameter's default before any access.
    ///
    /// # Errors
    /// For a value that is not an instance, the interpreter's `TypeError`
    /// for a non-instance operand. For an instance of another class, which
    /// the interpreter does not check, a `CompiledBody` error naming both.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    pub fn entering_class(&self, value: Value, class: u64) -> Result<Value, Box<VmError>> {
        if value.is_null()
            || instance_class_word(value) == class
            || value.bits() == Value::OMITTED_ARG.bits()
        {
            return Ok(value);
        }
        Err(self.class_boundary_error(value, class))
    }

    #[allow(
        clippy::unnecessary_box_returns,
        reason = "boxing out of line keeps the allocation off the inlined fast path"
    )]
    #[cold]
    #[inline(never)]
    fn class_boundary_error(&self, value: Value, class: u64) -> Box<VmError> {
        let got = instance_class_word(value);
        if got == 0 {
            return self.type_error(ObjectType::Instance, value);
        }
        Box::new(
            VmInternalError::CompiledBody {
                message: format!(
                    "an instance of `{}` entered compiled code where `{}` was expected",
                    self.class_word_name(got),
                    self.class_word_name(class),
                ),
            }
            .into(),
        )
    }

    /// The name of the class whose [`class_word`] is `word`.
    fn class_word_name(&self, word: u64) -> String {
        // SAFETY: a class word is the bits of a `Value` naming a class.
        let class = unsafe { Value::from_bits(word) };
        match class.as_object_ptr().map(|ptr| self.get_object(ptr)) {
            Some(Object::Class(class)) => class.name().to_string(),
            _ => String::from("<unknown class>"),
        }
    }
}

/// Compiled code's debug check before a field access it trusts: `value` is
/// null or an instance of the class whose [`class_word`] is `class`. Release
/// builds of generated code never call it (`class_checked!` in the
/// generated prelude).
///
/// # Panics
/// When `value` is another value: a boundary check was missed.
#[must_use]
pub fn debug_class(value: Value, class: u64) -> Value {
    assert!(
        value.is_null() || instance_class_word(value) == class,
        "compiled code accessed a field of a value whose class is not its static type",
    );
    value
}

impl FieldBase {
    /// `value` as an instance of the class whose [`class_word`] is `class`,
    /// or `None` (null, another object, another class) for the generic path.
    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    #[must_use]
    pub fn of(value: Value, class: u64) -> Option<Self> {
        let ptr = value.as_object_ptr()?;
        // SAFETY: a `Value` names a live object, and every object has a
        // header. An instance's class slot is a pointer slot set at
        // allocation and never rewritten.
        unsafe {
            if ptr.header().descriptor.0 < FIRST_INSTANCE_DESC.0 {
                return None;
            }
            let base = ptr.as_ptr().cast::<u8>().sub(HEADER_BYTES);
            let slot = &*base.add(CLASS_OFFSET as usize).cast::<AtomicU64>();
            (slot.load(Ordering::Relaxed) == class).then_some(Self(base))
        }
    }

    /// `value` as an instance of the class its static type names, or `None`
    /// for null, which the generic path rejects. Compiled code trusts a
    /// local's class type, as the interpreter's checker does: only a value
    /// of the class reaches a local typed by it.
    ///
    /// # Safety
    /// `value` is null or an instance of the class whose layout the caller
    /// reads it by.
    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    #[must_use]
    pub unsafe fn typed(value: Value) -> Option<Self> {
        let ptr = value.as_object_ptr()?;
        // SAFETY: an object pointer names the byte after its header.
        Some(Self(unsafe { ptr.as_ptr().cast::<u8>().sub(HEADER_BYTES) }))
    }

    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    unsafe fn word(self, offset: u32) -> &'static AtomicU64 {
        // SAFETY: forwarded to the caller.
        unsafe { &*self.0.add(offset as usize).cast::<AtomicU64>() }
    }

    /// The raw `int` slot at `offset`.
    ///
    /// # Safety
    /// `offset` is a raw `int` slot of this instance's layout.
    #[allow(clippy::inline_always, unsafe_code, clippy::cast_possible_wrap)]
    #[inline(always)]
    #[must_use]
    pub unsafe fn int(self, offset: u32) -> i64 {
        // SAFETY: forwarded to the caller.
        unsafe { self.word(offset) }.load(Ordering::Relaxed) as i64
    }

    /// The raw `float` slot at `offset`.
    ///
    /// # Safety
    /// `offset` is a raw `float` slot of this instance's layout.
    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    #[must_use]
    pub unsafe fn float(self, offset: u32) -> f64 {
        // SAFETY: forwarded to the caller.
        f64::from_bits(unsafe { self.word(offset) }.load(Ordering::Relaxed))
    }

    /// The raw `bool` slot at `offset`.
    ///
    /// # Safety
    /// `offset` is a raw `bool` slot of this instance's layout.
    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    #[must_use]
    pub unsafe fn bool(self, offset: u32) -> bool {
        // SAFETY: forwarded to the caller.
        unsafe { self.word(offset) }.load(Ordering::Relaxed) != 0
    }

    /// The pointer slot at `offset`, null or an object. Relaxed: the pointee
    /// is ordered by the address dependency on the writer's Release store.
    ///
    /// # Safety
    /// `offset` is a pointer slot of this instance's layout.
    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    #[must_use]
    pub unsafe fn ptr(self, offset: u32) -> Value {
        // SAFETY: forwarded to the caller; a pointer slot holds zero or the
        // bits of a live object's `Value`.
        unsafe { Value::from_bits(self.word(offset).load(Ordering::Relaxed)) }
    }

    /// Store an `int` to the raw slot at `offset`.
    ///
    /// # Safety
    /// `offset` is a raw `int` slot of this instance's layout.
    #[allow(clippy::inline_always, unsafe_code, clippy::cast_sign_loss)]
    #[inline(always)]
    pub unsafe fn set_int(self, offset: u32, value: i64) {
        // SAFETY: forwarded to the caller.
        unsafe { self.word(offset) }.store(value as u64, Ordering::Relaxed);
    }

    /// Store a `float` to the raw slot at `offset`.
    ///
    /// # Safety
    /// `offset` is a raw `float` slot of this instance's layout.
    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    pub unsafe fn set_float(self, offset: u32, value: f64) {
        // SAFETY: forwarded to the caller.
        unsafe { self.word(offset) }.store(value.to_bits(), Ordering::Relaxed);
    }

    /// Store a `bool` to the raw slot at `offset`.
    ///
    /// # Safety
    /// `offset` is a raw `bool` slot of this instance's layout.
    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    pub unsafe fn set_bool(self, offset: u32, value: bool) {
        // SAFETY: forwarded to the caller.
        unsafe { self.word(offset) }.store(u64::from(value), Ordering::Relaxed);
    }

    /// Store null or an object to the pointer slot at `offset`, Release so a
    /// reader that sees the pointer sees the object's contents.
    ///
    /// # Safety
    /// `offset` is a pointer slot of this instance's layout, and `value` is
    /// null or an object of the field's declared type.
    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    pub unsafe fn set_ptr(self, offset: u32, value: Value) {
        debug_assert!(value.is_null() || value.is_object());
        // SAFETY: forwarded to the caller.
        unsafe { self.word(offset) }.store(value.bits(), Ordering::Release);
    }
}

impl BexVm {
    /// The tagged slot at `offset` of `base`, a float boxed.
    ///
    /// # Safety
    /// `offset` is a tagged slot of `base`'s layout.
    #[allow(unsafe_code)]
    #[inline]
    pub unsafe fn load_tagged_at(&mut self, base: FieldBase, offset: u32) -> Value {
        // SAFETY: forwarded to the caller.
        let value = unsafe { read_tagged_inline(base.0.add(offset as usize)) };
        self.slot_value(value)
    }

    /// Store `value` to the tagged slot at `offset` of `base`.
    ///
    /// # Safety
    /// `offset` is a tagged slot of `base`'s layout.
    #[allow(unsafe_code)]
    #[inline]
    pub unsafe fn store_tagged_at(
        &mut self,
        base: FieldBase,
        offset: u32,
        value: Value,
    ) -> Result<(), Box<VmError>> {
        // SAFETY: forwarded to the caller.
        let stored = unsafe { write_tagged_inline(base.0.add(offset as usize), value) };
        stored.map_err(|mismatch| {
            Box::new(VmError::from(VmInternalError::CompiledBody {
                message: mismatch.to_string(),
            }))
        })
    }
}

/// A list compiled code reads and writes elements of in place.
///
/// Replaces, for a list whose element slot kind the emitter knows, the
/// object-kind dispatch of [`BexVm::load_array_element`] and
/// [`BexVm::store_array_element`]. [`ListBase::of`] checks the list's fixed
/// descriptor (`bex_vm_types::types::LIST_DESCS`), which fixes the element
/// slot kind. Reads follow the design's lock-free container rule: the
/// backing pointer and the length are Acquire loads, and the index is
/// bounded by both the length and the loaded backing's own capacity, so a
/// racing grow leaves a reader on valid, if stale, memory.
#[derive(Clone, Copy)]
pub struct ListBase(*const u8);

impl ListBase {
    /// `value` as a list with descriptor `descriptor`, or `None`.
    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    #[must_use]
    pub fn of(value: Value, descriptor: u32) -> Option<Self> {
        let ptr = value.as_object_ptr()?;
        // SAFETY: a `Value` names a live object with a header.
        unsafe {
            (ptr.header().descriptor.0 == descriptor)
                .then(|| Self(ptr.as_ptr().cast::<u8>().sub(HEADER_BYTES)))
        }
    }

    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    fn word(base: *const u8, offset: u32) -> &'static AtomicU64 {
        // SAFETY: callers pass a live list's or backing's header address and
        // one of its fixed word offsets.
        unsafe { &*base.add(offset as usize).cast::<AtomicU64>() }
    }

    /// The list's length, an Acquire load as the element reads make it.
    #[allow(clippy::inline_always, clippy::cast_possible_wrap)]
    #[inline(always)]
    #[must_use]
    pub fn len(self) -> i64 {
        Self::word(self.0, LIST_LEN_OFFSET).load(Ordering::Acquire) as i64
    }

    /// Element `index` (`stride` bytes apart), or `None` when it is negative
    /// or past the end; the generic path then decides.
    #[allow(
        clippy::inline_always,
        unsafe_code,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    #[inline(always)]
    #[must_use]
    pub fn elem(self, index: i64, stride: u32) -> Option<FieldBase> {
        let backing = Self::word(self.0, LIST_BACKING_OFFSET).load(Ordering::Acquire);
        if backing == 0 || index < 0 {
            return None;
        }
        let index = index as u64;
        let len = Self::word(self.0, LIST_LEN_OFFSET).load(Ordering::Acquire);
        // The backing slot holds a `Value`'s bits: the backing's payload.
        let backing = (backing as usize as *const u8).wrapping_sub(HEADER_BYTES);
        let cap = Self::word(backing, BACKING_CAP_OFFSET).load(Ordering::Relaxed);
        if index >= len || index >= cap {
            return None;
        }
        let offset = BACKING_ELEMS_OFFSET as usize + index as usize * stride as usize;
        // SAFETY: in bounds of the loaded backing.
        Some(FieldBase(unsafe { backing.add(offset) }))
    }
}
