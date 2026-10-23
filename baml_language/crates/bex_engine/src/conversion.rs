//! Value conversion functions between VM and external types.
//!
//! This module contains all the conversion logic for transforming values
//! between the VM representation (`Value`, `Object`) and the external
//! representation (`BexValue`, `BexExternalValue`).

use ::bex_heap::{BexValue, PermitProof};
use ::bex_vm_types::{HeapPtr, Object, Value, ValueKind, float_order};
use baml_type::{ClassCrossing, Literal};
use bex_external_types::{
    BexExternalAdt, BexExternalValue, Handle, UnionMetadata, WireHead, WireTy,
    is_canonical_json_alias, runtime_ty_structurally_equal, selected_arm_equal,
    value_satisfies_json,
};
use bex_vm::BexVm;

use crate::{BexEngine, EngineError};

/// A realized type as the annotation beside a value crossing to the host: a
/// compiled declaration by name, a runtime declaration by the label it is
/// displayed under. A label is no reference; a type the host may state again
/// names its runtime declarations by handle instead ([`BexEngine::to_statement`]).
///
/// Fails only on an unresolved head, an invariant break surfaced as an error
/// rather than a stand-in.
pub(crate) fn to_annotation<L: Clone>(
    ty: &bex_vm_types::RealizedTy,
) -> Result<baml_type::WireTy<L>, EngineError> {
    ty.try_map_heads(&mut bex_vm_types::TypeHead::to_annotation_head)
        .map_err(|head| EngineError::TypeMismatch {
            message: format!("type leaving the VM has an unresolved head: {head}"),
        })
}

#[derive(Clone, Copy)]
pub(crate) enum InboundDeclarationKind {
    Any,
    Class,
    Enum,
}

/// Who a call's values cross to and from: a host across the boundary, or the
/// runtime's own code (a sys-op calling back into BAML: an HTTP server's
/// handler, a finalizer). What differs is what each may hand the engine; see
/// the hydrator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Peer {
    Host,
    Runtime,
}

impl BexEngine {
    /// Resolve the name a host spells to a declaration compiled with the
    /// program: the program's own surface first, then every declaration the
    /// VM loaded. A runtime declaration is never found by name; a host
    /// references one by handle.
    pub(crate) fn resolve_static_declaration(
        &self,
        vm: &BexVm,
        name: &str,
        kind: InboundDeclarationKind,
    ) -> Option<HeapPtr> {
        let statically_resolved = match kind {
            InboundDeclarationKind::Class => self
                .resolved_class_names
                .get(name)
                .or_else(|| resolve_named_object(&self.resolved_class_names, name)),
            InboundDeclarationKind::Enum => self
                .resolved_enum_names
                .get(name)
                .or_else(|| resolve_named_object(&self.resolved_enum_names, name)),
            InboundDeclarationKind::Any => self
                .resolved_class_names
                .get(name)
                .or_else(|| resolve_named_object(&self.resolved_class_names, name))
                .or_else(|| self.resolved_enum_names.get(name))
                .or_else(|| resolve_named_object(&self.resolved_enum_names, name)),
        };
        if let Some(ptr) = statically_resolved {
            return Some(*ptr);
        }
        vm.declaration_head(&baml_type::TypeName::from_dotted_path(name))
            .map(bex_vm_types::TypeHead::ptr)
    }

    /// A type the host stated, over the program's declarations: a compiled
    /// declaration resolved by name, a runtime declaration followed through
    /// the host's handle to it, and every head a declaration of the kind the
    /// type spells. A label is refused: it references nothing.
    pub(crate) fn anchor_wire_ty(
        &self,
        permit: PermitProof<'_>,
        vm: &BexVm,
        ty: &WireTy,
    ) -> Result<bex_vm_types::RealizedTy, EngineError> {
        let anchored = ty.try_map_heads(&mut |head: &WireHead<Handle>| {
            let ptr = match head {
                WireHead::Static(name) => self
                    .resolve_static_declaration(vm, &name.to_string(), InboundDeclarationKind::Any)
                    .ok_or_else(|| EngineError::TypeMismatch {
                        message: format!(
                            "host-supplied type names unknown declaration `{}`",
                            bex_external_types::Bounded::text(name.to_string())
                        ),
                    })?,
                WireHead::Live(handle) => self.resolve_handle(permit, handle).ok_or_else(|| {
                    EngineError::TypeMismatch {
                        message: "host-supplied type references a declaration whose handle \
                                      no longer resolves"
                            .to_string(),
                    }
                })?,
                WireHead::Anonymous(label) => {
                    return Err(EngineError::TypeMismatch {
                        message: format!(
                            "host-supplied type names `{}` by label only; a runtime declaration \
                             is passed as it was received",
                            bex_external_types::Bounded::text(label.as_str())
                        ),
                    });
                }
            };
            let tag = match vm.get_object(ptr) {
                Object::Class(class) => class.type_tag,
                Object::Enum(enm) => enm.type_tag,
                Object::Interface(interface) => interface.type_tag,
                Object::TypeAlias(alias) => alias.type_tag,
                _ => {
                    return Err(EngineError::TypeMismatch {
                        message: "host-supplied type references an object that is not a \
                                  declaration"
                            .to_string(),
                    });
                }
            };
            Ok(bex_vm_types::TypeHead::new(ptr, tag))
        })?;
        heads_match_their_kind(vm, &anchored)?;
        Ok(anchored)
    }

    /// A realized type as a statement the host may make again: a compiled
    /// declaration by name, a runtime declaration by a rooted handle to it.
    pub(crate) fn to_statement(
        &self,
        ty: &bex_vm_types::RealizedTy,
    ) -> Result<WireTy, EngineError> {
        let statement = ty
            .try_map_heads(&mut |head: &bex_vm_types::TypeHead| {
                if !head.is_resolved() {
                    return Err(bex_vm_types::UnnameableHead(head.tag()));
                }
                Ok(if head.tag().is_dynamic() {
                    WireHead::Live(self.heap.create_handle(head.ptr()))
                } else {
                    WireHead::Static(head.to_name()?)
                })
            })
            .map_err(|head| EngineError::TypeMismatch {
                message: format!("type leaving the VM has an unresolved head: {head}"),
            })?;
        Ok(statement)
    }
}

/// Every head in a host-stated type is a declaration of the kind the type
/// spells, and a class head takes as many type arguments as the class
/// declares: `list<class Color>` for an enum `Color`, or `Box` with no
/// arguments, is refused rather than built.
fn heads_match_their_kind(vm: &BexVm, ty: &bex_vm_types::RealizedTy) -> Result<(), EngineError> {
    use bex_vm_types::RealizedTy as Anchored;
    let mismatch = |what: &str| EngineError::TypeMismatch {
        message: format!("host-supplied type `{ty}` names {what}"),
    };
    match ty {
        Anchored::Class(head, args) => match vm.get_object(head.ptr()) {
            Object::Class(class) => {
                if class.generic_param_count != args.len() {
                    return Err(EngineError::TypeMismatch {
                        message: format!(
                            "host-supplied type `{ty}` gives {} type argument(s) to a class that \
                             takes {}",
                            args.len(),
                            class.generic_param_count
                        ),
                    });
                }
            }
            _ => return Err(mismatch("as a class a declaration that is not one")),
        },
        Anchored::Enum(head) | Anchored::EnumVariant(head, _) => {
            if !matches!(vm.get_object(head.ptr()), Object::Enum(_)) {
                return Err(mismatch("as an enum a declaration that is not one"));
            }
        }
        Anchored::Interface(head, ..) => {
            if !matches!(vm.get_object(head.ptr()), Object::Interface(_)) {
                return Err(mismatch("as an interface a declaration that is not one"));
            }
        }
        Anchored::TypeAlias(head) => {
            if !matches!(vm.get_object(head.ptr()), Object::TypeAlias(_)) {
                return Err(mismatch("as an alias a declaration that is not one"));
            }
        }
        Anchored::Int
        | Anchored::Bigint
        | Anchored::Float
        | Anchored::String
        | Anchored::Bool
        | Anchored::Null
        | Anchored::Uint8Array
        | Anchored::Media(_)
        | Anchored::Literal(..)
        | Anchored::List(_)
        | Anchored::Map { .. }
        | Anchored::Union(_)
        | Anchored::Function { .. }
        | Anchored::Future(..)
        | Anchored::RustType
        | Anchored::Type
        | Anchored::Resource
        | Anchored::PromptAst
        | Anchored::Unknown
        | Anchored::Never => {}
    }
    let mut failure = None;
    ty.visit_children(&mut |child| {
        if failure.is_none()
            && let Err(error) = heads_match_their_kind(vm, child)
        {
            failure = Some(error);
        }
    });
    failure.map_or(Ok(()), Err)
}

fn host_call_parameter_types<'a>(
    params: &[bex_vm_types::RealizedFunctionParamTy],
    positional_count: usize,
    optional_names: impl IntoIterator<Item = &'a str>,
) -> Result<
    (
        Vec<bex_vm_types::RealizedTy>,
        indexmap::IndexMap<String, bex_vm_types::RealizedTy>,
    ),
    String,
> {
    let required = params
        .iter()
        .filter(|param| param.is_required())
        .map(|param| param.ty.clone())
        .collect::<Vec<_>>();
    if positional_count != required.len() {
        return Err(format!(
            "host-call positional argument count {positional_count} does not match declared required count {}",
            required.len()
        ));
    }
    let mut optional = indexmap::IndexMap::new();
    for name in optional_names {
        let param = params
            .iter()
            .find(|param| {
                param.is_optional()
                    && param
                        .name
                        .as_ref()
                        .is_some_and(|candidate| candidate.as_str() == name)
            })
            .ok_or_else(|| format!("host-call supplied unknown optional argument {name:?}"))?;
        optional.insert(name.to_string(), param.ty.clone());
    }
    Ok((required, optional))
}

#[cfg(test)]
mod host_call_parameter_type_tests {
    use baml_type::{FunctionParamMode, Name};

    use super::*;

    fn param(
        name: &str,
        mode: FunctionParamMode,
        ty: bex_vm_types::RealizedTy,
    ) -> bex_vm_types::RealizedFunctionParamTy {
        bex_vm_types::RealizedFunctionParamTy {
            name: Some(Name::new(name)),
            ty,
            mode,
        }
    }

    #[test]
    fn resolves_required_and_exact_optional_wire_names() {
        let union = bex_vm_types::RealizedTy::Union(Box::new([
            bex_vm_types::RealizedTy::int(),
            bex_vm_types::RealizedTy::string(),
        ]));
        let params = vec![
            param("value", FunctionParamMode::Required, union.clone()),
            param("foo_bar", FunctionParamMode::Optional, union),
        ];
        let (required, optional) = host_call_parameter_types(&params, 1, ["foo_bar"]).unwrap();
        assert!(matches!(
            required.as_slice(),
            [bex_vm_types::RealizedTy::Union(..)]
        ));
        assert!(matches!(
            optional["foo_bar"],
            bex_vm_types::RealizedTy::Union(..)
        ));
    }

    #[test]
    fn rejects_malformed_required_arity_and_optional_name() {
        let params = vec![
            param(
                "value",
                FunctionParamMode::Required,
                bex_vm_types::RealizedTy::int(),
            ),
            param(
                "foo_bar",
                FunctionParamMode::Optional,
                bex_vm_types::RealizedTy::string(),
            ),
        ];
        let arity = host_call_parameter_types(&params, 0, std::iter::empty::<&str>()).unwrap_err();
        assert!(arity.contains("count 0"));
        assert!(arity.contains("required count 1"));

        let name = host_call_parameter_types(&params, 1, ["fooBar"]).unwrap_err();
        assert!(name.contains("unknown optional argument \"fooBar\""));
    }
}

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
        declared_type: &bex_vm_types::RealizedTy,
        vm: &BexVm,
        permit: PermitProof<'_>,
    ) -> Result<BexExternalValue, EngineError> {
        let selected_interface = find_implemented_interface_union_member(value, declared_type, vm)?;
        // Select union arms while the value is still a live VM value. Some
        // values intentionally become opaque tagged handles at the host
        // boundary; re-selecting from that host carrier would discard the
        // heap-owned nominal identity that made the arm unambiguous.
        let selected_runtime = selected_interface.or_else(|| match declared_type {
            bex_vm_types::RealizedTy::Union(members) => find_matching_union_member(value, members),
            _ => None,
        });
        let effective_type =
            selected_runtime.unwrap_or_else(|| resolve_effective_type(value, declared_type));

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
                self.convert_heap_ptr_to_external_with_type(idx, effective_type, vm, permit)?
            }
        };

        // The union envelope names the declared type and the arm beside the
        // value, as annotations: labels for runtime declarations, never rows.
        let declared_annotation = to_annotation(declared_type)?;
        if let Some(selected) = selected_runtime {
            return wrap_selected_union_member(
                external,
                &declared_annotation,
                &to_annotation(selected)?,
            );
        }
        maybe_wrap_union(external, &declared_annotation)
    }

    /// Convert an object to a `BexExternalValue` using the effective (non-union) type.
    ///
    /// # Safety
    ///
    /// This method uses unsafe calls to dereference `HeapPtr`. It is safe because:
    /// - We only read objects, never write
    /// - The caller ensures the pointer is valid (from a handle which is a GC root)
    //
    // BUG: this walk has no cycle detection (no visited set, no depth
    // budget). BAML permits self-referencing values; one reaching the host
    // boundary makes this recursion diverge. Reproduced via the python
    // bridge with `class Node { value int  next Node? }` and a body doing
    // `n.next = n`: the call never returns — a process sample shows every
    // frame pinned in this function's recursion for minutes at full CPU
    // (no catchable failure ever surfaces, so `call_and_encode` cannot
    // fold it into the result envelope). Every language bridge is exposed
    // identically (`BexExternalValue` and the outbound protobuf are both
    // trees, so no bridge can see — let alone handle — a cycle). The fix
    // belongs here: track visited `HeapPtr`s (identity, not equality) and
    // return an `EngineError` so a cyclic value surfaces on the
    // envelope's error arm in every SDK.
    fn convert_heap_ptr_to_external_with_type(
        &self,
        ptr: HeapPtr,
        effective_type: &bex_vm_types::RealizedTy,
        vm: &BexVm,
        permit: PermitProof<'_>,
    ) -> Result<BexExternalValue, EngineError> {
        // SAFETY: We only read objects, and the pointer comes from a valid handle.
        let obj = unsafe { ptr.get() };

        match obj {
            Object::Float(f) => Ok(BexExternalValue::Float(*f)),
            Object::String(s) => Ok(BexExternalValue::String(s.clone())),

            Object::Array(arr) => {
                // The element type is the declared one where the declared type
                // is a list; where it is not (an `unknown` slot, a builtin
                // class's array), the array's own, which the value carries.
                let own_element_type;
                let element_type = match effective_type {
                    bex_vm_types::RealizedTy::List(elem_ty) => elem_ty.as_ref(),
                    _ => {
                        own_element_type = arr.element_ty.as_ref().clone();
                        &own_element_type
                    }
                };

                // Snapshot under the source's lock; the recursive
                // convert call may lock other containers.
                let snapshot = arr.to_vec();
                let items: Result<Vec<_>, _> = snapshot
                    .iter()
                    .map(|v| {
                        self.convert_vm_value_to_external_with_type(*v, element_type, vm, permit)
                    })
                    .collect();
                Ok(BexExternalValue::Array {
                    element_type: to_annotation(element_type)?,
                    items: items?,
                })
            }

            Object::Map(map) => {
                // The key and value types are the declared ones where the
                // declared type is a map; where it is not, the map's own,
                // which the value carries.
                let own_types;
                let (key_type, value_type) = match effective_type {
                    bex_vm_types::RealizedTy::Map { key, value, .. } => {
                        (key.as_ref(), value.as_ref())
                    }
                    _ => {
                        own_types = (map.key_ty.as_ref().clone(), map.value_ty.as_ref().clone());
                        (&own_types.0, &own_types.1)
                    }
                };

                if !baml_type::normalize::is_string_key(key_type.as_ty(), vm)
                    || !baml_type::normalize::is_string_key(map.key_ty.as_ty(), vm)
                {
                    return Err(EngineError::CannotConvert {
                        type_name:
                            "map with non-string keys (SDK interchange requires string keys)"
                                .to_owned(),
                    });
                }
                let snapshot = map.snapshot_entries();
                let entries: Result<indexmap::IndexMap<String, BexExternalValue>, EngineError> =
                    snapshot
                        .iter()
                        .map(|(k, v)| {
                            let key = match k.as_object_ptr().map(|ptr| unsafe { ptr.get() }) {
                                Some(Object::String(key)) => key.to_string(),
                                _ => return Err(EngineError::CannotConvert {
                                    type_name: "map with non-string keys (SDK interchange requires string keys)".to_owned(),
                                }),
                            };
                            Ok((
                                key,
                                self.convert_vm_value_to_external_with_type(
                                    *v, value_type, vm, permit,
                                )?,
                            ))
                        })
                        .collect();
                Ok(BexExternalValue::Map {
                    key_type: to_annotation(key_type)?,
                    value_type: to_annotation(value_type)?,
                    entries: entries?,
                })
            }

            Object::Instance(instance) => {
                let Object::Class(class) = (unsafe { instance.class.get() }) else {
                    panic!("Instance.class should point to a Class object")
                };
                debug_assert_eq!(
                    class.fields.len(),
                    instance.fields.len(),
                    "Class '{}' has {} fields but instance has {} fields",
                    class.name(),
                    class.fields.len(),
                    instance.fields.len(),
                );
                let ty = || {
                    to_annotation(&bex_vm_types::RealizedTy::Class(
                        bex_vm_types::TypeHead::new(instance.class, class.type_tag),
                        instance.class_type_args.clone(),
                    ))
                };
                match class.crossing() {
                    ClassCrossing::Fields => Ok(BexExternalValue::Instance {
                        class_name: class.name().to_string(),
                        type_args: instance
                            .class_type_args
                            .iter()
                            .map(to_annotation)
                            .collect::<Result<Vec<_>, _>>()?,
                        fields: self.instance_fields(instance, class, vm, permit)?,
                    }),
                    // An instance no host reads into stays on the heap and
                    // crosses as a rooted reference: a standard-library
                    // capability, or an instance of a declaration made at
                    // run time, which no host has a type for. The engine's
                    // proxy kinds select the host wrapper; the wire `ty` is
                    // annotation-only, and method generic substitution
                    // recovers the instance's head and class type arguments
                    // after resolving the handle.
                    ClassCrossing::Opaque => {
                        Ok(BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
                            kind: bex_external_types::TaggedHeapHandleKind::RuntimeValue,
                            ty: ty()?,
                            heap_handle: self.heap.create_handle(ptr),
                        }))
                    }
                    ClassCrossing::Stream => {
                        Ok(BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
                            kind: bex_external_types::TaggedHeapHandleKind::Stream,
                            ty: ty()?,
                            heap_handle: self.heap.create_handle(ptr),
                        }))
                    }
                    ClassCrossing::FunctionSpec => {
                        Ok(BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
                            kind: bex_external_types::TaggedHeapHandleKind::FunctionSpec,
                            ty: ty()?,
                            heap_handle: self.heap.create_handle(ptr),
                        }))
                    }
                    // A media value and a rendered prompt cross as the
                    // payload their wrapper holds; a host builds a fresh
                    // wrapper around it.
                    form @ (ClassCrossing::Media(_) | ClassCrossing::Prompt) => {
                        let payload = class
                            .fields
                            .iter()
                            .position(|field| {
                                field.name == bex_external_types::MEDIA_WRAPPER_DATA_FIELD
                            })
                            .and_then(|slot| instance.fields[slot].load().as_object_ptr())
                            .and_then(|data| match unsafe { data.get() } {
                                Object::RustData(arc) => {
                                    bex_external_types::try_convert_rust_data(arc)
                                }
                                _ => None,
                            });
                        match (form, payload) {
                            (
                                ClassCrossing::Media(kind),
                                Some(BexExternalValue::Adt(BexExternalAdt::Media(media))),
                            ) if media.kind == kind => {
                                Ok(BexExternalValue::Adt(BexExternalAdt::Media(media)))
                            }
                            (
                                ClassCrossing::Prompt,
                                Some(BexExternalValue::Adt(BexExternalAdt::PromptAst(prompt))),
                            ) => Ok(BexExternalValue::Adt(BexExternalAdt::PromptAst(prompt))),
                            _ => Err(EngineError::VmInternalError(
                                bex_vm::errors::VmInternalError::BridgeFailure {
                                    message: format!(
                                        "an instance of `{}` does not hold its payload",
                                        class.name()
                                    ),
                                },
                            )),
                        }
                    }
                }
            }

            Object::Variant(variant) => {
                // Get enum name and variant name from the Enum object
                let enum_obj = unsafe { variant.enm.get() };
                let Object::Enum(enm) = enum_obj else {
                    panic!("Variant.enm should point to an Enum object")
                };
                // As for a class instance above: only a statically compiled
                // enum has a codegen entry, and any other spelling can collide
                // with one that does.
                if enm.type_tag.is_dynamic() {
                    let ty = to_annotation(&bex_vm_types::RealizedTy::Enum(
                        bex_vm_types::TypeHead::new(variant.enm, enm.type_tag),
                    ))?;
                    return Ok(BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
                        kind: bex_external_types::TaggedHeapHandleKind::RuntimeValue,
                        ty,
                        heap_handle: self.heap.create_handle(ptr),
                    }));
                }
                let variant_name = enm
                    .variants
                    .get(variant.index)
                    .map(|v| v.name.clone())
                    .ok_or_else(|| EngineError::TypeMismatch {
                        message: format!(
                            "enum '{}' has {} variants but variant index is {}",
                            enm.name(),
                            enm.variants.len(),
                            variant.index,
                        ),
                    })?;
                let enum_name = enm.name().to_string();

                Ok(BexExternalValue::Variant {
                    enum_name,
                    variant_name,
                })
            }

            Object::Function(_)
            | Object::Closure(_)
            | Object::BoundMethod(_)
            | Object::GenericFunction(_) => {
                // Every function value is realized: the compiler seeds an
                // inferred instantiation exactly as a written one, so one that
                // reaches the host with a slot missing would state a type
                // (`(int) -> int`) its frame cannot honor.
                debug_assert!(
                    callable_frame_is_complete(vm, obj),
                    "a function value crossed to the host without a type argument for every \
                     slot of its generic frame"
                );
                let handle = self.heap.create_handle(ptr);
                Ok(BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
                    kind: bex_external_types::TaggedHeapHandleKind::Callable,
                    ty: to_annotation(effective_type)?,
                    heap_handle: handle,
                }))
            }
            Object::Interface(_) => Err(EngineError::CannotConvert {
                type_name: "interface".to_string(),
            }),
            // A compiled package reaches a host only inside the
            // `reflect.Package` that holds it, which crosses by reference.
            Object::Package(_) => Err(EngineError::CannotConvert {
                type_name: "package".to_string(),
            }),
            Object::ImplRule(_) => Err(EngineError::CannotConvert {
                type_name: "impl_rule".to_string(),
            }),
            Object::Class(_) => Err(EngineError::CannotConvert {
                type_name: "class".to_string(),
            }),
            Object::Enum(_) => Err(EngineError::CannotConvert {
                type_name: "enum".to_string(),
            }),
            Object::TypeAlias(_) => Err(EngineError::CannotConvert {
                type_name: "type alias".to_string(),
            }),
            Object::Future(_) => Err(EngineError::CannotConvert {
                type_name: "future".to_string(),
            }),
            Object::Bigint(bi) => Ok(BexExternalValue::Bigint((**bi).clone())),
            // A type value crosses as a statement: compiled declarations by
            // name, runtime declarations by a rooted handle each. The host
            // may state the same type back and it anchors to the same
            // declarations.
            Object::Type(type_value) => Ok(BexExternalValue::Adt(BexExternalAdt::WireType(
                self.to_statement(&type_value.ty)?,
            ))),
            Object::Uint8Array(bytes) => Ok(BexExternalValue::Uint8Array(bytes.to_vec())),
            Object::RustData(arc) => Ok(bex_external_types::try_convert_rust_data(arc)
                .unwrap_or_else(|| BexExternalValue::RustData(arc.clone()))),
            Object::HostClosure(_) => Err(EngineError::CannotConvert {
                type_name: "host_closure".to_string(),
            }),
            Object::Cell(_) => Err(EngineError::CannotConvert {
                type_name: "cell".to_string(),
            }),
            Object::Tombstone => Object::tombstone_reached(),
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => Err(EngineError::CannotSnapshot {
                type_name: "sentinel".to_string(),
            }),
        }
    }
}

impl BexEngine {
    /// An instance's fields, each at its type under the instance's type
    /// arguments.
    fn instance_fields(
        &self,
        instance: &bex_vm_types::Instance,
        class: &bex_vm_types::Class,
        vm: &BexVm,
        permit: PermitProof<'_>,
    ) -> Result<indexmap::IndexMap<String, BexExternalValue>, EngineError> {
        class
            .fields
            .iter()
            .zip(instance.fields.iter())
            .map(|(field, slot)| {
                // An instance is fully bound, so its field types are realized.
                let field_type = field
                    .field_template
                    .substitute(&instance.class_type_args, vm)
                    .map_err(|error| {
                        EngineError::VmInternalError(
                            bex_vm::errors::VmInternalError::TypeSubstitution {
                                message: format!(
                                    "field `{}` of an instance is not realized: {error}",
                                    field.name
                                ),
                            },
                        )
                    })?;
                Ok((
                    field.name.clone(),
                    self.convert_vm_value_to_external_with_type(
                        slot.load(),
                        &field_type,
                        vm,
                        permit,
                    )?,
                ))
            })
            .collect()
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
    /// Convert the VM's `[required_args, optional_args]` host-call pack using
    /// the callable's declared parameter types. Ordinary sys-op conversion is
    /// intentionally type-erased, but host callbacks need the declared type to
    /// preserve closed-union selected-arm metadata on the outbound wire.
    pub(crate) fn convert_host_call_args_pack(
        &self,
        pack: Value,
        params: &[bex_vm_types::RealizedFunctionParamTy],
        vm: &BexVm,
        permit: PermitProof<'_>,
    ) -> Result<BexExternalValue, EngineError> {
        let malformed = |message: String| {
            EngineError::VmInternalError(bex_vm::errors::VmInternalError::BridgeFailure { message })
        };
        let pack_ptr = pack
            .as_object_ptr()
            .ok_or_else(|| malformed("host-call argument pack is not a heap object".to_string()))?;
        // SAFETY: `permit` witnesses that the VM heap cannot move while these
        // pointers are inspected. Container snapshots release their locks
        // before recursive conversion.
        let Object::Array(pack_array) = (unsafe { pack_ptr.get() }) else {
            return Err(malformed(
                "host-call argument pack is not an array".to_string(),
            ));
        };
        let pack_items = pack_array.to_vec();
        if pack_items.len() != 2 {
            return Err(malformed(format!(
                "host-call argument pack has {} items, expected 2",
                pack_items.len()
            )));
        }

        let positional_ptr = pack_items[0].as_object_ptr().ok_or_else(|| {
            malformed("host-call positional arguments are not a heap object".to_string())
        })?;
        // SAFETY: covered by the active heap permit above.
        let Object::Array(positional_array) = (unsafe { positional_ptr.get() }) else {
            return Err(malformed(
                "host-call positional arguments are not an array".to_string(),
            ));
        };
        let positional_values = positional_array.to_vec();
        let optional_ptr = pack_items[1].as_object_ptr().ok_or_else(|| {
            malformed("host-call optional arguments are not a heap object".to_string())
        })?;
        // SAFETY: covered by the active heap permit above.
        let Object::Map(optional_map) = (unsafe { optional_ptr.get() }) else {
            return Err(malformed(
                "host-call optional arguments are not a map".to_string(),
            ));
        };
        let optional_values = optional_map
            .snapshot_entries()
            .into_iter()
            .map(
                |(key, value)| match key.as_object_ptr().map(|ptr| unsafe { ptr.get() }) {
                    Some(Object::String(key)) => Ok((key.clone(), value)),
                    _ => Err(malformed(
                        "host-call optional argument names must be strings".to_owned(),
                    )),
                },
            )
            .collect::<Result<indexmap::IndexMap<_, _>, _>>()?;
        let (required_types, optional_types) = host_call_parameter_types(
            params,
            positional_values.len(),
            optional_values
                .keys()
                .map(bex_external_types::BexStr::as_str),
        )
        .map_err(malformed)?;
        let positional = positional_values
            .into_iter()
            .zip(required_types)
            .map(|(value, ty)| self.convert_vm_value_to_external_with_type(value, &ty, vm, permit))
            .collect::<Result<Vec<_>, _>>()?;

        let mut optional = indexmap::IndexMap::with_capacity(optional_values.len());
        for (name, value) in optional_values {
            let name = name.to_string();
            let ty = &optional_types[&name];
            optional.insert(
                name,
                self.convert_vm_value_to_external_with_type(value, ty, vm, permit)?,
            );
        }

        Ok(BexExternalValue::Array {
            element_type: WireTy::unknown(),
            items: vec![
                BexExternalValue::Array {
                    element_type: WireTy::unknown(),
                    items: positional,
                },
                BexExternalValue::Map {
                    key_type: WireTy::String,
                    value_type: WireTy::unknown(),
                    entries: optional,
                },
            ],
        })
    }

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
        owned_external_value(&self.heap, permit, value)
    }
}

/// [`BexEngine::vm_value_to_owned`] for a caller that holds the heap but not
/// an engine — the `$init` run inside engine construction, which reports a
/// failing initializer's thrown value before any engine exists.
pub(crate) fn owned_external_value(
    heap: &std::sync::Arc<::bex_heap::BexHeap>,
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
                .as_owned_for_trace(heap, permit)
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

// ============================================================================
// Helper Functions (standalone, no &self needed)
// ============================================================================

/// Wrap a value in Union metadata if the declared type is a union.
pub(crate) fn maybe_wrap_union(
    value: BexExternalValue,
    declared_type: &WireTy,
) -> Result<BexExternalValue, EngineError> {
    match declared_type {
        // A nullable union (`T?` == `T | null`) is optionality, not a tagged
        // union: a null value is bare `Null`, and a single non-null member is
        // unwrapped to its bare value (recursing in case that member is itself a
        // real union). This preserves the pre-desugaring behavior where optional
        // values carried no union metadata.
        WireTy::Union(members) if members.iter().any(WireTy::is_null) => {
            if matches!(value, BexExternalValue::Null) {
                return Ok(BexExternalValue::Null);
            }
            let non_null: Vec<WireTy> = members.iter().filter(|m| !m.is_null()).cloned().collect();
            match non_null.len() {
                0 => Ok(value),
                1 => maybe_wrap_union(value, &non_null[0]),
                _ => {
                    let selected = find_matching_member(&value, &non_null)?;
                    let metadata = UnionMetadata::new(WireTy::Union(members.clone()), selected);
                    Ok(BexExternalValue::Union {
                        value: Box::new(value),
                        metadata,
                    })
                }
            }
        }
        WireTy::Union(members) => {
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

fn wrap_selected_union_member(
    value: BexExternalValue,
    declared_type: &WireTy,
    selected: &WireTy,
) -> Result<BexExternalValue, EngineError> {
    let WireTy::Union(members) = declared_type else {
        return Ok(value);
    };
    if members.iter().any(WireTy::is_null) {
        if matches!(value, BexExternalValue::Null) {
            return Ok(value);
        }
        let non_null: Vec<&WireTy> = members.iter().filter(|member| !member.is_null()).collect();
        if let [non_null] = non_null.as_slice() {
            // Optionality itself is untagged, but its sole non-null member may
            // still be a real (possibly nested/aliased) union. Preserve that
            // member's selected-arm envelope just like `maybe_wrap_union`.
            return maybe_wrap_union(value, non_null);
        }
    }

    Ok(BexExternalValue::Union {
        value: Box::new(value),
        metadata: UnionMetadata::new(declared_type.clone(), selected.clone()),
    })
}

/// Recover `TypeVar(name) -> concrete` bindings by walking a declared type and
/// a live value's type in parallel; the first binding of a variable wins.
/// Used for a generic receiver called by name: the declared `self` mentions
/// the class's type variables, and the rooted instance carries them
/// concretely (`Stream<string>`), so its own type, never descriptive handle
/// metadata, binds them.
pub(crate) fn collect_live_type_var_bindings<DeclaredHead: Clone, ConcreteHead: Clone>(
    declared: &baml_type::RuntimeTy<DeclaredHead>,
    concrete: &baml_type::RuntimeTy<ConcreteHead>,
    out: &mut indexmap::IndexMap<String, baml_type::RuntimeTy<ConcreteHead>>,
) {
    use baml_type::RuntimeTy;
    let mut zip = |declared: &[RuntimeTy<DeclaredHead>], concrete: &[RuntimeTy<ConcreteHead>]| {
        if declared.len() == concrete.len() {
            for (declared, concrete) in declared.iter().zip(concrete) {
                collect_live_type_var_bindings(declared, concrete, out);
            }
        }
    };
    // The declared side decides where a variable can sit; a concrete side of
    // another shape binds nothing there. First binding wins.
    match (declared, concrete) {
        (RuntimeTy::TypeVar(name), _) => {
            out.entry(name.to_string())
                .or_insert_with(|| concrete.clone());
        }
        (RuntimeTy::Class(_, declared_args), RuntimeTy::Class(_, concrete_args)) => {
            zip(declared_args, concrete_args);
        }
        (
            RuntimeTy::Interface(_, declared_args, declared_assoc),
            RuntimeTy::Interface(_, concrete_args, concrete_assoc),
        ) => {
            zip(declared_args, concrete_args);
            for (name, declared) in declared_assoc {
                if let Some((_, concrete)) = concrete_assoc.iter().find(|(other, _)| other == name)
                {
                    collect_live_type_var_bindings(declared, concrete, out);
                }
            }
        }
        (RuntimeTy::List(declared), RuntimeTy::List(concrete)) => {
            collect_live_type_var_bindings(declared, concrete, out);
        }
        (
            RuntimeTy::Map {
                key: declared_key,
                value: declared_value,
            },
            RuntimeTy::Map {
                key: concrete_key,
                value: concrete_value,
            },
        ) => {
            collect_live_type_var_bindings(declared_key, concrete_key, out);
            collect_live_type_var_bindings(declared_value, concrete_value, out);
        }
        (RuntimeTy::Union(declared), RuntimeTy::Union(concrete)) => zip(declared, concrete),
        (
            RuntimeTy::Future(declared_value, declared_error),
            RuntimeTy::Future(concrete_value, concrete_error),
        ) => {
            collect_live_type_var_bindings(declared_value, concrete_value, out);
            collect_live_type_var_bindings(declared_error, concrete_error, out);
        }
        (
            RuntimeTy::Function {
                params: declared_params,
                ret: declared_ret,
                throws: declared_throws,
            },
            RuntimeTy::Function {
                params: concrete_params,
                ret: concrete_ret,
                throws: concrete_throws,
            },
        ) => {
            if declared_params.len() == concrete_params.len() {
                for (declared, concrete) in declared_params.iter().zip(concrete_params) {
                    collect_live_type_var_bindings(&declared.ty, &concrete.ty, out);
                }
            }
            collect_live_type_var_bindings(declared_ret, concrete_ret, out);
            collect_live_type_var_bindings(declared_throws, concrete_throws, out);
        }
        (
            RuntimeTy::Class(..)
            | RuntimeTy::Interface(..)
            | RuntimeTy::List(_)
            | RuntimeTy::Map { .. }
            | RuntimeTy::Union(_)
            | RuntimeTy::Future(..)
            | RuntimeTy::Function { .. },
            _,
        ) => {}
        // A leaf, or a position whose type the call binds elsewhere, holds no
        // variable to bind from here.
        (
            RuntimeTy::Int
            | RuntimeTy::Bigint
            | RuntimeTy::Float
            | RuntimeTy::String
            | RuntimeTy::Bool
            | RuntimeTy::Null
            | RuntimeTy::Uint8Array
            | RuntimeTy::Media(_)
            | RuntimeTy::Literal(..)
            | RuntimeTy::Enum(_)
            | RuntimeTy::EnumVariant(..)
            | RuntimeTy::RustType
            | RuntimeTy::Type
            | RuntimeTy::Resource
            | RuntimeTy::PromptAst
            | RuntimeTy::Unknown
            | RuntimeTy::Never,
            _,
        ) => {}
        // A recursive alias is named, not unfolded, in a declared signature,
        // and a projection is reduced once its base is bound: neither binds a
        // variable by position.
        (RuntimeTy::TypeAlias(_) | RuntimeTy::AssociatedTypeProjection { .. }, _) => {}
    }
}

/// The name of the first `TypeVar` reachable in `ty`, or `None` if the type
/// mentions none.
pub(crate) fn first_unbound_type_var<N: Clone>(ty: &baml_type::RuntimeTy<N>) -> Option<String> {
    if let baml_type::RuntimeTy::TypeVar(name) = ty {
        return Some(name.to_string());
    }
    let mut found = None;
    ty.visit_children(&mut |child| {
        if found.is_none() {
            found = first_unbound_type_var(child);
        }
    });
    found
}

/// Whether `ty` mentions any `TypeVar` (i.e. the callee is generic at this
/// position). Thin wrapper over [`first_unbound_type_var`].
pub(crate) fn contains_type_var<N: Clone>(ty: &baml_type::RuntimeTy<N>) -> bool {
    first_unbound_type_var(ty).is_some()
}

// ===========================================================================
// Inbound value-inference (01a/01b). The runtime engine drives call-site
// `TypeVar` solving from argument *values* (synthesized by `synth_ty_from_value`)
// instead of typed expressions. The unifier itself is the SHARED one in
// `baml_type_runtime`: the two seams below widen the engine's `RuntimeTy`
// inputs up to `Ty`, run `infer_value_bindings` / `union_ty`, and narrow each
// result back. This replaces the hand-maintained `RuntimeTy` port that used to
// duplicate the TIR's `infer_bindings_inner` / `union_ty` arm-for-arm (see
// `01c-inbound-inference-reuse.md`). Narrowing never fails: every inferred
// binding is a subterm (or union) of a `RuntimeTy`-derived actual, so it can
// carry no `tir`-axis variant.
// ===========================================================================

/// Combine two types into a normalized union — the `RuntimeTy` seam over the
/// shared [`baml_type_runtime::union_ty`]. Used when the same `TypeVar` is
/// inferred from several arguments (`choose(5, "a")` → `T = int | string`) or a
/// host-only sibling (`choose(5, host_obj)` → `int | rust_type`, the type an
/// opaque host value has).
#[cfg(test)]
pub(crate) fn union_runtime_ty(
    a: &baml_type::RuntimeTy,
    b: &baml_type::RuntimeTy,
) -> baml_type::RuntimeTy {
    baml_type::RuntimeTy::try_from(baml_type_runtime::union_ty(
        &baml_type::Ty::from(a),
        &baml_type::Ty::from(b),
    ))
    .expect("union of runtime types is a runtime type")
}

/// Recover `TypeVar(name) -> concrete` bindings from a `(formal, actual)` pair,
/// **union-merging** repeat bindings of the same variable across calls. The
/// `RuntimeTy` seam over the shared [`baml_type_runtime::infer_value_bindings`]
/// — the runtime variant of the TIR unifier, which binds a `Class` arm only when
/// the formal and actual name the same class.
///
/// Contrast [`collect_live_type_var_bindings`], the *first-wins* self-receiver
/// path, which is intentionally a separate, simpler walk.
///
/// Superseded at the call seam by [`infer_bindings_runtime_checked`] (which
/// solves all arguments together with variance tracking); retained as a
/// best-effort per-pair primitive exercised by the unit tests below.
#[cfg(test)]
#[expect(
    deprecated,
    reason = "fact-free by necessity: a per-pair primitive with no VM to supply facts"
)]
pub(crate) fn infer_bindings_runtime(
    formal: &baml_type::RuntimeTy,
    actual: &baml_type::RuntimeTy,
    out: &mut indexmap::IndexMap<String, baml_type::RuntimeTy>,
) {
    let mut bindings: rustc_hash::FxHashMap<
        baml_type::ParamTy,
        baml_type::Ty<baml_type::TypeName>,
    > = rustc_hash::FxHashMap::default();
    baml_type_runtime::infer_value_bindings(
        &baml_type::Ty::from(formal),
        &baml_type::Ty::from(actual),
        &mut bindings,
        &baml_type::normalize::NoFacts,
    );
    for (name, ty) in bindings {
        // A binding is always a subterm/union of a runtime-derived actual, so the
        // narrow cannot fail; skip defensively rather than panic if it ever does.
        let Ok(rt) = baml_type::RuntimeTy::try_from(ty) else {
            continue;
        };
        out.entry(name.to_string())
            .and_modify(|existing| *existing = union_runtime_ty(existing, &rt))
            .or_insert(rt);
    }
}

/// Variance-aware inference over *all* arguments of a generic call at once
/// (`02d`/`02e`). Accumulates every `(formal, actual)` pair into one
/// [`baml_type_runtime::InferenceConstraints`] and solves with variance
/// tracking, so a `TypeVar` used at conflicting variances across arguments —
/// `pair<T>(a: T[], b: T[])` over `int[]`/`string[]`, or `glue<T>(bare: T, arr:
/// T[])` over `int`/`string[]` — is *rejected* (returns `Err`) rather than
/// silently union-merged into an unsound binding. The error string is the
/// solver's "no consistent `T`" reason, surfaced by the caller as a Gate-A
/// `TypeMismatch`.
///
/// Contrast [`infer_bindings_runtime`], the per-argument best-effort merge kept
/// for the self-receiver and callable-summary paths.
// BUG: the shared solver has no rule binding an interface formal
// (`Container<T>`) from a concrete actual that implements it (`IntBox`), so
// `unwrap(IntBox{..})` does not infer `T = int` even with the VM's facts.
pub(crate) fn infer_bindings_runtime_checked<N: baml_type::Head + baml_type::HeadDisplay>(
    pairs: &[(baml_type::RuntimeTy<N>, baml_type::RuntimeTy<N>)],
    ctx: &impl baml_type::normalize::TypeContext<N>,
) -> Result<indexmap::IndexMap<String, baml_type::RuntimeTy<N>>, String> {
    let mut cons = baml_type_runtime::InferenceConstraints::new();
    for (formal, actual) in pairs {
        cons.record(
            &baml_type::Ty::from(formal),
            &baml_type::Ty::from(actual),
            ctx,
        );
    }
    let bindings = cons.solve(ctx).map_err(|e| e.message)?;
    let mut out = indexmap::IndexMap::new();
    for (name, ty) in bindings {
        // A binding is always a subterm/union of a runtime-derived actual, so the
        // narrow cannot fail; skip defensively rather than panic if it ever does.
        if let Ok(rt) = baml_type::RuntimeTy::try_from(ty) {
            out.insert(name.to_string(), rt);
        }
    }
    Ok(out)
}

/// The type variables that occur **inside a function-typed parameter's
/// signature** (its params, return or throws, at any depth) across a generic
/// call's parameter types. A host callable is opaque to BAML, so such a var
/// cannot be inferred from it *or* validated against it: it is poisoned and
/// must be specified explicitly (TYPEVAR BINDING POLICY rule 2), overriding
/// any value-position evidence it might also have.
pub(crate) fn closure_poisoned_vars<N: Clone>(
    params: &[baml_type::RuntimeTy<N>],
) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for p in params {
        walk_closure_vars(p, false, &mut out);
    }
    out
}

/// The type variables that occur under a union (an optional is one) in a
/// parameter whose value arrived (`arrived[i]`), outside any function-typed
/// position. A value under such a union that a concrete sibling took binds
/// the variable to nothing; the caller then binds it to `never` (TYPEVAR
/// BINDING POLICY rule 4).
pub(crate) fn vars_under_unions<N: Clone>(
    params: &[baml_type::RuntimeTy<N>],
    arrived: &[bool],
) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for (param, arrived) in params.iter().zip(arrived) {
        if *arrived {
            walk_union_vars(param, false, &mut out);
        }
    }
    out
}

fn walk_union_vars<N: Clone>(
    ty: &baml_type::RuntimeTy<N>,
    under_union: bool,
    out: &mut std::collections::HashSet<String>,
) {
    match ty {
        baml_type::RuntimeTy::TypeVar(name) => {
            if under_union {
                out.insert(name.to_string());
            }
        }
        // A function-typed position is rule 2's: nothing under it is evidence.
        baml_type::RuntimeTy::Function { .. } => {}
        baml_type::RuntimeTy::Union(members) => {
            for m in members {
                walk_union_vars(m, true, out);
            }
        }
        other => other.visit_children(&mut |child| walk_union_vars(child, under_union, out)),
    }
}

fn walk_closure_vars<N: Clone>(
    ty: &baml_type::RuntimeTy<N>,
    in_closure: bool,
    out: &mut std::collections::HashSet<String>,
) {
    match ty {
        baml_type::RuntimeTy::TypeVar(name) => {
            if in_closure {
                out.insert(name.to_string());
            }
        }
        // Descending into a function-typed position poisons every var reached
        // through it — params, return, and throws alike.
        baml_type::RuntimeTy::Function { .. } => {
            ty.visit_children(&mut |child| walk_closure_vars(child, true, out));
        }
        other => other.visit_children(&mut |child| walk_closure_vars(child, in_closure, out)),
    }
}

/// Find which union member matches a value.
///
/// `Unknown` arms match any value (see `value_matches_type`) and are
/// considered last so a more-specific arm wins. This keeps the union
/// metadata's `selected_option` faithful when concrete arms (e.g.
/// `Done` in `Unknown | Done`) actually fit.
fn find_matching_member(
    value: &BexExternalValue,
    members: &[WireTy],
) -> Result<WireTy, EngineError> {
    if let BexExternalValue::Union { metadata, value } = value
        && members
            .iter()
            .any(|member| selected_arm_equal(member, &metadata.selected_option))
        && value_matches_type(value, &metadata.selected_option)
    {
        return Ok(metadata.selected_option.clone());
    }
    // Realized container descriptors are an exact discriminator even when a
    // broader arm would also accept every payload value (for example
    // `int[]` also inhabits `int?[]`). Prefer that identity before ordinary
    // assignability checks.
    for member in members {
        let exact_container = match (value, member) {
            (BexExternalValue::Array { element_type, .. }, WireTy::List(expected)) => {
                runtime_ty_structurally_equal(element_type, expected)
            }
            (
                BexExternalValue::Map {
                    key_type,
                    value_type,
                    ..
                },
                WireTy::Map { key, value, .. },
            ) => {
                runtime_ty_structurally_equal(key_type, key)
                    && runtime_ty_structurally_equal(value_type, value)
            }
            _ => false,
        };
        if exact_container {
            return Ok(member.clone());
        }
    }
    // Exact literal arms outrank their broad primitive (`"draft"` before
    // `string`) regardless of declaration order.
    for member in members {
        if matches!(member, WireTy::Literal(..)) && value_matches_type(value, member) {
            return Ok(member.clone());
        }
    }
    // A singleton enum-variant arm likewise outranks its broad enum type.
    for member in members {
        if matches!(member, WireTy::EnumVariant(..)) && value_matches_type(value, member) {
            return Ok(member.clone());
        }
    }
    let mut matching: Vec<&WireTy> = Vec::new();
    for member in members.iter().filter(|member| {
        !matches!(
            member,
            WireTy::Literal(..) | WireTy::EnumVariant(..) | WireTy::Unknown
        ) && value_matches_type(value, member)
    }) {
        if matching
            .iter()
            .all(|matched| !runtime_ty_structurally_equal(matched, member))
        {
            matching.push(member);
        }
    }
    match matching.as_slice() {
        [member] => return Ok((*member).clone()),
        [] => {}
        _ => {
            return Err(EngineError::TypeMismatch {
                message: format!(
                    "value of type `{}` matches multiple union members; add an inbound `value_type` annotation to select one",
                    value.type_name()
                ),
            });
        }
    }
    for member in members {
        if matches!(member, WireTy::Unknown) {
            return Ok(member.clone());
        }
    }
    // This indicates a type system inconsistency - the value should match one of the members
    Err(EngineError::TypeMismatch {
        message: format!(
            "Value of type '{}' does not match any member of union {:?}",
            described_value_type(value),
            members
        ),
    })
}

/// `type_name`, refined for the mismatch diagnostics: an instance or variant
/// names its class/enum, since "instance" alone cannot identify which
/// declaration failed to match.
fn described_value_type(value: &BexExternalValue) -> String {
    match value {
        BexExternalValue::Instance { class_name, .. } => format!("instance of `{class_name}`"),
        BexExternalValue::Variant { enum_name, .. } => format!("variant of `{enum_name}`"),
        other => other.type_name().to_string(),
    }
}

pub(crate) fn type_name_matches_external_name(
    external_name: &str,
    head: &WireHead<Handle>,
) -> bool {
    // A value spelled by name is an instance of a compiled declaration; a
    // runtime declaration is referenced, never named, so no spelling
    // matches it.
    let WireHead::Static(type_name) = head else {
        return false;
    };
    external_name == type_name.display_name().as_str()
        || external_name == type_name.render_dotted(false)
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
    // MIR uses `display_name`, which strips the `user.` prefix for
    // user-package types, so an `Instance` built for a declared class
    // may carry `lorem.MyLorem` while the engine registered
    // `user.lorem.MyLorem`. Try the `user.`
    // prefix as a fallback before giving up. Builtin/vendor types keep
    // their full FQN so this only fires for user-package classes.
    let user_qualified = format!("user.{name}");
    if let Some(found) = objects.get(&user_qualified) {
        return Some(found);
    }
    None
}

/// Whether a value the engine produced is of a declared type, as it is: the
/// outbound side asks this to say which member of a union a value came out
/// of. Nothing is converted on the way out.
fn value_matches_type(value: &BexExternalValue, ty: &WireTy) -> bool {
    match (value, ty) {
        // `Unknown` is the engine's "any value matches" sentinel
        // (TypeScript `unknown` semantics — see `baml_type::WireTy::Unknown`).
        // Used by the stdlib generics hardcode in `baml_compiler2_mir::lower`
        // so e.g. `Stream<T>.next() -> T | Done`
        // accepts any partial-stream payload as the `T` arm.
        (_, WireTy::Unknown) => true,
        (BexExternalValue::Null, WireTy::Null) => true,
        (BexExternalValue::Int(_), WireTy::Int) => true,
        (BexExternalValue::Bigint(_), WireTy::Bigint) => true,
        (BexExternalValue::Float(_), WireTy::Float) => true,
        (BexExternalValue::Bool(_), WireTy::Bool) => true,
        (BexExternalValue::String(_), WireTy::String) => true,
        // Literal types match their corresponding runtime values
        (BexExternalValue::Int(value), WireTy::Literal(Literal::Int(expected), _)) => {
            value == expected
        }
        (BexExternalValue::Bigint(value), WireTy::Literal(Literal::Bigint(expected), _)) => {
            value == expected
        }
        (BexExternalValue::Float(value), WireTy::Literal(Literal::Float(expected), _)) => {
            float_literal_matches(*value, expected)
        }
        (BexExternalValue::Uint8Array(_), WireTy::Uint8Array) => true,
        (BexExternalValue::String(value), WireTy::Literal(Literal::String(expected), _)) => {
            value.as_str() == expected
        }
        (BexExternalValue::Bool(value), WireTy::Literal(Literal::Bool(expected), _)) => {
            value == expected
        }
        (BexExternalValue::Array { items, .. }, WireTy::List(expected_element)) => items
            .iter()
            .all(|item| value_matches_type(item, expected_element)),
        (
            BexExternalValue::Map { entries, .. },
            WireTy::Map {
                value: expected_value,
                ..
            },
        ) => entries
            .values()
            .all(|value| value_matches_type(value, expected_value)),
        // `BexExternalValue::Instance` now carries its wire-supplied class
        // type args, so we can disambiguate `Foo<int>` from `Foo<string>` at
        // the FFI boundary instead of name-only. The comparison is name-only
        // where the *declared* `Class`'s args are absent/erased (a non-generic
        // class, or args still bare TypeVars); against a concrete generic slot
        // the wire instance must supply matching args — see
        // `class_type_args_compatible`.
        (
            BexExternalValue::Instance {
                class_name,
                type_args,
                ..
            },
            WireTy::Class(tn, expected_args),
        ) => {
            (class_name.is_empty() || type_name_matches_external_name(class_name, tn))
                && (type_args.is_empty() || class_type_args_compatible(type_args, expected_args))
        }
        // Media wrapper instances flatten to their canonical portable ADT at
        // the host boundary. That owned value still inhabits the corresponding
        // stdlib wrapper arm (for example `ai.PromptPart`'s
        // `baml.media.Image` member) even though it no longer has class shape.
        (BexExternalValue::Adt(BexExternalAdt::Media(media)), wrapper @ WireTy::Class(..)) => {
            stdlib_media_wrapper_kind(wrapper).is_some_and(|kind| kind == media.kind)
        }
        (BexExternalValue::Variant { enum_name, .. }, WireTy::Enum(tn)) => {
            type_name_matches_external_name(enum_name, tn)
        }
        (
            BexExternalValue::Variant {
                enum_name,
                variant_name,
            },
            WireTy::EnumVariant(tn, expected_variant),
        ) => {
            type_name_matches_external_name(enum_name, tn)
                && variant_name == expected_variant.as_str()
        }
        (BexExternalValue::Adt(BexExternalAdt::Media(media)), WireTy::Media(expected_kind)) => {
            *expected_kind == baml_type::MediaKind::Generic || media.kind == *expected_kind
        }
        (BexExternalValue::HostValue(value), WireTy::Function { .. }) => {
            value.kind == bex_external_types::HostValueKind::Callable
        }
        (BexExternalValue::RustData(_), WireTy::RustType) => true,
        (BexExternalValue::HostValue(value), WireTy::RustType) => {
            value.kind == bex_external_types::HostValueKind::Opaque
        }
        (
            BexExternalValue::Adt(BexExternalAdt::Type(_) | BexExternalAdt::WireType(_)),
            WireTy::Type,
        ) => true,
        (union_value @ BexExternalValue::Union { metadata, .. }, WireTy::Union(members)) => {
            members.iter().any(|member| {
                selected_arm_equal(member, &metadata.selected_option)
                    && value_matches_type(union_value, member)
            })
        }
        (BexExternalValue::Union { value, .. }, ty) => value_matches_type(value, ty),
        (value, WireTy::TypeAlias(WireHead::Static(name))) if is_canonical_json_alias(name) => {
            value_satisfies_json(value)
        }
        // Handle nested unions (including nullable `T | null`) in the type.
        (value, WireTy::Union(members)) => members.iter().any(|m| value_matches_type(value, m)),
        _ => false,
    }
}

/// Compare a generic instance's wire-supplied class type args against a declared
/// `Class`'s args at the FFI boundary. Strict where the declared type is
/// *concrete*: a generic instance must arrive fully bound (Phase 2/3), so an
/// instance that omits the args a concrete generic slot requires — or whose
/// arity disagrees — is a positive mismatch, not a shape surprise to wave
/// through.
///
/// - `expected_args` empty → compatible: the declared class is non-generic, so
///   there is nothing to check (fall back to name-only).
/// - `wire_args` empty against a non-empty `expected_args`: reject only if the
///   expected args are concrete. If they are still erased/unconcretized
///   wildcards (`TypeVar`/`Unknown` — e.g. an instance method's class
///   param that couldn't be bound, lowered to runtime `unknown`), there is
///   nothing concrete to contradict, so stay lenient.
/// - differing (non-zero) arity → reject.
/// - per-arg: an expected `TypeVar`/`Unknown` is a wildcard; otherwise the
///   wire arg must be compatible with the expected arg.
pub(crate) fn class_type_args_compatible(wire_args: &[WireTy], expected_args: &[WireTy]) -> bool {
    if expected_args.is_empty() {
        return true;
    }
    if wire_args.is_empty() {
        return expected_args.iter().all(is_wildcard_ty);
    }
    if wire_args.len() != expected_args.len() {
        return false;
    }
    wire_args
        .iter()
        .zip(expected_args)
        .all(|(wire, expected)| runtime_ty_compatible(wire, expected))
}

/// A type-arg position that imposes no concrete constraint: the `unknown`
/// sentinel. Such a position can't positively
/// contradict a wire arg, so the structural matcher treats it as a wildcard.
fn is_wildcard_ty(ty: &WireTy) -> bool {
    matches!(ty, WireTy::Unknown)
}

/// Structural compatibility of a wire-supplied type against a declared
/// (substituted) type, used to disambiguate generic instances at the FFI
/// boundary without depending on full subtyping. Lenient:
/// only a *positive* leaf/shape mismatch returns `false`.
///
/// - an expected `unknown` is a wildcard;
/// - two primitives must be the same primitive (this is what separates
///   `Foo<int>` from `Foo<string>`);
/// - matching containers recurse; class names must match, then args recurse;
/// - anything else is treated as compatible (don't over-reject).
fn runtime_ty_compatible(wire: &WireTy, expected: &WireTy) -> bool {
    use WireTy as T;
    match (wire, expected) {
        (_, T::Unknown) | (T::Unknown, _) => true,
        (T::Int, T::Int)
        | (T::String, T::String)
        | (T::Bool, T::Bool)
        | (T::Float, T::Float)
        | (T::Null, T::Null)
        | (T::Bigint, T::Bigint)
        | (T::Uint8Array, T::Uint8Array) => true,
        (T::Media(wire), T::Media(expected)) => {
            *expected == baml_type::MediaKind::Generic || wire == expected
        }
        // Distinct primitives: a positive mismatch.
        (
            T::Int | T::String | T::Bool | T::Float | T::Null | T::Bigint | T::Uint8Array,
            T::Int | T::String | T::Bool | T::Float | T::Null | T::Bigint | T::Uint8Array,
        ) => false,
        (T::List(w), T::List(e)) => runtime_ty_compatible(w, e),
        (
            T::Map {
                key: wk, value: wv, ..
            },
            T::Map {
                key: ek, value: ev, ..
            },
        ) => runtime_ty_compatible(wk, ek) && runtime_ty_compatible(wv, ev),
        (T::Class(wn, wa), T::Class(en, ea)) => wn == en && class_type_args_compatible(wa, ea),
        // Everything else (unions, enums, aliases, opaque, mixed kinds) is
        // treated leniently as compatible.
        _ => true,
    }
}

pub(crate) fn float_literal_matches(value: f64, source: &str) -> bool {
    source
        .parse::<f64>()
        .is_ok_and(|expected| float_order::eq(value, expected))
}

/// For union types, find which member matches the actual runtime value.
///
/// If the declared type is not a union, returns it unchanged.
/// Whether a callable value carries a type argument for every runtime frame
/// slot of the function it runs (`Function::type_param_names`; a synthetic
/// effect parameter has none). A raw `Object::Function` is never a data value,
/// so it has no frame to judge.
fn callable_frame_is_complete(vm: &BexVm, callable: &Object) -> bool {
    let (function, carried) = match callable {
        Object::GenericFunction(generic) => match vm.generic_function_authored_ptr(generic) {
            Ok(function) => (function, generic.type_args.len()),
            Err(_) => return false,
        },
        Object::Closure(closure) => (closure.function, closure.captured_type_args.len()),
        Object::BoundMethod(method) => (method.function, method.type_args.len()),
        _ => return true,
    };
    // SAFETY: a callable's function pointer is live while the callable is,
    // the invariant `call_callable` relies on when it unwraps one.
    match unsafe { function.get() } {
        Object::Function(function) => carried >= function.type_param_names.len(),
        _ => false,
    }
}

fn resolve_effective_type(
    value: Value,
    declared_type: &bex_vm_types::RealizedTy,
) -> &bex_vm_types::RealizedTy {
    match declared_type {
        bex_vm_types::RealizedTy::Union(members) => find_matching_union_member(value, members)
            .unwrap_or_else(|| members.first().unwrap_or(declared_type)),
        _ => declared_type,
    }
}

/// Select a declared interface arm using nominal facts from the live program.
/// The ordinary VM selector runs first so exact literals, variants, containers,
/// and classes retain precedence over interfaces they may implement.
fn find_implemented_interface_union_member<'a>(
    value: Value,
    declared_type: &'a bex_vm_types::RealizedTy,
    vm: &BexVm,
) -> Result<Option<&'a bex_vm_types::RealizedTy>, EngineError> {
    let bex_vm_types::RealizedTy::Union(members) = declared_type else {
        return Ok(None);
    };
    if members.iter().any(bex_vm_types::RealizedTy::is_null)
        && members.iter().filter(|member| !member.is_null()).count() == 1
    {
        return Ok(None);
    }
    if find_matching_union_member(value, members).is_some() {
        return Ok(None);
    }

    let Some(runtime_ty) = BexEngine::landed_ty(vm, value) else {
        return Ok(None);
    };
    let mut matching: Vec<&'a bex_vm_types::RealizedTy> = Vec::new();
    for member in members.iter().filter(|member| {
        matches!(member, bex_vm_types::RealizedTy::Interface(..))
            && baml_type::normalize::is_subtype(runtime_ty.as_ty(), member.as_ty(), vm)
    }) {
        if matching
            .iter()
            .all(|matched| !runtime_ty_structurally_equal(matched, member))
        {
            matching.push(member);
        }
    }

    match matching.as_slice() {
        [] => Ok(None),
        [selected] => Ok(Some(*selected)),
        _ => Err(EngineError::TypeMismatch {
            message: format!(
                "value of type `{runtime_ty}` matches multiple interface members of union `{declared_type}`"
            ),
        }),
    }
}

/// Find the union member that matches the runtime value's type.
fn find_matching_union_member(
    value: Value,
    members: &[bex_vm_types::RealizedTy],
) -> Option<&bex_vm_types::RealizedTy> {
    let direct = match value.kind() {
        ValueKind::OmittedArg => None,
        ValueKind::Null => members.iter().find(|m| matches!(m, bex_vm_types::RealizedTy::Null)),
        ValueKind::Int(value) => members
            .iter()
            .find(|member| {
                matches!(member, bex_vm_types::RealizedTy::Literal(Literal::Int(expected), _) if value == *expected)
            })
            .or_else(|| {
                members
                    .iter()
                    .find(|member| matches!(member, bex_vm_types::RealizedTy::Int))
            }),
        ValueKind::Bool(value) => members
            .iter()
            .find(|member| {
                matches!(member, bex_vm_types::RealizedTy::Literal(Literal::Bool(expected), _) if value == *expected)
            })
            .or_else(|| {
                members
                    .iter()
                    .find(|member| matches!(member, bex_vm_types::RealizedTy::Bool))
            }),
        ValueKind::Object(ptr) => {
            let obj = unsafe { ptr.get() };
            match obj {
                Object::Float(value) => members
                    .iter()
                    .find(|member| {
                        matches!(member, bex_vm_types::RealizedTy::Literal(Literal::Float(expected), _)
                            if float_literal_matches(*value, expected))
                    })
                    .or_else(|| {
                        members
                            .iter()
                            .find(|member| matches!(member, bex_vm_types::RealizedTy::Float))
                    }),
                Object::String(value) => members
                    .iter()
                    .find(|member| {
                        matches!(member, bex_vm_types::RealizedTy::Literal(Literal::String(expected), _)
                            if value.as_str() == expected)
                    })
                    .or_else(|| {
                        members
                            .iter()
                            .find(|member| matches!(member, bex_vm_types::RealizedTy::String))
                    }),
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
                        let head = bex_vm_types::TypeHead::new(inst.class, class.type_tag);
                        members.iter().find(|m| {
                            matches!(m, bex_vm_types::RealizedTy::Class(tn, expected_args)
                                if *tn == head
                                && (expected_args.is_empty()
                                    || (expected_args.len() == inst.class_type_args.len()
                                        && expected_args
                                            .iter()
                                            .zip(inst.class_type_args.iter())
                                            .all(|(e, a)| {
                                                e == a
                                            }))))
                        })
                    } else {
                        None
                    }
                }
                Object::Variant(variant) => {
                    let enum_obj = unsafe { variant.enm.get() };
                    if let Object::Enum(enm) = enum_obj {
                        let actual_variant = enm.variants.get(variant.index).map(|v| v.name.as_str());
                        let head = bex_vm_types::TypeHead::new(variant.enm, enm.type_tag);
                        members
                            .iter()
                            .find(|m| {
                                matches!(
                                    (m, actual_variant),
                                    (bex_vm_types::RealizedTy::EnumVariant(tn, expected), Some(actual))
                                        if *tn == head && expected.as_str() == actual
                                )
                            })
                            .or_else(|| {
                                members
                                    .iter()
                                    .find(|m| matches!(m, bex_vm_types::RealizedTy::Enum(tn) if *tn == head))
                            })
                    } else {
                        None
                    }
                }
                // Containers carry their realized element/key/value types in
                // the VM heap. Those descriptors remain authoritative even
                // when the container is empty, where payload inspection cannot
                // distinguish `int[]` from `string[]` (or analogous maps).
                Object::Array(array) => {
                    let actual_element = array.element_ty.as_ref();
                    members.iter().find(|member| {
                        matches!(member, bex_vm_types::RealizedTy::List(expected)
                            if runtime_ty_structurally_equal(actual_element, expected))
                    })
                }
                Object::Map(map) => {
                    let actual_key = map.key_ty.as_ref();
                    let actual_value = map.value_ty.as_ref();
                    members.iter().find(|member| {
                        matches!(member, bex_vm_types::RealizedTy::Map { key, value, .. }
                            if runtime_ty_structurally_equal(actual_key, key)
                                && runtime_ty_structurally_equal(actual_value, value))
                    })
                }
                Object::Uint8Array(_) => members
                    .iter()
                    .find(|m| matches!(m, bex_vm_types::RealizedTy::Uint8Array)),
                Object::Bigint(value) => members
                    .iter()
                    .find(|member| {
                        matches!(member, bex_vm_types::RealizedTy::Literal(Literal::Bigint(expected), _)
                            if value.as_ref() == expected)
                    })
                    .or_else(|| {
                        members
                            .iter()
                            .find(|member| matches!(member, bex_vm_types::RealizedTy::Bigint))
                    }),
                // Types that don't participate in union discrimination.
                Object::Function(_)
                | Object::TypeAlias(_)
                | Object::Interface(_)
                | Object::Package(_)
                | Object::ImplRule(_)
                | Object::Closure(_)
                | Object::BoundMethod(_)
                | Object::GenericFunction(_)
                | Object::HostClosure(_)
                | Object::Cell(_)
                | Object::Class(_)
                | Object::Enum(_)
                | Object::Future(_)
                | Object::RustData(_)
                | Object::Type(_) => None,
                Object::Tombstone => Object::tombstone_reached(),
                #[cfg(feature = "heap_debug")]
                Object::Sentinel(_) => None,
            }
        }
    };
    direct.or_else(|| {
        members.iter().find(|member| {
            matches!(member, bex_vm_types::RealizedTy::Union(nested)
                if find_matching_union_member(value, nested).is_some())
        })
    })
}

/// The generated host SDKs expose primitive media through the corresponding
/// handle-backed stdlib wrapper class. Some bridges therefore annotate the
/// class-shaped `_data` payload with that wrapper's exact FQN even when the
/// contextual BAML slot is the primitive `image`/`audio`/`video`/`pdf` type.
///
/// Keep this conversion contextual: the same wrapper remains an ordinary
/// nominal class when passed as the receiver of a `baml.media.Image` method.
pub(crate) fn stdlib_media_wrapper_kind(annotation: &WireTy) -> Option<baml_type::MediaKind> {
    let WireTy::Class(WireHead::Static(name), args) = annotation else {
        return None;
    };
    if !args.is_empty() {
        return None;
    }
    baml_type::MediaKind::from_wrapper_class_name(&name.render_dotted(false))
}

#[cfg(test)]
mod outbound_selection_tests {
    use std::sync::Arc;

    use baml_type::{Freshness, MediaKind, Name, TypeName};
    use bex_heap::{BexHeap, Tlab};
    use bex_vm_types::{
        EnumVariant, MediaContent, MediaValue, Object, Value,
        types::{Class, Enum},
    };

    use super::*;

    fn list(inner: WireTy) -> WireTy {
        WireTy::List(Box::new(inner))
    }

    fn map(value: WireTy) -> WireTy {
        WireTy::Map {
            key: Box::new(WireTy::string()),
            value: Box::new(value),
        }
    }

    fn string_literal(value: &str) -> WireTy {
        WireTy::Literal(Literal::String(value.to_string()), Freshness::Regular)
    }

    fn float_literal(value: &str) -> WireTy {
        WireTy::Literal(Literal::Float(value.to_string()), Freshness::Regular)
    }

    fn media_value(kind: MediaKind) -> BexExternalValue {
        BexExternalValue::Adt(BexExternalAdt::Media(Arc::new(MediaValue::new(
            kind,
            MediaContent::Url {
                url: "https://example.test/asset".to_string(),
                base64_data: None,
            },
            None,
        ))))
    }

    fn media_wrapper_ty(kind: MediaKind) -> WireTy {
        let name = match kind {
            MediaKind::Image => "baml.media.Image",
            MediaKind::Audio => "baml.media.Audio",
            MediaKind::Video => "baml.media.Video",
            MediaKind::Pdf => "baml.media.Pdf",
            MediaKind::Generic => panic!("generic media has no stdlib wrapper class"),
        };
        WireTy::static_class_with_args(TypeName::from_dotted_path(name), Box::new([]))
    }

    #[test]
    fn vm_enum_variant_selection_prefers_exact_variant_then_broad_enum() {
        let mut tlab = Tlab::new(BexHeap::new(Vec::new()));
        // The enum renders as `user.callbacks.Mood` because the package that
        // owns it is labelled `user`.
        let user = tlab.alloc(Object::Package(Box::new(
            bex_vm_types::types::Package::empty(Name::new("user")),
        )));
        let enum_ptr = tlab.alloc(Object::Enum(Box::new(Enum {
            type_tag: baml_type::typetag::TypeTag::from_i64(200),
            path: bex_vm_types::types::LocalName::new(
                vec![Name::new("callbacks")],
                Name::new("Mood"),
            ),
            variants: vec![
                EnumVariant {
                    name: "HAPPY".to_string(),
                    description: None,
                    alias: None,
                    docstring: None,
                    other: indexmap::IndexMap::new(),
                    skip: false,
                },
                EnumVariant {
                    name: "SAD".to_string(),
                    description: None,
                    alias: None,
                    docstring: None,
                    other: indexmap::IndexMap::new(),
                    skip: false,
                },
            ],
            description: None,
            alias: None,
            docstring: None,
            other: indexmap::IndexMap::new(),
            owner: bex_vm_types::types::Owner::Package(user),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
        })));
        let head =
            bex_vm_types::TypeHead::new(enum_ptr, baml_type::typetag::TypeTag::from_i64(200));
        let happy = bex_vm_types::RealizedTy::EnumVariant(head, Name::new("HAPPY"));
        let broad = bex_vm_types::RealizedTy::Enum(head);
        let members = [broad.clone(), happy.clone()];

        let happy_value = Value::object(tlab.alloc_variant(enum_ptr, 0));
        assert_eq!(
            find_matching_union_member(happy_value, &members),
            Some(&happy)
        );

        let sad_value = Value::object(tlab.alloc_variant(enum_ptr, 1));
        assert_eq!(
            find_matching_union_member(sad_value, &members),
            Some(&broad)
        );
    }

    #[test]
    fn vm_dynamic_class_selects_nested_partial_union_while_done_stays_direct() {
        fn alloc_class(
            tlab: &mut Tlab,
            path: bex_vm_types::types::LocalName,
            owner: bex_vm_types::types::Owner,
            type_tag: baml_type::typetag::TypeTag,
        ) -> bex_vm_types::HeapPtr {
            tlab.alloc(Object::Class(Box::new(Class {
                declared_crossing: baml_type::ClassCrossing::Fields,
                path,
                fields: Vec::new(),
                description: None,
                alias: None,
                docstring: None,
                other: indexmap::IndexMap::new(),
                stream_done: false,
                type_tag,
                cleanup: None,
                methods: indexmap::IndexMap::new(),
                generic_param_count: 0,
                owner,
                telemetry_definition: bex_vm_types::DefinitionCell::default(),
            })))
        }

        let mut tlab = Tlab::new(BexHeap::new(Vec::new()));
        // A minted class: no package, so its name is its item name alone.
        let dynamic_name = Name::new("Hs7Collision");
        let dynamic_tag = baml_type::typetag::TypeTag::fresh_dynamic();
        let dynamic_class = alloc_class(
            &mut tlab,
            bex_vm_types::types::LocalName::new(Vec::new(), dynamic_name),
            bex_vm_types::types::Owner::anonymous(),
            dynamic_tag,
        );
        let dynamic_value = Value::object(tlab.alloc_instance(dynamic_class, Vec::new()));

        // A declared class: it renders as `ai.stream.Done` because its package
        // is labelled `ai`.
        let ai = tlab.alloc(Object::Package(Box::new(
            bex_vm_types::types::Package::empty(Name::new("ai")),
        )));
        let done_tag = baml_type::typetag::TypeTag::of_static_index(0);
        let done_class = alloc_class(
            &mut tlab,
            bex_vm_types::types::LocalName::new(vec![Name::new("stream")], Name::new("Done")),
            bex_vm_types::types::Owner::Package(ai),
            done_tag,
        );
        let done_value = Value::object(tlab.alloc_instance(done_class, Vec::new()));

        // Members name their declarations by head, as every declared type does.
        let partial_arm = bex_vm_types::RealizedTy::Union(Box::new([
            bex_vm_types::RealizedTy::Class(
                bex_vm_types::TypeHead::new(dynamic_class, dynamic_tag),
                Box::new([]),
            ),
            bex_vm_types::RealizedTy::Null,
        ]));
        let done_arm = bex_vm_types::RealizedTy::Class(
            bex_vm_types::TypeHead::new(done_class, done_tag),
            Box::new([]),
        );
        let members = [partial_arm.clone(), done_arm.clone()];

        assert_eq!(
            find_matching_union_member(dynamic_value, &members),
            Some(&partial_arm),
        );
        assert_eq!(
            find_matching_union_member(done_value, &members),
            Some(&done_arm),
        );
    }

    #[test]
    fn portable_media_selects_the_nested_prompt_part_wrapper_union() {
        let media_part = WireTy::Union(Box::new([
            media_wrapper_ty(MediaKind::Image),
            media_wrapper_ty(MediaKind::Audio),
            media_wrapper_ty(MediaKind::Video),
            media_wrapper_ty(MediaKind::Pdf),
        ]));
        let prompt_part = WireTy::Union(Box::new([WireTy::string(), media_part.clone()]));

        let wrapped = maybe_wrap_union(media_value(MediaKind::Image), &prompt_part).unwrap();
        let BexExternalValue::Union { value, metadata } = wrapped else {
            panic!("expected prompt-part union metadata")
        };
        assert!(runtime_ty_structurally_equal(
            &metadata.selected_option,
            &media_part
        ));
        assert!(matches!(
            *value,
            BexExternalValue::Adt(BexExternalAdt::Media(ref media))
                if media.kind == MediaKind::Image
        ));
    }

    #[test]
    fn selected_nested_union_inside_optional_retains_inner_envelope() {
        let inner = WireTy::Union(Box::new([
            WireTy::int(),
            WireTy::string(),
            WireTy::Float,
            WireTy::Bool,
        ]));
        let optional = WireTy::Union(Box::new([inner.clone(), WireTy::null()]));

        let wrapped =
            wrap_selected_union_member(BexExternalValue::String("alias".into()), &optional, &inner)
                .unwrap();
        let BexExternalValue::Union { metadata, .. } = wrapped else {
            panic!("expected the inner union envelope")
        };
        assert!(runtime_ty_structurally_equal(&metadata.union_type, &inner));
        assert_eq!(metadata.selected_option, WireTy::string());
    }

    #[test]
    fn empty_array_uses_its_declared_element_type_to_select_union_arm() {
        let int_list = list(WireTy::int());
        let string_list = list(WireTy::string());
        let value = BexExternalValue::Array {
            element_type: WireTy::string(),
            items: Vec::new(),
        };
        let wrapped = maybe_wrap_union(
            value,
            &WireTy::Union(Box::new([int_list, string_list.clone()])),
        )
        .unwrap();
        let BexExternalValue::Union { metadata, .. } = wrapped else {
            panic!("expected union wrapper")
        };
        assert_eq!(metadata.selected_option, string_list);
    }

    #[test]
    fn empty_map_uses_its_declared_value_type_to_select_union_arm() {
        let int_map = map(WireTy::int());
        let string_map = map(WireTy::string());
        let value = BexExternalValue::Map {
            key_type: WireTy::string(),
            value_type: WireTy::string(),
            entries: indexmap::IndexMap::new(),
        };
        let wrapped = maybe_wrap_union(
            value,
            &WireTy::Union(Box::new([int_map, string_map.clone()])),
        )
        .unwrap();
        let BexExternalValue::Union { metadata, .. } = wrapped else {
            panic!("expected union wrapper")
        };
        assert_eq!(metadata.selected_option, string_map);
    }

    #[test]
    fn literal_matching_checks_value_and_prefers_exact_literal() {
        let draft = string_literal("draft");
        let published = string_literal("published");
        let value = BexExternalValue::String("draft".into());
        assert!(value_matches_type(&value, &draft));
        assert!(!value_matches_type(&value, &published));
        assert_eq!(
            find_matching_member(&value, &[WireTy::string(), draft.clone()]).unwrap(),
            draft
        );
    }

    #[test]
    fn enum_variant_matching_checks_value_and_prefers_exact_variant() {
        let mood = TypeName::from_dotted_path("user.callbacks.Mood");
        let head = WireHead::Static(mood.clone());
        let happy = WireTy::EnumVariant(head.clone(), Name::new("HAPPY"));
        let sad = WireTy::EnumVariant(head.clone(), Name::new("SAD"));
        let broad = WireTy::Enum(head);
        let value = BexExternalValue::Variant {
            enum_name: mood.to_string(),
            variant_name: "HAPPY".to_string(),
        };

        assert!(value_matches_type(&value, &happy));
        assert!(!value_matches_type(&value, &sad));
        assert_eq!(
            find_matching_member(&value, &[broad, sad, happy.clone()]).unwrap(),
            happy
        );
    }

    #[test]
    fn float_literal_matching_uses_baml_equality_and_preserves_decimal_precision() {
        let negative_zero = float_literal("-0.0");
        let precise = float_literal("1.2345678901234567e-300");
        assert!(value_matches_type(
            &BexExternalValue::Float(-0.0),
            &negative_zero
        ));
        assert!(value_matches_type(
            &BexExternalValue::Float(0.0),
            &negative_zero
        ));
        assert!(value_matches_type(
            &BexExternalValue::Float(1.234_567_890_123_456_7e-300),
            &precise
        ));
        assert_eq!(
            find_matching_member(
                &BexExternalValue::Float(-0.0),
                &[WireTy::Float, negative_zero.clone()]
            )
            .unwrap(),
            negative_zero
        );
    }

    #[test]
    fn structurally_duplicate_members_select_the_first() {
        let declared = WireTy::Union(Box::new([WireTy::int(), WireTy::int(), WireTy::string()]));

        let WireTy::Union(members) = &declared else {
            unreachable!()
        };
        let outbound_selected = find_matching_member(&BexExternalValue::Int(7), members).unwrap();
        assert!(runtime_ty_structurally_equal(
            &outbound_selected,
            &WireTy::int()
        ));
    }

    #[test]
    fn enum_variant_selected_arm_has_exact_structural_identity() {
        let mood = WireHead::<Handle>::Static(TypeName::from_dotted_path("user.callbacks.Mood"));
        let happy = WireTy::EnumVariant(mood.clone(), Name::new("HAPPY"));
        let another_happy = WireTy::EnumVariant(mood.clone(), Name::new("HAPPY"));
        let sad = WireTy::EnumVariant(mood, Name::new("SAD"));
        assert!(runtime_ty_structurally_equal(&happy, &another_happy));
        assert!(!runtime_ty_structurally_equal(&happy, &sad));
    }

    #[test]
    fn structural_type_identity_preserves_nested_optional_arms() {
        let int_list = list(WireTy::int());
        let optional_int_list = list(WireTy::optional(WireTy::int()));
        assert!(!runtime_ty_structurally_equal(
            &int_list,
            &optional_int_list
        ));
        assert!(!runtime_ty_structurally_equal(
            &map(WireTy::int()),
            &map(WireTy::optional(WireTy::int()))
        ));
    }
}

#[cfg(test)]
mod inference_unifier_tests {
    //! Unit tests for the `RuntimeTy` generic unifier (01a/01b): `union_runtime_ty`
    //! and `infer_bindings_runtime`. These mirror the TIR's `union_ty` /
    //! `infer_bindings_inner` semantics — union-merge, `null`-strip, no arm
    //! routing (00b3 G5).
    use baml_type::{Name, RuntimeTy};

    use super::*;

    fn tv(name: &str) -> RuntimeTy {
        RuntimeTy::TypeVar(baml_type::ParamTy::new(0, Name::new(name)))
    }
    fn int() -> RuntimeTy {
        RuntimeTy::int()
    }
    fn string() -> RuntimeTy {
        RuntimeTy::string()
    }
    fn null() -> RuntimeTy {
        RuntimeTy::null()
    }
    fn rust() -> RuntimeTy {
        RuntimeTy::RustType
    }
    fn never() -> RuntimeTy {
        RuntimeTy::Never
    }
    fn list(inner: RuntimeTy) -> RuntimeTy {
        RuntimeTy::List(Box::new(inner))
    }
    fn map(value: RuntimeTy) -> RuntimeTy {
        RuntimeTy::Map {
            key: Box::new(string()),
            value: Box::new(value),
        }
    }
    fn union(members: Vec<RuntimeTy>) -> RuntimeTy {
        RuntimeTy::Union(members.into())
    }
    fn class(name: &str, args: Vec<RuntimeTy>) -> RuntimeTy {
        RuntimeTy::Class(baml_type::TypeName::local(Name::new(name)), args.into())
    }
    fn infer(formal: &RuntimeTy, actual: &RuntimeTy) -> indexmap::IndexMap<String, RuntimeTy> {
        let mut out = indexmap::IndexMap::new();
        infer_bindings_runtime(formal, actual, &mut out);
        out
    }

    // ── union_runtime_ty ──────────────────────────────────────────────────
    #[test]
    fn union_dedups_to_bare() {
        assert_eq!(union_runtime_ty(&int(), &int()), int());
    }
    #[test]
    fn union_distinct_members() {
        assert_eq!(
            union_runtime_ty(&int(), &string()),
            union(vec![int(), string()])
        );
    }
    #[test]
    fn union_flattens_nested() {
        assert_eq!(
            union_runtime_ty(&int(), &union(vec![string(), RuntimeTy::bool()])),
            union(vec![int(), string(), RuntimeTy::bool()])
        );
    }
    #[test]
    fn union_drops_never() {
        assert_eq!(union_runtime_ty(&never(), &int()), int());
    }
    #[test]
    fn union_host_only_with_known() {
        // 00b3 T25: choose(5, host_obj) ⇒ int | rust_type.
        assert_eq!(
            union_runtime_ty(&int(), &rust()),
            union(vec![int(), rust()])
        );
    }

    // ── infer_bindings_runtime ────────────────────────────────────────────
    #[test]
    fn infer_bare_var() {
        let out = infer(&tv("T"), &int());
        assert_eq!(out.len(), 1);
        assert_eq!(out.get("T"), Some(&int()));
    }
    #[test]
    fn infer_union_merges_repeat_bindings() {
        // choose<T>(left: T, right: T) with (int, string) ⇒ T = int | string.
        let mut out = indexmap::IndexMap::new();
        infer_bindings_runtime(&tv("T"), &int(), &mut out);
        infer_bindings_runtime(&tv("T"), &string(), &mut out);
        assert_eq!(out.get("T"), Some(&union(vec![int(), string()])));
    }
    #[test]
    fn infer_skips_typevar_actual() {
        assert!(infer(&tv("T"), &tv("U")).is_empty());
    }
    #[test]
    fn infer_binds_top_type_actual() {
        // The runtime unifier no longer special-cases the top type `unknown`
        // (the `skip_top` knob was removed): it binds like any concrete actual,
        // matching compile-time inference. In practice the boundary never
        // feeds `unknown` here: a host states `unknown` only for a value it
        // cannot type, and inference then binds it as any other type.
        let unknown = RuntimeTy::unknown();
        assert_eq!(infer(&tv("T"), &unknown).get("T"), Some(&unknown));
    }
    #[test]
    fn infer_through_list() {
        assert_eq!(infer(&list(tv("T")), &list(int())).get("T"), Some(&int()));
    }
    #[test]
    fn infer_through_map_value() {
        assert_eq!(
            infer(&map(tv("T")), &map(RuntimeTy::bool())).get("T"),
            Some(&RuntimeTy::bool())
        );
    }
    #[test]
    fn infer_through_class_args() {
        // second_of<T>(p: GenericPair<int, T>) with GenericPair<int, string> ⇒ T = string.
        let formal = class("GenericPair", vec![int(), tv("T")]);
        let actual = class("GenericPair", vec![int(), string()]);
        assert_eq!(infer(&formal, &actual).get("T"), Some(&string()));
    }
    #[test]
    fn infer_nullable_strips_and_binds() {
        // maybe_id<T>(x: T?) with int ⇒ T = int (00b3 §I, T32).
        assert_eq!(
            infer(&union(vec![tv("T"), null()]), &int()).get("T"),
            Some(&int())
        );
    }
    #[test]
    fn infer_nullable_null_actual_binds_nothing() {
        // maybe_id(None): the `null` member of `T?` absorbs the actual, so
        // nothing pins `T`; the caller's union rule then makes it `never`.
        assert_eq!(infer(&union(vec![tv("T"), null()]), &null()).get("T"), None);
    }
    #[test]
    fn infer_union_with_concrete_sibling_routes_to_typevar() {
        // 02a reverses 00b3 G5/§H: `T | string | null` vs `int` now routes the
        // residual (int, not absorbed by the string/null siblings) to `T`.
        assert_eq!(
            infer(&union(vec![tv("T"), string(), null()]), &int()).get("T"),
            Some(&int())
        );
    }
    #[test]
    fn infer_union_concrete_sibling_absorbs_actual_is_noop() {
        // The string sibling absorbs a string actual ⇒ T stays unbound (Gate A
        // then governs); only the *residual* routes to T.
        assert!(!infer(&union(vec![tv("T"), string(), null()]), &string()).contains_key("T"));
    }
    #[test]
    fn infer_equal_length_union_with_direct_typevars_is_ambiguous() {
        // Regression: a same-length union actual must NOT be positionally zipped
        // into direct TypeVar members. `A | B` vs `int | string` has no
        // principled split (>1 TypeVar member) ⇒ bind nothing (the equal-length
        // zip arm previously bound A=int, B=string by accidental ordering).
        let formal = union(vec![tv("A"), tv("B")]);
        let actual = union(vec![int(), string()]);
        let out = infer(&formal, &actual);
        assert!(!out.contains_key("A"));
        assert!(!out.contains_key("B"));
    }
    #[test]
    fn infer_single_typevar_union_routes_residual_not_positional() {
        // Regression: `T | int` vs `int | string` (equal length) must route the
        // unmatched `string` atom to T, not positionally bind T = int.
        let formal = union(vec![tv("T"), int()]);
        let actual = union(vec![int(), string()]);
        let out = infer(&formal, &actual);
        assert_eq!(out.get("T"), Some(&string()));
    }
}
