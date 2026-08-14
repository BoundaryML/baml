//! Safe accessor API for external code to read heap objects.
//!
//! External code cannot safely hold bare `HeapPtr` values across GC. This
//! module provides an API that takes a `PermitProof<'_>` (obtained from any
//! held `ActiveHeapPermit<T>`) to witness GC-exclusion at the type level.

use baml_type::RuntimeTy;
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
    OwnedValue(Value),
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

impl<'a> BexClass<'a> {
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
                let field = instance
                    .fields
                    .get(field_idx)
                    .ok_or_else(|| AccessError::FieldNotFound {
                        expected: name.to_string(),
                    })?
                    .load();
                Ok(BexValue::OwnedValue(field))
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
            BexValue::OwnedValue(value) => value.to_string(),
        }
    }

    pub fn as_int(self) -> Result<i64, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::Int(i)) => Ok(*i),
            BexValue::Value(v) => v.as_int().ok_or_else(|| AccessError::TypeMismatch {
                expected: "int",
                actual: v.to_string(),
            }),
            BexValue::OwnedValue(v) => v.as_int().ok_or_else(|| AccessError::TypeMismatch {
                expected: "int",
                actual: v.to_string(),
            }),
            other => Err(AccessError::TypeMismatch {
                expected: "int",
                actual: other.type_name(),
            }),
        }
    }

    pub fn as_bool(self) -> Result<bool, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::Bool(b)) => Ok(*b),
            BexValue::Value(v) => v.as_bool().ok_or_else(|| AccessError::TypeMismatch {
                expected: "bool",
                actual: v.to_string(),
            }),
            BexValue::OwnedValue(v) => v.as_bool().ok_or_else(|| AccessError::TypeMismatch {
                expected: "bool",
                actual: v.to_string(),
            }),
            other => Err(AccessError::TypeMismatch {
                expected: "bool",
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
            BexValue::HeapPtr(ptr) => f(ptr),
            BexValue::Value(v) if v.is_object() => {
                let ptr = v.as_object_ptr().expect("just checked is_object");
                f(&ptr)
            }
            BexValue::OwnedValue(v) if v.is_object() => {
                let ptr = v.as_object_ptr().expect("just checked is_object");
                f(&ptr)
            }
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

    /// Extract a rooted host [`Handle`](bex_external_types::Handle) to a callable
    /// BAML value (function, closure, or bound method).
    ///
    /// Used for `function`-typed sys-op arguments: a callable cannot be
    /// serialized into a `BexExternalValue`, but it crosses the boundary as a
    /// `BexExternalValue::Handle` (a GC root into the shared heap). The sys-op
    /// holds the handle and later invokes it via `VmSpawner::spawn_with_callable`,
    /// which validates that the handle actually points at a callable object.
    pub fn as_callable_handle(
        self,
        _heap: &BexHeap,
        _permit: PermitProof<'a>,
    ) -> Result<bex_external_types::Handle, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::Handle(handle)) => Ok(handle.clone()),
            other => Err(AccessError::TypeMismatch {
                expected: "function",
                actual: other.type_name(),
            }),
        }
    }

    pub fn as_string(
        self,
        heap: &BexHeap,
        permit: PermitProof<'a>,
    ) -> Result<&'a bex_str::BexStr, AccessError> {
        match self {
            // Phase 3: BexExternalValue::String now holds BexStr directly.
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

    /// Extract an `Arc<BigInt>` from a bigint value. External bigints are
    /// stored as `BigInt` by value; heap bigints share an `Arc<BigInt>` —
    /// in both cases the caller gets an owned `Arc` (cheap clone for the
    /// heap case, fresh allocation for the external case).
    pub fn as_bigint(
        self,
        heap: &BexHeap,
        permit: PermitProof<'a>,
    ) -> Result<std::sync::Arc<num_bigint::BigInt>, AccessError> {
        match self {
            BexValue::ExternalValue(BexExternalValue::Bigint(bi)) => {
                Ok(std::sync::Arc::new(bi.clone()))
            }
            other => other.as_object("bigint", heap, permit, |ptr| {
                let obj = unsafe { ptr.get() };
                let Object::Bigint(arc) = obj else {
                    return Err(AccessError::TypeMismatch {
                        expected: "bigint",
                        actual: obj.to_string(),
                    });
                };
                Ok(std::sync::Arc::clone(arc))
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
            BexValue::ExternalValue(BexExternalValue::Instance {
                class_name, fields, ..
            }) => {
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
                if class.name.display_name().as_str() != expected_class_name {
                    return Err(AccessError::TypeMismatch {
                        expected: expected_class_name,
                        actual: class.name.to_string(),
                    });
                }
                Ok(BexClass::Value(class, instance))
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

    pub fn as_baml_type_owned(
        self,
        heap: &BexHeap,
        _permit: PermitProof<'_>,
    ) -> Result<baml_type::RuntimeTy, AccessError> {
        fn from_ptr(ptr: &HeapPtr) -> Result<baml_type::RuntimeTy, AccessError> {
            let obj = unsafe { ptr.get() };
            let Object::Type(ty) = obj else {
                return Err(AccessError::TypeMismatch {
                    expected: "type",
                    actual: obj.to_string(),
                });
            };
            // `Object::Type` stores a realized type; widen it into `RuntimeTy`.
            Ok((**ty).clone().into())
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
            BexValue::HeapPtr(ptr) => from_ptr(ptr),
            BexValue::Value(v) if v.is_object() => {
                let ptr = v.as_object_ptr().expect("just checked is_object");
                from_ptr(&ptr)
            }
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
            Ok(BexExternalValue::String(bex_str::BexStr::from(format!(
                "<{reason}>"
            ))))
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
            BexExternalValue::Bigint(b) => Ok(BexExternalValue::Bigint(b.clone())),
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
            BexExternalValue::Instance {
                class_name,
                type_args,
                fields,
            } => Ok(BexExternalValue::Instance {
                class_name: class_name.clone(),
                type_args: type_args.clone(),
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
            BexExternalValue::HostValue(hv) => {
                Ok(BexExternalValue::HostValue(std::sync::Arc::clone(hv)))
            }
        },
        // Object path: either `HeapPtr` directly, or a `Value` whose payload
        // is an object pointer. The two cases share a single object deep-
        // copy via `convert_object`; scalar-typed `Value`s fall through to
        // a separate `ValueKind` match.
        BexValue::HeapPtr(ptr) => convert_object(*ptr, heap, lossy),
        BexValue::Value(v) => {
            if let Some(ptr) = v.as_object_ptr() {
                convert_object(ptr, heap, lossy)
            } else {
                match v.kind() {
                    bex_vm_types::ValueKind::OmittedArg => unconvertible("omitted argument"),
                    bex_vm_types::ValueKind::Null => Ok(BexExternalValue::Null),
                    bex_vm_types::ValueKind::Int(i) => Ok(BexExternalValue::Int(i)),
                    bex_vm_types::ValueKind::Bool(b) => Ok(BexExternalValue::Bool(b)),
                    bex_vm_types::ValueKind::Object(_) => {
                        unreachable!("object path handled above")
                    }
                }
            }
        }
        BexValue::OwnedValue(v) => owned_inner(BexValue::Value(&v), heap, lossy),
    }
}

fn convert_object(
    heap_ptr: HeapPtr,
    heap: &BexHeap,
    lossy: bool,
) -> Result<BexExternalValue, AccessError> {
    let unconvertible = |type_name: &str| -> Result<BexExternalValue, AccessError> {
        if lossy {
            Ok(BexExternalValue::String(bex_str::BexStr::from(format!(
                "<{type_name}>"
            ))))
        } else {
            Err(AccessError::CannotConvertToOwned {
                reason: format!("cannot convert {type_name} to BexExternalValue"),
            })
        }
    };
    let obj = unsafe { heap_ptr.get() };
    match obj {
        Object::Function(..) => unconvertible("function"),
        Object::Interface(..) => unconvertible("interface"),
        Object::Package(..) => unconvertible("package"),
        Object::ImplRule(..) => unconvertible("impl_rule"),
        Object::Class(..) => unconvertible("class"),
        Object::Enum(..) => unconvertible("enum"),
        Object::Future(..) => unconvertible("future"),
        Object::UnscheduledFuture(..) => unconvertible("unscheduled_future"),

        Object::String(s) => Ok(BexExternalValue::String(s.clone())),
        // Deep-copy path for trace payloads: no declared type is available here,
        // so placeholder types with default attr are used.
        Object::Array(array) => Ok(BexExternalValue::Array {
            element_type: RuntimeTy::BuiltinUnknown {
                attr: baml_type::TyAttr::default(),
            },
            items: array
                .to_vec()
                .into_iter()
                .map(|item| owned_inner(BexValue::OwnedValue(item), heap, lossy))
                .collect::<Result<_, _>>()?,
        }),
        Object::Map(map) => Ok(BexExternalValue::Map {
            key_type: RuntimeTy::String {
                attr: baml_type::TyAttr::default(),
            },
            value_type: RuntimeTy::BuiltinUnknown {
                attr: baml_type::TyAttr::default(),
            },
            entries: map
                .to_index_map()
                .into_iter()
                .map(|(k, v)| {
                    Ok((
                        k.as_str().to_owned(),
                        owned_inner(BexValue::OwnedValue(v), heap, lossy)?,
                    ))
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
                .map(|(field, slot)| {
                    Ok((
                        field.name.clone(),
                        owned_inner(BexValue::OwnedValue(slot.load()), heap, lossy)?,
                    ))
                })
                .collect::<Result<_, _>>()?;
            Ok(BexExternalValue::Instance {
                class_name: class.name.to_string(),
                // Instances store realized class type args; widen them into the
                // `RuntimeTy` the external boundary carries.
                type_args: instance
                    .class_type_args
                    .iter()
                    .map(baml_type::RuntimeTy::from)
                    .collect(),
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
            let variant_def =
                enum_
                    .variants
                    .get(variant.index)
                    .ok_or_else(|| AccessError::FieldNotFound {
                        expected: format!("variant index {}", variant.index),
                    })?;
            Ok(BexExternalValue::Variant {
                enum_name: enum_.name.to_string(),
                variant_name: variant_def.name.clone(),
            })
        }
        Object::Collector(c) => Ok(BexExternalValue::Adt(BexExternalAdt::Collector(c.clone()))),
        Object::Type(ty) => Ok(BexExternalValue::Adt(BexExternalAdt::Type(
            (**ty).clone().into(),
        ))),
        Object::Bigint(bi) => Ok(BexExternalValue::Bigint((**bi).clone())),
        Object::Uint8Array(bytes) => Ok(BexExternalValue::Uint8Array(bytes.to_vec())),
        Object::RustData(data) => Ok(bex_external_types::try_convert_rust_data(data)
            .unwrap_or_else(|| BexExternalValue::RustData(data.clone()))),
        Object::Float(f) => Ok(BexExternalValue::Float(*f)),
        Object::Closure(_) => unconvertible("closure"),
        Object::BoundMethod(_) => unconvertible("bound_method"),
        Object::GenericFunction(_) => unconvertible("generic_function"),
        // `HostClosure` is a callable wrapper for a host-owned value.
        // The native sysop impl matches on `BexExternalValue::HostValue`
        // and extracts the `Arc<HostValueArc>` from it, so unwrap the
        // closure here to its underlying handle. The `ret_ty` field is
        // *not* surfaced through this conversion (it flows separately
        // via the type-arg channel of `SysOp::BamlHostCallHostValue`).
        Object::HostClosure(hc) => Ok(BexExternalValue::HostValue(std::sync::Arc::clone(
            &hc.handle,
        ))),
        Object::Cell(_) => unconvertible("cell"),
        #[cfg(feature = "heap_debug")]
        Object::Sentinel(sentinel_kind) => unconvertible(&format!("sentinel: {:?}", sentinel_kind)),
    }
}

pub trait BuiltinClass<'a>: Sized + From<BexClass<'a>> {
    fn name() -> &'static str;
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use bex_external_types::BexExternalValue;
    use bex_str::BexStr;
    use bex_vm_types::RootHaver;

    use crate::{BexHeap, BexValue, HeapPermit as _, HeapPermitManager, Tlab, TlabHolder};

    struct EmptyRoots {
        tlab: Tlab,
    }

    impl RootHaver for EmptyRoots {
        fn collect_roots(&self, _roots: &mut Vec<bex_vm_types::HeapPtr>) {}

        fn forward_roots(
            &mut self,
            _forward: &HashMap<bex_vm_types::HeapPtr, bex_vm_types::HeapPtr>,
        ) {
        }
    }

    impl TlabHolder for EmptyRoots {
        fn tlab(&self) -> &Tlab {
            &self.tlab
        }

        fn tlab_mut(&mut self) -> &mut Tlab {
            &mut self.tlab
        }
    }

    #[tokio::test]
    async fn trace_owned_function_refs_become_string_placeholders() {
        let heap = BexHeap::new(Vec::new());
        let manager = HeapPermitManager::new();
        let permit = manager
            .new_permit(EmptyRoots {
                tlab: Tlab::new(Arc::clone(&heap)),
            })
            .await
            .acquire()
            .await;
        let value = BexExternalValue::FunctionRef { global_index: 7 };

        let owned = BexValue::ExternalValue(&value)
            .as_owned_for_trace(&heap, permit.proof())
            .unwrap();

        assert_eq!(owned, BexExternalValue::String(BexStr::from("<function>")));
    }
}
