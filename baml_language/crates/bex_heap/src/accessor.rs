//! Safe accessor API for external code to read heap objects.
//!
//! External code (`baml_sys`) runs outside the epoch system and cannot
//! safely hold bare `HeapPtr` values. This module provides an API
//! that holds the handle table lock during access, preventing GC races.
//!
//! # Example
//!
//! ```ignore
//! // In baml_sys code (no EpochGuard available)
//! let content = heap.read_string(&handle)?;
//!
//! // Or for complex access with GC protection:
//! let result = heap.with_gc_protection(|protected| {
//!     let ptr = protected.resolve_handle(handle.slab_key())?;
//!     // ptr is safe to use - GC cannot run while we hold the guard
//!     Some(recursive_snapshot(protected, ptr))
//! });
//! ```

use std::sync::RwLockReadGuard;

use baml_type::Ty;
use bex_external_types::{BexExternalAdt, BexExternalValue};
use bex_vm_types::{HeapPtr, Object, Value};

use crate::BexHeap;

/// Guard type proving the handles read lock is held.
///
/// This type can only be obtained from `BexHeap::with_gc_protection`.
/// Methods that return `HeapPtr` require this guard to ensure
/// the pointer remains valid (GC cannot run while the lock is held).
pub struct GcProtectedHeap<'a> {
    // Hold the read lock - prevents GC from updating handles
    _guard: RwLockReadGuard<'a, std::collections::HashMap<usize, HeapPtr>>,
}

impl<'a> GcProtectedHeap<'a> {
    /// Resolve a handle's slab key to a HeapPtr.
    ///
    /// Safe because we hold the handles read lock, preventing GC from
    /// moving objects and invalidating pointers.
    pub fn resolve_handle(&self, slab_key: usize) -> Option<HeapPtr> {
        self._guard.get(&slab_key).copied()
    }

    pub fn epoch_guard(&self) -> bex_external_types::EpochGuard<'_> {
        unsafe { bex_external_types::EpochGuard::new() }
    }
}

/// Safe object access API for external code.
///
/// All methods hold the handle table read lock during access,
/// ensuring GC cannot run and invalidate pointers mid-operation.
impl BexHeap {
    /// Execute a closure while holding the handles read lock.
    ///
    /// This prevents GC from updating handle pointers during the operation.
    /// The closure receives a `GcProtectedHeap` which provides safe access
    /// to `resolve_handle` - you can't accidentally call it without the lock.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Resolve a handle while protected from GC
    /// let snapshot = heap.with_gc_protection(|protected| {
    ///     let ptr = protected.resolve_handle(handle.slab_key())?;
    ///     // ... use ptr to read objects
    /// });
    /// ```
    pub fn with_gc_protection<R>(
        self: &std::sync::Arc<Self>,
        f: impl FnOnce(GcProtectedHeap<'_>) -> R,
    ) -> R {
        let guard = self.handles.read().expect("handles lock poisoned");
        f(GcProtectedHeap { _guard: guard })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AccessError {
    #[error("Invalid handle: expected {expected}")]
    InvalidHandle { expected: &'static str },

    #[error("Type mismatch: expected {expected}, got {actual}")]
    TypeMismatch {
        expected: &'static str,
        actual: String,
    },

    #[error("Field not found: expected {expected}")]
    FieldNotFound { expected: String },

    #[error("Function not found: {expected}")]
    FunctionNotFound { expected: String },

    #[error("Cannot convert to owned: {reason}")]
    CannotConvertToOwned { reason: String },
}

pub enum BexValue<'a> {
    ExternalValue(&'a BexExternalValue),
    HeapPtr(&'a HeapPtr),
    Value(&'a Value),
}

impl<'a> From<&'a BexExternalValue> for BexValue<'a> {
    fn from(value: &'a BexExternalValue) -> Self {
        BexValue::ExternalValue(value)
    }
}

pub enum BexClass<'a> {
    ExternalClass {
        name: &'a String,
        fields: &'a indexmap::IndexMap<String, BexExternalValue>,
    },
    Value(&'a bex_vm_types::Class, &'a bex_vm_types::Instance),
}

pub enum BexVariant<'a> {
    ExternalVariant { name: &'a String, value: &'a String },
    Value(&'a bex_vm_types::Enum, &'a bex_vm_types::Variant),
}

impl<'a> BexClass<'a> {
    pub fn class_name(&self) -> &'a String {
        match self {
            BexClass::ExternalClass { name, .. } => name,
            BexClass::Value(class, ..) => &class.name,
        }
    }

    pub fn field(&self, name: &str) -> Result<BexValue<'a>, AccessError> {
        match self {
            BexClass::ExternalClass { fields, .. } => match fields.get(name) {
                Some(value) => Ok(BexValue::ExternalValue(value)),
                None => Err(AccessError::FieldNotFound {
                    expected: name.to_string(),
                }),
            },
            BexClass::Value(class, instance) => {
                let field_idx = class
                    .fields
                    .iter()
                    .position(|field| field.name == name)
                    .ok_or_else(|| AccessError::FieldNotFound {
                        expected: name.to_string(),
                    })?;
                let field =
                    instance
                        .fields
                        .get(field_idx)
                        .ok_or_else(|| AccessError::FieldNotFound {
                            expected: name.to_string(),
                        })?;
                Ok(BexValue::Value(field))
            }
        }
    }
}

impl<'a> BexVariant<'a> {
    pub fn enum_name(&self) -> &'a String {
        match self {
            BexVariant::ExternalVariant { name, .. } => name,
            BexVariant::Value(enum_, ..) => &enum_.name,
        }
    }

    pub fn value(&self) -> Result<&'a String, AccessError> {
        match self {
            BexVariant::ExternalVariant { name: _, value } => Ok(value),
            BexVariant::Value(variant, value) => {
                let value = variant.variants.get(value.index).ok_or_else(|| {
                    AccessError::FieldNotFound {
                        expected: value.to_string(),
                    }
                })?;
                Ok(&value.name)
            }
        }
    }
}

impl<'a> BexValue<'a> {
    fn type_name(&self) -> String {
        match self {
            BexValue::ExternalValue(value) => value.type_name().to_string(),
            BexValue::HeapPtr(ptr) => ptr.to_string(),
            BexValue::Value(value) => value.to_string().to_string(),
        }
    }

    pub fn as_int(self) -> Result<i64, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::Int(i)) => Ok(*i),
            BexValue::Value(Value::Int(i)) => Ok(*i),
            other => Err(AccessError::TypeMismatch {
                expected: "int",
                actual: other.type_name(),
            }),
        }
    }

    pub fn as_float(self) -> Result<f64, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::Float(f)) => Ok(*f),
            BexValue::Value(Value::Float(f)) => Ok(*f),
            other => Err(AccessError::TypeMismatch {
                expected: "float",
                actual: other.type_name(),
            }),
        }
    }

    pub fn as_bool(self) -> Result<bool, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::Bool(b)) => Ok(*b),
            BexValue::Value(Value::Bool(b)) => Ok(*b),
            other => Err(AccessError::TypeMismatch {
                expected: "bool",
                actual: other.type_name(),
            }),
        }
    }

    pub fn as_null(self) -> Result<(), AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::Null) => Ok(()),
            BexValue::Value(Value::Null) => Ok(()),
            other => Err(AccessError::TypeMismatch {
                expected: "null",
                actual: other.type_name(),
            }),
        }
    }

    #[allow(clippy::wrong_self_convention)]
    fn as_object<T>(
        self,
        expected: &'static str,
        heap: &GcProtectedHeap<'_>,
        f: impl FnOnce(&HeapPtr) -> Result<T, AccessError>,
    ) -> Result<T, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::Handle(ptr)) => {
                let ptr = heap
                    .resolve_handle(ptr.slab_key())
                    .ok_or(AccessError::InvalidHandle { expected })?;
                f(&ptr)
            }
            BexValue::Value(Value::Object(ptr)) | BexValue::HeapPtr(ptr) => f(ptr),
            other => Err(AccessError::TypeMismatch {
                expected,
                actual: other.type_name(),
            }),
        }
    }

    pub fn as_resource_handle(
        self,
        heap: &GcProtectedHeap<'_>,
    ) -> Result<bex_resource_types::ResourceHandle, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::Resource(handle)) => Ok(handle.clone()),
            other => other.as_object("resource", heap, |ptr| {
                let obj = unsafe { ptr.get() };
                let Object::Resource(resource) = obj else {
                    return Err(AccessError::TypeMismatch {
                        expected: "resource",
                        actual: obj.to_string(),
                    });
                };
                Ok(resource.clone())
            }),
        }
    }
    pub fn as_string(self, heap: &GcProtectedHeap<'_>) -> Result<&'a String, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::String(s)) => Ok(s),
            other => other.as_object("string", heap, |ptr| {
                let obj = unsafe { ptr.get() };
                let Object::String(s) = obj else {
                    return Err(AccessError::TypeMismatch {
                        expected: "string",
                        actual: obj.to_string(),
                    });
                };
                Ok(s)
            }),
        }
    }

    pub fn as_array(self, heap: &GcProtectedHeap<'_>) -> Result<Vec<BexValue<'a>>, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::Array { items, .. }) => {
                Ok(items.iter().map(BexValue::ExternalValue).collect())
            }
            other => other.as_object("array", heap, |ptr| {
                let obj = unsafe { ptr.get() };
                let Object::Array(array) = obj else {
                    return Err(AccessError::TypeMismatch {
                        expected: "array",
                        actual: obj.to_string(),
                    });
                };
                Ok(array.iter().map(BexValue::Value).collect())
            }),
        }
    }

    pub fn as_map(
        self,
        heap: &GcProtectedHeap<'_>,
    ) -> Result<indexmap::IndexMap<String, BexValue<'a>>, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::Map { entries, .. }) => Ok(entries
                .iter()
                .map(|(k, v)| (k.clone(), BexValue::ExternalValue(v)))
                .collect()),
            other => other.as_object("map", heap, |ptr| {
                let obj = unsafe { ptr.get() };
                let Object::Map(map) = obj else {
                    return Err(AccessError::TypeMismatch {
                        expected: "map",
                        actual: obj.to_string(),
                    });
                };
                Ok(map
                    .iter()
                    .map(|(k, v)| (k.clone(), BexValue::Value(v)))
                    .collect())
            }),
        }
    }

    #[allow(clippy::wrong_self_convention)]
    fn as_class(
        self,
        heap: &GcProtectedHeap<'_>,
        expected_class_name: &'static str,
    ) -> Result<BexClass<'a>, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::Instance { class_name, fields }) => {
                if class_name != expected_class_name {
                    return Err(AccessError::TypeMismatch {
                        expected: expected_class_name,
                        actual: class_name.to_string(),
                    });
                }
                Ok(BexClass::ExternalClass {
                    name: class_name,
                    fields,
                })
            }
            other => other.as_object("instance", heap, |ptr| {
                let obj = unsafe { ptr.get() };
                let Object::Instance(instance) = obj else {
                    return Err(AccessError::TypeMismatch {
                        expected: "instance",
                        actual: obj.to_string(),
                    });
                };
                let class_obj = unsafe { instance.class.get() };
                let Object::Class(class) = class_obj else {
                    return Err(AccessError::TypeMismatch {
                        expected: "class",
                        actual: class_obj.to_string(),
                    });
                };
                if class.name != expected_class_name {
                    return Err(AccessError::TypeMismatch {
                        expected: expected_class_name,
                        actual: class.name.to_string(),
                    });
                }
                Ok(BexClass::Value(class, instance))
            }),
        }
    }

    pub fn as_enum<T>(
        self,
        heap: &GcProtectedHeap<'_>,
        expected_enum_name: &'static str,
        map_fn: impl FnOnce(BexVariant<'_>) -> T,
    ) -> Result<T, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::Variant {
                enum_name,
                variant_name,
            }) => {
                if enum_name != expected_enum_name {
                    return Err(AccessError::TypeMismatch {
                        expected: expected_enum_name,
                        actual: enum_name.to_string(),
                    });
                }
                Ok(map_fn(BexVariant::ExternalVariant {
                    name: enum_name,
                    value: variant_name,
                }))
            }
            BexValue::ExternalValue(BexExternalValue::Handle(ptr)) => {
                let ptr = heap
                    .resolve_handle(ptr.slab_key())
                    .ok_or(AccessError::InvalidHandle { expected: "enum" })?;
                BexValue::HeapPtr(&ptr).as_enum(heap, expected_enum_name, map_fn)
            }
            BexValue::Value(Value::Object(ptr)) | BexValue::HeapPtr(ptr) => {
                let obj = unsafe { ptr.get() };
                let Object::Variant(variant) = obj else {
                    return Err(AccessError::TypeMismatch {
                        expected: expected_enum_name,
                        actual: obj.to_string(),
                    });
                };
                let enum_obj = unsafe { variant.enm.get() };
                let Object::Enum(enum_) = enum_obj else {
                    return Err(AccessError::TypeMismatch {
                        expected: "enum",
                        actual: enum_obj.to_string(),
                    });
                };
                if enum_.name != expected_enum_name {
                    return Err(AccessError::TypeMismatch {
                        expected: expected_enum_name,
                        actual: enum_.name.to_string(),
                    });
                }
                Ok(map_fn(BexVariant::Value(enum_, variant)))
            }
            other => Err(AccessError::TypeMismatch {
                expected: expected_enum_name,
                actual: other.type_name(),
            }),
        }
    }

    pub fn as_builtin_class<T: BuiltinClass<'a>>(
        self,
        heap: &GcProtectedHeap<'_>,
    ) -> Result<T, AccessError> {
        self.as_class(heap, T::name()).map(|cls| T::from(cls))
    }

    pub fn as_media(
        self,
        heap: &GcProtectedHeap<'_>,
    ) -> Result<bex_vm_types::MediaValue, AccessError> {
        fn from_ptr(ptr: &HeapPtr) -> Result<bex_vm_types::MediaValue, AccessError> {
            let obj = unsafe { ptr.get() };
            let Object::Media(media) = obj else {
                return Err(AccessError::TypeMismatch {
                    expected: "media",
                    actual: obj.to_string(),
                });
            };
            Ok(media.clone())
        }

        match self {
            BexValue::ExternalValue(BexExternalValue::Adt(BexExternalAdt::Media(media))) => {
                Ok(media.clone())
            }
            BexValue::ExternalValue(BexExternalValue::Handle(handle)) => {
                let ptr = heap
                    .resolve_handle(handle.slab_key())
                    .ok_or(AccessError::InvalidHandle { expected: "media" })?;
                from_ptr(&ptr)
            }
            BexValue::Value(Value::Object(ptr)) | BexValue::HeapPtr(ptr) => from_ptr(ptr),
            other => Err(AccessError::TypeMismatch {
                expected: "media",
                actual: other.type_name(),
            }),
        }
    }

    pub fn as_prompt_ast_owned(
        self,
        heap: &GcProtectedHeap<'_>,
    ) -> Result<bex_vm_types::PromptAst, AccessError> {
        fn from_ptr(ptr: &HeapPtr) -> Result<bex_vm_types::PromptAst, AccessError> {
            let obj = unsafe { ptr.get() };
            let Object::PromptAst(ast) = obj else {
                return Err(AccessError::TypeMismatch {
                    expected: "prompt ast",
                    actual: obj.to_string(),
                });
            };
            Ok(ast.clone())
        }

        match self {
            BexValue::ExternalValue(BexExternalValue::Adt(BexExternalAdt::PromptAst(ast))) => {
                Ok(ast.clone())
            }
            BexValue::ExternalValue(BexExternalValue::Handle(handle)) => {
                let ptr =
                    heap.resolve_handle(handle.slab_key())
                        .ok_or(AccessError::InvalidHandle {
                            expected: "prompt ast",
                        })?;
                from_ptr(&ptr)
            }
            BexValue::Value(Value::Object(ptr)) | BexValue::HeapPtr(ptr) => from_ptr(ptr),
            other => Err(AccessError::TypeMismatch {
                expected: "prompt_ast",
                actual: other.type_name(),
            }),
        }
    }

    /// Attempts to own as much as possible.
    /// If it can't be owned, it fails.
    pub fn as_owned_but_very_slow(
        self,
        heap: &GcProtectedHeap<'_>,
    ) -> Result<BexExternalValue, AccessError> {
        match self {
            BexValue::ExternalValue(bex_external_value) => match bex_external_value {
                BexExternalValue::Handle(handle) => {
                    let heap_ptr = heap
                        .resolve_handle(handle.slab_key())
                        .ok_or(AccessError::InvalidHandle { expected: "handle" })?;
                    BexValue::HeapPtr(&heap_ptr).as_owned_but_very_slow(heap)
                }
                BexExternalValue::FunctionRef { .. } => Err(AccessError::CannotConvertToOwned {
                    reason: "function definition".to_string(),
                }),
                BexExternalValue::Null => Ok(BexExternalValue::Null),
                BexExternalValue::Int(i) => Ok(BexExternalValue::Int(*i)),
                BexExternalValue::Float(f) => Ok(BexExternalValue::Float(*f)),
                BexExternalValue::Bool(b) => Ok(BexExternalValue::Bool(*b)),
                BexExternalValue::String(s) => Ok(BexExternalValue::String(s.clone())),
                BexExternalValue::Array {
                    element_type,
                    items,
                } => Ok(BexExternalValue::Array {
                    element_type: element_type.clone(),
                    items: items
                        .iter()
                        .map(|item| BexValue::ExternalValue(item).as_owned_but_very_slow(heap))
                        .collect::<Result<_, _>>()?,
                }),
                BexExternalValue::Map {
                    key_type,
                    value_type,
                    entries,
                } => Ok(BexExternalValue::Map {
                    key_type: key_type.clone(),
                    value_type: value_type.clone(),
                    entries: entries
                        .iter()
                        .map(|(k, v)| {
                            Ok((
                                k.clone(),
                                BexValue::ExternalValue(v).as_owned_but_very_slow(heap)?,
                            ))
                        })
                        .collect::<Result<_, _>>()?,
                }),
                BexExternalValue::Instance { class_name, fields } => {
                    Ok(BexExternalValue::Instance {
                        class_name: class_name.clone(),
                        fields: fields
                            .iter()
                            .map(|(k, v)| {
                                Ok((
                                    k.clone(),
                                    BexValue::ExternalValue(v).as_owned_but_very_slow(heap)?,
                                ))
                            })
                            .collect::<Result<_, _>>()?,
                    })
                }
                BexExternalValue::Variant {
                    enum_name,
                    variant_name,
                } => Ok(BexExternalValue::Variant {
                    enum_name: enum_name.clone(),
                    variant_name: variant_name.clone(),
                }),
                BexExternalValue::Union { value, metadata } => Ok(BexExternalValue::Union {
                    value: Box::new(BexValue::ExternalValue(value).as_owned_but_very_slow(heap)?),
                    metadata: metadata.clone(),
                }),
                BexExternalValue::Resource(resource_handle) => {
                    Ok(BexExternalValue::Resource(resource_handle.clone()))
                }
                BexExternalValue::Adt(adt) => Ok(BexExternalValue::Adt(adt.clone())),
            },
            BexValue::Value(Value::Object(heap_ptr)) | BexValue::HeapPtr(heap_ptr) => {
                let obj = unsafe { heap_ptr.get() };
                match obj {
                    Object::Function(..) => Err(AccessError::CannotConvertToOwned {
                        reason: "function definition".to_string(),
                    }),
                    Object::Class(..) => Err(AccessError::CannotConvertToOwned {
                        reason: "class definition".to_string(),
                    }),
                    Object::Enum(..) => Err(AccessError::CannotConvertToOwned {
                        reason: "enum definition".to_string(),
                    }),
                    Object::Future(..) => Err(AccessError::CannotConvertToOwned {
                        reason: "future".to_string(),
                    }),

                    Object::String(s) => Ok(BexExternalValue::String(s.clone())),
                    Object::Array(array) => Ok(BexExternalValue::Array {
                        element_type: Ty::BuiltinUnknown,
                        items: array
                            .iter()
                            .map(|item| BexValue::Value(item).as_owned_but_very_slow(heap))
                            .collect::<Result<_, _>>()?,
                    }),
                    Object::Map(map) => Ok(BexExternalValue::Map {
                        key_type: Ty::String,
                        value_type: Ty::BuiltinUnknown,
                        entries: map
                            .iter()
                            .map(|(k, v)| {
                                Ok((k.clone(), BexValue::Value(v).as_owned_but_very_slow(heap)?))
                            })
                            .collect::<Result<_, _>>()?,
                    }),
                    Object::Instance(instance) => {
                        let class_obj = unsafe { instance.class.get() };
                        let Object::Class(class) = class_obj else {
                            return Err(AccessError::TypeMismatch {
                                expected: "class",
                                actual: class_obj.to_string(),
                            });
                        };
                        let fields = class
                            .fields
                            .iter()
                            .zip(instance.fields.iter())
                            .map(|(field, value)| {
                                Ok((
                                    field.name.clone(),
                                    BexValue::Value(value).as_owned_but_very_slow(heap)?,
                                ))
                            })
                            .collect::<Result<_, _>>()?;
                        Ok(BexExternalValue::Instance {
                            class_name: class.name.clone(),
                            fields,
                        })
                    }
                    Object::Variant(variant) => {
                        let variant_obj = unsafe { variant.enm.get() };
                        let Object::Enum(enum_) = variant_obj else {
                            return Err(AccessError::TypeMismatch {
                                expected: "enum",
                                actual: variant_obj.to_string(),
                            });
                        };
                        let variant_def = enum_.variants.get(variant.index).ok_or_else(|| {
                            AccessError::FieldNotFound {
                                expected: format!("variant index {}", variant.index),
                            }
                        })?;
                        Ok(BexExternalValue::Variant {
                            enum_name: enum_.name.clone(),
                            variant_name: variant_def.name.clone(),
                        })
                    }
                    Object::Resource(resource_handle) => {
                        Ok(BexExternalValue::Resource(resource_handle.clone()))
                    }
                    Object::Media(media_value) => Ok(BexExternalValue::Adt(BexExternalAdt::Media(
                        media_value.clone(),
                    ))),
                    Object::PromptAst(prompt_ast) => Ok(BexExternalValue::Adt(
                        BexExternalAdt::PromptAst(prompt_ast.clone()),
                    )),
                    #[cfg(feature = "heap_debug")]
                    Object::Sentinel(sentinel_kind) => Err(AccessError::CannotConvertToOwned {
                        reason: format!("sentinel: {:?}", sentinel_kind),
                    }),
                }
            }
            BexValue::Value(Value::Null) => Ok(BexExternalValue::Null),
            BexValue::Value(Value::Int(i)) => Ok(BexExternalValue::Int(*i)),
            BexValue::Value(Value::Float(f)) => Ok(BexExternalValue::Float(*f)),
            BexValue::Value(Value::Bool(b)) => Ok(BexExternalValue::Bool(*b)),
        }
    }
}

pub trait BuiltinClass<'a>: Sized + From<BexClass<'a>> {
    fn name() -> &'static str;
}

pub mod builtin_types {
    use super::*;

    pub mod owned {
        use bex_external_types::{AsBexExternalValue, BexExternalValue};

        #[derive(Debug)]
        pub struct LlmPrimitiveClient {
            pub name: String,
            pub provider: String,
            pub default_role: String,
            pub allowed_roles: Vec<String>,
            pub options: indexmap::IndexMap<String, bex_external_types::BexExternalValue>,
        }

        impl AsBexExternalValue for LlmPrimitiveClient {
            fn into_bex_external_value(self) -> BexExternalValue {
                let allowed_roles = BexExternalValue::Array {
                    element_type: bex_external_types::Ty::String,
                    items: self
                        .allowed_roles
                        .into_iter()
                        .map(BexExternalValue::String)
                        .collect(),
                };
                let options = BexExternalValue::Map {
                    key_type: bex_external_types::Ty::String,
                    value_type: bex_external_types::Ty::String,
                    entries: self.options,
                };
                BexExternalValue::Instance {
                    class_name: "baml.llm.PrimitiveClient".to_string(),
                    fields: indexmap::indexmap! {
                        "name".to_string() => BexExternalValue::String(self.name),
                        "provider".to_string() => BexExternalValue::String(self.provider),
                        "default_role".to_string() => BexExternalValue::String(self.default_role),
                        "allowed_roles".to_string() => allowed_roles,
                        "options".to_string() => options,
                    },
                }
            }
        }

        #[derive(Debug)]
        pub struct HttpRequest {
            pub method: String,
            pub url: String,
            pub headers: indexmap::IndexMap<String, String>,
            pub body: String,
        }

        impl AsBexExternalValue for HttpRequest {
            fn into_bex_external_value(self) -> BexExternalValue {
                let headers = BexExternalValue::Map {
                    key_type: bex_external_types::Ty::String,
                    value_type: bex_external_types::Ty::String,
                    entries: self
                        .headers
                        .into_iter()
                        .map(|(k, v)| (k, BexExternalValue::String(v)))
                        .collect(),
                };
                BexExternalValue::Instance {
                    class_name: "baml.http.Request".to_string(),
                    fields: indexmap::indexmap! {
                        "method".to_string() => BexExternalValue::String(self.method),
                        "url".to_string() => BexExternalValue::String(self.url),
                        "headers".to_string() => headers,
                        "body".to_string() => BexExternalValue::String(self.body),
                    },
                }
            }
        }

        #[derive(Debug)]
        pub struct HttpResponse {
            pub status_code: i64,
            pub headers: indexmap::IndexMap<String, String>,
            pub url: String,
            pub _handle: bex_resource_types::ResourceHandle,
        }

        impl AsBexExternalValue for HttpResponse {
            fn into_bex_external_value(self) -> BexExternalValue {
                let headers = BexExternalValue::Map {
                    key_type: bex_external_types::Ty::String,
                    value_type: bex_external_types::Ty::String,
                    entries: self
                        .headers
                        .into_iter()
                        .map(|(k, v)| (k, BexExternalValue::String(v)))
                        .collect(),
                };
                BexExternalValue::Instance {
                    class_name: "baml.http.Response".to_string(),
                    fields: indexmap::indexmap! {
                        "_handle".to_string() => BexExternalValue::Resource(self._handle),
                        "status_code".to_string() => BexExternalValue::Int(self.status_code),
                        "headers".to_string() => headers,
                        "url".to_string() => BexExternalValue::String(self.url),
                    },
                }
            }
        }

        #[derive(Debug)]
        pub struct FsFile {
            pub _handle: bex_resource_types::ResourceHandle,
        }

        impl AsBexExternalValue for FsFile {
            fn into_bex_external_value(self) -> BexExternalValue {
                BexExternalValue::Instance {
                    class_name: "baml.fs.File".to_string(),
                    fields: indexmap::indexmap! {
                        "_handle".to_string() => BexExternalValue::Resource(self._handle),
                    },
                }
            }
        }

        #[derive(Debug)]
        pub struct NetSocket {
            pub _handle: bex_resource_types::ResourceHandle,
        }

        impl AsBexExternalValue for NetSocket {
            fn into_bex_external_value(self) -> BexExternalValue {
                BexExternalValue::Instance {
                    class_name: "baml.net.Socket".to_string(),
                    fields: indexmap::indexmap! {
                        "_handle".to_string() => BexExternalValue::Resource(self._handle),
                    },
                }
            }
        }
    }

    pub struct LlmPrimitiveClient<'a> {
        cls: super::BexClass<'a>,
    }

    impl<'a> From<BexClass<'a>> for LlmPrimitiveClient<'a> {
        fn from(cls: BexClass<'a>) -> Self {
            Self { cls }
        }
    }

    impl<'a> BuiltinClass<'a> for LlmPrimitiveClient<'a> {
        fn name() -> &'static str {
            "baml.llm.PrimitiveClient"
        }
    }

    impl<'a> LlmPrimitiveClient<'a> {
        pub fn name(&self, heap: &'a GcProtectedHeap<'a>) -> Result<&'a String, AccessError> {
            self.cls
                .field("name")
                .and_then(|value| value.as_string(heap))
        }

        pub fn provider(&self, heap: &'a GcProtectedHeap<'a>) -> Result<&'a String, AccessError> {
            self.cls
                .field("provider")
                .and_then(|value| value.as_string(heap))
        }

        pub fn default_role(
            &self,
            heap: &'a GcProtectedHeap<'a>,
        ) -> Result<&'a String, AccessError> {
            self.cls
                .field("default_role")
                .and_then(|value| value.as_string(heap))
        }

        pub fn allowed_roles(
            &self,
            heap: &'a GcProtectedHeap<'a>,
        ) -> Result<Vec<&'a String>, AccessError> {
            self.cls.field("allowed_roles").and_then(|value| {
                value.as_array(heap).and_then(|items| {
                    items
                        .into_iter()
                        .map(|item| item.as_string(heap))
                        .collect::<Result<Vec<_>, AccessError>>()
                })
            })
        }

        pub fn options(
            &self,
            heap: &'a GcProtectedHeap<'a>,
        ) -> Result<indexmap::IndexMap<String, BexValue<'a>>, AccessError> {
            self.cls
                .field("options")
                .and_then(|value| value.as_map(heap))
        }

        pub fn into_owned(
            self,
            heap: &'a GcProtectedHeap<'a>,
        ) -> Result<owned::LlmPrimitiveClient, AccessError> {
            Ok(owned::LlmPrimitiveClient {
                name: self.name(heap)?.clone(),
                provider: self.provider(heap)?.clone(),
                default_role: self.default_role(heap)?.clone(),
                allowed_roles: self.allowed_roles(heap)?.into_iter().cloned().collect(),
                options: self
                    .options(heap)?
                    .into_iter()
                    .map(|(k, v)| Ok((k, v.as_owned_but_very_slow(heap)?)))
                    .collect::<Result<_, _>>()?,
            })
        }
    }

    pub struct HttpRequest<'a> {
        cls: super::BexClass<'a>,
    }

    impl<'a> From<BexClass<'a>> for HttpRequest<'a> {
        fn from(cls: BexClass<'a>) -> Self {
            Self { cls }
        }
    }

    impl<'a> BuiltinClass<'a> for HttpRequest<'a> {
        fn name() -> &'static str {
            "baml.http.Request"
        }
    }

    impl<'a> HttpRequest<'a> {
        pub fn method(&self, heap: &'a GcProtectedHeap<'a>) -> Result<&'a String, AccessError> {
            self.cls
                .field("method")
                .and_then(|value| value.as_string(heap))
        }

        pub fn url(&self, heap: &'a GcProtectedHeap<'a>) -> Result<&'a String, AccessError> {
            self.cls
                .field("url")
                .and_then(|value| value.as_string(heap))
        }

        pub fn headers(
            &self,
            heap: &'a GcProtectedHeap<'a>,
        ) -> Result<indexmap::IndexMap<String, &'a String>, AccessError> {
            self.cls
                .field("headers")
                .and_then(|value| value.as_map(heap))
                .and_then(|map| {
                    map.into_iter()
                        .map(|(k, v)| v.as_string(heap).map(|s| (k, s)))
                        .collect::<Result<_, _>>()
                })
        }

        pub fn body(&self, heap: &'a GcProtectedHeap<'a>) -> Result<&'a String, AccessError> {
            self.cls
                .field("body")
                .and_then(|value| value.as_string(heap))
        }

        pub fn into_owned(
            self,
            heap: &'a GcProtectedHeap<'a>,
        ) -> Result<owned::HttpRequest, AccessError> {
            Ok(owned::HttpRequest {
                method: self.method(heap)?.clone(),
                url: self.url(heap)?.clone(),
                headers: self
                    .headers(heap)?
                    .into_iter()
                    .map(|(k, v)| (k, v.clone()))
                    .collect(),
                body: self.body(heap)?.clone(),
            })
        }
    }

    pub struct HttpResponse<'a> {
        cls: super::BexClass<'a>,
    }

    impl<'a> From<BexClass<'a>> for HttpResponse<'a> {
        fn from(cls: BexClass<'a>) -> Self {
            Self { cls }
        }
    }

    impl<'a> BuiltinClass<'a> for HttpResponse<'a> {
        fn name() -> &'static str {
            "baml.http.Response"
        }
    }

    impl<'a> HttpResponse<'a> {
        pub fn status_code(&self) -> Result<i64, AccessError> {
            self.cls
                .field("status_code")
                .and_then(|value| value.as_int())
        }

        pub fn headers(
            &self,
            heap: &'a GcProtectedHeap<'a>,
        ) -> Result<indexmap::IndexMap<String, &'a String>, AccessError> {
            self.cls
                .field("headers")
                .and_then(|value| value.as_map(heap))
                .and_then(|map| {
                    map.into_iter()
                        .map(|(k, v)| v.as_string(heap).map(|s| (k, s)))
                        .collect::<Result<_, _>>()
                })
        }

        pub fn url(&self, heap: &'a GcProtectedHeap<'a>) -> Result<&'a String, AccessError> {
            self.cls
                .field("url")
                .and_then(|value| value.as_string(heap))
        }

        pub fn _handle(
            &self,
            heap: &'a GcProtectedHeap<'a>,
        ) -> Result<bex_resource_types::ResourceHandle, AccessError> {
            self.cls
                .field("_handle")
                .and_then(|value| value.as_resource_handle(heap))
        }

        pub fn into_owned(
            self,
            heap: &'a GcProtectedHeap<'a>,
        ) -> Result<owned::HttpResponse, AccessError> {
            Ok(owned::HttpResponse {
                status_code: self.status_code()?,
                headers: self
                    .headers(heap)?
                    .into_iter()
                    .map(|(k, v)| (k, v.clone()))
                    .collect(),
                url: self.url(heap)?.clone(),
                _handle: self._handle(heap)?,
            })
        }
    }

    pub struct NetSocket<'a> {
        cls: super::BexClass<'a>,
    }

    impl<'a> From<BexClass<'a>> for NetSocket<'a> {
        fn from(cls: BexClass<'a>) -> Self {
            Self { cls }
        }
    }

    impl<'a> BuiltinClass<'a> for NetSocket<'a> {
        fn name() -> &'static str {
            "baml.net.Socket"
        }
    }

    impl<'a> NetSocket<'a> {
        pub fn _handle(
            &self,
            heap: &'a GcProtectedHeap<'a>,
        ) -> Result<bex_resource_types::ResourceHandle, AccessError> {
            self.cls
                .field("_handle")
                .and_then(|value| value.as_resource_handle(heap))
        }

        pub fn into_owned(
            self,
            heap: &'a GcProtectedHeap<'a>,
        ) -> Result<owned::NetSocket, AccessError> {
            Ok(owned::NetSocket {
                _handle: self._handle(heap)?,
            })
        }
    }

    pub struct FsFile<'a> {
        cls: super::BexClass<'a>,
    }

    impl<'a> From<BexClass<'a>> for FsFile<'a> {
        fn from(cls: BexClass<'a>) -> Self {
            Self { cls }
        }
    }

    impl<'a> BuiltinClass<'a> for FsFile<'a> {
        fn name() -> &'static str {
            "baml.fs.File"
        }
    }

    impl<'a> FsFile<'a> {
        pub fn _handle(
            &self,
            heap: &'a GcProtectedHeap<'a>,
        ) -> Result<bex_resource_types::ResourceHandle, AccessError> {
            self.cls
                .field("_handle")
                .and_then(|value| value.as_resource_handle(heap))
        }

        pub fn into_owned(
            self,
            heap: &'a GcProtectedHeap<'a>,
        ) -> Result<owned::FsFile, AccessError> {
            Ok(owned::FsFile {
                _handle: self._handle(heap)?,
            })
        }
    }
}
