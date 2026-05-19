//! Value conversion functions between VM and external types.
//!
//! This module contains all the conversion logic for transforming values
//! between the VM representation (`Value`, `Object`) and the external
//! representation (`BexValue`, `BexExternalValue`).

use ::bex_heap::{BexValue, HeapPermit, PermitProof, TlabHolder};
use ::bex_vm_types::{HeapPtr, Object, ObjectType, RootHaver, Value};
use baml_type::Literal;
use bex_external_types::{BexExternalAdt, BexExternalValue, Ty, UnionMetadata};
use bex_vm::BexVm;

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
        permit: PermitProof<'_>,
    ) -> Result<BexExternalValue, EngineError> {
        // If declared type is a union, find which member matches the actual value
        let effective_type = resolve_effective_type(value, declared_type);

        let external = match value {
            Value::OmittedArg => {
                return Err(EngineError::TypeMismatch {
                    message: "internal omitted argument escaped to external conversion".to_string(),
                });
            }
            Value::Null => BexExternalValue::Null,
            Value::Int(i) => BexExternalValue::Int(*i),
            Value::Float(f) => BexExternalValue::Float(*f),
            Value::Bool(b) => BexExternalValue::Bool(*b),
            Value::Object(idx) => {
                self.convert_heap_ptr_to_external_with_type(*idx, effective_type, permit)?
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
        permit: PermitProof<'_>,
    ) -> Result<BexExternalValue, EngineError> {
        // SAFETY: We only read objects, and the pointer comes from a valid handle.
        let obj = unsafe { ptr.get() };

        match obj {
            Object::String(s) => Ok(BexExternalValue::String(s.clone())),

            Object::Array(arr) => {
                // Get element type from declared type, falling back to Null when
                // the declared type doesn't resolve (e.g., builtin class arrays)

                let element_type = match effective_type {
                    Ty::List(elem_ty, _) => elem_ty.as_ref(),
                    _ => &Ty::Null {
                        attr: baml_type::TyAttr::default(),
                    },
                };

                let items: Result<Vec<_>, _> = arr
                    .iter()
                    .map(|v| self.convert_vm_value_to_external_with_type(v, element_type, permit))
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
                    Ty::Map { key, value, .. } => (key.as_ref(), value.as_ref()),
                    _ => (
                        &Ty::String {
                            attr: baml_type::TyAttr::default(),
                        },
                        &Ty::Null {
                            attr: baml_type::TyAttr::default(),
                        },
                    ),
                };

                let entries: Result<indexmap::IndexMap<String, BexExternalValue>, EngineError> =
                    map.iter()
                        .map(|(k, v)| {
                            Ok((
                                k.clone(),
                                self.convert_vm_value_to_external_with_type(v, value_type, permit)?,
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

                // Lift `baml.llm.Stream` to an opaque ADT handle.  The four
                // child fields (_client/_acc/_sse/_cache) stay on the heap
                // behind the GC-rooted handle so the BAML interpreter can
                // walk them when running `Stream.next` / `Stream.final`
                // bodies on subsequent calls.  See plan 21b §"Phase 1a".
                //
                // `ty` is computed from the class FQN + `class_type_args`
                // once at lift time and carried inline on the variant so
                // the wire encoder doesn't need a heap permit. See plan
                // 23a §"Engine-side ripple effects".
                if class.name.display_name.as_str() == "baml.llm.Stream" {
                    let handle = self.heap.create_handle(ptr);
                    let ty = Ty::Class(
                        class.name.clone(),
                        instance.class_type_args.clone(),
                        baml_type::TyAttr::default(),
                    );
                    return Ok(BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
                        ty,
                        heap_handle: handle,
                    }));
                }

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
                                    permit,
                                )?,
                            ))
                        })
                        .collect();

                Ok(BexExternalValue::Instance {
                    class_name: class.name.to_string(),
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
                let enum_name = enm.name.to_string();

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
            Object::UnscheduledFuture(_) => Err(EngineError::CannotConvert {
                type_name: "unscheduled_future".to_string(),
            }),
            Object::Collector(c) => Ok(BexExternalValue::Adt(BexExternalAdt::Collector(c.clone()))),
            Object::Type(ty) => Ok(BexExternalValue::Adt(BexExternalAdt::Type((**ty).clone()))),
            Object::Uint8Array(bytes) => Ok(BexExternalValue::Uint8Array(bytes.clone())),
            Object::RustData(arc) => Ok(bex_external_types::try_convert_rust_data(arc)
                .unwrap_or_else(|| BexExternalValue::RustData(arc.clone()))),
            Object::Closure(_) => Err(EngineError::CannotConvert {
                type_name: "closure".to_string(),
            }),
            Object::BoundMethod(_) => Err(EngineError::CannotConvert {
                type_name: "bound_method".to_string(),
            }),
            Object::Cell(_) => Err(EngineError::CannotConvert {
                type_name: "cell".to_string(),
            }),
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
    ///
    /// Returns `EngineError::TypeMismatch` for malformed external values
    /// (unknown class/enum names, missing required fields, …) so that bad
    /// external input — from `--json-args`, language bindings, or buggy
    /// sys ops — surfaces as a graceful error instead of crashing the
    /// process.
    pub(crate) fn convert_external_to_vm_value<T: RootHaver + TlabHolder>(
        &self,
        holder: &mut impl HeapPermit<T>,
        external: BexExternalValue,
    ) -> Result<Value, EngineError> {
        Ok(match external {
            BexExternalValue::Handle(handle) => Value::Object(
                self.resolve_handle(holder.proof(), &handle)
                    .expect("Handle should be valid - object was returned to external code"),
            ),
            BexExternalValue::Null => Value::Null,
            BexExternalValue::Int(i) => Value::Int(i),
            BexExternalValue::Float(f) => Value::Float(f),
            BexExternalValue::Bool(b) => Value::Bool(b),
            BexExternalValue::String(s) => {
                Value::Object(holder.holder_mut().tlab_mut().alloc_string(s))
            }
            BexExternalValue::Array { items, .. } => {
                let values = items
                    .into_iter()
                    .map(|v| self.convert_external_to_vm_value(holder, v))
                    .collect::<Result<Vec<_>, _>>()?;
                Value::Object(holder.holder_mut().tlab_mut().alloc_array(values))
            }
            BexExternalValue::Map { entries, .. } => {
                let values = entries
                    .into_iter()
                    .map(|(k, v)| self.convert_external_to_vm_value(holder, v).map(|v| (k, v)))
                    .collect::<Result<indexmap::IndexMap<String, Value>, _>>()?;
                Value::Object(holder.holder_mut().tlab_mut().alloc_map(values))
            }
            BexExternalValue::Uint8Array(bytes) => {
                Value::Object(holder.holder_mut().tlab_mut().alloc_uint8array(bytes))
            }
            BexExternalValue::RustData(data) => {
                Value::Object(holder.holder_mut().tlab_mut().alloc_rust_data(data))
            }
            // Allocate instance by looking up class and converting fields
            BexExternalValue::Instance { class_name, fields } => {
                let class_ptr = self
                    .resolved_class_names
                    .get(&class_name)
                    .or_else(|| resolve_named_object(&self.resolved_class_names, &class_name))
                    .ok_or_else(|| EngineError::TypeMismatch {
                        message: format!("Unknown class `{class_name}` in external Instance value"),
                    })?;

                // SAFETY: class_ptr points to a compile-time Class object
                let class_fields = match unsafe { class_ptr.get() } {
                    Object::Class(class) => &class.fields,
                    _ => {
                        return Err(EngineError::TypeMismatch {
                            message: format!(
                                "Resolved name `{class_name}` does not point to a class"
                            ),
                        });
                    }
                };

                // Build field values in the order defined by the class
                let mut values = Vec::with_capacity(class_fields.len());
                for class_field in class_fields {
                    let ext = fields.get(&class_field.name).ok_or_else(|| {
                        EngineError::TypeMismatch {
                            message: format!(
                                "Missing field `{}` in external Instance for class `{class_name}`",
                                class_field.name
                            ),
                        }
                    })?;
                    values.push(self.convert_external_to_vm_value(holder, ext.clone())?);
                }
                Value::Object(
                    holder
                        .holder_mut()
                        .tlab_mut()
                        .alloc_instance(*class_ptr, values),
                )
            }
            BexExternalValue::Variant {
                enum_name,
                variant_name,
            } => {
                let enum_ptr = self
                    .resolved_enum_names
                    .get(&enum_name)
                    .or_else(|| resolve_named_object(&self.resolved_enum_names, &enum_name))
                    .ok_or_else(|| EngineError::TypeMismatch {
                        message: format!("Unknown enum `{enum_name}` in external Variant value"),
                    })?;
                #[allow(unsafe_code)]
                let bex_vm_types::Object::Enum(enum_obj) = (unsafe { enum_ptr.get() }) else {
                    return Err(EngineError::TypeMismatch {
                        message: format!("Resolved name `{enum_name}` does not point to an enum"),
                    });
                };
                let index = enum_obj
                    .variants
                    .iter()
                    .position(|v| v.name == variant_name)
                    .ok_or_else(|| EngineError::TypeMismatch {
                        message: format!("Unknown variant `{variant_name}` in enum `{enum_name}`"),
                    })?;
                Value::Object(
                    holder
                        .holder_mut()
                        .tlab_mut()
                        .alloc_variant(*enum_ptr, index),
                )
            }
            BexExternalValue::Union { value, .. } => {
                return self.convert_external_to_vm_value(holder, *value);
            }
            BexExternalValue::Adt(BexExternalAdt::Collector(c)) => {
                Value::Object(holder.holder_mut().tlab_mut().alloc_collector(c))
            }
            BexExternalValue::Adt(BexExternalAdt::Type(ty)) => {
                Value::Object(holder.holder_mut().tlab_mut().alloc_type(ty))
            }
            BexExternalValue::Adt(BexExternalAdt::PromptAst(_)) => {
                return Err(EngineError::CannotConvert {
                    type_name: "PromptAst".to_string(),
                });
            }
            BexExternalValue::Adt(BexExternalAdt::Media(arc)) => {
                Value::Object(holder.holder_mut().tlab_mut().alloc_rust_data(arc))
            }
            BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle { heap_handle, .. }) => {
                Value::Object(self.resolve_handle(holder.proof(), &heap_handle).expect(
                    "TaggedHeapHandle should be valid - object was returned to external code",
                ))
            }
            BexExternalValue::FunctionRef { global_index } => {
                // `convert_external_to_vm_value` runs while `holder`'s
                // active heap permit is held, so we route through the
                // permit-proof-gated `SharedGlobals` API. The slice's
                // lifetime is tied to the proof, which is bounded by
                // `holder.proof()`'s borrow of `holder` — both end at the
                // close of this function call.
                let proof = holder.proof();
                let len = self.globals.len(proof);
                let slice = self.globals.as_slice(proof);
                if global_index >= len {
                    return Err(EngineError::TypeMismatch {
                        message: format!(
                            "FunctionRef global_index {global_index} out of \
                             bounds (globals len {len})"
                        ),
                    });
                }
                slice[global_index]
            }
        })
    }
}

// ============================================================================
// SysOp Argument Conversion
// ============================================================================

impl BexEngine {
    pub(crate) fn vm_arg_to_bex_value(&self, value: &Value) -> BexExternalValue {
        match value {
            Value::OmittedArg => {
                panic!("Cannot convert omitted argument sentinel to BexExternalValue")
            }
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
    pub(crate) fn vm_value_to_owned(
        &self,
        permit: PermitProof<'_>,
        value: &Value,
    ) -> BexExternalValue {
        match value {
            Value::OmittedArg => {
                panic!("Cannot convert omitted argument sentinel to BexExternalValue")
            }
            Value::Null => BexExternalValue::Null,
            Value::Int(i) => BexExternalValue::Int(*i),
            Value::Float(f) => BexExternalValue::Float(*f),
            Value::Bool(b) => BexExternalValue::Bool(*b),
            Value::Object(ptr) => BexValue::HeapPtr(ptr)
                .as_owned_for_trace(&self.heap, permit)
                .unwrap_or_else(|e| {
                    // Remaining errors here (InvalidHandle, TypeMismatch,
                    // FieldNotFound) indicate engine-level invariant
                    // violations — they shouldn't happen in normal operation.
                    // Surface via structured tracing rather than stderr so
                    // they're visible in logs without polluting CLI output,
                    // and embed the error in the trace payload so it shows
                    // up wherever traces are consumed.
                    tracing::error!(error = %e, "trace payload deep-copy failed");
                    BexExternalValue::String(format!("<trace-error: {e}>"))
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
        Ty::Union(members, _) => {
            let selected = find_matching_member(&value, members)?;
            let metadata = UnionMetadata::new(declared_type.clone(), selected);
            Ok(BexExternalValue::Union {
                value: Box::new(value),
                metadata,
            })
        }
        Ty::Optional(inner, _opt_attr) => {
            // Optional is just T | null. If the value is null, return Null directly.
            // If non-null, recurse into the inner type to preserve union metadata.
            if matches!(value, BexExternalValue::Null) {
                Ok(BexExternalValue::Null)
            } else {
                maybe_wrap_union(value, inner)
            }
        }
        _ => Ok(value),
    }
}

/// Find which union member matches a value.
///
/// `BuiltinUnknown` arms match any value (see `value_matches_type`) and are
/// considered last so a more-specific arm wins. This keeps the union
/// metadata's `selected_option` faithful when concrete arms (e.g.
/// `StreamFinished` in `BuiltinUnknown | StreamFinished`) actually fit.
fn find_matching_member(value: &BexExternalValue, members: &[Ty]) -> Result<Ty, EngineError> {
    for member in members {
        if !matches!(member, Ty::BuiltinUnknown { .. }) && value_matches_type(value, member) {
            return Ok(member.clone());
        }
    }
    for member in members {
        if matches!(member, Ty::BuiltinUnknown { .. }) {
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

fn type_name_matches_external_name(external_name: &str, type_name: &baml_type::TypeName) -> bool {
    if external_name == type_name.display_name.as_str() {
        return true;
    }

    if type_name.module_path.is_empty() {
        return external_name == type_name.name.as_str();
    }

    let qualified_name = type_name
        .module_path
        .iter()
        .map(baml_type::Name::as_str)
        .chain(std::iter::once(type_name.name.as_str()))
        .collect::<Vec<_>>()
        .join(".");

    external_name == qualified_name
}

fn resolve_named_object<'a>(
    objects: &'a indexmap::IndexMap<String, HeapPtr>,
    name: &str,
) -> Option<&'a HeapPtr> {
    // Direct hit (engine FQN, e.g., "user.lorem.MyLorem" or
    // "baml.http.Response").
    if let Some(found) = objects.get(name) {
        return Some(found);
    }
    // MIR's `qtn_to_type_name` strips the `user.` prefix from
    // `display_name` for user-package types, so an `Instance` arriving
    // from `coerce_arg_to_declared_type` may carry `lorem.MyLorem` while
    // the engine registered `user.lorem.MyLorem`. Try the `user.`
    // prefix as a fallback before giving up. Builtin/vendor types keep
    // their full FQN so this only fires for user-package classes.
    let user_qualified = format!("user.{name}");
    if let Some(found) = objects.get(&user_qualified) {
        return Some(found);
    }
    None
}

/// Check if a value matches a declared type.
fn value_matches_type(value: &BexExternalValue, ty: &Ty) -> bool {
    match (value, ty) {
        // `BuiltinUnknown` is the engine's "any value matches" sentinel
        // (TypeScript `unknown` semantics — see `baml_type::Ty::BuiltinUnknown`).
        // Used by the stdlib generics hardcode in `baml_compiler2_mir::lower`
        // so e.g. `Stream<TStream, TFinal>.next() -> TStream | StreamFinished`
        // accepts any partial-stream payload as the `TStream` arm.
        (_, Ty::BuiltinUnknown { .. }) => true,
        (BexExternalValue::Null, Ty::Null { .. }) => true,
        (BexExternalValue::Int(_), Ty::Int { .. }) => true,
        (BexExternalValue::Float(_), Ty::Float { .. }) => true,
        (BexExternalValue::Bool(_), Ty::Bool { .. }) => true,
        (BexExternalValue::String(_), Ty::String { .. }) => true,
        // Literal types match their corresponding runtime values
        (BexExternalValue::Int(_), Ty::Literal(Literal::Int(_), _)) => true,
        (BexExternalValue::Float(_), Ty::Literal(Literal::Float(_), _)) => true,
        (BexExternalValue::Uint8Array(_), Ty::Uint8Array { .. }) => true,
        (BexExternalValue::String(_), Ty::Literal(Literal::String(_), _)) => true,
        (BexExternalValue::Bool(_), Ty::Literal(Literal::Bool(_), _)) => true,
        (BexExternalValue::Array { .. }, Ty::List(_, _)) => true,
        (BexExternalValue::Map { .. }, Ty::Map { .. }) => true,
        // For FFI-boundary matching we only compare class names because
        // `BexExternalValue::Instance` does not carry class_type_args (that
        // field lives on the VM-side `Object::Instance`).  Fine-grained
        // generic disambiguation (e.g. `Foo<int>` vs `Foo<string>`) is
        // handled in-VM via `IsType` instructions (Phase 8.6) and
        // `find_matching_union_member` below.
        (BexExternalValue::Instance { class_name, .. }, Ty::Class(tn, _, _)) => {
            type_name_matches_external_name(class_name, tn)
        }
        (BexExternalValue::Variant { enum_name, .. }, Ty::Enum(tn, _)) => {
            type_name_matches_external_name(enum_name, tn)
        }
        (BexExternalValue::Adt(BexExternalAdt::Collector(_)), _) => false,
        (BexExternalValue::Adt(BexExternalAdt::Type(_)), ty)
            if ty.is_opaque("baml.reflect.Type") =>
        {
            true
        }
        (BexExternalValue::Union { value, .. }, ty) => value_matches_type(value, ty),
        // Handle nested unions/optionals in the type
        (value, Ty::Union(members, _)) => members.iter().any(|m| value_matches_type(value, m)),
        (value, Ty::Optional(inner, _)) => {
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
        Ty::Union(members, _) => find_matching_union_member(value, members)
            .unwrap_or_else(|| members.first().unwrap_or(declared_type)),
        Ty::Optional(inner, _) => {
            if matches!(value, Value::Null) {
                declared_type
            } else {
                resolve_effective_type(value, inner)
            }
        }
        _ => declared_type,
    }
}

/// Find the union member that matches the runtime value's type.
fn find_matching_union_member<'a>(value: &Value, members: &'a [Ty]) -> Option<&'a Ty> {
    match value {
        Value::OmittedArg => None,
        Value::Null => members.iter().find(|m| matches!(m, Ty::Null { .. })),
        Value::Int(_) => members
            .iter()
            .find(|m| matches!(m, Ty::Int { .. } | Ty::Literal(Literal::Int(_), _))),
        Value::Float(_) => members
            .iter()
            .find(|m| matches!(m, Ty::Float { .. } | Ty::Literal(Literal::Float(_), _))),
        Value::Bool(_) => members
            .iter()
            .find(|m| matches!(m, Ty::Bool { .. } | Ty::Literal(Literal::Bool(_), _))),
        Value::Object(ptr) => {
            let obj = unsafe { ptr.get() };
            match obj {
                Object::String(_) => members
                    .iter()
                    .find(|m| matches!(m, Ty::String { .. } | Ty::Literal(Literal::String(_), _))),
                Object::Instance(inst) => {
                    let class_obj = unsafe { inst.class.get() };
                    if let Object::Class(class) = class_obj {
                        // Compare both class name and type args.  When the
                        // union member has empty type args (e.g. a bare `Foo`)
                        // it matches any instance of `Foo` regardless of its
                        // class_type_args — preserving first-match semantics
                        // for `Foo<int> | Foo`.  When type args are present
                        // on the union member they must equal the instance's
                        // class_type_args exactly.
                        members.iter().find(|m| {
                            matches!(m, Ty::Class(tn, expected_args, _)
                                if *tn == class.name
                                && (expected_args.is_empty()
                                    || expected_args == &inst.class_type_args))
                        })
                    } else {
                        None
                    }
                }
                Object::Variant(variant) => {
                    let enum_obj = unsafe { variant.enm.get() };
                    if let Object::Enum(enm) = enum_obj {
                        members
                            .iter()
                            .find(|m| matches!(m, Ty::Enum(tn, _) if *tn == enm.name))
                    } else {
                        None
                    }
                }
                Object::Array(elements) => {
                    // For arrays, check first element to determine which List type
                    if let Some(first) = elements.first() {
                        members.iter().find(|m| {
                            if let Ty::List(elem_ty, _) = m {
                                find_matching_union_member(first, &[elem_ty.as_ref().clone()])
                                    .is_some()
                            } else {
                                false
                            }
                        })
                    } else {
                        // Empty array - match any List type
                        members.iter().find(|m| matches!(m, Ty::List(_, _)))
                    }
                }
                Object::Map(_) => members.iter().find(|m| matches!(m, Ty::Map { .. })),
                Object::Uint8Array(_) => {
                    members.iter().find(|m| matches!(m, Ty::Uint8Array { .. }))
                }
                // Types that don't participate in union discrimination.
                Object::Function(_)
                | Object::Closure(_)
                | Object::BoundMethod(_)
                | Object::Cell(_)
                | Object::Class(_)
                | Object::Enum(_)
                | Object::Future(_)
                | Object::UnscheduledFuture(_)
                | Object::RustData(_)
                | Object::Collector(_)
                | Object::Type(_) => None,
                #[cfg(feature = "heap_debug")]
                Object::Sentinel(_) => None,
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
        Value::OmittedArg => {
            panic!("Cannot convert omitted argument sentinel to BexExternalValue")
        }
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
                        element_type: bex_external_types::Ty::Null {
                            attr: baml_type::TyAttr::default(),
                        },
                        items,
                    }
                }
                Object::Map(map) => {
                    let entries: indexmap::IndexMap<String, BexExternalValue> = map
                        .iter()
                        .map(|(k, v)| (k.clone(), vm_arg_to_external(vm, v)))
                        .collect();
                    BexExternalValue::Map {
                        key_type: bex_external_types::Ty::String {
                            attr: baml_type::TyAttr::default(),
                        },
                        value_type: bex_external_types::Ty::Null {
                            attr: baml_type::TyAttr::default(),
                        },
                        entries,
                    }
                }
                Object::Instance(instance) => {
                    // Get class name from the class object
                    let class_obj = vm.get_object(instance.class);
                    let class_name = match class_obj {
                        Object::Class(class) => class.name.to_string(),
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
                Object::Uint8Array(bytes) => BexExternalValue::Uint8Array(bytes.clone()),
                Object::Variant(variant) => {
                    let enum_obj = vm.get_object(variant.enm);
                    let Object::Enum(enm) = enum_obj else {
                        panic!("variant.enm doesn't point to an Enum");
                    };
                    let variant_name = enm
                        .variants
                        .get(variant.index)
                        .map(|v| v.name.clone())
                        .unwrap_or_else(|| format!("<variant {}>", variant.index));
                    BexExternalValue::Variant {
                        enum_name: enm.name.to_string(),
                        variant_name,
                    }
                }
                // These types should not appear as sys op arguments.
                Object::Function(_)
                | Object::Closure(_)
                | Object::BoundMethod(_)
                | Object::Cell(_)
                | Object::Class(_)
                | Object::Enum(_)
                | Object::Future(_)
                | Object::UnscheduledFuture(_)
                | Object::RustData(_)
                | Object::Collector(_)
                | Object::Type(_) => {
                    panic!(
                        "Cannot convert object type to BexExternalValue for sys op: {:?}",
                        ObjectType::of(obj)
                    )
                }
                #[cfg(feature = "heap_debug")]
                Object::Sentinel(_) => {
                    panic!("Cannot convert sentinel to BexExternalValue")
                }
            }
        }
    }
}

/// Convert a compiled `TestArgValue` to a `BexExternalValue` for function calls.
pub fn test_arg_to_external(v: &bex_vm_types::TestArgValue) -> BexExternalValue {
    match v {
        bex_vm_types::TestArgValue::Null => BexExternalValue::Null,
        bex_vm_types::TestArgValue::Int(i) => BexExternalValue::Int(*i),
        bex_vm_types::TestArgValue::Float(f) => BexExternalValue::Float(*f),
        bex_vm_types::TestArgValue::Bool(b) => BexExternalValue::Bool(*b),
        bex_vm_types::TestArgValue::String(s) => BexExternalValue::String(s.clone()),
        bex_vm_types::TestArgValue::Array {
            element_type,
            items,
        } => BexExternalValue::Array {
            element_type: element_type.clone(),
            items: items.iter().map(test_arg_to_external).collect(),
        },
        bex_vm_types::TestArgValue::Map {
            key_type,
            value_type,
            entries,
        } => BexExternalValue::Map {
            key_type: key_type.clone(),
            value_type: value_type.clone(),
            entries: entries
                .iter()
                .map(|(k, v)| (k.clone(), test_arg_to_external(v)))
                .collect(),
        },
    }
}
