use std::collections::HashSet;

use bex_vm_types::{HeapPtr, Object, RealizedTy, RootHaver, RootVisitor, Value, ValueKind};

use super::{Continuation, ImplResolver, NativeCallResult};
use crate::{BexVm, VmPanic, errors::VmRustFnError};

enum Work {
    Value(Value, bool),
    Exit(HeapPtr),
    MapEntry(Value, Value),
    /// The entry's hasher is finished, and the state it replaced is the one
    /// to restore.
    FinishMapEntry(Value),
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
                    let state = super::hasher::new_state(vm);
                    let previous = std::mem::replace(&mut self.state, state);
                    self.pending.push(Work::FinishMapEntry(previous));
                    self.pending.push(Work::Value(value, true));
                    self.pending.push(Work::Value(key, true));
                    continue;
                }
                Work::FinishMapEntry(previous) => {
                    let entry = std::mem::replace(&mut self.state, previous);
                    let hash = match super::hasher::finish_state(vm, entry) {
                        Ok(hash) => hash,
                        Err(error) => return error.into(),
                    };
                    self.map_hashes
                        .last_mut()
                        .expect("map entry belongs to an active map")
                        .push(hash);
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
                        let Some(head) = vm.lang_head(crate::lang_items::BAML_HASH) else {
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
                            // Equal only by identity, and an identity moves with
                            // the collector, so no kind hashes.
                            Object::RustInstance(instance) => match &instance.payload {
                                bex_vm_types::RustPayload::Foreign(_)
                                | bex_vm_types::RustPayload::DefaultHasher(_)
                                | bex_vm_types::RustPayload::Sha256(_)
                                | bex_vm_types::RustPayload::Aes256GcmSiv(_)
                                | bex_vm_types::RustPayload::Aes128GcmSiv(_)
                                | bex_vm_types::RustPayload::ChaCha20Poly1305(_)
                                | bex_vm_types::RustPayload::XChaCha20Poly1305(_)
                                | bex_vm_types::RustPayload::Regex(_)
                                | bex_vm_types::RustPayload::CsvRecord(_)
                                | bex_vm_types::RustPayload::Limit(_)
                                | bex_vm_types::RustPayload::CancelToken(_)
                                | bex_vm_types::RustPayload::SpanId(_)
                                | bex_vm_types::RustPayload::TraceOptions(_)
                                | bex_vm_types::RustPayload::ReservedSpan(_) => {
                                    return panic("Value does not support structural hashing");
                                }
                            },
                            _ => return panic("Value does not support structural hashing"),
                        },
                    }
                }
            };
            let Some(ty) = vm.value_concrete_ty(self.state).map(RealizedTy::from) else {
                return panic("Hash state does not implement baml.hash.Hasher");
            };
            let Some(head) = vm.lang_head(crate::lang_items::BAML_HASH_HASHER) else {
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

impl RootHaver for Work {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        match self {
            Work::Value(value, _) => value.visit_roots(visitor),
            Work::Exit(ptr) => ptr.visit_roots(visitor),
            Work::MapEntry(key, value) => {
                key.visit_roots(visitor);
                value.visit_roots(visitor);
            }
            Work::FinishMapEntry(previous) => previous.visit_roots(visitor),
            Work::FinishMap => {}
        }
    }
}

impl RootHaver for HashDriver {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        let Self {
            state,
            pending,
            active,
            map_hashes: _,
        } = self;
        state.visit_roots(visitor);
        pending.visit_roots(visitor);
        // The pointers are the set's keys, so it is rebuilt through the visitor.
        let mut moved: Vec<HeapPtr> = active.drain().collect();
        moved.visit_roots(visitor);
        active.extend(moved);
    }
}

impl Continuation for HashDriver {
    fn call(self: Box<Self>, vm: &mut BexVm, _value: Value) -> NativeCallResult {
        (*self).drive(vm)
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
