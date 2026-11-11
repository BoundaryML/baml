//! The layout ABI over today's heap: the interpreter's instantiation of the
//! shared builtin bodies.
//!
//! Lists are shared-format list objects; maps and strings are `Map`/`String` holding tagged
//! `Value`s behind their `LockedContainer`s. The interpreter runs every
//! adapter builtin's one generic body at `E = Value` through [`ValueModel`]:
//! `Value` is the 16-byte tagged slot of today's format, so elements move
//! without conversion and boxed floats keep their identity.
//!
//! Compiled code runs the same bodies through the generated adapters over
//! [`ValueModel::compiled`], whose containers report their declared slot
//! kinds ([`run_compiled`]).
//!
//! This replaces the `Value` bodies those builtins had in `array.rs`,
//! `map.rs`, `string.rs` and `ops.rs`; the generated `__glue_*` methods call
//! [`run`] / [`run_thread_local`] with the body and, for builtins that may
//! yield, the yielding path to take on [`Fault::NeedsVm`].

use std::{num::NonZeroU64, ops::Range};

use bex_heap::TlabHolder;
use bex_vm_types::{
    HeapPtr, ListRef, MapRef, Object, RealizedTy, Value, ValueKind,
    errors::{VmBamlError, VmInternalError, VmPanic, VmRustFnError},
    layout::{
        Descriptors, Elem, Fault, Kind, ListOf, Lists, Maps, ObjPtr, RawWord, Shape, SlotKind,
        Slots, StrKey, Strings, Tagged,
    },
    object_format::{Access, SlotValue},
    types::{MapGuard, ObjectType, SharedKind, Type},
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
    /// Slots follow each container's declared element type: compiled code's
    /// view, under which the generated adapters run the raw, pointer and
    /// tagged instantiations of the bodies on the shared layouts.
    Declared,
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

/// Run compiled code's glue: `body` over the [`ValueModel::compiled`] model.
/// `None` when the body deferred: the caller then takes the builtin's
/// interpreter path, which may yield.
pub(crate) fn run_compiled(
    vm: &mut BexVm,
    args: &[Value],
    body: impl FnOnce(&mut ValueModel<'_>, &[Value]) -> Result<Value, GlueError>,
) -> Option<NativeCallResult> {
    match body(&mut ValueModel::compiled(vm), args) {
        Ok(value) => Some(NativeCallResult::Done(value)),
        Err(GlueError::Vm(error)) => Some(NativeCallResult::Error(error)),
        Err(GlueError::Defer) => None,
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

    /// The model compiled code runs the generated adapters over: containers
    /// report slots from their declared element types, so each adapter
    /// takes its raw, pointer or tagged instantiation.
    pub(crate) fn compiled(vm: &'vm mut BexVm) -> Self {
        Self::with(vm, Locality::Shared, View::Declared)
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

    fn array(&self, list: ObjPtr) -> ListRef {
        self.vm
            .list_at(heap_ptr(list))
            .unwrap_or_else(|| panic!("list ABI call on {}", ObjectType::of(self.object(list))))
    }

    /// The slot access the locality allows.
    fn access(&self) -> Access {
        match self.locality {
            Locality::Shared => Access::Shared,
            Locality::ThreadLocal => Access::ThreadLocal,
        }
    }

    fn map(&self, map: ObjPtr) -> MapRef {
        self.vm
            .map_at(heap_ptr(map))
            .unwrap_or_else(|| panic!("map ABI call on {}", ObjectType::of(self.object(map))))
    }

    /// The map, held under its lock as the locality allows.
    fn entries(&self, map: ObjPtr) -> MapGuard {
        self.map(map).lock(self.access())
    }

    /// The slot a container whose elements have type `ty` reports to
    /// dispatch. Element conversions use the declared type's slot instead
    /// (`SlotKind::of_type`), which is what a raw or pointer representation
    /// stands for in either view.
    fn slot_of(&self, ty: &RealizedTy) -> SlotKind {
        match self.view {
            View::Values => SlotKind::Tagged16,
            View::Declared => SlotKind::of_type(ty),
        }
    }

    fn list_slot(&self, list: ObjPtr) -> SlotKind {
        SlotKind::of_type(self.array(list).element_ty())
    }

    fn map_value_slot(&self, map: ObjPtr) -> SlotKind {
        SlotKind::of_type(self.map(map).value_ty())
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
            (ListOf::SameAs(list), View::Declared) => self.array(list).element_ty().clone(),
            (ListOf::ValuesOf(map), View::Declared) => self.map(map).value_ty().clone(),
        }
    }

    fn key_list_ty(&self, map: ObjPtr) -> RealizedTy {
        match self.view {
            View::Values => self
                .vm
                .current_call_type_args()
                .first()
                .cloned()
                .unwrap_or_else(RealizedTy::unknown),
            View::Declared => self.map(map).key_ty().clone(),
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

    /// The position of the entry whose key equals `key`.
    fn find_entry(&self, guard: &MapGuard, key: StrKey) -> Option<usize> {
        guard.find(key.hash, |candidate| {
            self.is_key(candidate.expect_value(), key)
        })
    }
}

/// How each slot representation moves in and out of a slot. A pure
/// representation converts through its universal form, a float staying
/// unboxed; the interpreter's `Value` boxes a float it reads.
trait ValueElem: Elem {
    fn from_slot(model: &ValueModel<'_>, value: SlotValue, slot: SlotKind) -> Self;
    fn into_slot(self, model: &ValueModel<'_>, slot: SlotKind) -> SlotValue;
}

impl ValueElem for Value {
    fn from_slot(model: &ValueModel<'_>, value: SlotValue, _slot: SlotKind) -> Self {
        model.vm.box_slot(value)
    }

    fn into_slot(self, _model: &ValueModel<'_>, _slot: SlotKind) -> SlotValue {
        SlotValue::Value(self)
    }
}

/// The universal form of a slot's contents.
fn slot_tagged(model: &ValueModel<'_>, value: SlotValue) -> Tagged {
    match value {
        SlotValue::Value(value) => model.tagged(value),
        SlotValue::Float(float) => Tagged::Float(float),
    }
}

/// A slot's contents for a universal value; a float stays unboxed.
fn tagged_slot(value: Tagged) -> SlotValue {
    match value {
        Tagged::Null => SlotValue::Value(Value::NULL),
        Tagged::Bool(boolean) => SlotValue::Value(Value::bool(boolean)),
        Tagged::Int(int) => SlotValue::Value(Value::int(int)),
        Tagged::Float(float) => SlotValue::Float(float),
        Tagged::Ptr(ptr) => SlotValue::Value(ptr_value(ptr)),
    }
}

/// The pure representations go through their universal form.
macro_rules! value_elem_via_tagged {
    ($($elem:ty),*) => {$(
        impl ValueElem for $elem {
            fn from_slot(model: &ValueModel<'_>, value: SlotValue, slot: SlotKind) -> Self {
                let mut unit = ();
                Slots::<$elem>::untag(&mut unit, slot_tagged(model, value), slot)
                    .unwrap_or_else(|fault| panic!("stored element does not fit its slot: {fault:?}"))
            }

            fn into_slot(self, _model: &ValueModel<'_>, slot: SlotKind) -> SlotValue {
                tagged_slot(Slots::<$elem>::to_tagged(&(), self, slot))
            }
        }
    )*};
}

value_elem_via_tagged!(RawWord, ObjPtr, Tagged);

impl ValueModel<'_> {
    fn elem<E: ValueElem>(&self, value: SlotValue, slot: SlotKind) -> E {
        E::from_slot(self, value, slot)
    }

    fn slots<E: ValueElem>(&self, elems: &[E], slot: SlotKind) -> Vec<SlotValue> {
        elems
            .iter()
            .map(|elem| elem.into_slot(self, slot))
            .collect()
    }

    fn elems<E: ValueElem>(&self, values: Vec<SlotValue>, slot: SlotKind) -> Vec<E> {
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
            Object::Shared(SharedKind::List) => Shape::List {
                elem: self.slot_of(self.array(obj).element_ty()),
            },
            Object::Shared(SharedKind::Map) => {
                let map = self.map(obj);
                Shape::Map {
                    key: self.slot_of(map.key_ty()),
                    value: self.slot_of(map.value_ty()),
                }
            }
            Object::String(_) => Shape::Str,
            Object::Shared(SharedKind::Instance) => Shape::Other(Kind::Instance),
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
        self.array(list).len()
    }

    fn list_get(&self, list: ObjPtr, index: usize) -> Option<E> {
        let value = self.array(list).get_with(index, self.access())?;
        Some(self.elem(value, self.list_slot(list)))
    }

    fn list_to_vec(&self, list: ObjPtr) -> Vec<E> {
        let values = self.array(list).snapshot(self.access());
        self.elems(values, self.list_slot(list))
    }

    fn list_splice(&mut self, list: ObjPtr, range: Range<usize>, with: &[E]) -> Vec<E> {
        let slot = self.list_slot(list);
        let inserted = self.slots(with, slot);
        let guard = self.array(list).lock(self.access());
        let mut values = guard.to_vec();
        let range = clamp(range, values.len());
        let removed: Vec<SlotValue> = values.splice(range, inserted).collect();
        guard
            .replace_all(&mut self.vm.tlab, &values)
            .unwrap_or_else(|error| panic!("a list element does not fit its slot: {error}"));
        drop(guard);
        self.elems(removed, slot)
    }

    fn list_push(&mut self, list: ObjPtr, elem: E) -> usize {
        let value = elem.into_slot(self, self.list_slot(list));
        self.array(list)
            .lock(self.access())
            .push(&mut self.vm.tlab, value)
            .unwrap_or_else(|error| panic!("a list element does not fit its slot: {error}"))
    }

    fn list_pop(&mut self, list: ObjPtr) -> Option<E> {
        let popped = self.array(list).lock(self.access()).pop()?;
        Some(self.elem(popped, self.list_slot(list)))
    }

    fn list_alloc(&mut self, of: ListOf, elems: &[E]) -> ObjPtr {
        let element_ty = self.element_ty(of);
        let values = self.slots(elems, SlotKind::of_type(&element_ty));
        obj_ptr(Value::object(
            self.vm.tlab.alloc_list_of(element_ty, &values).ptr(),
        ))
    }
}

impl<V: ValueElem> Maps<V> for ValueModel<'_> {
    fn map_len(&self, map: ObjPtr) -> usize {
        self.map(map).len()
    }

    fn map_get(&self, map: ObjPtr, key: StrKey) -> Option<V> {
        let guard = self.entries(map);
        let pos = self.find_entry(&guard, key)?;
        Some(self.elem(guard.value(pos), self.map_value_slot(map)))
    }

    fn map_set(&mut self, map: ObjPtr, key: StrKey, value: V) -> Option<V> {
        let slot = self.map_value_slot(map);
        let value = value.into_slot(self, slot);
        let guard = self.entries(map);
        let previous = match self.find_entry(&guard, key) {
            Some(pos) => {
                let previous = guard.value(pos);
                guard
                    .set_value(pos, value)
                    .expect("a map value fits its slot");
                Some(previous)
            }
            None => {
                let key_value = SlotValue::Value(ptr_value(key.ptr));
                guard
                    .insert(&mut self.vm.tlab, key.hash, key_value, value)
                    .expect("a map entry fits its slots");
                None
            }
        };
        drop(guard);
        previous.map(|previous| self.elem(previous, slot))
    }

    fn map_remove(&mut self, map: ObjPtr, key: StrKey) -> Option<V> {
        let slot = self.map_value_slot(map);
        let guard = self.entries(map);
        let removed = guard.remove(self.find_entry(&guard, key)?);
        drop(guard);
        Some(self.elem(removed, slot))
    }

    fn map_clear(&mut self, map: ObjPtr) {
        self.entries(map).clear();
    }

    fn map_key_list(&mut self, map: ObjPtr) -> ObjPtr {
        let keys: Vec<Value> = self
            .entries(map)
            .entries()
            .into_iter()
            .map(|(key, _)| self.vm.box_slot(key))
            .collect();
        let element_ty = self.key_list_ty(map);
        obj_ptr(Value::object(self.vm.alloc_array(element_ty, keys)))
    }

    fn map_values(&self, map: ObjPtr) -> Vec<V> {
        let values = self
            .entries(map)
            .entries()
            .into_iter()
            .map(|(_, value)| value)
            .collect();
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

    fn str_char_bytes(&self, string: ObjPtr, chars: Range<usize>) -> Range<usize> {
        match self.object(string) {
            Object::String(text) => text.char_to_byte(chars.start)..text.char_to_byte(chars.end),
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
