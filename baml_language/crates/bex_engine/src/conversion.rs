//! Value conversion functions between VM and external types.
//!
//! This module contains all the conversion logic for transforming values
//! between the VM representation (`Value`, `Object`) and the external
//! representation (`BexValue`, `BexExternalValue`).

use ::bex_heap::{BexValue, HeapPermit, PermitProof, TlabHolder};
use ::bex_vm_types::{HeapPtr, Object, ObjectType, RootHaver, Value, ValueKind};
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
        value: Value,
        declared_type: &Ty,
        permit: PermitProof<'_>,
    ) -> Result<BexExternalValue, EngineError> {
        // If declared type is a union, find which member matches the actual value
        let effective_type = resolve_effective_type(value, declared_type);

        let external = match value.kind() {
            ValueKind::OmittedArg => {
                return Err(EngineError::TypeMismatch {
                    message: "internal omitted argument escaped to external conversion".to_string(),
                });
            }
            ValueKind::Null => BexExternalValue::Null,
            ValueKind::Int(i) => BexExternalValue::Int(i),
            ValueKind::Bool(b) => BexExternalValue::Bool(b),
            ValueKind::Object(idx) => {
                self.convert_heap_ptr_to_external_with_type(idx, effective_type, permit)?
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
            Object::Float(f) => Ok(BexExternalValue::Float(*f)),
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

                // Snapshot under the source's lock; the recursive
                // convert call may lock other containers.
                let snapshot = arr.to_vec();
                let items: Result<Vec<_>, _> = snapshot
                    .iter()
                    .map(|v| self.convert_vm_value_to_external_with_type(*v, element_type, permit))
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

                let snapshot = map.to_index_map();
                let entries: Result<indexmap::IndexMap<String, BexExternalValue>, EngineError> =
                    snapshot
                        .iter()
                        .map(|(k, v)| {
                            Ok((
                                k.to_string(),
                                self.convert_vm_value_to_external_with_type(
                                    *v, value_type, permit,
                                )?,
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
                        .map(|(class_field, slot)| {
                            let value = slot.load();
                            let field_type = class_field
                                .field_template
                                .substitute(&instance.class_type_args);
                            Ok((
                                class_field.name.clone(),
                                self.convert_vm_value_to_external_with_type(
                                    value,
                                    &field_type,
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
            Object::Bigint(bi) => Ok(BexExternalValue::Bigint((**bi).clone())),
            Object::Collector(c) => Ok(BexExternalValue::Adt(BexExternalAdt::Collector(c.clone()))),
            Object::Type(ty) => Ok(BexExternalValue::Adt(BexExternalAdt::Type((**ty).clone()))),
            Object::Uint8Array(bytes) => Ok(BexExternalValue::Uint8Array(bytes.to_vec())),
            Object::RustData(arc) => Ok(bex_external_types::try_convert_rust_data(arc)
                .unwrap_or_else(|| BexExternalValue::RustData(arc.clone()))),
            Object::Closure(_) => Err(EngineError::CannotConvert {
                type_name: "closure".to_string(),
            }),
            Object::BoundMethod(_) => Err(EngineError::CannotConvert {
                type_name: "bound_method".to_string(),
            }),
            Object::GenericFunction(_) => Err(EngineError::CannotConvert {
                type_name: "generic_function".to_string(),
            }),
            Object::HostClosure(_) => Err(EngineError::CannotConvert {
                type_name: "host_closure".to_string(),
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
        // Default: no declared-type context. Inbound `HostValue` arguments
        // need the declared `Ty::Function` to materialize an
        // `Object::HostClosure` — callers that thread the type in should
        // use `convert_external_to_vm_value_with_ty`.
        self.convert_external_to_vm_value_with_ty(holder, external, None)
    }

    /// Like [`Self::convert_external_to_vm_value`], but threads the declared
    /// parameter `Ty` for the top-level value so a `BexExternalValue::HostValue`
    /// can be bound to its function signature as an [`Object::HostClosure`].
    ///
    /// `expected_ty` is honoured only at the top level — nested array
    /// elements / map values / instance fields fall back to the untyped path
    /// (`None`). Adding type-driven element handling here would require
    /// re-traversing the declared `Ty` in lockstep with the value; we don't
    /// yet support host callables in collection positions, so the
    /// type-context is dropped on entry into containers and any nested
    /// `HostValue` is rejected with `EngineError::CannotConvert`.
    pub(crate) fn convert_external_to_vm_value_with_ty<T: RootHaver + TlabHolder>(
        &self,
        holder: &mut impl HeapPermit<T>,
        external: BexExternalValue,
        expected_ty: Option<&Ty>,
    ) -> Result<Value, EngineError> {
        Ok(match external {
            BexExternalValue::Handle(handle) => Value::object(
                self.resolve_handle(holder.proof(), &handle)
                    .expect("Handle should be valid - object was returned to external code"),
            ),
            BexExternalValue::Null => Value::NULL,
            // The host integer is an `i64`, but the VM integer is `i63` (`Value`
            // reserves the low bit as a tag). Reject values outside the i63
            // range here rather than letting `Value::int` wrap them — a
            // release-build silent truncation, a debug-build panic. This also
            // gates the `bigint → int` FFI narrowing, whose `i64::try_from` only
            // bounds to the full i64 range.
            BexExternalValue::Int(i) => {
                Value::try_int(i).ok_or_else(|| EngineError::TypeMismatch {
                    message: format!(
                        "integer {i} is outside the BAML integer range [{}, {}]",
                        Value::INT_MIN,
                        Value::INT_MAX
                    ),
                })?
            }
            BexExternalValue::Bigint(bi) => {
                // Defense-in-depth: every upstream decoder (FFI hex,
                // SAP, Node.js/Python bridges) already caps oversized
                // bigints, but `Tlab::alloc_bigint` itself is the raw
                // allocator with no bound check. Guard here so any
                // future entry point that bypasses an upstream cap
                // still surfaces a graceful error instead of allocating
                // hundreds of MB on the VM heap.
                let bits = bi.bits();
                if bits > baml_type::MAX_BIGINT_BITS {
                    return Err(EngineError::TypeMismatch {
                        message: format!(
                            "bigint value requires {bits} bits (limit: {})",
                            baml_type::MAX_BIGINT_BITS
                        ),
                    });
                }
                Value::object(holder.holder_mut().tlab_mut().alloc_bigint(bi))
            }
            BexExternalValue::Float(f) => {
                Value::object(holder.holder_mut().tlab_mut().alloc(Object::Float(f)))
            }
            BexExternalValue::Bool(b) => Value::bool(b),
            BexExternalValue::String(s) => {
                Value::object(holder.holder_mut().tlab_mut().alloc_string(s))
            }
            BexExternalValue::Array { items, .. } => {
                let values = items
                    .into_iter()
                    .map(|v| self.convert_external_to_vm_value(holder, v))
                    .collect::<Result<Vec<_>, _>>()?;
                Value::object(holder.holder_mut().tlab_mut().alloc_array(values))
            }
            BexExternalValue::Map { entries, .. } => {
                let values = entries
                    .into_iter()
                    .map(|(k, v)| {
                        self.convert_external_to_vm_value(holder, v)
                            .map(|v| (bex_vm_types::BexStr::from(k.as_str()), v))
                    })
                    .collect::<Result<indexmap::IndexMap<bex_vm_types::BexStr, Value>, _>>()?;
                Value::object(holder.holder_mut().tlab_mut().alloc_map(values))
            }
            BexExternalValue::Uint8Array(bytes) => {
                Value::object(holder.holder_mut().tlab_mut().alloc_uint8array(bytes))
            }
            BexExternalValue::RustData(data) => {
                Value::object(holder.holder_mut().tlab_mut().alloc_rust_data(data))
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

                // Build field values in the order defined by the class.
                // Each field's declared `Ty` is passed as the conversion
                // context so type-polymorphic external values (notably
                // `BexExternalValue::HostValue`, which can land in either a
                // function-typed slot as a callable or a `$rust_type` slot
                // as an opaque handle) can branch on the declared shape.
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
                    values.push(self.convert_external_to_vm_value_with_ty(
                        holder,
                        ext.clone(),
                        Some(&class_field.field_type),
                    )?);
                }
                Value::object(
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
                Value::object(
                    holder
                        .holder_mut()
                        .tlab_mut()
                        .alloc_variant(*enum_ptr, index),
                )
            }
            BexExternalValue::Union { value, .. } => {
                return self.convert_external_to_vm_value_with_ty(holder, *value, expected_ty);
            }
            BexExternalValue::Adt(BexExternalAdt::Collector(c)) => {
                Value::object(holder.holder_mut().tlab_mut().alloc_collector(c))
            }
            BexExternalValue::Adt(BexExternalAdt::Type(ty)) => {
                Value::object(holder.holder_mut().tlab_mut().alloc_type(ty))
            }
            BexExternalValue::Adt(BexExternalAdt::PromptAst(_)) => {
                return Err(EngineError::CannotConvert {
                    type_name: "PromptAst".to_string(),
                });
            }
            BexExternalValue::Adt(BexExternalAdt::Media(arc)) => {
                Value::object(holder.holder_mut().tlab_mut().alloc_rust_data(arc))
            }
            BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle { heap_handle, .. }) => {
                Value::object(self.resolve_handle(holder.proof(), &heap_handle).expect(
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
            BexExternalValue::HostValue(arc) => {
                // A `HostValue` lands in one of two declared shapes:
                //
                // - `Ty::RustType` (opaque `$rust_type` field, e.g. the
                //   `_handle` slot on `baml.errors.HostCallable`): wrap the
                //   arc in `Object::RustData` so the BAML→host decoder can
                //   later downcast it back to a `HostValueArc`. No
                //   function signature involved.
                // - `Ty::Function` (host callable passed as a function
                //   argument): build a `HostClosure` bound to the declared
                //   signature so the call site can invoke it.
                let ty = expected_ty.ok_or_else(|| EngineError::CannotConvert {
                    type_name: "host_value (no declared type in context)".to_string(),
                })?;
                if matches!(peel_to_rust_type(ty), Some(())) {
                    let dyn_arc: std::sync::Arc<dyn std::any::Any + Send + Sync> = arc;
                    return Ok(Value::object(
                        holder.holder_mut().tlab_mut().alloc_rust_data(dyn_arc),
                    ));
                }
                // Peel through Optional / Union to land on the function type.
                let function_ty =
                    peel_function_ty(ty).ok_or_else(|| EngineError::TypeMismatch {
                        message: format!(
                            "host callable cannot be passed where the declared type \
                             is `{ty}`; expected a function type or `$rust_type`",
                        ),
                    })?;
                let (params, ret, throws) = match function_ty {
                    Ty::Function {
                        params,
                        ret,
                        throws,
                        ..
                    } => (params, ret.as_ref().clone(), throws.as_ref().clone()),
                    other => {
                        return Err(EngineError::TypeMismatch {
                            message: format!(
                                "host callable cannot be passed where the declared type \
                                 is `{other}`; expected a function type",
                            ),
                        });
                    }
                };
                // The host's returned value is validated against `ret` when
                // the call completes. A generic return type erases to
                // `Ty::Void` at MIR lowering (the post-erasure form of an
                // unbounded TypeVar; see [convert_tir2_ty]); a return type
                // explicitly declared `unknown` reaches here as
                // `Ty::BuiltinUnknown`. The return validator treats both as
                // "accept anything" — letting the host inject a value of any
                // type into a position BAML treats as the instantiated type
                // variable. Reject such a callable at bind time rather than
                // admit an unvalidatable return. (This also rejects a
                // genuine bare `-> void` host callable, which is
                // indistinguishable from an erased generic at runtime; such a
                // callable must declare a concrete return type.)
                if ret_ty_has_unvalidatable_position(&ret) {
                    return Err(EngineError::TypeMismatch {
                        message: format!(
                            "host callable cannot be bound: its return type `{ret}` is generic \
                             or void, so the host's returned value cannot be validated; host \
                             callables require a concrete return type",
                        ),
                    });
                }
                // `throws` is the callable's declared error contract `E`
                // (`call_host_value<T, E>`). An omitted throws on the
                // parameter is a synthesized generic effect param. By MIR
                // lowering ([baml_compiler2_mir::lower::convert_tir2_ty]
                // around the unbounded-TypeVar arm), such a generic erases
                // to `Ty::Void` at runtime — but `Void` is also a valid
                // declared type (a bare `-> void` throws contract, however
                // unusual). To keep the FFI boundary's intent explicit and
                // forward-compatible with a future FFI mechanism that
                // supplies a real host-reported error type, normalize the
                // erased-generic shape to `BuiltinUnknown` here. Concrete
                // throws (e.g. `throws ParseError`) pass through unchanged.
                let normalized_throws = match throws {
                    Ty::Void { attr } => Ty::BuiltinUnknown { attr },
                    other => other,
                };
                let host_closure = bex_vm_types::HostClosure {
                    handle: arc,
                    ret_ty: Box::new(ret),
                    throws_ty: Box::new(normalized_throws),
                    arity: params.len(),
                };
                Value::object(
                    holder
                        .holder_mut()
                        .tlab_mut()
                        .alloc(bex_vm_types::Object::HostClosure(host_closure)),
                )
            }
        })
    }
}

// ============================================================================
// SysOp Argument Conversion
// ============================================================================

/// If `ptr` references an `Object::Float`, return its `BexExternalValue::Float`
/// projection so callers can surface a primitive instead of an opaque handle.
///
/// # Safety
///
/// Caller must hold a GC permit (the `HeapPtr` deref invariant). Mirrors the
/// surrounding accessors that take `PermitProof` and dereference `HeapPtr`.
unsafe fn unbox_float_object(ptr: HeapPtr) -> Option<BexExternalValue> {
    if let Object::Float(f) = unsafe { ptr.get() } {
        Some(BexExternalValue::Float(*f))
    } else {
        None
    }
}

impl BexEngine {
    pub(crate) fn vm_arg_to_bex_value(&self, value: Value) -> BexExternalValue {
        match value.kind() {
            ValueKind::OmittedArg => {
                panic!("Cannot convert omitted argument sentinel to BexExternalValue")
            }
            ValueKind::Null => BexExternalValue::Null,
            ValueKind::Int(i) => BexExternalValue::Int(i),
            ValueKind::Bool(b) => BexExternalValue::Bool(b),
            ValueKind::Object(ptr) => {
                // SAFETY: caller holds the engine permit (heap is borrowed).
                if let Some(v) = unsafe { unbox_float_object(ptr) } {
                    return v;
                }
                let handle = self.heap.create_handle(ptr);
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
        value: Value,
    ) -> BexExternalValue {
        match value.kind() {
            ValueKind::OmittedArg => {
                panic!("Cannot convert omitted argument sentinel to BexExternalValue")
            }
            ValueKind::Null => BexExternalValue::Null,
            ValueKind::Int(i) => BexExternalValue::Int(i),
            ValueKind::Bool(b) => BexExternalValue::Bool(b),
            ValueKind::Object(ptr) => {
                // SAFETY: `permit` witnesses GC liveness for the deref.
                if let Some(v) = unsafe { unbox_float_object(ptr) } {
                    return v;
                }
                BexValue::HeapPtr(&ptr)
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
                        BexExternalValue::String(format!("<trace-error: {e}>").into())
                    })
            }
        }
    }

    /// Convert VM values to `BexExternalValues` for sys ops.
    ///
    /// This is simpler than `vm_value_to_external` because sys ops only receive
    /// primitives, strings, arrays, maps, and resources - not instances/variants.
    #[allow(unused)]
    pub(crate) fn vm_args_to_external(vm: &BexVm, args: &[Value]) -> Vec<BexExternalValue> {
        args.iter().map(|v| vm_arg_to_external(vm, *v)).collect()
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
        // A nullable union (`T?` == `T | null`) is optionality, not a tagged
        // union: a null value is bare `Null`, and a single non-null member is
        // unwrapped to its bare value (recursing in case that member is itself a
        // real union). This preserves the pre-desugaring behavior where optional
        // values carried no union metadata.
        Ty::Union(members, attr) if members.iter().any(Ty::is_null) => {
            if matches!(value, BexExternalValue::Null) {
                return Ok(BexExternalValue::Null);
            }
            let non_null: Vec<Ty> = members.iter().filter(|m| !m.is_null()).cloned().collect();
            match non_null.len() {
                0 => Ok(value),
                1 => maybe_wrap_union(value, &non_null[0]),
                _ => {
                    // Keep `null` in the recorded union type so the value stays
                    // marked optional (preserving the nullable FFI wire shape);
                    // select the matching non-null arm.
                    let selected = find_matching_member(&value, &non_null)?;
                    let metadata =
                        UnionMetadata::new(Ty::Union(members.clone(), attr.clone()), selected);
                    Ok(BexExternalValue::Union {
                        value: Box::new(value),
                        metadata,
                    })
                }
            }
        }
        Ty::Union(members, _) => {
            let selected = find_matching_member(&value, members)?;
            let metadata = UnionMetadata::new(declared_type.clone(), selected);
            Ok(BexExternalValue::Union {
                value: Box::new(value),
                metadata,
            })
        }
        _ => Ok(value),
    }
}

/// Peel `Optional` and singleton-`Union` wrappers off `ty`, returning the
/// underlying `Ty::Function` if there is one. Returns `None` if the type is
/// not a function (after peeling).
///
/// Used by `convert_external_to_vm_value_with_ty` to find the function
/// signature behind, e.g., `(int) -> int` and `((int) -> int)?` so that an
/// inbound `BexExternalValue::HostValue` can be bound to it as an
/// `Object::HostClosure`.
/// Returns `Some(())` if `ty` is the runtime representation of
/// `$rust_type` — i.e. `Ty::Opaque("baml.rust.RustType", _)` — possibly
/// wrapped in a `Union` (the post-`Ty::Optional`-removal encoding of
/// `T?` is `Ty::Union([T, Null], _)`, so nullable forms flow through
/// the union arm). Mirrors [`peel_function_ty`] for the `$rust_type`
/// field shape that a `HostValue` argument can land in.
pub(crate) fn peel_to_rust_type(ty: &Ty) -> Option<()> {
    if ty.is_opaque("baml.rust.RustType") {
        return Some(());
    }
    match ty {
        Ty::Union(members, _) => {
            let mut found = false;
            for m in members {
                if peel_to_rust_type(m).is_some() {
                    if found {
                        return None;
                    }
                    found = true;
                }
            }
            if found { Some(()) } else { None }
        }
        _ => None,
    }
}

pub(crate) fn peel_function_ty(ty: &Ty) -> Option<&Ty> {
    match ty {
        Ty::Function { .. } => Some(ty),
        Ty::Union(members, _) => {
            // Find the single function member, if any. If there are multiple
            // function members or none, we can't pick deterministically.
            let mut found: Option<&Ty> = None;
            for m in members {
                if let Some(f) = peel_function_ty(m) {
                    if found.is_some() {
                        return None;
                    }
                    found = Some(f);
                }
            }
            found
        }
        _ => None,
    }
}

/// Whether a host-callable's declared return type contains a position the
/// host-return validator treats as "accept anything": a `Ty::Void` (the runtime
/// form of an erased generic type variable, and also a bare `-> void`) or a
/// `Ty::BuiltinUnknown`. Recurses through `Optional` / `List` / `Map`-value /
/// `Union` / `Class`-generic-args so a nested erased position (`(T)[]`,
/// `Box<T>`) is caught too. A host callable with such a return type cannot have
/// its returned value validated, so binding one is rejected.
fn ret_ty_has_unvalidatable_position(ty: &Ty) -> bool {
    match ty {
        Ty::Void { .. } | Ty::BuiltinUnknown { .. } => true,
        Ty::List(elem, _) => ret_ty_has_unvalidatable_position(elem),
        Ty::Map { value, .. } => ret_ty_has_unvalidatable_position(value),
        Ty::Union(members, _) => members.iter().any(ret_ty_has_unvalidatable_position),
        Ty::Class(_, generic_args, _) => generic_args.iter().any(ret_ty_has_unvalidatable_position),
        _ => false,
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
        (BexExternalValue::Bigint(_), Ty::Bigint { .. }) => true,
        (BexExternalValue::Float(_), Ty::Float { .. }) => true,
        (BexExternalValue::Bool(_), Ty::Bool { .. }) => true,
        (BexExternalValue::String(_), Ty::String { .. }) => true,
        // Literal types match their corresponding runtime values
        (BexExternalValue::Int(_), Ty::Literal(Literal::Int(_), _)) => true,
        (BexExternalValue::Bigint(_), Ty::Literal(Literal::Bigint(_), _)) => true,
        (BexExternalValue::Float(_), Ty::Literal(Literal::Float(_), _)) => true,
        (BexExternalValue::Uint8Array(_), Ty::Uint8Array { .. }) => true,
        (BexExternalValue::String(_), Ty::Literal(Literal::String(_), _)) => true,
        (BexExternalValue::Bool(_), Ty::Literal(Literal::Bool(_), _)) => true,
        (BexExternalValue::Array { .. }, Ty::List(_, _)) => true,
        (BexExternalValue::Map { .. }, Ty::Map { .. }) => true,
        // A host-encoded object arrives as a bare `Map` (the JS encoder emits
        // every non-builtin object as `map_value`, no FQN), so a `Map`
        // matches a `Class` slot at the FFI boundary — it is promoted to an
        // `Instance` during materialization. This lets a host-built class
        // value satisfy a union's class member (e.g. `T | string`).
        (BexExternalValue::Map { .. }, Ty::Class(..)) => true,
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
        // Handle nested unions (including nullable `T | null`) in the type.
        (value, Ty::Union(members, _)) => members.iter().any(|m| value_matches_type(value, m)),
        _ => false,
    }
}

impl BexEngine {
    /// Schema-aware strict validation of a host-callable's returned value
    /// against its declared return type.
    ///
    /// This is the engine-side complement to the bridges' shared
    /// `bex_external_types::validate_host_return` guard. The shared guard runs
    /// at the FFI boundary and enforces everything checkable without a schema
    /// (scalar discrimination including `int` ≠ `float`, container recursion,
    /// enum identity, class-*name* identity). This method adds the one check
    /// the shared guard cannot perform — class *field types* — by resolving
    /// the declared class against the engine's compiled schema
    /// (`resolved_class_names`) and recursively validating each declared
    /// field's value against its instantiated field type.
    ///
    /// Returns `Err(message)` describing the first mismatch; the caller maps
    /// it to a `VmBamlError::HostCallable` so it surfaces as a catchable
    /// `root.errors.HostCallable`.
    pub(crate) fn validate_host_return_schema(
        &self,
        value: &BexExternalValue,
        expected: &Ty,
    ) -> Result<(), String> {
        match expected {
            // `unknown` / opaque-any: accept (defensive — concrete at the FFI
            // boundary).
            Ty::BuiltinUnknown { .. } => Ok(()),

            // Union (including nullable `T | null`): must satisfy at least one
            // member (schema-aware).
            Ty::Union(members, _) => {
                let inner = match value {
                    BexExternalValue::Union { value: inner, .. } => inner.as_ref(),
                    other => other,
                };
                if members
                    .iter()
                    .any(|m| self.validate_host_return_schema(inner, m).is_ok())
                {
                    Ok(())
                } else {
                    Err(format!(
                        "host callable returned a value of type `{}` that does not match the \
                         declared return type `{expected}`",
                        inner.type_name(),
                    ))
                }
            }

            // A `Union`-wrapped value against a non-union declared type:
            // validate the inner value.
            _ if matches!(value, BexExternalValue::Union { .. }) => {
                let BexExternalValue::Union { value: inner, .. } = value else {
                    unreachable!("guarded by the matches! above")
                };
                self.validate_host_return_schema(inner, expected)
            }

            Ty::List(inner, _) => match value {
                BexExternalValue::Array { items, .. } => {
                    for item in items {
                        self.validate_host_return_schema(item, inner)?;
                    }
                    Ok(())
                }
                other => Err(format!(
                    "host callable returned `{}` where a list was declared",
                    other.type_name(),
                )),
            },

            Ty::Map { value: v_ty, .. } => match value {
                BexExternalValue::Map { entries, .. } => {
                    for v in entries.values() {
                        self.validate_host_return_schema(v, v_ty)?;
                    }
                    Ok(())
                }
                other => Err(format!(
                    "host callable returned `{}` where a map was declared",
                    other.type_name(),
                )),
            },

            // The schema-aware part: validate each declared field's value
            // against its declared field type. A bare `Map` does NOT satisfy a
            // class type here: result materialization is value-driven, so a
            // `Map` becomes an `Object::Map`, never an instance of the declared
            // class — accepting it would hand back a value that cannot inhabit
            // the declared return type. A host returning a class must encode it
            // as a class value (→ `Instance`), not a plain map.
            Ty::Class(tn, expected_args, _) => match value {
                BexExternalValue::Instance { class_name, fields } => {
                    if !type_name_matches_external_name(class_name, tn) {
                        return Err(format!(
                            "host callable returned an instance of `{class_name}` where class \
                             `{tn}` was declared",
                        ));
                    }
                    let Some(class_ptr) = self
                        .resolved_class_names
                        .get(class_name)
                        .or_else(|| resolve_named_object(&self.resolved_class_names, class_name))
                    else {
                        // Unknown class: leave it to the engine's
                        // `convert_external_to_vm_value`, which errors with a
                        // clear "Unknown class" message.
                        return Ok(());
                    };
                    // SAFETY: class_ptr points to a compile-time Class object
                    // (a GC root for the program's lifetime).
                    #[expect(
                        unsafe_code,
                        reason = "reading a compile-time Class object via its GC-rooted pointer"
                    )]
                    let Object::Class(class) = (unsafe { class_ptr.get() }) else {
                        return Ok(());
                    };
                    for class_field in &class.fields {
                        if let Some(field_value) = fields.get(&class_field.name) {
                            let field_ty = class_field.field_template.substitute(expected_args);
                            self.validate_host_return_schema(field_value, &field_ty)?;
                        }
                        // Missing fields are reported by
                        // `convert_external_to_vm_value` at materialization.
                    }
                    Ok(())
                }
                other => Err(format!(
                    "host callable returned `{}` where class `{tn}` was declared",
                    other.type_name(),
                )),
            },

            // Enum identity: a `Variant` must name the declared enum, and the
            // variant must exist on that enum (the latter is also enforced by
            // `convert_external_to_vm_value`).
            Ty::Enum(tn, _) => match value {
                BexExternalValue::Variant {
                    enum_name,
                    variant_name,
                } => {
                    if !type_name_matches_external_name(enum_name, tn) {
                        return Err(format!(
                            "host callable returned a variant of enum `{enum_name}` where enum \
                             `{tn}` was declared",
                        ));
                    }
                    if let Some(enum_ptr) = self
                        .resolved_enum_names
                        .get(enum_name)
                        .or_else(|| resolve_named_object(&self.resolved_enum_names, enum_name))
                    {
                        #[expect(
                            unsafe_code,
                            reason = "reading a compile-time Enum object via its GC-rooted pointer"
                        )]
                        if let Object::Enum(enum_obj) = unsafe { enum_ptr.get() } {
                            if !enum_obj.variants.iter().any(|v| &v.name == variant_name) {
                                return Err(format!(
                                    "host callable returned unknown variant `{variant_name}` of \
                                     enum `{enum_name}`",
                                ));
                            }
                        }
                    }
                    Ok(())
                }
                other => Err(format!(
                    "host callable returned `{}` where enum `{tn}` was declared",
                    other.type_name(),
                )),
            },

            // A function-typed return position is not supported yet. The host
            // call result is materialized by `convert_external_to_vm_value`
            // *without* a declared type, so a returned `HostValue` (the only
            // value that inhabits a function type) cannot be bound to an
            // `Object::HostClosure` and would otherwise fail downstream as a raw
            // `EngineError::CannotConvert`. Reject it here so it surfaces as a
            // structured, catchable `HostCallable` instead. This arm covers a
            // top-level function return and — via the `List` / `Map` / `Class`
            // recursion above — any nested function position (`(() -> int)[]`,
            // `class { cb: () -> int }`, …).
            Ty::Function { .. } => Err(format!(
                "host callable returned a value typed `{expected}`; returning a \
                 callable (a function-typed value) is not supported",
            )),

            // Scalars and everything else: defer to the schema-free shape
            // check (int ≠ float, exact tags, literal equality, media).
            _ => {
                bex_external_types::validate_host_return(value, expected).map_err(|e| e.to_string())
            }
        }
    }
}

/// For union types, find which member matches the actual runtime value.
///
/// If the declared type is not a union, returns it unchanged.
fn resolve_effective_type(value: Value, declared_type: &Ty) -> &Ty {
    match declared_type {
        Ty::Union(members, _) => find_matching_union_member(value, members)
            .unwrap_or_else(|| members.first().unwrap_or(declared_type)),
        _ => declared_type,
    }
}

/// Find the union member that matches the runtime value's type.
fn find_matching_union_member(value: Value, members: &[Ty]) -> Option<&Ty> {
    match value.kind() {
        ValueKind::OmittedArg => None,
        ValueKind::Null => members.iter().find(|m| matches!(m, Ty::Null { .. })),
        ValueKind::Int(_) => members
            .iter()
            .find(|m| matches!(m, Ty::Int { .. } | Ty::Literal(Literal::Int(_), _))),
        ValueKind::Bool(_) => members
            .iter()
            .find(|m| matches!(m, Ty::Bool { .. } | Ty::Literal(Literal::Bool(_), _))),
        ValueKind::Object(ptr) => {
            let obj = unsafe { ptr.get() };
            match obj {
                Object::Float(_) => members
                    .iter()
                    .find(|m| matches!(m, Ty::Float { .. } | Ty::Literal(Literal::Float(_), _))),
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
                    if let Some(first) = elements.get(0) {
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
                Object::Bigint(_) => members
                    .iter()
                    .find(|m| matches!(m, Ty::Bigint { .. } | Ty::Literal(Literal::Bigint(_), _))),
                // Types that don't participate in union discrimination.
                Object::Function(_)
                | Object::Closure(_)
                | Object::BoundMethod(_)
                | Object::GenericFunction(_)
                | Object::HostClosure(_)
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
pub(crate) fn vm_arg_to_external(vm: &BexVm, value: Value) -> BexExternalValue {
    match value.kind() {
        ValueKind::OmittedArg => {
            panic!("Cannot convert omitted argument sentinel to BexExternalValue")
        }
        ValueKind::Null => BexExternalValue::Null,
        ValueKind::Int(i) => BexExternalValue::Int(i),
        ValueKind::Bool(b) => BexExternalValue::Bool(b),
        ValueKind::Object(idx) => {
            let obj = vm.get_object(idx);
            match obj {
                Object::Float(f) => BexExternalValue::Float(*f),
                Object::String(s) => BexExternalValue::String(s.clone()),
                Object::Array(arr) => {
                    let snap = arr.to_vec();
                    let items: Vec<BexExternalValue> =
                        snap.iter().map(|v| vm_arg_to_external(vm, *v)).collect();
                    BexExternalValue::Array {
                        element_type: bex_external_types::Ty::Null {
                            attr: baml_type::TyAttr::default(),
                        },
                        items,
                    }
                }
                Object::Map(map) => {
                    let snap = map.to_index_map();
                    let entries: indexmap::IndexMap<String, BexExternalValue> = snap
                        .iter()
                        .map(|(k, v)| (k.to_string(), vm_arg_to_external(vm, *v)))
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
                        .map(|(class_field, slot)| {
                            (
                                class_field.name.clone(),
                                vm_arg_to_external(vm, slot.load()),
                            )
                        })
                        .collect();

                    BexExternalValue::Instance { class_name, fields }
                }
                Object::Bigint(bi) => BexExternalValue::Bigint((**bi).clone()),
                Object::Uint8Array(bytes) => BexExternalValue::Uint8Array(bytes.to_vec()),
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
                | Object::GenericFunction(_)
                | Object::HostClosure(_)
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

/// Coerce a host-encoded **incoming** value to match the declared param type.
///
/// Handles two layers of bridge mismatch:
///
/// 1. **Class / enum naming:** host encoders carry an informational class or
///    variant name (e.g. `root.lorem.MyLorem`); rewrite it to the
///    engine-registered FQN (`user.lorem.MyLorem`) so VM heap lookups hit. A
///    plain `Map` arriving at a class slot is also promoted to `Instance`.
/// 2. **Numeric / optional / union coercion:** see `coerce_numeric_to_declared_type`.
///
/// Nested container types (arrays/maps with mismatched element types) are not
/// walked; host-side schema-aware encoders own that shaping.
pub(crate) fn coerce_arg_to_declared_type(
    value: BexExternalValue,
    ty: &Ty,
) -> Result<BexExternalValue, EngineError> {
    match (value, ty) {
        // ── Class / enum naming (incoming only) ──────────────────────────
        (BexExternalValue::Map { entries, .. }, Ty::Class(type_name, _, _)) => {
            Ok(BexExternalValue::Instance {
                class_name: type_name.to_string(),
                fields: entries,
            })
        }
        (BexExternalValue::Instance { fields, .. }, Ty::Class(type_name, _, _)) => {
            Ok(BexExternalValue::Instance {
                class_name: type_name.to_string(),
                fields,
            })
        }
        (BexExternalValue::Variant { variant_name, .. }, Ty::Enum(type_name, _)) => {
            Ok(BexExternalValue::Variant {
                enum_name: type_name.to_string(),
                variant_name,
            })
        }

        // ── Union with a class member (incoming only) ────────────────────
        // A host-encoded object arrives as a bare `Map` (the JS encoder emits
        // every non-builtin object as `map_value`, with no FQN). Against a
        // union it would otherwise fail `value_matches_type` ("Value of type
        // 'map' does not match any member of union [...]"). Route it to the
        // union's class-typed member (unwrapping `Optional`) and promote it to
        // an `Instance`. The wire value carries no class name, so we pick the
        // first class arm — sufficient while a union has at most one class
        // member; numeric/string arms are left to the existing routing.
        (
            value @ (BexExternalValue::Map { .. } | BexExternalValue::Instance { .. }),
            Ty::Union(members, _),
        ) => {
            if let Some(class_arm) = members.iter().find_map(union_class_arm) {
                coerce_arg_to_declared_type(value, class_arm)
            } else {
                Ok(value)
            }
        }

        // ── Numeric / optional / union ───────────────────────────────────
        (v, ty) => coerce_numeric_to_declared_type(v, ty),
    }
}

/// If `ty` is a class (directly, or inside an `Optional`), return that class
/// `Ty`. Used to route a host-encoded object value to a union's class member.
fn union_class_arm(ty: &Ty) -> Option<&Ty> {
    match ty {
        Ty::Class(..) => Some(ty),
        _ => None,
    }
}

/// Coerce an **outgoing** return value to match the declared return type.
///
/// Handles int↔bigint conversion at the FFI boundary. These conversions exist
/// **only** at the host boundary — the type system is purely structural and
/// does not relate `int` and `bigint` (see
/// `baml_compiler2_tir::normalize::is_subtype_of`). `int → bigint` widens
/// unconditionally; `bigint → int` succeeds when the value fits in i64,
/// erroring on overflow rather than silently truncating. Also performs optional
/// unwrap and numeric-singleton union routing.
///
/// Class / enum naming is intentionally *not* rewritten here — the
/// engine-side FQN (e.g. `user.lorem.MyLorem`) is the authoritative
/// output and stripping it back to the bare display name would break
/// host-side type lookups.
pub(crate) fn coerce_return_to_declared_type(
    value: BexExternalValue,
    ty: &Ty,
) -> Result<BexExternalValue, EngineError> {
    coerce_numeric_to_declared_type(value, ty)
}

/// Shared numeric / optional / union coercion used by both arg and return
/// paths.
///
/// These conversions exist only at the FFI boundary. The compile-time subtype
/// relation (`baml_compiler2_tir::normalize::is_subtype_of`,
/// `baml_type::Ty::is_subtype_of`) is purely structural and does **not** widen
/// `int` to `bigint`; the arms below add that widening (plus a checked
/// `bigint → int` narrowing) only when crossing the host boundary.
fn coerce_numeric_to_declared_type(
    value: BexExternalValue,
    ty: &Ty,
) -> Result<BexExternalValue, EngineError> {
    match (value, ty) {
        // Int → Bigint widening (FFI boundary only — `int` is not a subtype of
        // `bigint` in the type system).
        (BexExternalValue::Int(i), Ty::Bigint { .. } | Ty::Literal(Literal::Bigint(_), _)) => {
            Ok(BexExternalValue::Bigint(num_bigint::BigInt::from(i)))
        }

        // Bigint → Int narrowing: host-supplied bigint must fit in i64, otherwise
        // there is no safe representation in the `int` slot and we reject the
        // call rather than silently truncate.
        (BexExternalValue::Bigint(bi), Ty::Int { .. } | Ty::Literal(Literal::Int(_), _)) => {
            i64::try_from(&bi)
                .map(BexExternalValue::Int)
                .map_err(|_| EngineError::TypeMismatch {
                    message: format!("bigint value {bi} does not fit in i64"),
                })
        }

        // Union with exactly one of {Int, Bigint}: route to that member.
        // Nullable numeric unions (`int | null`) flow through here too — a
        // `Null` value coerced against the chosen numeric member falls to the
        // catch-all `(v, _) => Ok(v)` and is preserved.
        // Unions containing both are left alone; `find_matching_union_member`
        // picks by value shape at the VM boundary.
        (v, Ty::Union(members, _)) => {
            let has_int = members
                .iter()
                .any(|m| matches!(m, Ty::Int { .. } | Ty::Literal(Literal::Int(_), _)));
            let has_bigint = members
                .iter()
                .any(|m| matches!(m, Ty::Bigint { .. } | Ty::Literal(Literal::Bigint(_), _)));
            if has_int == has_bigint {
                Ok(v)
            } else if let Some(target) = members.iter().find(|m| {
                matches!(
                    m,
                    Ty::Int { .. }
                        | Ty::Bigint { .. }
                        | Ty::Literal(Literal::Int(_) | Literal::Bigint(_), _)
                )
            }) {
                coerce_numeric_to_declared_type(v, target)
            } else {
                Ok(v)
            }
        }

        (v, _) => Ok(v),
    }
}

/// Convert a compiled `TestArgValue` to a `BexExternalValue` for function calls.
pub fn test_arg_to_external(v: &bex_vm_types::TestArgValue) -> BexExternalValue {
    match v {
        bex_vm_types::TestArgValue::Null => BexExternalValue::Null,
        bex_vm_types::TestArgValue::Int(i) => BexExternalValue::Int(*i),
        bex_vm_types::TestArgValue::Float(f) => BexExternalValue::Float(*f),
        bex_vm_types::TestArgValue::Bool(b) => BexExternalValue::Bool(*b),
        bex_vm_types::TestArgValue::String(s) => BexExternalValue::String(s.as_str().into()),
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

#[cfg(test)]
mod peel_to_rust_type_tests {
    use baml_type::{TyAttr, TypeName};

    use super::*;

    /// `Ty::Opaque("baml.rust.RustType", _)` — the canonical shape.
    fn rust_type() -> Ty {
        Ty::Opaque(
            TypeName::from_dotted_path("baml.rust.RustType"),
            TyAttr::default(),
        )
    }

    #[test]
    fn direct_rust_type_matches() {
        assert_eq!(peel_to_rust_type(&rust_type()), Some(()));
    }

    #[test]
    fn optional_rust_type_peels_through() {
        // `Ty::optional(RustType)` lowers to `Ty::Union([RustType, Null])`
        // post-`Ty::Optional`-removal; the union arm in `peel_to_rust_type`
        // picks the single RustType member.
        let ty = Ty::optional(rust_type());
        assert_eq!(peel_to_rust_type(&ty), Some(()));
    }

    #[test]
    fn nested_optional_rust_type_peels_through() {
        // `T??` collapses to `T?` per `Ty::optional`'s idempotence rule
        // (a union already containing `null` is returned unchanged), so
        // this is effectively the same shape as the single-optional case
        // — still a single non-null member that peels.
        let ty = Ty::optional(Ty::optional(rust_type()));
        assert_eq!(peel_to_rust_type(&ty), Some(()));
    }

    #[test]
    fn singleton_union_with_rust_type_and_null_matches() {
        // `RustType | null` — only one non-null arm so the peel
        // unambiguously picks `RustType`.
        let ty = Ty::Union(
            vec![
                rust_type(),
                Ty::Null {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        );
        assert_eq!(peel_to_rust_type(&ty), Some(()));
    }

    #[test]
    fn union_with_rust_type_plus_non_rust_arm_still_unique_matches() {
        // `RustType | string` — there's still exactly one `RustType` arm,
        // and `peel_to_rust_type` only cares about uniqueness of *that*
        // shape (non-`RustType` arms count as "doesn't match" and don't
        // contribute to the duplicate-count).
        let ty = Ty::Union(
            vec![
                rust_type(),
                Ty::String {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        );
        assert_eq!(peel_to_rust_type(&ty), Some(()));
    }

    #[test]
    fn union_with_two_rust_type_arms_is_ambiguous() {
        // `RustType | RustType` — two arms peel to the target. The
        // function rejects to avoid silently picking one.
        let ty = Ty::Union(vec![rust_type(), rust_type()], TyAttr::default());
        assert_eq!(peel_to_rust_type(&ty), None);
    }

    #[test]
    fn plain_string_does_not_match() {
        assert_eq!(
            peel_to_rust_type(&Ty::String {
                attr: TyAttr::default()
            }),
            None,
        );
    }

    #[test]
    fn unrelated_opaque_does_not_match() {
        // A different opaque type — e.g. `baml.llm.PromptAst` — must
        // not be confused with `baml.rust.RustType`.
        let ty = Ty::Opaque(
            TypeName::from_dotted_path("baml.llm.PromptAst"),
            TyAttr::default(),
        );
        assert_eq!(peel_to_rust_type(&ty), None);
    }

    #[test]
    fn optional_of_unrelated_type_does_not_match() {
        let ty = Ty::optional(Ty::String {
            attr: TyAttr::default(),
        });
        assert_eq!(peel_to_rust_type(&ty), None);
    }

    #[test]
    fn union_with_no_rust_type_arm_does_not_match() {
        let ty = Ty::Union(
            vec![
                Ty::String {
                    attr: TyAttr::default(),
                },
                Ty::Int {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        );
        assert_eq!(peel_to_rust_type(&ty), None);
    }
}

#[cfg(test)]
mod peel_function_ty_tests {
    use baml_type::TyAttr;

    use super::*;

    /// `(int) -> string` — the canonical concrete function shape.
    fn fn_ty() -> Ty {
        Ty::Function {
            params: vec![Ty::Int {
                attr: TyAttr::default(),
            }],
            ret: Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Void {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        }
    }

    /// A second, distinct function shape — `() -> int` — used to verify the
    /// uniqueness rule rejects two function members in a union.
    fn other_fn_ty() -> Ty {
        Ty::Function {
            params: vec![],
            ret: Box::new(Ty::Int {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Void {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        }
    }

    #[test]
    fn direct_function_returns_itself() {
        let ty = fn_ty();
        let peeled = peel_function_ty(&ty).expect("must peel a direct Function");
        assert!(matches!(peeled, Ty::Function { .. }));
    }

    #[test]
    fn optional_function_peels_through() {
        // `Ty::optional(fn)` lowers to `Ty::Union([fn, Null])`; the union
        // arm in `peel_function_ty` picks the single function member.
        let ty = Ty::optional(fn_ty());
        let peeled = peel_function_ty(&ty).expect("Union<fn, Null> must peel");
        assert!(matches!(peeled, Ty::Function { .. }));
    }

    #[test]
    fn nested_optional_function_peels_through() {
        // `Ty::optional` is idempotent — `T??` collapses to `T?` — so this
        // is effectively the same shape as the single-optional case.
        let ty = Ty::optional(Ty::optional(fn_ty()));
        assert!(peel_function_ty(&ty).is_some());
    }

    #[test]
    fn union_with_single_function_arm_peels_through() {
        // `((int) -> string) | null` — only one function member.
        let ty = Ty::Union(
            vec![
                fn_ty(),
                Ty::Null {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        );
        assert!(peel_function_ty(&ty).is_some());
    }

    #[test]
    fn union_with_function_plus_non_function_arm_peels_through() {
        // `((int) -> string) | string` — exactly one function member.
        let ty = Ty::Union(
            vec![
                fn_ty(),
                Ty::String {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        );
        assert!(peel_function_ty(&ty).is_some());
    }

    #[test]
    fn union_with_two_distinct_function_arms_is_ambiguous() {
        // `((int) -> string) | (() -> int)` — two function members.
        // The peel rejects to avoid silently picking one. Pins the
        // determinism contract of the helper.
        let ty = Ty::Union(vec![fn_ty(), other_fn_ty()], TyAttr::default());
        assert!(peel_function_ty(&ty).is_none());
    }

    #[test]
    fn plain_string_does_not_match() {
        let ty = Ty::String {
            attr: TyAttr::default(),
        };
        assert!(peel_function_ty(&ty).is_none());
    }

    #[test]
    fn optional_of_non_function_does_not_match() {
        let ty = Ty::optional(Ty::String {
            attr: TyAttr::default(),
        });
        assert!(peel_function_ty(&ty).is_none());
    }

    #[test]
    fn union_with_no_function_arm_does_not_match() {
        let ty = Ty::Union(
            vec![
                Ty::String {
                    attr: TyAttr::default(),
                },
                Ty::Int {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        );
        assert!(peel_function_ty(&ty).is_none());
    }

    #[test]
    fn empty_union_does_not_match() {
        let ty = Ty::Union(vec![], TyAttr::default());
        assert!(peel_function_ty(&ty).is_none());
    }
}
