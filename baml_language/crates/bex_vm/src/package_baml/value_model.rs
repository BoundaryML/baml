//! The layout ABI over today's heap: the interpreter's instantiation of the
//! shared builtin bodies.
//!
//! Lists, maps and strings are `Object::Array`/`Map`/`String` holding tagged
//! `Value`s behind their `LockedContainer`s. The interpreter runs every
//! adapter builtin's one generic body at `E = Value` through [`ValueModel`]:
//! `Value` is the 16-byte tagged slot of today's format, so elements move
//! without conversion and boxed floats keep their identity.
//!
//! This replaces the `Value` bodies those builtins had in `array.rs`,
//! `map.rs`, `string.rs` and `ops.rs`; the generated `__glue_*` methods call
//! [`run`] / [`run_thread_local`] with the body and, for builtins that may
//! yield, the yielding path to take on [`Fault::NeedsVm`].

use std::{num::NonZeroU64, ops::Range};

use bex_heap::TlabHolder;
use bex_vm_types::{
    EntryId, HeapPtr, MapReadGuard, MapWriteGuard, Object, RealizedTy, Value, ValueKind,
    errors::{VmBamlError, VmInternalError, VmPanic, VmRustFnError},
    layout::{
        Descriptors, Elem, Fault, Kind, ListOf, Lists, Maps, ObjPtr, RawWord, Shape, SlotKind,
        Slots, StrKey, Strings, Tagged,
    },
    types::{Array, ArrayReadGuard, ArrayWriteGuard, Map, ObjectType, Type},
};

use super::NativeCallResult;
use crate::BexVm;

/// Which container access the model uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Locality {
    /// Take each container's lock.
    Shared,
    /// Skip the locks: escape analysis proved the receiver thread-local.
    ThreadLocal,
}

/// Which slot kinds containers report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    /// Every element is a tagged `Value`: today's format, used by the
    /// interpreter.
    Values,
    /// Slots follow each container's declared element type, so tests can run
    /// the raw and pointer instantiations of the bodies on this heap.
    #[cfg(test)]
    Typed,
}

/// The ABI view of one VM's heap.
pub struct ValueModel<'vm> {
    vm: &'vm mut BexVm,
    locality: Locality,
    view: View,
}

/// A yielding `Value` path taken when a body reports [`Fault::NeedsVm`].
pub(crate) type Yielding = fn(&mut BexVm, &[Value], Locality) -> NativeCallResult;

/// Why interpreter glue did not produce a value.
pub(crate) enum GlueError {
    /// The builtin's error, as the interpreter raises it.
    Vm(VmRustFnError),
    /// Take the yielding path.
    Defer,
}

impl From<VmRustFnError> for GlueError {
    fn from(error: VmRustFnError) -> Self {
        GlueError::Vm(error)
    }
}

impl From<VmInternalError> for GlueError {
    fn from(error: VmInternalError) -> Self {
        GlueError::Vm(error.into())
    }
}

impl From<Fault> for GlueError {
    fn from(fault: Fault) -> Self {
        if let Fault::NeedsVm(_) = fault {
            return GlueError::Defer;
        }
        let error = vm_error(&fault)
            .unwrap_or_else(|| unreachable!("the value model never reports {fault:?}"));
        GlueError::Vm(error)
    }
}

/// Run interpreter glue: `body` over a locked model, then `fallback` if the
/// body deferred.
pub(crate) fn run(
    vm: &mut BexVm,
    args: &[Value],
    fallback: Option<Yielding>,
    body: impl FnOnce(&mut ValueModel<'_>, &[Value]) -> Result<Value, GlueError>,
) -> NativeCallResult {
    finish(vm, args, Locality::Shared, fallback, body)
}

/// [`run`] without container locks.
///
/// # Safety
///
/// No other thread may reach the receiver while the glue runs; escape
/// analysis proves this where it selects a `$local` builtin.
#[allow(unsafe_code)]
pub(crate) unsafe fn run_thread_local(
    vm: &mut BexVm,
    args: &[Value],
    fallback: Option<Yielding>,
    body: impl FnOnce(&mut ValueModel<'_>, &[Value]) -> Result<Value, GlueError>,
) -> NativeCallResult {
    finish(vm, args, Locality::ThreadLocal, fallback, body)
}

fn finish(
    vm: &mut BexVm,
    args: &[Value],
    locality: Locality,
    fallback: Option<Yielding>,
    body: impl FnOnce(&mut ValueModel<'_>, &[Value]) -> Result<Value, GlueError>,
) -> NativeCallResult {
    let result = body(&mut ValueModel::with(vm, locality, View::Values), args);
    match (result, fallback) {
        (Ok(value), _) => NativeCallResult::Done(value),
        (Err(GlueError::Vm(error)), _) => NativeCallResult::Error(error),
        (Err(GlueError::Defer), Some(yielding)) => yielding(vm, args, locality),
        (Err(GlueError::Defer), None) => unreachable!("a builtin that cannot yield deferred"),
    }
}

/// The interpreter error a fault surfaces as, or `None` when the fault has
/// no fixed counterpart: `NeedsVm` means "take the yielding path", a slot
/// mismatch cannot reach the interpreter (its elements are all tagged), and
/// a wrong receiver surfaces as the accessor's `TypeError`, which
/// [`ValueModel::receiver`] builds from the value.
pub fn vm_error(fault: &Fault) -> Option<VmRustFnError> {
    match fault {
        Fault::InvalidArgument { message } => Some(
            VmBamlError::InvalidArgument {
                message: message.clone(),
            }
            .into(),
        ),
        Fault::MapKeyNotFound => Some(VmPanic::MapKeyNotFound.into()),
        Fault::WrongReceiver { .. } | Fault::SlotMismatch { .. } | Fault::NeedsVm(_) => None,
    }
}

/// A body's `int` result, checked against the interpreter's int range.
pub(crate) fn int_value(int: i64) -> Result<Value, GlueError> {
    Value::try_int(int).ok_or_else(|| {
        let message = format!(
            "native int return value {int} is outside the BAML int range [{}, {}]",
            Value::INT_MIN,
            Value::INT_MAX
        );
        VmRustFnError::from(VmBamlError::InvalidArgument { message }).into()
    })
}

/// The ABI pointer for an object value.
pub(crate) fn obj_ptr(value: Value) -> ObjPtr {
    ObjPtr::from_bits(NonZeroU64::new(value.bits()).expect("object values are non-null"))
}

/// The object value an ABI pointer came from.
#[allow(
    unsafe_code,
    reason = "rebuilds a value from bits this model handed out"
)]
pub(crate) fn ptr_value(ptr: ObjPtr) -> Value {
    // SAFETY: every `ObjPtr` this model hands out comes from `obj_ptr` of a
    // live object value.
    unsafe { Value::from_bits(ptr.bits().get()) }
}

fn heap_ptr(ptr: ObjPtr) -> HeapPtr {
    ptr_value(ptr)
        .as_object_ptr()
        .expect("an ObjPtr is an object value")
}

/// The universal form of an interpreter value; a boxed float is read out.
pub(crate) fn tagged(vm: &BexVm, value: Value) -> Tagged {
    match value.kind() {
        ValueKind::Null | ValueKind::OmittedArg => Tagged::Null,
        ValueKind::Bool(boolean) => Tagged::Bool(boolean),
        ValueKind::Int(int) => Tagged::Int(int),
        ValueKind::Object(ptr) => match vm.get_object(ptr) {
            Object::Float(float) => Tagged::Float(*float),
            _ => Tagged::Ptr(obj_ptr(value)),
        },
    }
}

/// `range` clamped to `len`.
fn clamp(range: Range<usize>, len: usize) -> Range<usize> {
    let end = range.end.min(len);
    range.start.min(end)..end
}

impl<'vm> ValueModel<'vm> {
    fn with(vm: &'vm mut BexVm, locality: Locality, view: View) -> Self {
        Self { vm, locality, view }
    }

    /// The model the interpreter uses, with container locks.
    pub fn new(vm: &'vm mut BexVm) -> Self {
        Self::with(vm, Locality::Shared, View::Values)
    }

    /// A model whose containers report slots from their declared element
    /// types, for running the raw and pointer instantiations in tests.
    #[cfg(test)]
    pub(crate) fn typed(vm: &'vm mut BexVm) -> Self {
        Self::with(vm, Locality::Shared, View::Typed)
    }

    /// The universal form of an interpreter value.
    pub fn tagged(&self, value: Value) -> Tagged {
        tagged(self.vm, value)
    }

    /// The interpreter value for a universal one; floats are boxed.
    pub fn value(&mut self, tagged: Tagged) -> Value {
        match tagged {
            Tagged::Null => Value::NULL,
            Tagged::Bool(boolean) => Value::bool(boolean),
            Tagged::Int(int) => Value::int(int),
            Tagged::Float(float) => Value::object(self.vm.alloc_float(float)),
            Tagged::Ptr(ptr) => ptr_value(ptr),
        }
    }

    // ── Glue argument extraction, with the `Value` glue's errors ─────────

    /// The receiver as an ABI pointer, or the `TypeError` its accessor raises.
    pub(crate) fn receiver(&self, value: Value, kind: Kind) -> Result<ObjPtr, GlueError> {
        let fits = value
            .as_object_ptr()
            .is_some_and(|_| self.shape(obj_ptr(value)).kind() == kind);
        if fits {
            return Ok(obj_ptr(value));
        }
        let error = match kind {
            Kind::List => self.vm.as_array(&value).err(),
            Kind::Map => self.vm.as_map(&value).err(),
            Kind::Str => self.vm.as_string(&value).err(),
            other => unreachable!("no adapter receiver is a {other:?}"),
        };
        Err(error
            .expect("the accessor rejects what the shape rejects")
            .into())
    }

    pub(crate) fn int_arg(&self, value: Value) -> Result<i64, GlueError> {
        value
            .as_int()
            .ok_or_else(|| self.type_error(Type::Int, value))
    }

    fn type_error(&self, expected: Type, value: Value) -> GlueError {
        VmInternalError::TypeError {
            expected,
            got: self.vm.type_of(&value),
        }
        .into()
    }

    pub(crate) fn string_arg(&self, value: Value) -> Result<ObjPtr, GlueError> {
        self.vm.as_string(&value)?;
        Ok(obj_ptr(value))
    }

    pub(crate) fn list_arg(&self, value: Value) -> Result<ObjPtr, GlueError> {
        self.vm.as_array(&value)?;
        Ok(obj_ptr(value))
    }

    /// A map key the bodies can hash: a string. Any other key defers to the
    /// yielding path, whose hashing may run user code.
    pub(crate) fn key_arg(&self, value: Value) -> Result<ObjPtr, GlueError> {
        match self.tagged(value) {
            Tagged::Ptr(ptr) if self.shape(ptr) == Shape::Str => Ok(ptr),
            _ => Err(GlueError::Defer),
        }
    }

    // ── Heap access ──────────────────────────────────────────────────────

    fn object(&self, ptr: ObjPtr) -> &Object {
        self.vm.get_object(heap_ptr(ptr))
    }

    fn array(&self, list: ObjPtr) -> &Array {
        match self.object(list) {
            Object::Array(array) => array,
            other => panic!("list ABI call on {}", ObjectType::of(other)),
        }
    }

    fn map(&self, map: ObjPtr) -> &Map {
        match self.object(map) {
            Object::Map(map) => map,
            other => panic!("map ABI call on {}", ObjectType::of(other)),
        }
    }

    #[allow(unsafe_code)]
    fn read(&self, list: ObjPtr) -> ArrayReadGuard<'_> {
        let array = self.array(list);
        match self.locality {
            Locality::Shared => array.lock(),
            // SAFETY: `ThreadLocal` is only chosen through `run_thread_local`,
            // whose caller proves the receiver thread-local.
            Locality::ThreadLocal => unsafe { array.lock_thread_local() },
        }
    }

    #[allow(unsafe_code)]
    fn write(&self, list: ObjPtr) -> ArrayWriteGuard<'_> {
        let array = self.array(list);
        let debt = self.vm.tlab.alloc_debt();
        match self.locality {
            Locality::Shared => array.lock_mut(debt),
            // SAFETY: as for `read`.
            Locality::ThreadLocal => unsafe { array.lock_mut_thread_local(debt) },
        }
    }

    #[allow(unsafe_code)]
    fn entries(&self, map: ObjPtr) -> MapReadGuard<'_> {
        let map = self.map(map);
        match self.locality {
            Locality::Shared => map.lock(),
            // SAFETY: as for `read`.
            Locality::ThreadLocal => unsafe { map.lock_thread_local() },
        }
    }

    #[allow(unsafe_code)]
    fn entries_mut(&self, map: ObjPtr) -> MapWriteGuard<'_> {
        let map = self.map(map);
        let debt = self.vm.tlab.alloc_debt();
        match self.locality {
            Locality::Shared => map.lock_mut(debt),
            // SAFETY: as for `read`.
            Locality::ThreadLocal => unsafe { map.lock_mut_thread_local(debt) },
        }
    }

    /// The slot a container whose elements have type `ty` reports to
    /// dispatch. Element conversions use the declared type's slot instead
    /// (`SlotKind::of_type`), which is what a raw or pointer representation
    /// stands for in either view.
    #[cfg_attr(
        not(test),
        expect(unused_variables, reason = "only the test view reads it")
    )]
    fn slot_of(&self, ty: &RealizedTy) -> SlotKind {
        match self.view {
            View::Values => SlotKind::Tagged16,
            #[cfg(test)]
            View::Typed => SlotKind::of_type(ty),
        }
    }

    fn list_slot(&self, list: ObjPtr) -> SlotKind {
        SlotKind::of_type(&self.array(list).element_ty)
    }

    fn map_value_slot(&self, map: ObjPtr) -> SlotKind {
        SlotKind::of_type(&self.map(map).value_ty)
    }

    /// The element type of a list allocated as `of`. The interpreter tags a
    /// generic result with the call's type argument, as its `Value` glue did;
    /// the typed view uses the receiver's declared type.
    fn element_ty(&self, of: ListOf) -> RealizedTy {
        let type_arg = |index: usize| -> RealizedTy {
            self.vm
                .current_call_type_args()
                .get(index)
                .cloned()
                .unwrap_or_else(RealizedTy::unknown)
        };
        match (of, self.view) {
            (ListOf::Strings, _) => RealizedTy::string(),
            (ListOf::Ints, _) => RealizedTy::int(),
            (ListOf::SameAs(_), View::Values) => type_arg(0),
            (ListOf::ValuesOf(_), View::Values) => type_arg(1),
            #[cfg(test)]
            (ListOf::SameAs(list), View::Typed) => (*self.array(list).element_ty).clone(),
            #[cfg(test)]
            (ListOf::ValuesOf(map), View::Typed) => (*self.map(map).value_ty).clone(),
        }
    }

    #[cfg_attr(
        not(test),
        expect(unused_variables, reason = "only the test view reads it")
    )]
    fn key_list_ty(&self, map: ObjPtr) -> RealizedTy {
        match self.view {
            View::Values => self
                .vm
                .current_call_type_args()
                .first()
                .cloned()
                .unwrap_or_else(RealizedTy::unknown),
            #[cfg(test)]
            View::Typed => (*self.map(map).key_ty).clone(),
        }
    }

    fn is_key(&self, candidate: Value, key: StrKey) -> bool {
        let Some(ptr) = candidate.as_object_ptr() else {
            return false;
        };
        match self.vm.get_object(ptr) {
            Object::String(text) => text.as_str() == self.str_text(key.ptr),
            _ => false,
        }
    }

    /// The map's epoch and the entry whose key equals `key`.
    fn find_entry(&self, map: ObjPtr, key: StrKey) -> (u64, Option<EntryId>) {
        let (epoch, bucket) = self.entries(map).snapshot_bucket(key.hash);
        let found = bucket
            .into_iter()
            .find(|(_, entry)| self.is_key(entry.key, key))
            .map(|(id, _)| id);
        (epoch, found)
    }
}

/// How each slot representation moves in and out of a `Value` container.
trait ValueElem: Elem {
    fn from_value(model: &ValueModel<'_>, value: Value, slot: SlotKind) -> Self;
    fn into_value(self, model: &mut ValueModel<'_>, slot: SlotKind) -> Value;
}

impl ValueElem for Value {
    fn from_value(_model: &ValueModel<'_>, value: Value, _slot: SlotKind) -> Self {
        value
    }

    fn into_value(self, _model: &mut ValueModel<'_>, _slot: SlotKind) -> Value {
        self
    }
}

/// The pure representations go through their universal form.
macro_rules! value_elem_via_tagged {
    ($($elem:ty),*) => {$(
        impl ValueElem for $elem {
            fn from_value(model: &ValueModel<'_>, value: Value, slot: SlotKind) -> Self {
                let mut unit = ();
                Slots::<$elem>::untag(&mut unit, model.tagged(value), slot)
                    .unwrap_or_else(|fault| panic!("stored element does not fit its slot: {fault:?}"))
            }

            fn into_value(self, model: &mut ValueModel<'_>, slot: SlotKind) -> Value {
                let tagged = Slots::<$elem>::to_tagged(&(), self, slot);
                model.value(tagged)
            }
        }
    )*};
}

value_elem_via_tagged!(RawWord, ObjPtr, Tagged);

impl ValueModel<'_> {
    fn elem<E: ValueElem>(&self, value: Value, slot: SlotKind) -> E {
        E::from_value(self, value, slot)
    }

    fn values<E: ValueElem>(&mut self, elems: &[E], slot: SlotKind) -> Vec<Value> {
        elems
            .iter()
            .map(|elem| elem.into_value(self, slot))
            .collect()
    }

    fn elems<E: ValueElem>(&self, values: Vec<Value>, slot: SlotKind) -> Vec<E> {
        values
            .into_iter()
            .map(|value| self.elem(value, slot))
            .collect()
    }
}

impl Slots<Value> for ValueModel<'_> {
    fn to_tagged(&self, elem: Value, _slot: SlotKind) -> Tagged {
        self.tagged(elem)
    }

    fn untag(&mut self, value: Tagged, _slot: SlotKind) -> Result<Value, Fault> {
        Ok(self.value(value))
    }
}

impl Descriptors for ValueModel<'_> {
    fn shape(&self, obj: ObjPtr) -> Shape {
        match self.object(obj) {
            Object::Array(array) => Shape::List {
                elem: self.slot_of(&array.element_ty),
            },
            Object::Map(map) => Shape::Map {
                key: self.slot_of(&map.key_ty),
                value: self.slot_of(&map.value_ty),
            },
            Object::String(_) => Shape::Str,
            Object::Shared(_) => Shape::Other(Kind::Instance),
            Object::Closure(_) => Shape::Other(Kind::Closure),
            Object::Future(_) => Shape::Other(Kind::Future),
            Object::Uint8Array(_) => Shape::Other(Kind::Bytes),
            _ => Shape::Other(Kind::Opaque),
        }
    }

    fn list_of_slot(&self, of: ListOf) -> SlotKind {
        self.slot_of(&self.element_ty(of))
    }
}

impl<E: ValueElem> Lists<E> for ValueModel<'_> {
    fn list_len(&self, list: ObjPtr) -> usize {
        self.read(list).len()
    }

    fn list_get(&self, list: ObjPtr, index: usize) -> Option<E> {
        let value = self.read(list).get(index).copied()?;
        Some(self.elem(value, self.list_slot(list)))
    }

    fn list_to_vec(&self, list: ObjPtr) -> Vec<E> {
        let values = self.read(list).to_vec();
        self.elems(values, self.list_slot(list))
    }

    fn list_splice(&mut self, list: ObjPtr, range: Range<usize>, with: &[E]) -> Vec<E> {
        let slot = self.list_slot(list);
        let inserted = self.values(with, slot);
        let removed: Vec<Value> = {
            let mut values = self.write(list);
            let range = clamp(range, values.len());
            values.splice(range, inserted).collect()
        };
        self.elems(removed, slot)
    }

    fn list_push(&mut self, list: ObjPtr, elem: E) -> usize {
        let value = elem.into_value(self, self.list_slot(list));
        let mut values = self.write(list);
        values.push(value);
        values.len()
    }

    fn list_pop(&mut self, list: ObjPtr) -> Option<E> {
        let popped = self.write(list).pop()?;
        Some(self.elem(popped, self.list_slot(list)))
    }

    fn list_alloc(&mut self, of: ListOf, elems: &[E]) -> ObjPtr {
        let element_ty = self.element_ty(of);
        let values = self.values(elems, SlotKind::of_type(&element_ty));
        obj_ptr(Value::object(self.vm.alloc_array(element_ty, values)))
    }
}

impl<V: ValueElem> Maps<V> for ValueModel<'_> {
    fn map_len(&self, map: ObjPtr) -> usize {
        self.entries(map).len()
    }

    fn map_get(&self, map: ObjPtr, key: StrKey) -> Option<V> {
        let value = self
            .entries(map)
            .get_hashed(key.hash, |candidate| self.is_key(*candidate, key))?;
        Some(self.elem(value, self.map_value_slot(map)))
    }

    fn map_set(&mut self, map: ObjPtr, key: StrKey, value: V) -> Option<V> {
        let slot = self.map_value_slot(map);
        let value = value.into_value(self, slot);
        let (epoch, found) = self.find_entry(map, key);
        let previous = self
            .entries_mut(map)
            .set_if_epoch(epoch, found, key.hash, ptr_value(key.ptr), value)
            .expect("no writer runs between the lookup and the store");
        previous.map(|previous| self.elem(previous, slot))
    }

    fn map_remove(&mut self, map: ObjPtr, key: StrKey) -> Option<V> {
        let slot = self.map_value_slot(map);
        let (epoch, found) = self.find_entry(map, key);
        let removed = self
            .entries_mut(map)
            .remove_if_epoch(epoch, found)
            .expect("no writer runs between the lookup and the removal");
        removed.map(|removed| self.elem(removed, slot))
    }

    fn map_clear(&mut self, map: ObjPtr) {
        self.entries_mut(map).clear();
    }

    fn map_key_list(&mut self, map: ObjPtr) -> ObjPtr {
        let keys: Vec<Value> = self.entries(map).keys().copied().collect();
        let element_ty = self.key_list_ty(map);
        obj_ptr(Value::object(self.vm.alloc_array(element_ty, keys)))
    }

    fn map_values(&self, map: ObjPtr) -> Vec<V> {
        let values: Vec<Value> = self.entries(map).values().copied().collect();
        self.elems(values, self.map_value_slot(map))
    }
}

impl Strings for ValueModel<'_> {
    fn str_text(&self, string: ObjPtr) -> &str {
        match self.object(string) {
            Object::String(text) => text.as_str(),
            other => panic!("string ABI call on {}", ObjectType::of(other)),
        }
    }

    fn str_char_len(&self, string: ObjPtr) -> usize {
        match self.object(string) {
            Object::String(text) => text.char_len(),
            other => panic!("string ABI call on {}", ObjectType::of(other)),
        }
    }

    fn str_alloc(&mut self, text: &str) -> ObjPtr {
        obj_ptr(Value::object(self.vm.alloc_string(text.to_string())))
    }

    fn str_substring(&mut self, string: ObjPtr, bytes: Range<usize>) -> ObjPtr {
        let sliced = match self.object(string) {
            Object::String(text) => text.byte_slice(bytes),
            other => panic!("string ABI call on {}", ObjectType::of(other)),
        };
        obj_ptr(Value::object(self.vm.alloc_string(sliced)))
    }

    fn bytes_alloc(&mut self, bytes: &[u8]) -> ObjPtr {
        obj_ptr(Value::object(self.vm.alloc_uint8array(bytes.to_vec())))
    }
}
