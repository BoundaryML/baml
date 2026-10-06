use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use bex_vm_types::{HeapPtr, Object, RealizedTy, Value, ValueKind};

use super::{Continuation, ImplResolver, NativeCallResult};
use crate::{BexVm, VmPanic, errors::VmRustFnError};

enum Work {
    Value(Value, bool),
    Exit(HeapPtr),
    MapEntry(Value, Value),
    FinishMapEntry(Value, Arc<super::hasher::HasherState>),
    FinishMap,
}

struct HashDriver {
    state: Value,
    pending: Vec<Work>,
    active: HashSet<HeapPtr>,
    map_hashes: Vec<Vec<u64>>,
}

fn panic(message: &str) -> NativeCallResult {
    NativeCallResult::Error(VmRustFnError::Panic(VmPanic::UserPanic {
        message: message.to_owned(),
    }))
}

fn framed(tag: u8, bytes: &[u8]) -> Vec<u8> {
    let mut result = vec![tag];
    result.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    result.extend_from_slice(bytes);
    result
}

impl HashDriver {
    fn drive(mut self, vm: &mut BexVm) -> NativeCallResult {
        while let Some(work) = self.pending.pop() {
            let bytes = match work {
                Work::Exit(ptr) => {
                    self.active.remove(&ptr);
                    continue;
                }
                Work::MapEntry(key, value) => {
                    let (state, handle) = super::hasher::new_state(vm);
                    let previous = std::mem::replace(&mut self.state, state);
                    self.pending.push(Work::FinishMapEntry(previous, handle));
                    self.pending.push(Work::Value(value, true));
                    self.pending.push(Work::Value(key, true));
                    continue;
                }
                Work::FinishMapEntry(previous, handle) => {
                    self.state = previous;
                    self.map_hashes
                        .last_mut()
                        .expect("map entry belongs to an active map")
                        .push(super::hasher::finish_state(&handle));
                    continue;
                }
                Work::FinishMap => {
                    let mut hashes = self.map_hashes.pop().expect("active map hash frame");
                    hashes.sort_unstable();
                    let mut bytes = Vec::with_capacity(hashes.len() * 8);
                    for hash in hashes {
                        bytes.extend_from_slice(&hash.to_le_bytes());
                    }
                    framed(8, &bytes)
                }
                Work::Value(value, dispatch) => {
                    if let Some(ptr) = value.as_object_ptr() {
                        if !self.active.insert(ptr) {
                            return panic("Cannot hash a cyclic value");
                        }
                        self.pending.push(Work::Exit(ptr));
                        if let Object::Cell(cell) = vm.get_object(ptr) {
                            self.pending.push(Work::Value(cell.load(), dispatch));
                            continue;
                        }
                    }
                    if dispatch {
                        let Some(ty) = vm.value_concrete_ty(value).map(RealizedTy::from) else {
                            return panic("Value does not implement baml.Hash");
                        };
                        let Some(head) = vm.declaration_head(
                            &baml_type::QualifiedTypeName::from_dotted_path("baml.Hash"),
                        ) else {
                            return panic("baml.Hash is unavailable");
                        };
                        let resolver = ImplResolver::for_value(vm, value);
                        let Some(implementation) = resolver.resolve_implementation(&ty, head, &[])
                        else {
                            return panic("Value does not implement baml.Hash");
                        };
                        if let super::resolve::ResolvedInterfaceImpl::Explicit { rule, .. } =
                            &implementation
                        {
                            let method = match resolver.rule_method_impl(rule, "hash") {
                                Ok(method) => method,
                                Err(error) => return error.into(),
                            };
                            if !method.is_default {
                                let (callee, type_args) =
                                    match resolver.implementation_method(&implementation, "hash") {
                                        Ok(method) => method,
                                        Err(error) => return error.into(),
                                    };
                                return NativeCallResult::YieldToCall {
                                    callee,
                                    args: vec![value, self.state],
                                    type_args,
                                    continuation: Box::new(self),
                                };
                            }
                        }
                    }
                    match value.kind() {
                        ValueKind::Null => vec![0],
                        ValueKind::Int(value) => framed(1, &value.to_le_bytes()),
                        ValueKind::Bool(value) => vec![2, u8::from(value)],
                        ValueKind::OmittedArg => return panic("Cannot hash an omitted argument"),
                        ValueKind::Object(ptr) => match vm.get_object(ptr) {
                            Object::Float(value) => {
                                let bits = if value.is_nan() {
                                    f64::NAN.to_bits()
                                } else if *value == 0.0 {
                                    0
                                } else {
                                    value.to_bits()
                                };
                                framed(3, &bits.to_le_bytes())
                            }
                            Object::Bigint(value) => framed(4, &value.to_signed_bytes_le()),
                            Object::String(value) => framed(5, value.as_bytes()),
                            Object::Uint8Array(value) => framed(6, &value.lock()),
                            Object::Array(array) => {
                                let values = array.data.lock().to_vec();
                                let bytes = framed(7, &(values.len() as u64).to_le_bytes());
                                self.pending.extend(
                                    values
                                        .into_iter()
                                        .rev()
                                        .map(|value| Work::Value(value, true)),
                                );
                                bytes
                            }
                            Object::Map(map) => {
                                let entries = map.snapshot_entries();
                                self.map_hashes.push(Vec::with_capacity(entries.len()));
                                self.pending.push(Work::FinishMap);
                                for (key, value) in entries.into_iter().rev() {
                                    self.pending.push(Work::MapEntry(key, value));
                                }
                                continue;
                            }
                            Object::Instance(instance) => {
                                let Object::Class(class) = vm.get_object(instance.class) else {
                                    return panic("Invalid class while hashing");
                                };
                                let mut bytes = framed(9, &class.type_tag.as_i64().to_le_bytes());
                                bytes.extend_from_slice(
                                    &(instance.fields.len() as u64).to_le_bytes(),
                                );
                                self.pending.extend(
                                    instance
                                        .fields
                                        .iter()
                                        .rev()
                                        .map(|field| Work::Value(field.load(), true)),
                                );
                                bytes
                            }
                            Object::Variant(variant) => {
                                let Object::Enum(enm) = vm.get_object(variant.enm) else {
                                    return panic("Invalid enum while hashing");
                                };
                                let mut bytes = framed(10, &enm.type_tag.as_i64().to_le_bytes());
                                bytes.extend_from_slice(&(variant.index as u64).to_le_bytes());
                                bytes
                            }
                            _ => return panic("Value does not support structural hashing"),
                        },
                    }
                }
            };
            let Some(ty) = vm.value_concrete_ty(self.state).map(RealizedTy::from) else {
                return panic("Hash state does not implement baml.hash.Hasher");
            };
            let Some(head) = vm.declaration_head(&baml_type::QualifiedTypeName::from_dotted_path(
                "baml.hash.Hasher",
            )) else {
                return panic("baml.hash.Hasher is unavailable");
            };
            let resolver = ImplResolver::for_value(vm, self.state);
            let Some(implementation) = resolver.resolve_implementation(&ty, head, &[]) else {
                return panic("Hash state does not implement baml.hash.Hasher");
            };
            let (callee, type_args) = match resolver.implementation_method(&implementation, "write")
            {
                Ok(method) => method,
                Err(error) => return error.into(),
            };
            let bytes = Value::object(vm.tlab.alloc_uint8array(bytes));
            return NativeCallResult::YieldToCall {
                callee,
                args: vec![self.state, bytes],
                type_args,
                continuation: Box::new(self),
            };
        }
        NativeCallResult::Done(Value::NULL)
    }
}

impl Continuation for HashDriver {
    fn call(self: Box<Self>, vm: &mut BexVm, _value: Value) -> NativeCallResult {
        (*self).drive(vm)
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        let mut roots: Vec<_> = self.state.as_object_ptr().into_iter().collect();
        roots.extend(self.active.iter().copied());
        for work in &self.pending {
            match work {
                Work::Value(value, _) => roots.extend(value.as_object_ptr()),
                Work::Exit(ptr) => roots.push(*ptr),
                Work::MapEntry(key, value) => {
                    roots.extend(key.as_object_ptr());
                    roots.extend(value.as_object_ptr());
                }
                Work::FinishMapEntry(previous, _) => roots.extend(previous.as_object_ptr()),
                Work::FinishMap => {}
            }
        }
        roots
    }

    fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        let forward = |ptr| forwarding.get(&ptr).copied().unwrap_or(ptr);
        if let Some(ptr) = self.state.as_object_ptr() {
            self.state = Value::object(forward(ptr));
        }
        for work in &mut self.pending {
            match work {
                Work::Value(value, _) => {
                    if let Some(ptr) = value.as_object_ptr() {
                        *value = Value::object(forward(ptr));
                    }
                }
                Work::Exit(ptr) => *ptr = forward(*ptr),
                Work::MapEntry(key, value) => {
                    for value in [key, value] {
                        if let Some(ptr) = value.as_object_ptr() {
                            *value = Value::object(forward(ptr));
                        }
                    }
                }
                Work::FinishMapEntry(previous, _) => {
                    if let Some(ptr) = previous.as_object_ptr() {
                        *previous = Value::object(forward(ptr));
                    }
                }
                Work::FinishMap => {}
            }
        }
        self.active = self.active.iter().copied().map(forward).collect();
    }
}

pub(super) fn hash_structural_default(
    vm: &mut BexVm,
    value: Value,
    state: Value,
) -> NativeCallResult {
    HashDriver {
        state,
        pending: vec![Work::Value(value, false)],
        active: HashSet::new(),
        map_hashes: Vec::new(),
    }
    .drive(vm)
}

pub(super) fn hash_value(vm: &mut BexVm, value: Value, state: Value) -> NativeCallResult {
    HashDriver {
        state,
        pending: vec![Work::Value(value, true)],
        active: HashSet::new(),
        map_hashes: Vec::new(),
    }
    .drive(vm)
}
