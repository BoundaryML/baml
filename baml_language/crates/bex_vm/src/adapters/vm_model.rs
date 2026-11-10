//! The layout ABI over today's heap: lists, maps and strings are
//! `Object::Array`/`Map`/`String` holding tagged `Value`s, and a list's slot
//! kind comes from its stored element type.
//!
//! This is the stub the generated adapters run against until the new heap
//! implements the same traits. It goes through the same container locks and
//! `MapData` operations as the `Value` bodies, so results are comparable.

use std::{num::NonZeroU64, ops::Range};

use bex_heap::TlabHolder;
use bex_vm_types::{
    EntryId, HeapPtr, MapReadGuard, MapWriteGuard, Object, RealizedTy, Value, ValueKind,
    errors::{VmBamlError, VmPanic, VmRustFnError},
    layout::{
        DescriptorId, Descriptors, Elem, Fault, Kind, ListOf, Lists, Maps, ObjPtr, Shape, SlotKind,
        StrKey, Strings, Tagged,
    },
    types::ObjectType,
};

use crate::BexVm;

/// The ABI view of one VM's heap.
pub struct VmModel<'vm> {
    vm: &'vm mut BexVm,
    /// Element types for [`ListOf::Described`], indexed by the id.
    described: Vec<RealizedTy>,
}

impl<'vm> VmModel<'vm> {
    pub fn new(vm: &'vm mut BexVm) -> Self {
        Self {
            vm,
            described: Vec::new(),
        }
    }

    pub fn vm(&mut self) -> &mut BexVm {
        self.vm
    }

    /// Register an element type for lists allocated as
    /// [`ListOf::Described`].
    pub fn describe(&mut self, element_ty: RealizedTy) -> DescriptorId {
        let id = u32::try_from(self.described.len()).expect("descriptor ids fit u32");
        self.described.push(element_ty);
        DescriptorId(id)
    }

    /// The universal form of an interpreter value.
    pub fn tagged(&self, value: Value) -> Tagged {
        match value.kind() {
            ValueKind::Null | ValueKind::OmittedArg => Tagged::Null,
            ValueKind::Bool(boolean) => Tagged::Bool(boolean),
            ValueKind::Int(int) => Tagged::Int(int),
            ValueKind::Object(ptr) => match self.vm.get_object(ptr) {
                Object::Float(float) => Tagged::Float(*float),
                _ => Tagged::Ptr(obj_ptr(value)),
            },
        }
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

    fn object(&self, ptr: ObjPtr) -> &Object {
        self.vm.get_object(heap_ptr(ptr))
    }

    fn array(&self, list: ObjPtr) -> &bex_vm_types::types::Array {
        match self.object(list) {
            Object::Array(array) => array,
            other => panic!("list ABI call on {}", ObjectType::of(other)),
        }
    }

    fn map(&self, map: ObjPtr) -> &bex_vm_types::types::Map {
        match self.object(map) {
            Object::Map(map) => map,
            other => panic!("map ABI call on {}", ObjectType::of(other)),
        }
    }

    fn element_ty(&self, of: ListOf) -> RealizedTy {
        match of {
            ListOf::SameAs(list) => (*self.array(list).element_ty).clone(),
            ListOf::KeysOf(map) => (*self.map(map).key_ty).clone(),
            ListOf::ValuesOf(map) => (*self.map(map).value_ty).clone(),
            ListOf::Strings => RealizedTy::string(),
            ListOf::Ints => RealizedTy::int(),
            ListOf::Described(DescriptorId(id)) => self.described[id as usize].clone(),
        }
    }

    fn list_slot(&self, list: ObjPtr) -> SlotKind {
        SlotKind::of_type(&self.array(list).element_ty)
    }

    fn map_value_slot(&self, map: ObjPtr) -> SlotKind {
        SlotKind::of_type(&self.map(map).value_ty)
    }

    fn elem<E: Elem>(&self, value: Value, slot: SlotKind) -> E {
        E::from_tagged(self.tagged(value), slot)
            .unwrap_or_else(|fault| panic!("stored element does not fit its slot: {fault:?}"))
    }

    fn values<E: Elem>(&mut self, elems: &[E], slot: SlotKind) -> Vec<Value> {
        elems
            .iter()
            .map(|elem| self.value(elem.to_tagged(slot)))
            .collect()
    }

    fn map_entries(&self, map: ObjPtr) -> MapReadGuard<'_> {
        self.map(map).lock()
    }

    fn map_entries_mut(&self, map: ObjPtr) -> MapWriteGuard<'_> {
        self.map(map).lock_mut(self.vm.tlab.alloc_debt())
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
        let (epoch, bucket) = self.map_entries(map).snapshot_bucket(key.hash);
        let found = bucket
            .into_iter()
            .find(|(_, entry)| self.is_key(entry.key, key))
            .map(|(id, _)| id);
        (epoch, found)
    }
}

/// The ABI pointer for an object value.
pub(crate) fn obj_ptr(value: Value) -> ObjPtr {
    ObjPtr::from_bits(NonZeroU64::new(value.bits()).expect("object values are non-null"))
}

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

/// `range` clamped to `len`.
fn clamp(range: Range<usize>, len: usize) -> Range<usize> {
    let end = range.end.min(len);
    range.start.min(end)..end
}

impl Descriptors for VmModel<'_> {
    fn shape(&self, obj: ObjPtr) -> Shape {
        match self.object(obj) {
            Object::Array(array) => Shape::List {
                elem: SlotKind::of_type(&array.element_ty),
            },
            Object::Map(map) => Shape::Map {
                key: SlotKind::of_type(&map.key_ty),
                value: SlotKind::of_type(&map.value_ty),
            },
            Object::String(_) => Shape::Str,
            Object::Instance(_) => Shape::Other(Kind::Instance),
            Object::Closure(_) => Shape::Other(Kind::Closure),
            Object::Future(_) => Shape::Other(Kind::Future),
            Object::Uint8Array(_) => Shape::Other(Kind::Bytes),
            _ => Shape::Other(Kind::Opaque),
        }
    }

    fn list_of_slot(&self, of: ListOf) -> SlotKind {
        SlotKind::of_type(&self.element_ty(of))
    }
}

impl<E: Elem> Lists<E> for VmModel<'_> {
    fn list_len(&self, list: ObjPtr) -> usize {
        self.array(list).len()
    }

    fn list_get(&self, list: ObjPtr, index: usize) -> Option<E> {
        let slot = self.list_slot(list);
        let value = self.array(list).get(index)?;
        Some(self.elem(value, slot))
    }

    fn list_to_vec(&self, list: ObjPtr) -> Vec<E> {
        let slot = self.list_slot(list);
        let values = self.array(list).to_vec();
        values
            .into_iter()
            .map(|value| self.elem(value, slot))
            .collect()
    }

    fn list_splice(&mut self, list: ObjPtr, range: Range<usize>, with: &[E]) -> Vec<E> {
        let slot = self.list_slot(list);
        let inserted = self.values(with, slot);
        let removed: Vec<Value> = {
            let mut values = self.array(list).lock_mut(self.vm.tlab.alloc_debt());
            let range = clamp(range, values.len());
            values.splice(range, inserted).collect()
        };
        removed
            .into_iter()
            .map(|value| self.elem(value, slot))
            .collect()
    }

    fn list_alloc(&mut self, of: ListOf, elems: &[E]) -> ObjPtr {
        let element_ty = self.element_ty(of);
        let values = self.values(elems, SlotKind::of_type(&element_ty));
        obj_ptr(Value::object(self.vm.alloc_array(element_ty, values)))
    }
}

impl<V: Elem> Maps<V> for VmModel<'_> {
    fn map_len(&self, map: ObjPtr) -> usize {
        self.map_entries(map).len()
    }

    fn map_get(&self, map: ObjPtr, key: StrKey) -> Option<V> {
        let value = self
            .map_entries(map)
            .get_hashed(key.hash, |candidate| self.is_key(*candidate, key))?;
        Some(self.elem(value, self.map_value_slot(map)))
    }

    fn map_set(&mut self, map: ObjPtr, key: StrKey, value: V) -> Option<V> {
        let slot = self.map_value_slot(map);
        let value = self.value(value.to_tagged(slot));
        let (epoch, found) = self.find_entry(map, key);
        let previous = self
            .map_entries_mut(map)
            .set_if_epoch(epoch, found, key.hash, ptr_value(key.ptr), value)
            .expect("no writer runs between the lookup and the store");
        previous.map(|previous| self.elem(previous, slot))
    }

    fn map_remove(&mut self, map: ObjPtr, key: StrKey) -> Option<V> {
        let slot = self.map_value_slot(map);
        let (epoch, found) = self.find_entry(map, key);
        let removed = self
            .map_entries_mut(map)
            .remove_if_epoch(epoch, found)
            .expect("no writer runs between the lookup and the removal");
        removed.map(|removed| self.elem(removed, slot))
    }

    fn map_clear(&mut self, map: ObjPtr) {
        self.map_entries_mut(map).clear();
    }

    fn map_keys(&self, map: ObjPtr) -> Vec<Tagged> {
        let keys: Vec<Value> = self.map_entries(map).keys().copied().collect();
        keys.into_iter().map(|key| self.tagged(key)).collect()
    }

    fn map_values(&self, map: ObjPtr) -> Vec<V> {
        let slot = self.map_value_slot(map);
        let values: Vec<Value> = self.map_entries(map).values().copied().collect();
        values
            .into_iter()
            .map(|value| self.elem(value, slot))
            .collect()
    }
}

impl Strings for VmModel<'_> {
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

    fn bytes_alloc(&mut self, bytes: &[u8]) -> ObjPtr {
        obj_ptr(Value::object(self.vm.alloc_uint8array(bytes.to_vec())))
    }
}

/// The interpreter error a fault surfaces as, or `None` when the fault has
/// no fixed counterpart: `NeedsVm` means "take the `Value` glue path", a
/// slot mismatch cannot reach the interpreter (its arrays are untyped), and
/// a wrong receiver surfaces as the VM's `TypeError` naming the actual object
/// type, which only the caller holding the value can build.
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
