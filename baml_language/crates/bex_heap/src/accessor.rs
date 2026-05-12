//! Safe accessor API for external code to read heap objects.
//!
//! External code cannot safely hold bare `HeapPtr` values across GC. This
//! module provides an API that takes a `PermitProof<'_>` (obtained from any
//! held `ActiveHeapPermit<T>`) to witness GC-exclusion at the type level.

use baml_type::Ty;
use bex_external_types::{BexExternalAdt, BexExternalValue, WeakHeapRef};
use bex_vm_types::{HeapPtr, Object, PermitProof, Value};

use crate::BexHeap;

#[derive(Debug, PartialEq, thiserror::Error, Clone)]
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
    pub fn class_name(&self) -> &str {
        match self {
            BexClass::ExternalClass { name, .. } => name,
            BexClass::Value(class, ..) => class.name.display_name.as_str(),
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
    pub fn enum_name(&self) -> &str {
        match self {
            BexVariant::ExternalVariant { name, .. } => name,
            BexVariant::Value(enum_, ..) => enum_.name.display_name.as_str(),
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
            BexValue::Value(value) => value.to_string(),
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
    fn as_object<R>(
        self,
        expected: &'static str,
        heap: &BexHeap,
        _permit: PermitProof<'a>,
        f: impl FnOnce(&HeapPtr) -> Result<R, AccessError>,
    ) -> Result<R, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::Handle(ptr)) => {
                let ptr = heap
                    .resolve_handle_ptr(ptr.slab_key())
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

    /// Extract an opaque `Arc<dyn Any + Send + Sync>` from a RustData value.
    /// Handles both external values (`BexExternalValue::RustData`) and
    /// heap values (`Object::RustData`).
    pub fn as_rust_data(
        self,
        heap: &BexHeap,
        permit: PermitProof<'a>,
    ) -> Result<std::sync::Arc<dyn std::any::Any + Send + Sync>, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::RustData(data)) => {
                Ok(std::sync::Arc::clone(data))
            }
            other => other.as_object("rust_data", heap, permit, |ptr| {
                let obj = unsafe { ptr.get() };
                let Object::RustData(arc) = obj else {
                    return Err(AccessError::TypeMismatch {
                        expected: "rust_data",
                        actual: obj.to_string(),
                    });
                };
                Ok(std::sync::Arc::clone(arc))
            }),
        }
    }

    pub fn as_string(
        self,
        heap: &BexHeap,
        permit: PermitProof<'a>,
    ) -> Result<&'a String, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::String(s)) => Ok(s),
            other => other.as_object("string", heap, permit, |ptr| {
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

    pub fn as_array(
        self,
        heap: &BexHeap,
        permit: PermitProof<'a>,
    ) -> Result<Vec<BexValue<'a>>, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::Array { items, .. }) => {
                Ok(items.iter().map(BexValue::ExternalValue).collect())
            }
            other => other.as_object("array", heap, permit, |ptr| {
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
        heap: &BexHeap,
        permit: PermitProof<'a>,
    ) -> Result<indexmap::IndexMap<String, BexValue<'a>>, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::Map { entries, .. }) => Ok(entries
                .iter()
                .map(|(k, v)| (k.clone(), BexValue::ExternalValue(v)))
                .collect()),
            other => other.as_object("map", heap, permit, |ptr| {
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
        heap: &BexHeap,
        permit: PermitProof<'a>,
        expected_class_name: &'static str,
    ) -> Result<BexClass<'a>, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::Instance { class_name, fields }) => {
                if class_name != expected_class_name {
                    return Err(AccessError::TypeMismatch {
                        expected: expected_class_name,
                        actual: class_name.clone(),
                    });
                }
                Ok(BexClass::ExternalClass {
                    name: class_name,
                    fields,
                })
            }
            other => other.as_object("instance", heap, permit, |ptr| {
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
                if class.name.display_name.as_str() != expected_class_name {
                    return Err(AccessError::TypeMismatch {
                        expected: expected_class_name,
                        actual: class.name.to_string(),
                    });
                }
                Ok(BexClass::Value(class, instance))
            }),
        }
    }

    #[allow(clippy::only_used_in_recursion)]
    pub fn as_enum<T>(
        self,
        heap: &BexHeap,
        permit: PermitProof<'a>,
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
                        actual: enum_name.clone(),
                    });
                }
                Ok(map_fn(BexVariant::ExternalVariant {
                    name: enum_name,
                    value: variant_name,
                }))
            }
            BexValue::ExternalValue(BexExternalValue::Handle(ptr)) => {
                let ptr = heap
                    .resolve_handle_ptr(ptr.slab_key())
                    .ok_or(AccessError::InvalidHandle { expected: "enum" })?;
                BexValue::HeapPtr(&ptr).as_enum(heap, permit, expected_enum_name, map_fn)
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
                if enum_.name.display_name.as_str() != expected_enum_name {
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
        heap: &BexHeap,
        permit: PermitProof<'a>,
    ) -> Result<T, AccessError> {
        self.as_class(heap, permit, T::name())
            .map(|cls| T::from(cls))
    }

    pub fn as_collector_owned(
        self,
        heap: &BexHeap,
        _permit: PermitProof<'_>,
    ) -> Result<bex_vm_types::CollectorRef, AccessError> {
        fn from_ptr(ptr: &HeapPtr) -> Result<bex_vm_types::CollectorRef, AccessError> {
            let obj = unsafe { ptr.get() };
            let Object::Collector(c) = obj else {
                return Err(AccessError::TypeMismatch {
                    expected: "collector",
                    actual: obj.to_string(),
                });
            };
            Ok(c.clone())
        }

        match self {
            BexValue::ExternalValue(BexExternalValue::Adt(BexExternalAdt::Collector(c))) => {
                Ok(c.clone())
            }
            BexValue::ExternalValue(BexExternalValue::Handle(handle)) => {
                let ptr = heap.resolve_handle_ptr(handle.slab_key()).ok_or(
                    AccessError::InvalidHandle {
                        expected: "collector",
                    },
                )?;
                from_ptr(&ptr)
            }
            BexValue::Value(Value::Object(ptr)) | BexValue::HeapPtr(ptr) => from_ptr(ptr),
            other => Err(AccessError::TypeMismatch {
                expected: "collector",
                actual: other.type_name(),
            }),
        }
    }

    pub fn as_baml_type_owned(
        self,
        heap: &BexHeap,
        _permit: PermitProof<'_>,
    ) -> Result<baml_type::Ty, AccessError> {
        fn from_ptr(ptr: &HeapPtr) -> Result<baml_type::Ty, AccessError> {
            let obj = unsafe { ptr.get() };
            let Object::Type(ty) = obj else {
                return Err(AccessError::TypeMismatch {
                    expected: "type",
                    actual: obj.to_string(),
                });
            };
            Ok((**ty).clone())
        }

        match self {
            BexValue::ExternalValue(BexExternalValue::Adt(BexExternalAdt::Type(ty))) => {
                Ok(ty.clone())
            }
            BexValue::ExternalValue(BexExternalValue::Handle(handle)) => {
                let ptr = heap
                    .resolve_handle_ptr(handle.slab_key())
                    .ok_or(AccessError::InvalidHandle { expected: "type" })?;
                from_ptr(&ptr)
            }
            BexValue::Value(Value::Object(ptr)) | BexValue::HeapPtr(ptr) => from_ptr(ptr),
            other => Err(AccessError::TypeMismatch {
                expected: "type",
                actual: other.type_name(),
            }),
        }
    }

    /// Attempts to own as much as possible.
    /// If it can't be owned, it fails.
    pub fn as_owned_but_very_slow(
        self,
        heap: &BexHeap,
        _permit: PermitProof<'_>,
    ) -> Result<BexExternalValue, AccessError> {
        // `_permit` is a zero-sized witness that GC exclusion is held; we don't
        // need to thread it through the recursion since the proof only has to
        // exist at the API boundary.
        owned_inner(self, heap, /* lossy */ false)
    }

    /// Like `as_owned_but_very_slow`, but substitutes non-convertible leaves
    /// (closures, functions, futures, bound methods, cells, function refs,
    /// class/enum definitions) with a `<kind>` string placeholder rather than
    /// failing the entire conversion.
    ///
    /// Use this for trace event payloads, where dropping the whole tree because
    /// a single field happens to hold a closure is worse than emitting partial
    /// data with a stub.
    pub fn as_owned_for_trace(
        self,
        heap: &BexHeap,
        _permit: PermitProof<'_>,
    ) -> Result<BexExternalValue, AccessError> {
        owned_inner(self, heap, /* lossy */ true)
    }
}

fn owned_inner(
    value: BexValue<'_>,
    heap: &BexHeap,
    lossy: bool,
) -> Result<BexExternalValue, AccessError> {
    let unconvertible = |reason: &str| -> Result<BexExternalValue, AccessError> {
        if lossy {
            Ok(BexExternalValue::String(format!("<{reason}>")))
        } else {
            Err(AccessError::CannotConvertToOwned {
                reason: reason.to_string(),
            })
        }
    };

    match value {
        BexValue::ExternalValue(bex_external_value) => match bex_external_value {
            BexExternalValue::Handle(handle) => {
                let heap_ptr = heap
                    .resolve_handle_ptr(handle.slab_key())
                    .ok_or(AccessError::InvalidHandle { expected: "handle" })?;
                owned_inner(BexValue::HeapPtr(&heap_ptr), heap, lossy)
            }
            BexExternalValue::FunctionRef { .. } => unconvertible("function"),
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
                    .map(|item| owned_inner(BexValue::ExternalValue(item), heap, lossy))
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
                            owned_inner(BexValue::ExternalValue(v), heap, lossy)?,
                        ))
                    })
                    .collect::<Result<_, _>>()?,
            }),
            BexExternalValue::Instance { class_name, fields } => Ok(BexExternalValue::Instance {
                class_name: class_name.clone(),
                fields: fields
                    .iter()
                    .map(|(k, v)| {
                        Ok((
                            k.clone(),
                            owned_inner(BexValue::ExternalValue(v), heap, lossy)?,
                        ))
                    })
                    .collect::<Result<_, _>>()?,
            }),
            BexExternalValue::Variant {
                enum_name,
                variant_name,
            } => Ok(BexExternalValue::Variant {
                enum_name: enum_name.clone(),
                variant_name: variant_name.clone(),
            }),
            BexExternalValue::Union { value, metadata } => Ok(BexExternalValue::Union {
                value: Box::new(owned_inner(BexValue::ExternalValue(value), heap, lossy)?),
                metadata: metadata.clone(),
            }),
            BexExternalValue::Uint8Array(bytes) => Ok(BexExternalValue::Uint8Array(bytes.clone())),
            BexExternalValue::RustData(data) => {
                Ok(BexExternalValue::RustData(std::sync::Arc::clone(data)))
            }
            BexExternalValue::Adt(adt) => Ok(BexExternalValue::Adt(adt.clone())),
        },
        BexValue::Value(Value::Object(heap_ptr)) | BexValue::HeapPtr(heap_ptr) => {
            let obj = unsafe { heap_ptr.get() };
            match obj {
                Object::Function(..) => unconvertible("function"),
                Object::Class(..) => unconvertible("class"),
                Object::Enum(..) => unconvertible("enum"),
                Object::Future(..) => unconvertible("future"),
                Object::UnscheduledFuture(..) => unconvertible("unscheduled_future"),

                Object::String(s) => Ok(BexExternalValue::String(s.clone())),
                // Deep-copy path for trace payloads: no declared type is available here,
                // so placeholder types with default attr are used.
                Object::Array(array) => Ok(BexExternalValue::Array {
                    element_type: Ty::BuiltinUnknown {
                        attr: baml_type::TyAttr::default(),
                    },
                    items: array
                        .iter()
                        .map(|item| owned_inner(BexValue::Value(item), heap, lossy))
                        .collect::<Result<_, _>>()?,
                }),
                Object::Map(map) => Ok(BexExternalValue::Map {
                    key_type: Ty::String {
                        attr: baml_type::TyAttr::default(),
                    },
                    value_type: Ty::BuiltinUnknown {
                        attr: baml_type::TyAttr::default(),
                    },
                    entries: map
                        .iter()
                        .map(|(k, v)| {
                            Ok((k.clone(), owned_inner(BexValue::Value(v), heap, lossy)?))
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
                                owned_inner(BexValue::Value(value), heap, lossy)?,
                            ))
                        })
                        .collect::<Result<_, _>>()?;
                    Ok(BexExternalValue::Instance {
                        class_name: class.name.to_string(),
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
                        enum_name: enum_.name.to_string(),
                        variant_name: variant_def.name.clone(),
                    })
                }
                Object::Collector(c) => {
                    Ok(BexExternalValue::Adt(BexExternalAdt::Collector(c.clone())))
                }
                Object::Type(ty) => Ok(BexExternalValue::Adt(BexExternalAdt::Type((**ty).clone()))),
                Object::Uint8Array(bytes) => Ok(BexExternalValue::Uint8Array(bytes.clone())),
                Object::RustData(data) => Ok(bex_external_types::try_convert_rust_data(data)
                    .unwrap_or_else(|| BexExternalValue::RustData(data.clone()))),
                Object::Closure(_) => unconvertible("closure"),
                Object::BoundMethod(_) => unconvertible("bound_method"),
                Object::Cell(_) => unconvertible("cell"),
                #[cfg(feature = "heap_debug")]
                Object::Sentinel(sentinel_kind) => {
                    unconvertible(&format!("sentinel: {:?}", sentinel_kind))
                }
            }
        }
        BexValue::Value(Value::OmittedArg) => unconvertible("omitted argument"),
        BexValue::Value(Value::Null) => Ok(BexExternalValue::Null),
        BexValue::Value(Value::Int(i)) => Ok(BexExternalValue::Int(*i)),
        BexValue::Value(Value::Float(f)) => Ok(BexExternalValue::Float(*f)),
        BexValue::Value(Value::Bool(b)) => Ok(BexExternalValue::Bool(*b)),
    }
}

pub trait BuiltinClass<'a>: Sized + From<BexClass<'a>> {
    fn name() -> &'static str;
}
