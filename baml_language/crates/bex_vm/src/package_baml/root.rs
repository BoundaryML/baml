use std::{collections::HashMap, sync::Arc};

use bex_vm_types::{
    HeapPtr,
    types::{Future, Instance, Object, Value},
};
use indexmap::IndexMap;

use super::{BamlPackageBaml, PackageBamlImpl};
use crate::BexVm;

impl BamlPackageBaml for PackageBamlImpl {
    fn deep_copy(vm: &mut BexVm, value: &Value) -> Value {
        let mut copied_objects = HashMap::new();
        deep_copy_value_recursive(vm, *value, &mut copied_objects)
    }

    fn deep_equals(vm: &BexVm, a: &Value, b: &Value) -> bool {
        let mut visited = HashMap::new();
        deep_equals_recursive(vm, *a, *b, &mut visited)
    }
}

fn deep_copy_value_recursive(
    vm: &mut BexVm,
    value: Value,
    copied_objects: &mut HashMap<HeapPtr, HeapPtr>,
) -> Value {
    match value {
        Value::Null | Value::Int(_) | Value::Float(_) | Value::Bool(_) => value,

        Value::Object(ptr) => {
            if let Some(&new_ptr) = copied_objects.get(&ptr) {
                return Value::Object(new_ptr);
            }

            let object = vm.get_object(ptr).clone();

            let new_ptr = match object {
                Object::String(s) => vm.tlab.alloc(Object::String(s)),

                Object::Array(values) => {
                    let placeholder_ptr = vm.tlab.alloc(Object::Array(Vec::new()));
                    copied_objects.insert(ptr, placeholder_ptr);

                    let mut new_values = Vec::with_capacity(values.len());
                    for value in values {
                        new_values.push(deep_copy_value_recursive(vm, value, copied_objects));
                    }

                    *vm.get_object_mut(placeholder_ptr) = Object::Array(new_values);
                    placeholder_ptr
                }

                Object::Map(map) => {
                    let placeholder_ptr = vm.tlab.alloc(Object::Map(IndexMap::new()));
                    copied_objects.insert(ptr, placeholder_ptr);

                    let mut new_map = IndexMap::new();
                    for (key, value) in &map {
                        let new_value = deep_copy_value_recursive(vm, *value, copied_objects);
                        new_map.insert(key.clone(), new_value);
                    }

                    *vm.get_object_mut(placeholder_ptr) = Object::Map(new_map);
                    placeholder_ptr
                }

                Object::Instance(instance) => {
                    let placeholder_ptr = vm.tlab.alloc(Object::Instance(Instance {
                        class: instance.class,
                        fields: Vec::new(),
                    }));
                    copied_objects.insert(ptr, placeholder_ptr);

                    let mut new_fields = Vec::with_capacity(instance.fields.len());
                    for field in instance.fields {
                        new_fields.push(deep_copy_value_recursive(vm, field, copied_objects));
                    }

                    *vm.get_object_mut(placeholder_ptr) = Object::Instance(Instance {
                        class: instance.class,
                        fields: new_fields,
                    });
                    placeholder_ptr
                }

                Object::Function(f) => vm.tlab.alloc(Object::Function(f)),
                Object::Class(c) => vm.tlab.alloc(Object::Class(c)),
                Object::Enum(e) => vm.tlab.alloc(Object::Enum(e)),
                Object::Variant(v) => vm.tlab.alloc(Object::Variant(v)),
                Object::RustData(arc) => vm.tlab.alloc(Object::RustData(Arc::clone(&arc))),
                Object::Future(f) => vm.tlab.alloc(Object::Future(f)),
                Object::Collector(c) => vm.tlab.alloc(Object::Collector(c)),
                Object::Type(ty) => vm.tlab.alloc(Object::Type(ty)),
                #[cfg(feature = "heap_debug")]
                Object::Sentinel(kind) => vm.tlab.alloc(Object::Sentinel(kind)),
            };

            copied_objects.entry(ptr).or_insert(new_ptr);

            Value::Object(new_ptr)
        }
    }
}

#[allow(clippy::float_cmp)]
fn deep_equals_recursive(
    vm: &BexVm,
    a: Value,
    b: Value,
    visited: &mut HashMap<(HeapPtr, HeapPtr), bool>,
) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => (a.is_nan() && b.is_nan()) || a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,

        (Value::Object(a_ptr), Value::Object(b_ptr)) => {
            if a_ptr == b_ptr {
                return true;
            }

            let key = if a_ptr < b_ptr {
                (a_ptr, b_ptr)
            } else {
                (b_ptr, a_ptr)
            };

            if let Some(&result) = visited.get(&key) {
                return result;
            }

            visited.insert(key, true);

            let result = match (vm.get_object(a_ptr), vm.get_object(b_ptr)) {
                (Object::String(a), Object::String(b)) => a == b,

                (Object::Array(a_values), Object::Array(b_values)) => {
                    a_values.len() == b_values.len()
                        && a_values
                            .iter()
                            .zip(b_values.iter())
                            .all(|(a, b)| deep_equals_recursive(vm, *a, *b, visited))
                }

                (Object::Map(a_map), Object::Map(b_map)) => {
                    a_map.len() == b_map.len()
                        && a_map.iter().all(|(key, a_val)| {
                            b_map.get(key).is_some_and(|b_val| {
                                deep_equals_recursive(vm, *a_val, *b_val, visited)
                            })
                        })
                }

                (Object::Instance(a_inst), Object::Instance(b_inst)) => {
                    a_inst.class == b_inst.class
                        && a_inst.fields.len() == b_inst.fields.len()
                        && a_inst
                            .fields
                            .iter()
                            .zip(b_inst.fields.iter())
                            .all(|(a, b)| deep_equals_recursive(vm, *a, *b, visited))
                }

                (Object::Variant(a_var), Object::Variant(b_var)) => {
                    a_var.enm == b_var.enm && a_var.index == b_var.index
                }

                (Object::Enum(a_enum), Object::Enum(b_enum)) => {
                    a_enum.name == b_enum.name
                        && a_enum.variants.len() == b_enum.variants.len()
                        && a_enum
                            .variants
                            .iter()
                            .zip(b_enum.variants.iter())
                            .all(|(a, b)| a.name == b.name)
                }

                (Object::Class(a_class), Object::Class(b_class)) => {
                    a_class.name == b_class.name
                        && a_class.fields.len() == b_class.fields.len()
                        && a_class
                            .fields
                            .iter()
                            .zip(b_class.fields.iter())
                            .all(|(a, b)| a.name == b.name)
                }

                (Object::Function(_), Object::Function(_)) => a_ptr == b_ptr,

                (Object::Future(a_fut), Object::Future(b_fut)) => match (a_fut, b_fut) {
                    (Future::Ready(a_val), Future::Ready(b_val)) => {
                        deep_equals_recursive(vm, *a_val, *b_val, visited)
                    }
                    (Future::Pending(a_pend), Future::Pending(b_pend)) => {
                        a_pend.operation == b_pend.operation
                            && a_pend.args.len() == b_pend.args.len()
                            && a_pend
                                .args
                                .iter()
                                .zip(b_pend.args.iter())
                                .all(|(a, b)| deep_equals_recursive(vm, *a, *b, visited))
                    }
                    _ => false,
                },

                _ => false,
            };

            visited.insert(key, result);
            result
        }

        _ => false,
    }
}
