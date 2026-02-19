//! Value conversion functions between VM and external types.
//!
//! This module contains all the conversion logic for transforming values
//! between the VM representation (`Value`, `Object`) and the external
//! representation (`BexValue`, `BexExternalValue`).

use baml_type::Literal;
use bex_external_types::{BexExternalAdt, BexExternalValue, EpochGuard, Ty, UnionMetadata};
use bex_heap::BexValue;
use bex_vm::BexVm;
use bex_vm_types::{HeapPtr, Object, Value};

use crate::{BexEngine, EngineError};

// ============================================================================
// VM Value to External Conversion
// ============================================================================

impl BexEngine {
    /// Convert a VM Value to a `BexExternalValue` using the declared type.
    ///
    /// If the declared type is a union, the value is wrapped in `Union { value, metadata }`.
    pub(crate) fn convert_vm_value_to_external_with_type(
        &self,
        value: &Value,
        declared_type: &Ty,
        guard: &EpochGuard<'_>,
    ) -> Result<BexExternalValue, EngineError> {
        // If declared type is a union, find which member matches the actual value
        let effective_type = resolve_effective_type(value, declared_type);

        let external = match value {
            Value::Null => BexExternalValue::Null,
            Value::Int(i) => BexExternalValue::Int(*i),
            Value::Float(f) => BexExternalValue::Float(*f),
            Value::Bool(b) => BexExternalValue::Bool(*b),
            Value::Object(idx) => {
                self.convert_heap_ptr_to_external_with_type(*idx, effective_type, guard)?
            }
        };

        // Wrap in Union if declared type is a union
        maybe_wrap_union(external, declared_type)
    }

    /// Convert an object to a `BexExternalValue` using the effective (non-union) type.
    ///
    /// # Safety
    ///
    /// This method uses unsafe calls to dereference `HeapPtr`. It is safe because:
    /// - We only read objects, never write
    /// - The caller ensures the pointer is valid (from a handle which is a GC root)
    fn convert_heap_ptr_to_external_with_type(
        &self,
        ptr: HeapPtr,
        effective_type: &Ty,
        guard: &EpochGuard<'_>,
    ) -> Result<BexExternalValue, EngineError> {
        // SAFETY: We only read objects, and the pointer comes from a valid handle.
        let obj = unsafe { ptr.get() };

        match obj {
            Object::String(s) => Ok(BexExternalValue::String(s.clone())),

            Object::Array(arr) => {
                // Get element type from declared type, falling back to Null when
                // the declared type doesn't resolve (e.g., builtin class arrays)
                let element_type = match effective_type {
                    Ty::List(elem_ty) => elem_ty.as_ref(),
                    _ => &Ty::Null,
                };

                let items: Result<Vec<_>, _> = arr
                    .iter()
                    .map(|v| self.convert_vm_value_to_external_with_type(v, element_type, guard))
                    .collect();
                Ok(BexExternalValue::Array {
                    element_type: element_type.clone(),
                    items: items?,
                })
            }

            Object::Map(map) => {
                // Get key and value types from declared type, falling back to
                // Null when the declared type doesn't resolve
                let (key_type, value_type) = match effective_type {
                    Ty::Map { key, value } => (key.as_ref(), value.as_ref()),
                    _ => (&Ty::String, &Ty::Null),
                };

                let entries: Result<indexmap::IndexMap<String, BexExternalValue>, EngineError> =
                    map.iter()
                        .map(|(k, v)| {
                            Ok((
                                k.clone(),
                                self.convert_vm_value_to_external_with_type(v, value_type, guard)?,
                            ))
                        })
                        .collect();
                Ok(BexExternalValue::Map {
                    key_type: key_type.clone(),
                    value_type: value_type.clone(),
                    entries: entries?,
                })
            }

            Object::Instance(instance) => {
                // Get class name and fields from the Class object
                let class_obj = unsafe { instance.class.get() };
                let Object::Class(class) = class_obj else {
                    panic!("Instance.class should point to a Class object")
                };

                debug_assert_eq!(
                    class.fields.len(),
                    instance.fields.len(),
                    "Class '{}' has {} fields but instance has {} fields",
                    class.name,
                    class.fields.len(),
                    instance.fields.len(),
                );

                // Read field types directly from the Class object on the heap
                let fields: Result<indexmap::IndexMap<String, BexExternalValue>, EngineError> =
                    class
                        .fields
                        .iter()
                        .zip(instance.fields.iter())
                        .map(|(class_field, value)| {
                            Ok((
                                class_field.name.clone(),
                                self.convert_vm_value_to_external_with_type(
                                    value,
                                    &class_field.field_type,
                                    guard,
                                )?,
                            ))
                        })
                        .collect();

                Ok(BexExternalValue::Instance {
                    class_name: class.name.clone(),
                    fields: fields?,
                })
            }

            Object::Variant(variant) => {
                // Get enum name and variant name from the Enum object
                let enum_obj = unsafe { variant.enm.get() };
                let Object::Enum(enm) = enum_obj else {
                    panic!("Variant.enm should point to an Enum object")
                };
                let variant_name = enm
                    .variants
                    .get(variant.index)
                    .map(|v| v.name.clone())
                    .ok_or_else(|| EngineError::TypeMismatch {
                        message: format!(
                            "enum '{}' has {} variants but variant index is {}",
                            enm.name,
                            enm.variants.len(),
                            variant.index,
                        ),
                    })?;
                let enum_name = enm.name.clone();

                Ok(BexExternalValue::Variant {
                    enum_name,
                    variant_name,
                })
            }

            Object::Function(_) => Err(EngineError::CannotConvert {
                type_name: "function".to_string(),
            }),
            Object::Class(_) => Err(EngineError::CannotConvert {
                type_name: "class".to_string(),
            }),
            Object::Enum(_) => Err(EngineError::CannotConvert {
                type_name: "enum".to_string(),
            }),
            Object::Future(_) => Err(EngineError::CannotConvert {
                type_name: "future".to_string(),
            }),
            Object::Resource(handle) => Ok(BexExternalValue::Resource(handle.clone())),
            Object::Media(m) => Ok(BexExternalValue::Adt(BexExternalAdt::Media(m.clone()))),
            Object::PromptAst(ast) => Ok(BexExternalValue::Adt(BexExternalAdt::PromptAst(
                ast.clone(),
            ))),
            Object::Collector(c) => Ok(BexExternalValue::Adt(BexExternalAdt::Collector(c.clone()))),
            Object::Type(ty) => Ok(BexExternalValue::Adt(BexExternalAdt::Type(ty.clone()))),
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => Err(EngineError::CannotSnapshot {
                type_name: "sentinel".to_string(),
            }),
        }
    }
}

// ============================================================================
// External to VM Value Conversion
// ============================================================================

impl BexEngine {
    /// Convert a `BexExternalValue` result from sys ops back to a VM Value.
    pub(crate) fn convert_external_to_vm_value(
        &self,
        vm: &mut BexVm,
        external: BexExternalValue,
        guard: &EpochGuard<'_>,
    ) -> Value {
        match external {
            BexExternalValue::Handle(handle) => Value::Object(
                handle
                    .object_ptr(guard)
                    .expect("Handle should be valid - object was returned to external code"),
            ),
            BexExternalValue::Null => Value::Null,
            BexExternalValue::Int(i) => Value::Int(i),
            BexExternalValue::Float(f) => Value::Float(f),
            BexExternalValue::Bool(b) => Value::Bool(b),
            BexExternalValue::String(s) => vm.alloc_string(s),
            BexExternalValue::Array { items, .. } => {
                let values: Vec<Value> = items
                    .into_iter()
                    .map(|v| self.convert_external_to_vm_value(vm, v, guard))
                    .collect();
                vm.alloc_array(values)
            }
            BexExternalValue::Map { entries, .. } => {
                let values: indexmap::IndexMap<String, Value> = entries
                    .into_iter()
                    .map(|(k, v)| (k, self.convert_external_to_vm_value(vm, v, guard)))
                    .collect();
                vm.alloc_map(values)
            }
            BexExternalValue::Resource(handle) => vm.alloc_resource(handle),
            // Allocate instance by looking up class and converting fields
            BexExternalValue::Instance { class_name, fields } => {
                let class_ptr = self
                    .resolved_class_names
                    .get(&class_name)
                    .unwrap_or_else(|| {
                        panic!("Class '{class_name}' not found in resolved_class_names")
                    });

                // SAFETY: class_ptr points to a compile-time Class object
                let class_fields = match unsafe { class_ptr.get() } {
                    Object::Class(class) => &class.fields,
                    _ => panic!("class_ptr must point to Class"),
                };

                // Build field values in the order defined by the class
                let mut values = Vec::with_capacity(class_fields.len());
                for class_field in class_fields {
                    let ext = fields.get(&class_field.name).unwrap_or_else(|| {
                        panic!("missing field '{}' in Instance", class_field.name)
                    });
                    values.push(self.convert_external_to_vm_value(vm, ext.clone(), guard));
                }
                vm.alloc_instance(*class_ptr, values)
            }
            BexExternalValue::Variant {
                enum_name,
                variant_name,
            } => {
                let enum_ptr = self.resolved_enum_names.get(&enum_name).unwrap_or_else(|| {
                    panic!("Enum '{enum_name}' not found in resolved_enum_names")
                });
                #[allow(unsafe_code)]
                let bex_vm_types::Object::Enum(enum_obj) = (unsafe { enum_ptr.get() }) else {
                    panic!("Expected Object::Enum for '{enum_name}'");
                };
                let index = enum_obj
                    .variants
                    .iter()
                    .position(|v| v.name == variant_name)
                    .unwrap_or_else(|| {
                        panic!("Variant '{variant_name}' not found in enum '{enum_name}'")
                    });
                vm.alloc_variant(*enum_ptr, index)
            }
            BexExternalValue::Union { value, .. } => {
                self.convert_external_to_vm_value(vm, *value, guard)
            }
            BexExternalValue::Adt(BexExternalAdt::Media(media)) => vm.alloc_media(media),
            BexExternalValue::Adt(BexExternalAdt::PromptAst(ast)) => vm.alloc_prompt_ast(ast),
            BexExternalValue::Adt(BexExternalAdt::Collector(c)) => vm.alloc_collector(c),
            BexExternalValue::Adt(BexExternalAdt::Type(ty)) => vm.alloc_type(ty),
            BexExternalValue::FunctionRef { global_index } => {
                let idx = bex_vm_types::GlobalIndex::from_raw(global_index);
                assert!(
                    (global_index < vm.globals.len()),
                    "FunctionRef global_index {} out of bounds (globals len {})",
                    global_index,
                    vm.globals.len()
                );
                vm.globals[idx]
            }
        }
    }
}

// ============================================================================
// SysOp Argument Conversion
// ============================================================================

impl BexEngine {
    pub(crate) fn vm_arg_to_bex_value(&self, value: &Value) -> BexExternalValue {
        match value {
            Value::Null => BexExternalValue::Null,
            Value::Int(i) => BexExternalValue::Int(*i),
            Value::Float(f) => BexExternalValue::Float(*f),
            Value::Bool(b) => BexExternalValue::Bool(*b),
            Value::Object(ptr) => {
                let handle = self.heap.create_handle(*ptr);
                BexExternalValue::Handle(handle)
            }
        }
    }

    /// Convert a VM value to a fully owned `BexExternalValue` (deep copy).
    ///
    /// Unlike `vm_arg_to_bex_value` which creates `Handle` references for objects,
    /// this method deep-copies heap objects into standalone values. Use this for
    /// trace event payloads that escape the engine scope (e.g. event collectors).
    pub(crate) fn vm_value_to_owned(&self, value: &Value) -> BexExternalValue {
        match value {
            Value::Null => BexExternalValue::Null,
            Value::Int(i) => BexExternalValue::Int(*i),
            Value::Float(f) => BexExternalValue::Float(*f),
            Value::Bool(b) => BexExternalValue::Bool(*b),
            Value::Object(ptr) => self
                .heap
                .with_gc_protection(|protected| {
                    BexValue::HeapPtr(ptr).as_owned_but_very_slow(&protected)
                })
                .unwrap_or_else(|_| {
                    #[allow(clippy::print_stderr)]
                    {
                        eprintln!("Failed to deep-copy VM value for trace payload");
                    }
                    BexExternalValue::Null
                }),
        }
    }

    /// Convert VM values to `BexExternalValues` for sys ops.
    ///
    /// This is simpler than `vm_value_to_external` because sys ops only receive
    /// primitives, strings, arrays, maps, and resources - not instances/variants.
    #[allow(unused)]
    pub(crate) fn vm_args_to_external(vm: &BexVm, args: &[Value]) -> Vec<BexExternalValue> {
        args.iter().map(|v| vm_arg_to_external(vm, v)).collect()
    }
}

// ============================================================================
// Helper Functions (standalone, no &self needed)
// ============================================================================

/// Wrap a value in Union metadata if the declared type is a union.
pub(crate) fn maybe_wrap_union(
    value: BexExternalValue,
    declared_type: &Ty,
) -> Result<BexExternalValue, EngineError> {
    match declared_type {
        Ty::Union(members) => {
            let selected = find_matching_member(&value, members)?;
            let metadata = UnionMetadata::new(declared_type.clone(), selected);
            Ok(BexExternalValue::Union {
                value: Box::new(value),
                metadata,
            })
        }
        Ty::Optional(inner) => {
            let selected = if matches!(value, BexExternalValue::Null) {
                Ty::Null
            } else {
                (**inner).clone()
            };
            let metadata = UnionMetadata::new(declared_type.clone(), selected);
            Ok(BexExternalValue::Union {
                value: Box::new(value),
                metadata,
            })
        }
        _ => Ok(value),
    }
}

/// Find which union member matches a value.
fn find_matching_member(value: &BexExternalValue, members: &[Ty]) -> Result<Ty, EngineError> {
    for member in members {
        if value_matches_type(value, member) {
            return Ok(member.clone());
        }
    }
    // This indicates a type system inconsistency - the value should match one of the members
    Err(EngineError::TypeMismatch {
        message: format!(
            "Value of type '{}' does not match any member of union {:?}",
            value.type_name(),
            members
        ),
    })
}

/// Check if a value matches a declared type.
fn value_matches_type(value: &BexExternalValue, ty: &Ty) -> bool {
    match (value, ty) {
        (BexExternalValue::Null, Ty::Null) => true,
        (BexExternalValue::Int(_), Ty::Int) => true,
        (BexExternalValue::Float(_), Ty::Float) => true,
        (BexExternalValue::Bool(_), Ty::Bool) => true,
        (BexExternalValue::String(_), Ty::String) => true,
        // Literal types match their corresponding runtime values
        (BexExternalValue::Int(_), Ty::Literal(Literal::Int(_))) => true,
        (BexExternalValue::Float(_), Ty::Literal(Literal::Float(_))) => true,
        (BexExternalValue::String(_), Ty::Literal(Literal::String(_))) => true,
        (BexExternalValue::Bool(_), Ty::Literal(Literal::Bool(_))) => true,
        (BexExternalValue::Array { .. }, Ty::List(_)) => true,
        (BexExternalValue::Map { .. }, Ty::Map { .. }) => true,
        (BexExternalValue::Instance { class_name, .. }, Ty::Class(tn)) => {
            class_name.as_str() == tn.display_name.as_str()
        }
        (BexExternalValue::Variant { enum_name, .. }, Ty::Enum(tn)) => {
            enum_name.as_str() == tn.display_name.as_str()
        }
        (BexExternalValue::Adt(BexExternalAdt::Media(_)), Ty::Media(_)) => true,
        (BexExternalValue::Adt(BexExternalAdt::PromptAst(_)), ty)
            if ty.is_opaque("baml.llm.PromptAst") =>
        {
            true
        }
        (BexExternalValue::Adt(BexExternalAdt::Collector(_)), _) => false,
        (BexExternalValue::Adt(BexExternalAdt::Type(_)), ty)
            if ty.is_opaque("baml.reflect.Type") =>
        {
            true
        }
        (BexExternalValue::Union { value, .. }, ty) => value_matches_type(value, ty),
        // Handle nested unions/optionals in the type
        (value, Ty::Union(members)) => members.iter().any(|m| value_matches_type(value, m)),
        (value, Ty::Optional(inner)) => {
            matches!(value, BexExternalValue::Null) || value_matches_type(value, inner)
        }
        _ => false,
    }
}

/// For union types, find which member matches the actual runtime value.
///
/// If the declared type is not a union, returns it unchanged.
fn resolve_effective_type<'a>(value: &Value, declared_type: &'a Ty) -> &'a Ty {
    match declared_type {
        Ty::Union(members) => find_matching_union_member(value, members)
            .unwrap_or_else(|| members.first().unwrap_or(declared_type)),
        _ => declared_type,
    }
}

/// Find the union member that matches the runtime value's type.
fn find_matching_union_member<'a>(value: &Value, members: &'a [Ty]) -> Option<&'a Ty> {
    match value {
        Value::Null => members.iter().find(|m| matches!(m, Ty::Null)),
        Value::Int(_) => members
            .iter()
            .find(|m| matches!(m, Ty::Int | Ty::Literal(Literal::Int(_)))),
        Value::Float(_) => members
            .iter()
            .find(|m| matches!(m, Ty::Float | Ty::Literal(Literal::Float(_)))),
        Value::Bool(_) => members
            .iter()
            .find(|m| matches!(m, Ty::Bool | Ty::Literal(Literal::Bool(_)))),
        Value::Object(ptr) => {
            let obj = unsafe { ptr.get() };
            match obj {
                Object::String(_) => members
                    .iter()
                    .find(|m| matches!(m, Ty::String | Ty::Literal(Literal::String(_)))),
                Object::Instance(inst) => {
                    let class_obj = unsafe { inst.class.get() };
                    if let Object::Class(class) = class_obj {
                        members
                            .iter()
                            .find(|m| matches!(m, Ty::Class(tn) if tn.display_name.as_str() == class.name.as_str()))
                    } else {
                        None
                    }
                }
                Object::Variant(variant) => {
                    let enum_obj = unsafe { variant.enm.get() };
                    if let Object::Enum(enm) = enum_obj {
                        members
                            .iter()
                            .find(|m| matches!(m, Ty::Enum(tn) if tn.display_name.as_str() == enm.name.as_str()))
                    } else {
                        None
                    }
                }
                Object::Array(elements) => {
                    // For arrays, check first element to determine which List type
                    if let Some(first) = elements.first() {
                        members.iter().find(|m| {
                            if let Ty::List(elem_ty) = m {
                                find_matching_union_member(first, &[elem_ty.as_ref().clone()])
                                    .is_some()
                            } else {
                                false
                            }
                        })
                    } else {
                        // Empty array - match any List type
                        members.iter().find(|m| matches!(m, Ty::List(_)))
                    }
                }
                Object::Map(_) => members.iter().find(|m| matches!(m, Ty::Map { .. })),
                Object::Media(_) => members.iter().find(|m| matches!(m, Ty::Media(_))),
                Object::PromptAst(_) => members.iter().find(|m| m.is_opaque("baml.llm.PromptAst")),
                _ => None,
            }
        }
    }
}

/// Convert a VM value to a `BexExternalValue` for sys op arguments.
///
/// This is simpler than `vm_value_to_external` because sys ops only receive
/// primitives, strings, arrays, maps, and resources - not instances/variants.
pub(crate) fn vm_arg_to_external(vm: &BexVm, value: &Value) -> BexExternalValue {
    match value {
        Value::Null => BexExternalValue::Null,
        Value::Int(i) => BexExternalValue::Int(*i),
        Value::Float(f) => BexExternalValue::Float(*f),
        Value::Bool(b) => BexExternalValue::Bool(*b),
        Value::Object(idx) => {
            let obj = vm.get_object(*idx);
            match obj {
                Object::String(s) => BexExternalValue::String(s.clone()),
                Object::Array(arr) => {
                    let items: Vec<BexExternalValue> =
                        arr.iter().map(|v| vm_arg_to_external(vm, v)).collect();
                    BexExternalValue::Array {
                        element_type: bex_external_types::Ty::Null,
                        items,
                    }
                }
                Object::Map(map) => {
                    let entries: indexmap::IndexMap<String, BexExternalValue> = map
                        .iter()
                        .map(|(k, v)| (k.clone(), vm_arg_to_external(vm, v)))
                        .collect();
                    BexExternalValue::Map {
                        key_type: bex_external_types::Ty::String,
                        value_type: bex_external_types::Ty::Null,
                        entries,
                    }
                }
                Object::Resource(handle) => BexExternalValue::Resource(handle.clone()),
                Object::Instance(instance) => {
                    // Get class name from the class object
                    let class_obj = vm.get_object(instance.class);
                    let class_name = match class_obj {
                        Object::Class(class) => class.name.clone(),
                        _ => panic!("Instance class pointer doesn't point to a Class"),
                    };

                    // Get field names from class and convert fields
                    let class_fields = match class_obj {
                        Object::Class(class) => &class.fields,
                        _ => panic!("Instance class pointer doesn't point to a Class"),
                    };

                    let fields: indexmap::IndexMap<String, BexExternalValue> = class_fields
                        .iter()
                        .zip(instance.fields.iter())
                        .map(|(class_field, value)| {
                            (class_field.name.clone(), vm_arg_to_external(vm, value))
                        })
                        .collect();

                    BexExternalValue::Instance { class_name, fields }
                }
                other => {
                    panic!(
                        "Cannot convert object type to BexExternalValue for external op: {other:?}"
                    )
                }
            }
        }
    }
}
