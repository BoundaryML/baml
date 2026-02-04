//! Value conversion functions between VM and external types.
//!
//! This module contains all the conversion logic for transforming values
//! between the VM representation (`Value`, `Object`) and the external
//! representation (`BexValue`, `BexExternalValue`).

use bex_external_types::{BexExternalValue, BexValue, EpochGuard, Ty, UnionMetadata};
use bex_vm::BexVm;
use bex_vm_types::{HeapPtr, Object, Value};

use crate::{BexEngine, EngineError};

// ============================================================================
// Public Conversion Methods
// ============================================================================

impl BexEngine {
    /// Convert a `BexValue` to a `BexExternalValue` (owned data).
    ///
    /// - For `External` variants: returns the value directly
    /// - For `Opaque(Handle)`: resolves the handle and deep-copies
    ///
    /// If the declared type is a union, the value is wrapped in `Union { value, metadata }`.
    ///
    /// # Supported Object Types
    ///
    /// - `String` → `BexExternalValue::String`
    /// - `Array` → `BexExternalValue::Array` (recursively converts elements)
    /// - `Map` → `BexExternalValue::Map` (recursively converts values)
    /// - `Instance` → `BexExternalValue::Instance` (includes class name and fields)
    /// - `Variant` → `BexExternalValue::Variant` (includes enum and variant names)
    ///
    /// # Errors
    ///
    /// Returns `EngineError::CannotConvert` for object types that cannot be
    /// converted (Function, Class, Enum, Future, Media).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let result = engine.call_function("get_user", &[]).await?;
    /// match result {
    ///     BexExternalValue::Instance { class_name, fields } => {
    ///         println!("Got {} with {} fields", class_name, fields.len());
    ///     }
    ///     BexExternalValue::Union { value, metadata } => {
    ///         println!("Union selected: {}", metadata.selected_option);
    ///     }
    ///     _ => {}
    /// }
    /// ```
    pub fn to_bex_external(
        &self,
        value: BexValue,
        declared_type: &Ty,
    ) -> Result<BexExternalValue, EngineError> {
        match value {
            BexValue::External(s) => maybe_wrap_union(s, declared_type),
            BexValue::Opaque(handle) => self.handle_to_external(&handle, declared_type),
        }
    }

    /// Convert a handle to a `BexExternalValue` using the declared type.
    ///
    /// This is safe for external code to call (no `EpochGuard` needed) because
    /// we hold the handle table read lock for the entire operation, preventing
    /// GC from moving objects while we're converting.
    pub(crate) fn handle_to_external(
        &self,
        handle: &bex_external_types::Handle,
        declared_type: &Ty,
    ) -> Result<BexExternalValue, EngineError> {
        // Hold the handles read lock for the entire conversion operation.
        // This prevents GC from running update_handles (which needs write lock),
        // ensuring all ObjectIndex values remain valid during recursive conversion.
        //
        // The GcProtectedHeap guard ensures resolve_handle can only be called
        // while the lock is held - you can't accidentally use it unsafely.
        self.heap.with_gc_protection(|protected| {
            let idx = protected
                .resolve_handle(handle.slab_key())
                .expect("Handle is a GC root - object should never be collected");
            let value = Value::Object(idx);
            self.vm_value_to_external(&value, declared_type)
        })
    }

    /// Convert a VM Value to a `BexValue`.
    ///
    /// Primitives become `External(BexExternalValue)`, heap objects get a `Handle`.
    pub(crate) fn value_to_external(&self, value: Value) -> BexValue {
        match value {
            Value::Null => BexValue::External(BexExternalValue::Null),
            Value::Int(i) => BexValue::External(BexExternalValue::Int(i)),
            Value::Float(f) => BexValue::External(BexExternalValue::Float(f)),
            Value::Bool(b) => BexValue::External(BexExternalValue::Bool(b)),
            Value::Object(idx) => {
                let handle = self.heap().create_handle(idx);
                BexValue::Opaque(handle)
            }
        }
    }
}

// ============================================================================
// VM Value to External Conversion
// ============================================================================

impl BexEngine {
    /// Convert a VM Value to a `BexExternalValue` using the declared type.
    ///
    /// If the declared type is a union, the value is wrapped in `Union { value, metadata }`.
    pub(crate) fn vm_value_to_external(
        &self,
        value: &Value,
        declared_type: &Ty,
    ) -> Result<BexExternalValue, EngineError> {
        // If declared type is a union, find which member matches the actual value
        let effective_type = resolve_effective_type(value, declared_type);

        let external = match value {
            Value::Null => BexExternalValue::Null,
            Value::Int(i) => BexExternalValue::Int(*i),
            Value::Float(f) => BexExternalValue::Float(*f),
            Value::Bool(b) => BexExternalValue::Bool(*b),
            Value::Object(idx) => self.vm_object_to_external(*idx, effective_type)?,
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
    fn vm_object_to_external(
        &self,
        ptr: HeapPtr,
        effective_type: &Ty,
    ) -> Result<BexExternalValue, EngineError> {
        // SAFETY: We only read objects, and the pointer comes from a valid handle.
        let obj = unsafe { ptr.get() };

        match obj {
            Object::String(s) => Ok(BexExternalValue::String(s.clone())),

            Object::Array(arr) => {
                // Get element type from declared type
                let element_type = match effective_type {
                    Ty::List(elem_ty) => elem_ty.as_ref(),
                    other => {
                        return Err(EngineError::TypeMismatch {
                            message: format!("VM has Array but declared type is {other:?}"),
                        });
                    }
                };

                let items: Result<Vec<_>, _> = arr
                    .iter()
                    .map(|v| self.vm_value_to_external(v, element_type))
                    .collect();
                Ok(BexExternalValue::Array {
                    element_type: element_type.clone(),
                    items: items?,
                })
            }

            Object::Map(map) => {
                // Get key and value types from declared type
                let (key_type, value_type) = match effective_type {
                    Ty::Map { key, value } => (key.as_ref(), value.as_ref()),
                    other => {
                        return Err(EngineError::TypeMismatch {
                            message: format!("VM has Map but declared type is {other:?}"),
                        });
                    }
                };

                let entries: Result<indexmap::IndexMap<String, BexExternalValue>, EngineError> =
                    map.iter()
                        .map(|(k, v)| Ok((k.clone(), self.vm_value_to_external(v, value_type)?)))
                        .collect();
                Ok(BexExternalValue::Map {
                    key_type: key_type.clone(),
                    value_type: value_type.clone(),
                    entries: entries?,
                })
            }

            Object::Instance(instance) => {
                // Get class name from the Class object
                let class_obj = unsafe { instance.class.get() };
                let (class_name, field_names) = match class_obj {
                    Object::Class(class) => (class.name.clone(), &class.field_names),
                    _ => panic!("Instance.class should point to a Class object"),
                };

                // Look up field types from the schema
                let class_def = self.snapshot.classes.get(&class_name).ok_or_else(|| {
                    EngineError::SchemaInconsistency {
                        message: format!("Class '{class_name}' not found in schema"),
                    }
                })?;

                // Build field type lookup map once (O(n) instead of O(n^2))
                let field_types: std::collections::HashMap<&str, &Ty> = class_def
                    .fields
                    .iter()
                    .map(|f| (f.name.as_str(), &f.field_type))
                    .collect();

                // Convert fields with their declared types
                let fields: Result<indexmap::IndexMap<String, BexExternalValue>, EngineError> =
                    field_names
                        .iter()
                        .zip(instance.fields.iter())
                        .map(|(name, value)| {
                            // Look up the field's declared type from the pre-built map (O(1))
                            let field_type = field_types.get(name.as_str()).ok_or_else(|| {
                                EngineError::SchemaInconsistency {
                                    message: format!(
                                        "Field '{name}' not found in class '{class_name}'"
                                    ),
                                }
                            })?;

                            Ok((name.clone(), self.vm_value_to_external(value, field_type)?))
                        })
                        .collect();

                Ok(BexExternalValue::Instance {
                    class_name,
                    fields: fields?,
                })
            }

            Object::Variant(variant) => {
                // Get enum name and variant name from the Enum object
                let enum_obj = unsafe { variant.enm.get() };
                let (enum_name, variant_name) = match enum_obj {
                    Object::Enum(enm) => {
                        let variant_name = enm
                            .variant_names
                            .get(variant.index)
                            .cloned()
                            .unwrap_or_else(|| format!("variant_{}", variant.index));
                        (enm.name.clone(), variant_name)
                    }
                    _ => panic!("Variant.enm should point to an Enum object"),
                };

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
            Object::Media(m) => Ok(BexExternalValue::Media {
                handle: self.heap().create_handle(ptr),
                kind: m.kind,
            }),
            Object::Resource(handle) => Ok(BexExternalValue::Resource(handle.clone())),
            Object::PromptAst(ast) => {
                // Convert VM PromptAst to external PromptAst
                Ok(BexExternalValue::PromptAst(
                    sys_llm::vm_prompt_ast_to_external(ast),
                ))
            }
            Object::PrimitiveClient(_) => Err(EngineError::CannotConvert {
                type_name: "primitive_client".to_string(),
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
    /// Convert an `ExternalValue` to a VM `Value`.
    ///
    /// Requires `EpochGuard` because resolving handles returns an `ObjectIndex`
    /// that must remain valid while we use it.
    ///
    /// - `Opaque(Handle)` extracts the `HeapPtr`
    /// - `External(...)` recursively allocates on the heap
    pub(crate) fn externalize_to_value(
        vm: &mut BexVm,
        external: &BexValue,
        guard: &EpochGuard<'_>,
    ) -> Value {
        match external {
            BexValue::Opaque(handle) => {
                // Resolve through table to get current pointer after any GC
                let ptr = handle
                    .object_ptr(guard)
                    .expect("Handle should be valid - object was returned to external code");
                Value::Object(ptr)
            }
            BexValue::External(ext) => Self::allocate_from_external(vm, ext, guard),
        }
    }

    /// Recursively allocate a `BexExternalValue` onto the heap, returning a `Value`.
    pub(crate) fn allocate_from_external(
        vm: &mut BexVm,
        external: &BexExternalValue,
        guard: &EpochGuard<'_>,
    ) -> Value {
        match external {
            BexExternalValue::Null => Value::Null,
            BexExternalValue::Int(i) => Value::Int(*i),
            BexExternalValue::Float(f) => Value::Float(*f),
            BexExternalValue::Bool(b) => Value::Bool(*b),
            BexExternalValue::String(s) => vm.alloc_string(s.clone()),
            BexExternalValue::Array { items, .. } => {
                let values: Vec<Value> = items
                    .iter()
                    .map(|item| Self::allocate_from_external(vm, item, guard))
                    .collect();
                vm.alloc_array(values)
            }
            BexExternalValue::Map { entries, .. } => {
                let values: indexmap::IndexMap<String, Value> = entries
                    .iter()
                    .map(|(k, v): (&String, &BexExternalValue)| {
                        (k.clone(), Self::allocate_from_external(vm, v, guard))
                    })
                    .collect();
                vm.alloc_map(values)
            }
            BexExternalValue::Instance { .. } => {
                // Instance allocation requires class lookup - not supported from external
                todo!(
                    "Cannot allocate Instance from BexExternalValue. We need to do a string lookup for the right type in the schema."
                )
            }
            BexExternalValue::Variant { .. } => {
                // Variant allocation requires enum lookup - not supported from external
                todo!(
                    "Cannot allocate Variant from BexExternalValue. We need to do a string lookup for the right type in the schema."
                )
            }
            BexExternalValue::Union { value, .. } => {
                // Unwrap the union and allocate the inner value
                Self::allocate_from_external(vm, value, guard)
            }
            BexExternalValue::Media { handle, .. } => {
                let ptr = handle
                    .object_ptr(guard)
                    .expect("Handle should be valid - object was returned to external code");
                Value::Object(ptr)
            }
            BexExternalValue::Resource(handle) => vm.alloc_resource(handle.clone()),
            BexExternalValue::PromptAst(ast) => {
                // Convert external PromptAst to VM PromptAst
                let vm_ast = Self::external_prompt_ast_to_vm(vm, ast, guard);
                vm.alloc_prompt_ast(vm_ast)
            }
            BexExternalValue::PrimitiveClient(client) => {
                // Allocate options map to heap
                let options: indexmap::IndexMap<String, Value> = client
                    .options
                    .iter()
                    .map(|(k, v)| (k.clone(), Self::allocate_from_external(vm, v, guard)))
                    .collect();
                let options_ptr = vm.alloc_map(options);
                let Value::Object(options_heap_ptr) = options_ptr else {
                    panic!("alloc_map should return an Object");
                };
                vm.alloc_primitive_client(bex_vm_types::PrimitiveClient {
                    name: client.name.clone(),
                    provider: client.provider.clone(),
                    default_role: client.default_role.clone(),
                    allowed_roles: client.allowed_roles.clone(),
                    options: options_heap_ptr,
                })
            }
            BexExternalValue::FunctionRef { global_index } => {
                // Return the function value from the VM's globals array
                vm.globals[bex_vm_types::GlobalIndex::from_raw(*global_index)]
            }
        }
    }

    /// Convert external `PromptAst` to VM `PromptAst`.
    pub(crate) fn external_prompt_ast_to_vm(
        vm: &mut BexVm,
        ast: &bex_external_types::PromptAst,
        guard: &EpochGuard<'_>,
    ) -> bex_vm_types::PromptAst {
        match ast {
            bex_external_types::PromptAst::String(s) => bex_vm_types::PromptAst::String(s.clone()),
            bex_external_types::PromptAst::Media(handle) => bex_vm_types::PromptAst::Media(*handle),
            bex_external_types::PromptAst::Message {
                role,
                content,
                metadata,
            } => {
                let vm_content = Self::external_prompt_ast_to_vm(vm, content, guard);
                let metadata_value = Self::allocate_from_external(vm, metadata, guard);
                bex_vm_types::PromptAst::Message {
                    role: role.clone(),
                    content: Box::new(vm_content),
                    metadata: metadata_value,
                }
            }
            bex_external_types::PromptAst::Vec(items) => {
                let vm_items: Vec<_> = items
                    .iter()
                    .map(|item| Self::external_prompt_ast_to_vm(vm, item, guard))
                    .collect();
                bex_vm_types::PromptAst::Vec(vm_items)
            }
        }
    }

    /// Convert a `BexExternalValue` result from sys ops back to a VM Value.
    pub(crate) fn external_to_vm_value(&self, vm: &mut BexVm, external: BexExternalValue) -> Value {
        match external {
            BexExternalValue::Null => Value::Null,
            BexExternalValue::Int(i) => Value::Int(i),
            BexExternalValue::Float(f) => Value::Float(f),
            BexExternalValue::Bool(b) => Value::Bool(b),
            BexExternalValue::String(s) => vm.alloc_string(s),
            BexExternalValue::Array { items, .. } => {
                let values: Vec<Value> = items
                    .into_iter()
                    .map(|v| self.external_to_vm_value(vm, v))
                    .collect();
                vm.alloc_array(values)
            }
            BexExternalValue::Map { entries, .. } => {
                let values: indexmap::IndexMap<String, Value> = entries
                    .into_iter()
                    .map(|(k, v)| (k, self.external_to_vm_value(vm, v)))
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
                let field_names = match unsafe { class_ptr.get() } {
                    Object::Class(class) => &class.field_names,
                    _ => panic!("class_ptr must point to Class"),
                };

                // Build field values in the order defined by the class
                let mut values = Vec::with_capacity(field_names.len());
                for name in field_names {
                    let ext = fields
                        .get(name)
                        .unwrap_or_else(|| panic!("missing field '{name}' in Instance"));
                    values.push(self.external_to_vm_value(vm, ext.clone()));
                }
                vm.alloc_instance(*class_ptr, values)
            }
            BexExternalValue::Variant { .. } => {
                panic!("Unexpected Variant from sys op")
            }
            BexExternalValue::Union { value, .. } => self.external_to_vm_value(vm, *value),
            BexExternalValue::Media { .. } => {
                panic!("Unexpected Media from sys op")
            }
            BexExternalValue::PromptAst(ast) => {
                // Convert external PromptAst to VM PromptAst
                let vm_ast = self.external_prompt_ast_to_vm_owned(vm, ast);
                vm.alloc_prompt_ast(vm_ast)
            }
            BexExternalValue::PrimitiveClient(client) => {
                // Allocate options map to heap
                let options: indexmap::IndexMap<String, Value> = client
                    .options
                    .into_iter()
                    .map(|(k, v)| (k, self.external_to_vm_value(vm, v)))
                    .collect();
                let options_ptr = vm.alloc_map(options);
                let Value::Object(options_heap_ptr) = options_ptr else {
                    panic!("alloc_map should return an Object");
                };
                vm.alloc_primitive_client(bex_vm_types::PrimitiveClient {
                    name: client.name,
                    provider: client.provider,
                    default_role: client.default_role,
                    allowed_roles: client.allowed_roles,
                    options: options_heap_ptr,
                })
            }
            BexExternalValue::FunctionRef { global_index } => {
                // Return the function value from the VM's globals array
                vm.globals[bex_vm_types::GlobalIndex::from_raw(global_index)]
            }
        }
    }

    /// Convert owned external `PromptAst` to VM `PromptAst`.
    pub(crate) fn external_prompt_ast_to_vm_owned(
        &self,
        vm: &mut BexVm,
        ast: bex_external_types::PromptAst,
    ) -> bex_vm_types::PromptAst {
        match ast {
            bex_external_types::PromptAst::String(s) => bex_vm_types::PromptAst::String(s),
            bex_external_types::PromptAst::Media(handle) => bex_vm_types::PromptAst::Media(handle),
            bex_external_types::PromptAst::Message {
                role,
                content,
                metadata,
            } => {
                let vm_content = self.external_prompt_ast_to_vm_owned(vm, *content);
                let metadata_value = self.external_to_vm_value(vm, *metadata);
                bex_vm_types::PromptAst::Message {
                    role,
                    content: Box::new(vm_content),
                    metadata: metadata_value,
                }
            }
            bex_external_types::PromptAst::Vec(items) => {
                let vm_items: Vec<_> = items
                    .into_iter()
                    .map(|item| self.external_prompt_ast_to_vm_owned(vm, item))
                    .collect();
                bex_vm_types::PromptAst::Vec(vm_items)
            }
        }
    }
}

// ============================================================================
// SysOp Argument Conversion
// ============================================================================

impl BexEngine {
    /// Convert VM values to `BexValue` for sys ops.
    ///
    /// For primitive values (null, int, float, bool, string), creates `BexValue::External`.
    /// For objects (instances, arrays, maps), creates `BexValue::Opaque` with a handle.
    /// This allows sys ops to access heap objects via GC-protected handle resolution.
    pub(crate) fn vm_args_to_bex_values(&self, vm: &BexVm, args: &[Value]) -> Vec<BexValue> {
        args.iter()
            .map(|v| self.vm_arg_to_bex_value(vm, v))
            .collect()
    }

    pub(crate) fn vm_arg_to_bex_value(&self, vm: &BexVm, value: &Value) -> BexValue {
        match value {
            Value::Null => BexValue::External(BexExternalValue::Null),
            Value::Int(i) => BexValue::External(BexExternalValue::Int(*i)),
            Value::Float(f) => BexValue::External(BexExternalValue::Float(*f)),
            Value::Bool(b) => BexValue::External(BexExternalValue::Bool(*b)),
            Value::Object(ptr) => {
                let obj = vm.get_object(*ptr);
                match obj {
                    // Strings are small enough to copy
                    Object::String(s) => BexValue::External(BexExternalValue::String(s.clone())),
                    // For instances, create a handle so sys ops can access fields via GC protection
                    Object::Instance(_) => {
                        let handle = self.heap.create_handle(*ptr);
                        BexValue::Opaque(handle)
                    }
                    // For arrays and maps used as sys op args, copy them out
                    Object::Array(arr) => {
                        let items: Vec<BexExternalValue> =
                            arr.iter().map(|v| vm_arg_to_external(vm, v)).collect();
                        BexValue::External(BexExternalValue::Array {
                            element_type: bex_external_types::Ty::Null,
                            items,
                        })
                    }
                    Object::Map(map) => {
                        let entries: indexmap::IndexMap<String, BexExternalValue> = map
                            .iter()
                            .map(|(k, v)| (k.clone(), vm_arg_to_external(vm, v)))
                            .collect();
                        BexValue::External(BexExternalValue::Map {
                            key_type: bex_external_types::Ty::String,
                            value_type: bex_external_types::Ty::Null,
                            entries,
                        })
                    }
                    Object::Resource(handle) => {
                        BexValue::External(BexExternalValue::Resource(handle.clone()))
                    }
                    // PrimitiveClient needs to be copied out for render_prompt
                    Object::PrimitiveClient(client) => {
                        let options_map = vm.get_object(client.options);
                        let options = if let Object::Map(map) = options_map {
                            map.iter()
                                .map(|(k, v)| (k.clone(), vm_arg_to_external(vm, v)))
                                .collect()
                        } else {
                            indexmap::IndexMap::new()
                        };
                        BexValue::External(BexExternalValue::PrimitiveClient(
                            bex_external_types::PrimitiveClientValue {
                                name: client.name.clone(),
                                provider: client.provider.clone(),
                                default_role: client.default_role.clone(),
                                allowed_roles: client.allowed_roles.clone(),
                                options,
                            },
                        ))
                    }
                    // PromptAst needs to be copied out for specialize_prompt
                    Object::PromptAst(ast) => BexValue::External(BexExternalValue::PromptAst(
                        sys_llm::vm_prompt_ast_to_external(ast),
                    )),
                    other => {
                        panic!("Cannot convert object type to BexValue for sys op: {other:?}")
                    }
                }
            }
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
        (BexExternalValue::Int(_), Ty::Literal(baml_base::Literal::Int(_))) => true,
        (BexExternalValue::Float(_), Ty::Literal(baml_base::Literal::Float(_))) => true,
        (BexExternalValue::String(_), Ty::Literal(baml_base::Literal::String(_))) => true,
        (BexExternalValue::Bool(_), Ty::Literal(baml_base::Literal::Bool(_))) => true,
        (BexExternalValue::Array { .. }, Ty::List(_)) => true,
        (BexExternalValue::Map { .. }, Ty::Map { .. }) => true,
        (BexExternalValue::Instance { class_name, .. }, Ty::Class(tn)) => {
            class_name.as_str() == tn.display_name.as_str()
        }
        (BexExternalValue::Variant { enum_name, .. }, Ty::Enum(tn)) => {
            enum_name.as_str() == tn.display_name.as_str()
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
            .find(|m| matches!(m, Ty::Int | Ty::Literal(baml_base::Literal::Int(_)))),
        Value::Float(_) => members
            .iter()
            .find(|m| matches!(m, Ty::Float | Ty::Literal(baml_base::Literal::Float(_)))),
        Value::Bool(_) => members
            .iter()
            .find(|m| matches!(m, Ty::Bool | Ty::Literal(baml_base::Literal::Bool(_)))),
        Value::Object(ptr) => {
            let obj = unsafe { ptr.get() };
            match obj {
                Object::String(_) => members
                    .iter()
                    .find(|m| matches!(m, Ty::String | Ty::Literal(baml_base::Literal::String(_)))),
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
                Object::PrimitiveClient(client) => {
                    // Extract options map from heap
                    let options_map = vm.get_object(client.options);
                    let options = if let Object::Map(map) = options_map {
                        map.iter()
                            .map(|(k, v)| (k.clone(), vm_arg_to_external(vm, v)))
                            .collect()
                    } else {
                        indexmap::IndexMap::new()
                    };
                    BexExternalValue::PrimitiveClient(bex_external_types::PrimitiveClientValue {
                        name: client.name.clone(),
                        provider: client.provider.clone(),
                        default_role: client.default_role.clone(),
                        allowed_roles: client.allowed_roles.clone(),
                        options,
                    })
                }
                Object::Instance(instance) => {
                    // Get class name from the class object
                    let class_obj = vm.get_object(instance.class);
                    let class_name = match class_obj {
                        Object::Class(class) => class.name.clone(),
                        _ => panic!("Instance class pointer doesn't point to a Class"),
                    };

                    // Get field names from class and convert fields
                    let field_names = match class_obj {
                        Object::Class(class) => &class.field_names,
                        _ => panic!("Instance class pointer doesn't point to a Class"),
                    };

                    let fields: indexmap::IndexMap<String, BexExternalValue> = field_names
                        .iter()
                        .zip(instance.fields.iter())
                        .map(|(name, value)| (name.clone(), vm_arg_to_external(vm, value)))
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
