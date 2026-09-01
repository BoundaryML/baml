//! Value conversion functions between VM and external types.
//!
//! This module contains all the conversion logic for transforming values
//! between the VM representation (`Value`, `Object`) and the external
//! representation (`BexValue`, `BexExternalValue`).

use ::bex_heap::{BexValue, HeapPermit, PermitProof, TlabHolder};
use ::bex_vm_types::{HeapPtr, Object, ObjectType, Value, ValueKind};
use baml_type::{Literal, Ty};
use bex_external_types::{
    BexExternalAdt, BexExternalValue, HostValueKind, RuntimeTy, UnionMetadata,
    is_canonical_json_alias, runtime_ty_structurally_equal, selected_arm_equal,
    value_satisfies_json,
};
use bex_vm::BexVm;

use crate::{BexEngine, EngineError, thread::BexThread};

/// Narrow a host-supplied [`RuntimeTy`] to the [`baml_type::RealizedTy`] the VM
/// heap stores for a value's type (an array's element type, a map's key/value
/// types, an instance's class type args, a reflected `type` value, a host
/// callable's signature).
///
/// A runtime value always carries a fully realized type, so a host that supplies
/// a non-realized type — an unfilled type variable or an associated-type
/// projection, the only positions [`RuntimeTy`] admits that
/// [`baml_type::RealizedTy`] does not — is an FFI contract violation. It is
/// surfaced loudly as an [`EngineError::TypeMismatch`] rather than erased.
/// A runtime type in the wire's spelling: every head replaced by the name a
/// host can look up.
///
/// The outbound half of the boundary. A head with no nameable declaration is an
/// error rather than a stand-in — the host has nothing it could do with an
/// invented name, and quietly supplying one is how a runtime declaration ends
/// up impersonating a compiled one.
pub(crate) fn to_wire_ty(ty: &bex_vm_types::RuntimeTy) -> Result<RuntimeTy, EngineError> {
    ty.try_map_heads(&mut bex_vm_types::TypeHead::to_name)
        .map_err(|head| EngineError::TypeMismatch {
            message: format!("type leaving the VM names an unnameable declaration: {head}"),
        })
}

/// The per-call sys-op seam's spelling of a declaration name: the declared
/// qualified name, or the bare item name as a local spelling for an anonymous
/// declaration.
///
/// Valid only within one call: the overlay definition maps, per-call handles,
/// and definition graphs a call carries are all built through this same
/// spelling, so keys and references agree there. It is deliberately not
/// [`to_wire_ty`]'s contract — nothing outside the call can resolve the local
/// spelling, and the strict outbound paths still refuse anonymous heads.
pub(crate) fn overlay_type_name(name: &bex_vm_types::DeclarationName) -> baml_type::TypeName {
    name.overlay_name()
}

/// [`overlay_type_name`] over a whole type: every head spelled the way the
/// per-call overlay keys it.
///
/// Total by invariant: a live type's heads are resolved and point at
/// declarations, and declared interface/alias heads already answer through
/// `declared_name` — only class/enum heads can be anonymous.
pub(crate) fn overlay_wire_ty_under_permit(
    ty: &bex_vm_types::RuntimeTy,
    _permit: PermitProof<'_>,
) -> RuntimeTy {
    ty.try_map_heads(&mut bex_vm_types::TypeHead::to_overlay_name)
        .unwrap_or_else(|head| {
            unreachable!("a live type's heads are resolved declaration pointers: {head}")
        })
}

/// The fallible form of [`overlay_wire_ty_under_permit`] for this module's
/// value paths, which already run on the VM thread with the heap permit held
/// (the same contract as every raw object read here). Fails only on an
/// unresolved or non-declaration head — an invariant break surfaced as the
/// same error the strict converter raises.
pub(crate) fn overlay_wire_ty(ty: &bex_vm_types::RuntimeTy) -> Result<RuntimeTy, EngineError> {
    ty.try_map_heads(&mut bex_vm_types::TypeHead::to_overlay_name)
        .map_err(|head| EngineError::TypeMismatch {
            message: format!("type leaving the VM names an unnameable declaration: {head}"),
        })
}

/// The runtime's spelling of a wire type: every name resolved to the head of
/// the declaration this engine holds for it.
///
/// The inbound half. Resolution goes through the program's declaration
/// surfaces, so the head carries both halves off a real declaration — never a
/// name-shaped guess that no lookup would confirm.
pub(crate) fn anchor_wire_ty(
    vm: &crate::BexVm,
    ty: &RuntimeTy,
) -> Result<bex_vm_types::RuntimeTy, EngineError> {
    ty.try_map_heads(&mut |name: &baml_type::TypeName| {
        vm.declaration_head(name)
            .ok_or_else(|| EngineError::TypeMismatch {
                message: format!("host-supplied type names unknown declaration `{name}`"),
            })
    })
}

#[derive(Clone, Copy)]
enum InboundDeclarationKind {
    Any,
    Class,
    Enum,
}

/// The host proxy kind for a live stdlib capability.
///
/// A declaration's display name is never an identity: in particular,
/// `user.ai.FunctionSpec` displays as `ai.FunctionSpec`, and a runtime package
/// may compile that same local spelling again under a fresh head. Trust only
/// the exact stdlib declaration spelling together with the content-addressed
/// tag emitted for that spelling. Runtime-created heads are rejected by the
/// tag check before their name is inspected.
fn trusted_stdlib_capability_kind(
    class: &bex_vm_types::Class,
) -> Option<bex_external_types::TaggedHeapHandleKind> {
    if class.type_tag.is_dynamic() {
        return None;
    }

    let name = class.name.declared()?;
    let (qualified_name, kind) = match (
        name.package().as_str(),
        name.namespace().as_slice(),
        name.name().as_str(),
    ) {
        ("ai", [], "FunctionSpec") => (
            baml_type::qualified_name::AI_FUNCTION_SPEC,
            bex_external_types::TaggedHeapHandleKind::FunctionSpec,
        ),
        ("ai", [namespace], "Stream") if namespace.as_str() == "stream" => (
            baml_type::qualified_name::AI_STREAM_STREAM,
            bex_external_types::TaggedHeapHandleKind::Stream,
        ),
        _ => return None,
    };

    (class.type_tag == baml_type::typetag::TypeTag::of_head(qualified_name)).then_some(kind)
}

#[derive(Clone, Copy)]
pub(crate) struct InboundRuntimeOverlay<'a> {
    dynamic_classes: &'a indexmap::IndexMap<String, bex_external_types::Handle>,
    dynamic_enums: &'a indexmap::IndexMap<String, bex_external_types::Handle>,
    runtime_named_objects: Option<&'a indexmap::IndexMap<String, HeapPtr>>,
}

impl<'a> InboundRuntimeOverlay<'a> {
    pub(crate) fn new(
        dynamic_classes: &'a indexmap::IndexMap<String, bex_external_types::Handle>,
        dynamic_enums: &'a indexmap::IndexMap<String, bex_external_types::Handle>,
        runtime_named_objects: Option<&'a indexmap::IndexMap<String, HeapPtr>>,
    ) -> Self {
        Self {
            dynamic_classes,
            dynamic_enums,
            runtime_named_objects,
        }
    }
}

impl BexEngine {
    /// Resolve one inbound overlay spelling through the same per-call cascade
    /// used for external instances and variants, then through the VM's declared
    /// package surface. Keeping this lookup shared prevents container/type
    /// metadata from drifting from value allocation again.
    fn resolve_inbound_declaration(
        &self,
        vm: &BexVm,
        permit: PermitProof<'_>,
        name: &str,
        overlay: InboundRuntimeOverlay<'_>,
        kind: InboundDeclarationKind,
    ) -> Result<Option<HeapPtr>, EngineError> {
        let dynamic = match kind {
            InboundDeclarationKind::Class => overlay
                .dynamic_classes
                .get(name)
                .map(|handle| ("class", handle)),
            InboundDeclarationKind::Enum => overlay
                .dynamic_enums
                .get(name)
                .map(|handle| ("enum", handle)),
            InboundDeclarationKind::Any => overlay
                .dynamic_classes
                .get(name)
                .map(|handle| ("class", handle))
                .or_else(|| {
                    overlay
                        .dynamic_enums
                        .get(name)
                        .map(|handle| ("enum", handle))
                }),
        };
        if let Some((kind_name, handle)) = dynamic {
            return self
                .resolve_handle(permit, handle)
                .map(Some)
                .ok_or_else(|| EngineError::TypeMismatch {
                    message: format!(
                        "Runtime {kind_name} `{name}` expired before its value landed"
                    ),
                });
        }

        if let Some(ptr) = overlay.runtime_named_objects.and_then(|objects| {
            objects
                .get(name)
                .or_else(|| resolve_named_object(objects, name))
        }) {
            return Ok(Some(*ptr));
        }

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
            return Ok(Some(*ptr));
        }

        Ok(vm
            .declaration_head(&baml_type::TypeName::from_dotted_path(name))
            .map(bex_vm_types::TypeHead::ptr))
    }

    pub(crate) fn anchor_wire_ty_with_runtime(
        &self,
        vm: &BexVm,
        permit: PermitProof<'_>,
        ty: &RuntimeTy,
        overlay: InboundRuntimeOverlay<'_>,
    ) -> Result<bex_vm_types::RuntimeTy, EngineError> {
        ty.try_map_heads(&mut |name: &baml_type::TypeName| {
            let spelling = name.to_string();
            let ptr = self
                .resolve_inbound_declaration(
                    vm,
                    permit,
                    &spelling,
                    overlay,
                    InboundDeclarationKind::Any,
                )?
                .ok_or_else(|| EngineError::TypeMismatch {
                    message: format!("host-supplied type names unknown declaration `{name}`"),
                })?;
            let tag = match vm.get_object(ptr) {
                Object::Class(class) => class.type_tag,
                Object::Enum(enm) => enm.type_tag,
                Object::Interface(interface) => interface.type_tag,
                Object::TypeAlias(alias) => alias.type_tag,
                _ => {
                    return Err(EngineError::TypeMismatch {
                        message: format!(
                            "host-supplied type name `{name}` does not resolve to a declaration"
                        ),
                    });
                }
            };
            Ok(bex_vm_types::TypeHead::new(ptr, tag))
        })
    }

    fn realize_host_ty_with_runtime(
        &self,
        vm: &BexVm,
        permit: PermitProof<'_>,
        ty: &RuntimeTy,
        overlay: InboundRuntimeOverlay<'_>,
    ) -> Result<bex_vm_types::RealizedTy, EngineError> {
        let anchored = self.anchor_wire_ty_with_runtime(vm, permit, ty, overlay)?;
        bex_vm_types::RealizedTy::try_from(anchored).map_err(|e| EngineError::TypeMismatch {
            message: format!("host-supplied type is not realized: {e}"),
        })
    }
}

/// The runtime declarations a type reaches, in the wire's pointer-free form.
///
/// Reached by walking the type's own heads and each declaration's `owner`
/// package — there is no table beside the type that could describe a different
/// set. Names in the result are display data, not identities.
///
/// **Transitional.** This graph exists only because a `BamlTy` head is a bare
/// FQN string: a runtime declaration's synthesized name resolves to nothing in
/// the host's codegen registry, so the definitions have to travel beside the
/// type for the host to make anything of it. Once a head can say *which kind of
/// head it is*, the type carries its own identity and the graph is redundant —
/// note that two of the four consumers already discard it and read only `root`.
///
/// The inbound direction is **not** transitional and does not go through here:
/// `BamlTyArg.type_definition` is a host *request to declare*, and
/// `BexVm::materialize_portable_type_def` is what serves it.
#[deprecated = "transitional: the outbound definition graph only exists because a \
                BamlTy head is a bare FQN. Delete this, TypeDefRef::Live::def, and the \
                outbound TyDefValue arm once BamlTypeHead::{Static{fqn}, \
                Dynamic{identity, name}} lands. Does not apply to the inbound \
                authoring path (BamlTyArg.type_definition), which survives."]
fn portable_type_def(
    heap: &bex_heap::BexHeap,
    type_value: &bex_vm_types::types::TypeValue,
    permit: PermitProof<'_>,
) -> bex_vm_types::types::PortableTypeDef {
    use bex_vm_types::types::{
        PortableClassDef, PortableClassFieldDef, PortableEnumDef, PortableEnumVariantDef,
        PortableMetadata, PortableTypeDef,
    };

    let (mut class_ptrs, mut enum_ptrs) =
        bex_vm::reachable::runtime_nominals_under_permit(heap, &type_value.ty, permit);
    let mut owners = class_ptrs
        .iter()
        .chain(&enum_ptrs)
        .filter_map(|ptr| match unsafe { ptr.get() } {
            Object::Class(class) => Some(class.owner),
            Object::Enum(enm) => Some(enm.owner),
            _ => None,
        })
        .filter(|owner| !owner.is_null())
        .collect::<Vec<_>>();
    owners.dedup();
    for owner in owners {
        let Object::Package(package) = (unsafe { owner.get() }) else {
            continue;
        };
        // A package contributes its whole surface, minus the `$stream`
        // companions, which are synthesized rather than written.
        for ptr in package.classes.values().copied() {
            if !class_ptrs.contains(&ptr)
                && !matches!(unsafe { ptr.get() }, Object::Class(class) if class.name.item_name().as_str().ends_with("$stream"))
            {
                class_ptrs.push(ptr);
            }
        }
        for ptr in package.enums.values().copied() {
            if !enum_ptrs.contains(&ptr) {
                enum_ptrs.push(ptr);
            }
        }
    }
    let metadata =
        |description: &Option<String>,
         alias: &Option<String>,
         docstring: &Option<String>,
         other: &indexmap::IndexMap<String, String>| PortableMetadata {
            description: description.clone(),
            alias: alias.clone(),
            docstring: docstring.clone(),
            other: other.clone(),
        };
    let classes = class_ptrs
        .into_iter()
        .filter_map(|ptr| {
            let Object::Class(class) = (unsafe { ptr.get() }) else {
                return None;
            };
            Some(PortableClassDef {
                // Deprecated graph (see `portable_type_def`): names use
                // the per-call overlay spelling, so the graph's internal
                // links and the sys-op definition maps agree. Hosts never
                // resolved runtime declarations by name, so nothing host-
                // side keys off this.
                name: overlay_type_name(&class.name),
                fields: class
                    .fields
                    .iter()
                    .map(|field| PortableClassFieldDef {
                        name: field.name.clone(),
                        ty: overlay_wire_ty_under_permit(&field.field_type, permit),
                        metadata: metadata(
                            &field.description,
                            &field.alias,
                            &field.docstring,
                            &field.other,
                        ),
                        skip: field.skip,
                    })
                    .collect(),
                metadata: metadata(
                    &class.description,
                    &class.alias,
                    &class.docstring,
                    &class.other,
                ),
                generic_param_count: class.generic_param_count,
            })
        })
        .collect();
    let enums = enum_ptrs
        .into_iter()
        .filter_map(|ptr| {
            let Object::Enum(enm) = (unsafe { ptr.get() }) else {
                return None;
            };
            Some(PortableEnumDef {
                name: overlay_type_name(&enm.name),
                variants: enm
                    .variants
                    .iter()
                    .map(|variant| PortableEnumVariantDef {
                        name: variant.name.clone(),
                        metadata: metadata(
                            &variant.description,
                            &variant.alias,
                            &variant.docstring,
                            &variant.other,
                        ),
                        skip: variant.skip,
                    })
                    .collect(),
                metadata: metadata(&enm.description, &enm.alias, &enm.docstring, &enm.other),
            })
        })
        .collect();
    PortableTypeDef {
        root: overlay_wire_ty_under_permit(
            &bex_vm_types::RuntimeTy::from(type_value.ty.clone()),
            permit,
        ),
        classes,
        enums,
        // Witnesses are carried by the impl rules, not described alongside the
        // type; an outbound payload no longer restates them.
        witnesses: Vec::new(),
    }
}

fn host_call_parameter_types<'a>(
    params: &[bex_vm_types::RealizedFunctionParamTy],
    positional_count: usize,
    optional_names: impl IntoIterator<Item = &'a str>,
) -> Result<(Vec<RuntimeTy>, indexmap::IndexMap<String, RuntimeTy>), String> {
    let required = params
        .iter()
        .filter(|param| param.is_required())
        .map(|param| {
            overlay_wire_ty(&bex_vm_types::RuntimeTy::from(&param.ty)).map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;
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
        optional.insert(
            name.to_string(),
            overlay_wire_ty(&bex_vm_types::RuntimeTy::from(&param.ty))
                .map_err(|e| e.to_string())?,
        );
    }
    Ok((required, optional))
}

#[cfg(test)]
mod host_call_parameter_type_tests {
    use baml_type::{FunctionParamMode, Name, TyAttr};

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
        let union = bex_vm_types::RealizedTy::Union(
            Box::new([
                bex_vm_types::RealizedTy::int(),
                bex_vm_types::RealizedTy::string(),
            ]),
            TyAttr::default(),
        );
        let params = vec![
            param("value", FunctionParamMode::Required, union.clone()),
            param("foo_bar", FunctionParamMode::Optional, union),
        ];
        let (required, optional) = host_call_parameter_types(&params, 1, ["foo_bar"]).unwrap();
        assert!(matches!(required.as_slice(), [RuntimeTy::Union(..)]));
        assert!(matches!(optional["foo_bar"], RuntimeTy::Union(..)));
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
        declared_type: &RuntimeTy,
        vm: &BexVm,
        permit: PermitProof<'_>,
    ) -> Result<BexExternalValue, EngineError> {
        let selected_interface =
            find_implemented_interface_union_member(value, declared_type, vm, permit)?;
        // Select union arms while the value is still a live VM value. Some
        // values intentionally become opaque tagged handles at the host
        // boundary; re-selecting from that host carrier would discard the
        // heap-owned nominal identity that made the arm unambiguous.
        let selected_runtime = selected_interface.or_else(|| match declared_type {
            RuntimeTy::Union(members, _) => find_matching_union_member(value, members),
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

        if let Some(selected) = selected_runtime {
            return wrap_selected_union_member(external, declared_type, selected);
        }
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
        effective_type: &RuntimeTy,
        vm: &BexVm,
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
                    RuntimeTy::List(elem_ty, _) => elem_ty.as_ref(),
                    _ => &RuntimeTy::Null {
                        attr: baml_type::TyAttr::default(),
                    },
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
                    element_type: element_type.clone(),
                    items: items?,
                })
            }

            Object::Map(map) => {
                // Get key and value types from declared type, falling back to
                // Null when the declared type doesn't resolve

                let (key_type, value_type) = match effective_type {
                    RuntimeTy::Map { key, value, .. } => (key.as_ref(), value.as_ref()),
                    _ => (
                        &RuntimeTy::String {
                            attr: baml_type::TyAttr::default(),
                        },
                        &RuntimeTy::Null {
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
                                    *v, value_type, vm, permit,
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

                // Only a *statically compiled* declaration is addressable by
                // a host: codegen emitted a host type for its FQN, so the
                // structural form is well-typed on arrival. Every runtime
                // declaration crosses as an opaque handle instead (BEP-066:
                // dynamic types never leave the heap). The line is the tag,
                // not the name: a runtime-compiled package member is
                // `Declared` and may collide with a static or stdlib name, but
                // its freshly minted dynamic tag cannot impersonate either.
                if class.type_tag.is_dynamic() {
                    let ty = RuntimeTy::Class(
                        class
                            .name
                            .declared()
                            .cloned()
                            .unwrap_or_else(|| overlay_type_name(&class.name)),
                        instance
                            .class_type_args
                            .iter()
                            .map(|arg| overlay_wire_ty(&bex_vm_types::RuntimeTy::from(arg)))
                            .collect::<Result<Box<[_]>, _>>()?,
                        baml_type::TyAttr::default(),
                    );
                    return Ok(BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
                        kind: bex_external_types::TaggedHeapHandleKind::RuntimeValue,
                        ty,
                        heap_handle: self.heap.create_handle(ptr),
                    }));
                }

                // Live stdlib capabilities stay on the heap. The trusted kind
                // selects the host proxy; the wire `ty` is annotation-only.
                // Method generic substitution must recover the instance's
                // TypeHead/class_type_args after resolving this handle.
                let capability_kind = trusted_stdlib_capability_kind(class);
                if let Some(kind) = capability_kind {
                    let handle = self.heap.create_handle(ptr);
                    let ty = RuntimeTy::Class(
                        class.name.declared().cloned().unwrap_or_else(|| {
                            unreachable!("stdlib capability is a compiled declaration")
                        }),
                        instance
                            .class_type_args
                            .iter()
                            .map(|arg| overlay_wire_ty(&bex_vm_types::RuntimeTy::from(arg)))
                            .collect::<Result<Box<[_]>, _>>()?,
                        baml_type::TyAttr::default(),
                    );
                    return Ok(BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
                        kind,
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
                            let field_type = class_field.field_template.substitute_symbolic(
                                &instance
                                    .class_type_args
                                    .iter()
                                    .map(baml_type::RuntimeTy::from)
                                    .collect::<Vec<_>>(),
                            );
                            Ok((
                                class_field.name.clone(),
                                self.convert_vm_value_to_external_with_type(
                                    value,
                                    &overlay_wire_ty(&field_type)?,
                                    vm,
                                    permit,
                                )?,
                            ))
                        })
                        .collect();

                let mut fields = fields?;

                // Rust-backed value wrappers flatten to their canonical ADTs
                // on the wire. Hosts reconstruct fresh wrapper objects from
                // these portable payloads; neither media nor a rendered prompt
                // is a live engine capability.
                if class.name.to_string() == "ai.Prompt" {
                    if let Some(BexExternalValue::Adt(BexExternalAdt::PromptAst(arc))) =
                        fields.shift_remove(bex_external_types::MEDIA_WRAPPER_DATA_FIELD)
                    {
                        return Ok(BexExternalValue::Adt(BexExternalAdt::PromptAst(arc)));
                    }
                }

                if matches!(
                    class.name.to_string().as_str(),
                    "baml.media.Image" | "baml.media.Audio" | "baml.media.Video" | "baml.media.Pdf"
                ) {
                    if let Some(BexExternalValue::Adt(BexExternalAdt::Media(arc))) =
                        fields.shift_remove(bex_external_types::MEDIA_WRAPPER_DATA_FIELD)
                    {
                        return Ok(BexExternalValue::Adt(BexExternalAdt::Media(arc)));
                    }
                }

                Ok(BexExternalValue::Instance {
                    class_name: class.name.to_string(),
                    type_args: instance
                        .class_type_args
                        .iter()
                        .map(|arg| overlay_wire_ty(&bex_vm_types::RuntimeTy::from(arg)))
                        .collect::<Result<Vec<_>, _>>()?,
                    fields,
                })
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
                    let ty = RuntimeTy::Enum(
                        enm.name
                            .declared()
                            .cloned()
                            .unwrap_or_else(|| overlay_type_name(&enm.name)),
                        baml_type::TyAttr::default(),
                    );
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

            Object::Function(_)
            | Object::Closure(_)
            | Object::BoundMethod(_)
            | Object::GenericFunction(_) => {
                let handle = self.heap.create_handle(ptr);
                Ok(BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
                    kind: bex_external_types::TaggedHeapHandleKind::Callable,
                    ty: effective_type.clone(),
                    heap_handle: handle,
                }))
            }
            Object::Interface(_) => Err(EngineError::CannotConvert {
                type_name: "interface".to_string(),
            }),
            Object::Package(_) => Ok(BexExternalValue::Handle(self.heap.create_handle(ptr))),
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
            Object::UnscheduledFuture(_) => Err(EngineError::CannotConvert {
                type_name: "unscheduled_future".to_string(),
            }),
            Object::Bigint(bi) => Ok(BexExternalValue::Bigint((**bi).clone())),
            Object::Collector(c) => Ok(BexExternalValue::Adt(BexExternalAdt::Collector(c.clone()))),
            // Identity never crosses as *data* (BEP-066 H-4): no mint, digest
            // or pointer is serialized. It may cross as a rooted reference —
            // the handle resolves back to this same `Object::Type` in this
            // engine and is dropped by any wire encoder — so a value echoed
            // through a sys-op comes back as itself rather than a copy.
            #[expect(
                deprecated,
                reason = "the outbound definition graph is the only wire form a dynamic \
                          head has while a BamlTy head is a bare FQN; the deprecation \
                          marks the debt and fires again when BamlTypeHead lands"
            )]
            Object::Type(type_value) => Ok(BexExternalValue::Adt(BexExternalAdt::TypeDef(
                bex_external_types::TypeDefRef::Live {
                    handle: self.heap.create_handle(ptr),
                    def: portable_type_def(&self.heap, type_value, permit),
                },
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
    /// Synthesize the concrete `RuntimeTy` an inbound argument *value* inhabits
    /// (01a step 1), for driving call-site generic inference. Pure bottom-up:
    /// with union-arm routing out of scope (00b3 G5), no `expected`-guided
    /// disambiguation is needed. A generic instance's type is read from its
    /// wire-supplied `type_args` (the value-level source of truth); class/enum
    /// `TypeName`s are resolved against the engine registry so the binding
    /// survives Gate B's name match. Opaque host values — and anything else with
    /// no readable BAML type, including empty containers — are `HostOnly`
    /// (Case 2, → `rust_type`).
    pub(crate) fn synth_ty_from_value(&self, value: &BexExternalValue) -> SynthTy {
        let attr = baml_type::TyAttr::default;
        match value {
            BexExternalValue::Int(_) => SynthTy::Known(RuntimeTy::int()),
            BexExternalValue::Bigint(_) => SynthTy::Known(RuntimeTy::Bigint { attr: attr() }),
            BexExternalValue::Float(_) => SynthTy::Known(RuntimeTy::float()),
            BexExternalValue::Bool(_) => SynthTy::Known(RuntimeTy::bool()),
            // A `String` value widens to `string`, never a `Literal` (00b3
            // T2/T45 — `identity("hi")` binds `T = string`).
            BexExternalValue::String(_) => SynthTy::Known(RuntimeTy::string()),
            BexExternalValue::Uint8Array(_) => {
                SynthTy::Known(RuntimeTy::Uint8Array { attr: attr() })
            }
            // A bare `null` carries NO inference evidence (03b §I/§H, rule 4):
            // a `null`-only actual gives the value position no concrete leaf, so
            // we do NOT bind `T = null` (and do NOT null-strip a `T?` formal to
            // bind `T = null`). It rides host-only ⇒ the var defaults to
            // `rust_type` and the value round-trips unchanged (`identity(None)
            // == None`). This is a *runtime* leaf decision; the shared
            // compile-time unifier still binds `null` faithfully.
            BexExternalValue::Null => SynthTy::HostOnly,
            BexExternalValue::Array { items, .. } => {
                // An array always inhabits a list. When its elements carry no
                // evidence (an empty `[]`, or elements that are themselves
                // evidence-free), the element type is the host-only `rust_type`
                // rather than no-evidence — `identity<T>([])` binds
                // `T = rust_type[]`, never leaving `T` for Gate A.
                let elem = synth_collection_element(self, items.iter())
                    .unwrap_or_else(|| RuntimeTy::RustType { attr: attr() });
                SynthTy::Known(RuntimeTy::List(Box::new(elem), attr()))
            }
            BexExternalValue::Map { entries, .. } => {
                // A map always inhabits `map<string, _>`: every wire entry is
                // string-keyed, so the key is always `string`. When the values
                // carry no evidence (a genuinely empty `{}`, or values that are
                // themselves evidence-free), the value type is the host-only
                // `rust_type` — an empty map binds `Map<string, rust_type>`,
                // never leaving the value TypeVar for Gate A.
                let value_ty = synth_collection_element(self, entries.values())
                    .unwrap_or_else(|| RuntimeTy::RustType { attr: attr() });
                SynthTy::Known(RuntimeTy::Map {
                    key: Box::new(RuntimeTy::string()),
                    value: Box::new(value_ty),
                    attr: attr(),
                })
            }
            // A fully-bound generic instance carries its concrete args on the
            // wire (`GenericBox[int]` → `[int]`). An *unbound* generic instance —
            // a generic class whose wire type-args are EMPTY (Pydantic lets you
            // build `GenericBox(value=5)` without `[int]`) — carries no readable
            // BAML type, so it is host-only (03b G2/G3): the var binds `rust_type`
            // and the instance rides opaquely through the `OpaqueExternalValue`
            // carrier, staying distinct from a properly-bound instance (G4). A
            // *non*-generic class legitimately has empty args and types normally.
            // (Where a forcing formal `GenericPair<int,T>` should recover `T` from
            // an unbound instance's FIELDS instead — 03b G1 — that is handled
            // formal-aware in `synth_inference_actual`, not here.) An unresolvable
            // class name is host-only too.
            BexExternalValue::Instance {
                class_name,
                type_args,
                ..
            } => match self.resolve_class_type_name(class_name) {
                Some(tn) => {
                    if type_args.is_empty() && self.class_generic_arity(class_name) > 0 {
                        SynthTy::HostOnly
                    } else {
                        SynthTy::Known(RuntimeTy::Class(tn, type_args.clone().into(), attr()))
                    }
                }
                None => SynthTy::HostOnly,
            },
            BexExternalValue::Variant { enum_name, .. } => {
                // A resolvable enum binds its concrete type; an unresolved enum
                // name (no such enum registered) is an opaque carrier with no
                // BAML type ⇒ host-only (`rust_type`), mirroring the unresolved
                // `Instance` arm above.
                match self.resolve_enum_type_name(enum_name) {
                    Some(tn) => SynthTy::Known(RuntimeTy::Enum(tn, attr())),
                    None => SynthTy::HostOnly,
                }
            }
            // A reflected type passed as a value inhabits the `type` metatype.
            BexExternalValue::Adt(BexExternalAdt::Type(_) | BexExternalAdt::TypeDef(_)) => {
                SynthTy::Known(RuntimeTy::Type { attr: attr() })
            }
            // A tagged handle's wire type is annotation-only. Call paths that
            // hold a heap permit resolve the rooted object and supply its live
            // TypeHead; inference without that proof must treat it as opaque.
            BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle { .. }) => SynthTy::HostOnly,
            // Media is a concrete leaf BAML type (`image`/`audio`/...): its kind
            // is readable from the value, so `identity<T>(img)` binds T to the
            // real media type rather than the host-only `rust_type` catch-all.
            BexExternalValue::Adt(BexExternalAdt::Media(media)) => {
                SynthTy::Known(RuntimeTy::Media(media.kind, attr()))
            }
            // A collector inhabits the concrete `Resource` leaf type, and a
            // rendered prompt inhabits `ai.Prompt` — bind T to those rather than
            // falling into the host-only catch-all below.
            BexExternalValue::Adt(BexExternalAdt::Collector(_)) => {
                SynthTy::Known(RuntimeTy::resource())
            }
            BexExternalValue::Adt(BexExternalAdt::PromptAst(_)) => {
                SynthTy::Known(RuntimeTy::prompt_ast())
            }
            // A typed inbound value carries stronger evidence than payload
            // shape alone (for example a literal rather than `string`, or the
            // element type of an empty list). Actual union wrappers retain the
            // existing payload-based inference behavior.
            BexExternalValue::Union { value, metadata } => {
                if metadata.is_inbound_type_annotation {
                    SynthTy::Known(metadata.selected_option.clone())
                } else {
                    self.synth_ty_from_value(value)
                }
            }
            // Opaque host values (Case 2) and other non-BAML-typed carriers.
            // (Every `Adt` variant is handled explicitly above, so there is no
            // `Adt(_)` fallthrough — a new `BexExternalAdt` variant will surface
            // here as a non-exhaustive-match error, forcing a synth decision.)
            BexExternalValue::Handle(_)
            | BexExternalValue::RustData(_)
            | BexExternalValue::HostValue(_)
            | BexExternalValue::FunctionRef { .. } => SynthTy::HostOnly,
        }
    }

    /// Resolve a class FQN (as it arrives on an inbound `Instance`) to the
    /// engine-registered `TypeName`, so a synthesized `Class` type matches the
    /// declared slot's name in Gate B. `None` if no such class is registered.
    fn resolve_class_type_name(&self, class_name: &str) -> Option<baml_type::TypeName> {
        let ptr = self
            .resolved_class_names
            .get(class_name)
            .or_else(|| resolve_named_object(&self.resolved_class_names, class_name))?;
        // SAFETY: registered class names point to compile-time Class objects.
        match unsafe { ptr.get() } {
            Object::Class(class) => class.name.declared().cloned(),
            _ => None,
        }
    }

    /// Resolve an enum FQN to its engine-registered `TypeName`. See
    /// [`Self::resolve_class_type_name`].
    fn resolve_enum_type_name(&self, enum_name: &str) -> Option<baml_type::TypeName> {
        let ptr = self
            .resolved_enum_names
            .get(enum_name)
            .or_else(|| resolve_named_object(&self.resolved_enum_names, enum_name))?;
        // SAFETY: registered enum names point to compile-time Enum objects.
        match unsafe { ptr.get() } {
            Object::Enum(enum_obj) => enum_obj.name.declared().cloned(),
            _ => None,
        }
    }

    /// Run `f` against the resolved compile-time [`Object::Class`] for `class_name`.
    fn with_resolved_class<R>(
        &self,
        class_name: &str,
        f: impl FnOnce(&bex_vm_types::Class) -> R,
    ) -> Option<R> {
        let ptr = self
            .resolved_class_names
            .get(class_name)
            .or_else(|| resolve_named_object(&self.resolved_class_names, class_name))?;
        // SAFETY: registered class names point to compile-time Class objects.
        match unsafe { ptr.get() } {
            Object::Class(class) => Some(f(class)),
            _ => None,
        }
    }

    /// The number of class-level generic parameters `class_name` declares, read
    /// from the highest `TypeArgRef(N)` index across its field templates (`N+1`).
    /// `0` for a non-generic class — used to tell an *unbound generic* instance
    /// (empty wire args on a generic class ⇒ host-only) from a plain non-generic
    /// instance (empty args is correct). See the `Instance` synth arm.
    fn class_generic_arity(&self, class_name: &str) -> usize {
        self.with_resolved_class(class_name, |class| {
            class
                .fields
                .iter()
                .filter_map(|field| template_max_type_arg_ref(&field.field_template))
                .max()
                .map_or(0, |max| max as usize + 1)
        })
        .unwrap_or(0)
    }

    /// Reconstruct the type-args of an *unbound* generic instance from its field
    /// VALUES, by synthesizing the value of each field whose template is a direct
    /// `TypeArgRef(N)` into slot `N`. Slots with no directly-typed field stay
    /// `Unknown` (the unifier skips them). Returns `None` for a
    /// non-generic / unresolvable class. This is the call-time recovery a forcing
    /// formal needs to bind a var from an unbound instance (03b G1); it does NOT
    /// run for bound instances (they read the wire args) or bare-`T` formals
    /// (those keep the host-only `rust_type` synth).
    ///
    /// `formal_args` are the per-slot types from the formal generic-class type
    /// (`GenericPair<GenericPair<A,B>, …>` ⇒ slot 0 formal is `GenericPair<A,B>`).
    /// Each field is synthed against its slot's formal via the formal-aware
    /// [`Self::synth_inference_actual`], so a slot whose value is *itself* an
    /// unbound nested instance is recursively reconstructed (deep G1) rather than
    /// flattened to host-only `rust_type`.
    fn reconstruct_unbound_instance_args(
        &self,
        class_name: &str,
        fields: &indexmap::IndexMap<String, BexExternalValue>,
        formal_args: &[RuntimeTy],
    ) -> Option<Vec<RuntimeTy>> {
        let arity = self.class_generic_arity(class_name);
        if arity == 0 {
            return None;
        }
        let unknown = || RuntimeTy::Unknown {
            attr: baml_type::TyAttr::default(),
        };
        let mut args: Vec<RuntimeTy> = (0..arity).map(|_| unknown()).collect();
        self.with_resolved_class(class_name, |class| {
            for field in &class.fields {
                let slot = match &field.field_template {
                    baml_type::TyTemplate::TypeArgRef(n) => *n as usize,
                    _ => continue,
                };
                if let Some(value) = fields.get(&field.name) {
                    if let Some(arg) = args.get_mut(slot) {
                        // Recurse formal-first: a nested unbound instance under a
                        // generic-class formal slot is reconstructed in turn; any
                        // other slot formal (a bare `TypeVar`, a leaf) falls through
                        // to the value-only synth inside `synth_inference_actual`.
                        *arg = match formal_args.get(slot) {
                            Some(slot_formal) => self.synth_inference_actual(slot_formal, value),
                            None => self.synth_ty_from_value(value).into_runtime_ty(),
                        };
                    }
                }
            }
        })?;
        Some(args)
    }

    /// Synthesize the *actual* `RuntimeTy` for one inference pair, given the
    /// declared `formal`. Almost always this is just [`Self::synth_ty_from_value`].
    /// The one formal-aware exception: an **unbound** generic `Instance` (empty
    /// wire args) met by a generic-**class** formal. The wire carries no
    /// type-args, but the formal directs inference into specific slots, so we
    /// reconstruct the instance's args from its field values (03b G1:
    /// `second_of<T>(p: GenericPair<int,T>)` recovers `T=string`). A bare-`T`
    /// formal is not a `Class`, so it falls through to the host-only `rust_type`
    /// synth (G2/G3), keeping the unbound instance opaque on round-trip.
    pub(crate) fn synth_inference_actual(
        &self,
        formal: &RuntimeTy,
        value: &BexExternalValue,
    ) -> RuntimeTy {
        // A sparse class annotation is represented by the external union
        // carrier even when the declared type is not a union. For formal-aware
        // generic reconstruction, inspect the class payload beneath that
        // transient carrier; otherwise an erased `GenericPair(...)` would bind
        // every type variable to `$rust_type` instead of recovering evidence
        // recursively from its fields.
        let structural_value = match value {
            BexExternalValue::Union { value, metadata } if metadata.is_inbound_type_annotation => {
                value.as_ref()
            }
            _ => value,
        };
        if let (
            RuntimeTy::Class(formal_name, formal_args, _),
            BexExternalValue::Instance {
                class_name,
                type_args,
                fields,
            },
        ) = (formal, structural_value)
        {
            if type_args.is_empty() {
                let contextual_name;
                let class_name = if class_name.is_empty() {
                    contextual_name = formal_name.render_dotted(false);
                    contextual_name.as_str()
                } else {
                    class_name.as_str()
                };
                if let (Some(tn), Some(args)) = (
                    self.resolve_class_type_name(class_name),
                    self.reconstruct_unbound_instance_args(class_name, fields, formal_args),
                ) {
                    return RuntimeTy::Class(tn, args.into(), baml_type::TyAttr::default());
                }
            }
        }
        self.synth_ty_from_value(value).into_runtime_ty()
    }

    /// Convert a `BexExternalValue` result from sys ops back to a VM Value.
    ///
    /// Returns `EngineError::TypeMismatch` for malformed external values
    /// (unknown class/enum names, missing required fields, …) so that bad
    /// external input — from `--json-args`, language bindings, or buggy
    /// sys ops — surfaces as a graceful error instead of crashing the
    /// process.
    pub(crate) fn convert_external_to_vm_value(
        &self,
        holder: &mut impl HeapPermit<BexThread>,
        external: BexExternalValue,
    ) -> Result<Value, EngineError> {
        // Default: no declared-type context. Inbound `HostValue` arguments
        // need the declared `RuntimeTy::Function` to materialize an
        // `Object::HostClosure` — callers that thread the type in should
        // use `convert_external_to_vm_value_with_ty`.
        self.convert_external_to_vm_value_with_ty_and_runtime(
            holder,
            external,
            None,
            &indexmap::IndexMap::new(),
            &indexmap::IndexMap::new(),
            None,
        )
    }

    pub(crate) fn convert_external_to_vm_value_with_runtime_schema(
        &self,
        holder: &mut impl HeapPermit<BexThread>,
        external: BexExternalValue,
        overlay: &crate::RuntimeSchemaOverlay,
        dynamic_classes: &indexmap::IndexMap<String, bex_external_types::Handle>,
        dynamic_enums: &indexmap::IndexMap<String, bex_external_types::Handle>,
    ) -> Result<Value, EngineError> {
        let mut named = indexmap::IndexMap::new();
        for (name, handle) in &overlay.named_owners {
            let Some(owner) = self.resolve_handle(holder.proof(), handle) else {
                continue;
            };
            let Object::Package(package) = (unsafe { owner.get() }) else {
                continue;
            };
            let ptr = package
                .classes
                .values()
                .chain(package.enums.values())
                .copied()
                .find(|ptr| match unsafe { ptr.get() } {
                    Object::Class(class) => class.name.to_string() == *name,
                    Object::Enum(enm) => enm.name.to_string() == *name,
                    _ => false,
                });
            if let Some(ptr) = ptr {
                named.insert(name.clone(), ptr);
            }
        }
        self.convert_external_to_vm_value_with_ty_and_runtime(
            holder,
            external,
            None,
            dynamic_classes,
            dynamic_enums,
            Some(&named),
        )
    }

    /// Like [`Self::convert_external_to_vm_value`], but threads the effective
    /// contextual `RuntimeTy` alongside the value tree. Sparse inbound
    /// `value_type` annotations are validated during coercion and become the
    /// context for their payload subtree; unannotated children inherit their
    /// list, map, or class-field type from the parent. This also lets nested
    /// `BexExternalValue::HostValue` values bind to an [`Object::HostClosure`].
    pub(crate) fn convert_external_to_vm_value_with_ty(
        &self,
        holder: &mut impl HeapPermit<BexThread>,
        external: BexExternalValue,
        expected_ty: Option<&RuntimeTy>,
    ) -> Result<Value, EngineError> {
        self.convert_external_to_vm_value_with_ty_and_runtime(
            holder,
            external,
            expected_ty,
            &indexmap::IndexMap::new(),
            &indexmap::IndexMap::new(),
            None,
        )
    }

    /// Materialize an external value using call-local runtime class and enum
    /// side tables in addition to the engine's frozen static definition table.
    /// Recursive calls retain both tables for nested containers and classes.
    pub(crate) fn convert_external_to_vm_value_with_dynamic_types(
        &self,
        holder: &mut impl HeapPermit<BexThread>,
        external: BexExternalValue,
        expected_ty: Option<&RuntimeTy>,
        dynamic_classes: &indexmap::IndexMap<String, bex_external_types::Handle>,
        dynamic_enums: &indexmap::IndexMap<String, bex_external_types::Handle>,
    ) -> Result<Value, EngineError> {
        self.convert_external_to_vm_value_with_ty_and_runtime(
            holder,
            external,
            expected_ty,
            dynamic_classes,
            dynamic_enums,
            None,
        )
    }

    fn convert_external_to_vm_value_with_ty_and_runtime(
        &self,
        holder: &mut impl HeapPermit<BexThread>,
        mut external: BexExternalValue,
        expected_ty: Option<&RuntimeTy>,
        dynamic_classes: &indexmap::IndexMap<String, bex_external_types::Handle>,
        dynamic_enums: &indexmap::IndexMap<String, bex_external_types::Handle>,
        runtime_named_objects: Option<&indexmap::IndexMap<String, HeapPtr>>,
    ) -> Result<Value, EngineError> {
        // A `baml.json.json` slot materializes containers with the alias as
        // their element/value type, exactly like BAML-born `baml.json.parse`
        // values (`serde_to_value`), so runtime type tests (`match (j) { let
        // m: map<string, json> => ... }`) treat host and BAML json alike.
        // `coerce_inbound_arg` already re-annotates argument trees; this hook
        // covers the paths that convert without a coercion pass, notably
        // host-callable return values.
        if let Some(declared @ RuntimeTy::TypeAlias(name, _)) = expected_ty
            && is_canonical_json_alias(name)
            && matches!(
                external,
                BexExternalValue::Array { .. } | BexExternalValue::Map { .. }
            )
            && value_satisfies_json(&external)
        {
            external = annotate_json_container_types(external, declared);
        }
        // Structural host-only stash (03b §F/§G): when the declared slot resolves
        // to `RustType` (the generic var bound to `rust_type`) but the value is a
        // structural `BexExternalValue` (e.g. an unbound generic instance), ride
        // it through the VM verbatim as an opaque `Object::RustData` instead of
        // materializing an introspectable object. `try_convert_rust_data` re-emits
        // it unchanged on the way out, preserving its identity (an unbound
        // `GenericBox(value=5)` stays != a bound `GenericBox[int]`, G4). `HostValue`
        // keeps its dedicated arm below (it carries a release-keyed handle).
        if expected_ty.and_then(peel_to_rust_type).is_some() && is_structural_host_only(&external) {
            let arc: std::sync::Arc<dyn std::any::Any + Send + Sync> =
                std::sync::Arc::new(bex_external_types::OpaqueExternalValue(external));
            return Ok(Value::object(
                holder.holder_mut().tlab_mut().alloc_rust_data(arc),
            ));
        }
        let overlay =
            InboundRuntimeOverlay::new(dynamic_classes, dynamic_enums, runtime_named_objects);
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
            BexExternalValue::Array {
                element_type,
                items,
            } => {
                let declared_element_ty = expected_ty.and_then(peel_list_element_ty);
                let runtime_element_ty = declared_element_ty.unwrap_or(&element_type).clone();
                let values = items
                    .into_iter()
                    .map(|v| {
                        let v = match declared_element_ty {
                            Some(ty) => self.coerce_inbound_arg(v, ty)?,
                            None => v,
                        };
                        self.convert_external_to_vm_value_with_ty_and_runtime(
                            holder,
                            v,
                            declared_element_ty,
                            dynamic_classes,
                            dynamic_enums,
                            runtime_named_objects,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let element_ty = self.realize_host_ty_with_runtime(
                    &holder.holder().vm,
                    holder.proof(),
                    &runtime_element_ty,
                    overlay,
                )?;
                Value::object(
                    holder
                        .holder_mut()
                        .tlab_mut()
                        .alloc_array(element_ty, values),
                )
            }
            BexExternalValue::Map {
                key_type,
                value_type,
                entries,
            } => {
                let (key_type, value_type) = match expected_ty {
                    Some(RuntimeTy::Map { key, value, .. }) => {
                        (key.as_ref().clone(), value.as_ref().clone())
                    }
                    _ => (key_type, value_type),
                };
                let key_ty = self.realize_host_ty_with_runtime(
                    &holder.holder().vm,
                    holder.proof(),
                    &key_type,
                    overlay,
                )?;
                let declared_value_ty = expected_ty.and_then(peel_map_value_ty);
                let runtime_value_ty = declared_value_ty.unwrap_or(&value_type).clone();
                let values = entries
                    .into_iter()
                    .map(|(k, v)| {
                        let v = match declared_value_ty {
                            Some(ty) => self.coerce_inbound_arg(v, ty)?,
                            None => v,
                        };
                        self.convert_external_to_vm_value_with_ty_and_runtime(
                            holder,
                            v,
                            declared_value_ty,
                            dynamic_classes,
                            dynamic_enums,
                            runtime_named_objects,
                        )
                        .map(|v| (bex_vm_types::BexStr::from(k.as_str()), v))
                    })
                    .collect::<Result<indexmap::IndexMap<bex_vm_types::BexStr, Value>, _>>()?;
                let value_ty = self.realize_host_ty_with_runtime(
                    &holder.holder().vm,
                    holder.proof(),
                    &runtime_value_ty,
                    overlay,
                )?;
                Value::object(
                    holder
                        .holder_mut()
                        .tlab_mut()
                        .alloc_map(key_ty, value_ty, values),
                )
            }
            BexExternalValue::Uint8Array(bytes) => {
                Value::object(holder.holder_mut().tlab_mut().alloc_uint8array(bytes))
            }
            BexExternalValue::RustData(data) => {
                Value::object(holder.holder_mut().tlab_mut().alloc_rust_data(data))
            }
            // Allocate instance by looking up class and converting fields.
            // The wire-supplied class `type_args` are landed into the VM
            // `Object::Instance::class_type_args` via `alloc_instance_with_type_args`.
            BexExternalValue::Instance {
                mut class_name,
                mut type_args,
                fields,
            } => {
                if let Some(RuntimeTy::Class(expected_name, expected_args, _)) = expected_ty {
                    class_name = expected_name.to_string();
                    if type_args.is_empty() {
                        type_args = expected_args.to_vec();
                    }
                }
                let class_ptr = self
                    .resolve_inbound_declaration(
                        &holder.holder().vm,
                        holder.proof(),
                        &class_name,
                        overlay,
                        InboundDeclarationKind::Class,
                    )?
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

                // Anchor the host's type arguments once, here: everything below
                // works against the class's own head-typed templates.
                let type_args = type_args
                    .iter()
                    .map(|ty| {
                        self.anchor_wire_ty_with_runtime(
                            &holder.holder().vm,
                            holder.proof(),
                            ty,
                            overlay,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                // Build field values in the order defined by the class.
                // Each field's declared `RuntimeTy` is passed as the conversion
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
                    let field_ty = class_field.field_template.substitute_symbolic(&type_args);
                    let wire_field_ty = overlay_wire_ty(&field_ty)?;
                    let field_value = self.coerce_inbound_arg(ext.clone(), &wire_field_ty)?;
                    values.push(self.convert_external_to_vm_value_with_ty_and_runtime(
                        holder,
                        field_value,
                        Some(&wire_field_ty),
                        dynamic_classes,
                        dynamic_enums,
                        runtime_named_objects,
                    )?);
                }
                let realized_type_args = type_args
                    .into_iter()
                    .map(|ty| {
                        bex_vm_types::RealizedTy::try_from(ty).map_err(|e| {
                            EngineError::TypeMismatch {
                                message: format!("host-supplied type is not realized: {e}"),
                            }
                        })
                    })
                    .collect::<Result<Box<[_]>, _>>()?;
                Value::object(
                    holder
                        .holder_mut()
                        .tlab_mut()
                        .alloc_instance_with_type_args(class_ptr, realized_type_args, values),
                )
            }
            BexExternalValue::Variant {
                enum_name,
                variant_name,
            } => {
                let enum_ptr = self
                    .resolve_inbound_declaration(
                        &holder.holder().vm,
                        holder.proof(),
                        &enum_name,
                        overlay,
                        InboundDeclarationKind::Enum,
                    )?
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
                        .alloc_variant(enum_ptr, index),
                )
            }
            BexExternalValue::Union { value, metadata } => {
                // A typed host value carries an authoritative exact type.
                // First apply that type to shape-only class/container payloads,
                // then materialize against it. This path also handles host
                // throws, which have no declared parameter context.
                let selected_type = metadata.selected_option;
                let value = self.coerce_inbound_arg(*value, &selected_type)?;
                return self.convert_external_to_vm_value_with_ty_and_runtime(
                    holder,
                    value,
                    Some(&selected_type),
                    dynamic_classes,
                    dynamic_enums,
                    runtime_named_objects,
                );
            }
            BexExternalValue::Adt(BexExternalAdt::Collector(c)) => {
                Value::object(holder.holder_mut().tlab_mut().alloc_collector(c))
            }
            BexExternalValue::Adt(BexExternalAdt::Type(ty)) => {
                // A lane type lands here. An anonymous declaration has no
                // spelling to anchor against and is refused: it should have
                // crossed as a rooted handle, which lands as the declaration
                // itself rather than through a name.
                let named: RuntimeTy = ty.try_map_heads(&mut |head: &::sys_types::DefKey| {
                    head.declared()
                        .cloned()
                        .ok_or_else(|| EngineError::TypeMismatch {
                            message: format!(
                                "host-supplied type names the anonymous declaration `{}`, \
                                 which has no resolvable spelling; pass it as a handle instead",
                                head.display_name()
                            ),
                        })
                })?;
                let ty = self.realize_host_ty_with_runtime(
                    &holder.holder().vm,
                    holder.proof(),
                    &named,
                    overlay,
                )?;
                // The wire carries the definition only (H-4); derive a fresh
                // static identity with the receiving VM's complete fact context.
                Value::object(
                    holder
                        .holder_mut()
                        .vm
                        .alloc_type(bex_vm_types::types::TypeValue::new(ty)),
                )
            }
            BexExternalValue::Adt(BexExternalAdt::TypeDef(definition)) => {
                // A live reference from *this* engine lands as the original
                // object. `resolve_handle` rejects a foreign engine's handle,
                // so a cross-engine value falls through to materialization and
                // gets fresh identity, exactly as a wire payload does.
                let live = match &definition {
                    bex_external_types::TypeDefRef::Live { handle, .. } => {
                        self.resolve_handle(holder.proof(), handle)
                    }
                    bex_external_types::TypeDefRef::Portable(_) => None,
                };
                match live {
                    Some(ptr) => Value::object(ptr),
                    None => {
                        let vm = &mut holder.holder_mut().vm;
                        let type_value =
                            vm.materialize_portable_type_def(&definition.into_def())
                                .map_err(|message| EngineError::TypeMismatch { message })?;
                        Value::object(vm.alloc_type(type_value))
                    }
                }
            }
            BexExternalValue::Adt(BexExternalAdt::PromptAst(arc)) => {
                if matches!(expected_ty, Some(RuntimeTy::RustType { .. })) {
                    Value::object(holder.holder_mut().tlab_mut().alloc_rust_data(arc))
                } else {
                    let mut fields = indexmap::IndexMap::new();
                    fields.insert(
                        bex_external_types::MEDIA_WRAPPER_DATA_FIELD.to_string(),
                        BexExternalValue::Adt(BexExternalAdt::PromptAst(arc)),
                    );
                    return self.convert_external_to_vm_value_with_ty_and_runtime(
                        holder,
                        BexExternalValue::Instance {
                            class_name: "ai.Prompt".to_string(),
                            type_args: vec![],
                            fields,
                        },
                        None,
                        dynamic_classes,
                        dynamic_enums,
                        runtime_named_objects,
                    );
                }
            }
            BexExternalValue::Adt(BexExternalAdt::Media(arc)) => {
                // A bare rust_data is only correct when the destination slot
                // IS the wrapper's `_data: $rust_type` field. Anywhere else
                // (a declared `image`/`audio`/... parameter, a media slot in
                // a container, or no context at all) the usable value is the
                // stdlib wrapper class instance — methods and the prompt
                // renderer dispatch on that instance, so a bare rust_data
                // panics `mime_type()` and silently drops media parts from
                // rendered requests (renders as literal `<rust_data>`).
                // The OUTBOUND path flattens the wrapper back to the
                // canonical `Adt(Media(_))`, so the engine's wire contract
                // (media_roundtrip.rs) is ADT in both directions while the
                // VM-internal form matches BAML-constructed media.
                if matches!(expected_ty, Some(RuntimeTy::RustType { .. })) {
                    Value::object(holder.holder_mut().tlab_mut().alloc_rust_data(arc))
                } else {
                    let class_name = match arc.kind {
                        baml_type::MediaKind::Image => "baml.media.Image",
                        baml_type::MediaKind::Audio => "baml.media.Audio",
                        baml_type::MediaKind::Video => "baml.media.Video",
                        baml_type::MediaKind::Pdf => "baml.media.Pdf",
                        baml_type::MediaKind::Generic => {
                            return Err(EngineError::TypeMismatch {
                                message:
                                    "cannot materialize a generic media value as a wrapper instance"
                                        .to_string(),
                            });
                        }
                    };
                    let mut fields = indexmap::IndexMap::new();
                    fields.insert(
                        bex_external_types::MEDIA_WRAPPER_DATA_FIELD.to_string(),
                        BexExternalValue::Adt(BexExternalAdt::Media(arc)),
                    );
                    return self.convert_external_to_vm_value_with_ty_and_runtime(
                        holder,
                        BexExternalValue::Instance {
                            class_name: class_name.to_string(),
                            type_args: vec![],
                            fields,
                        },
                        None,
                        dynamic_classes,
                        dynamic_enums,
                        runtime_named_objects,
                    );
                }
            }
            BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
                kind, heap_handle, ..
            }) => {
                let ptr = self
                    .resolve_handle(holder.proof(), &heap_handle)
                    .ok_or_else(|| EngineError::TypeMismatch {
                        message: format!(
                            "{kind:?} handle is stale or belongs to a different BAML runtime"
                        ),
                    })?;
                Value::object(ptr)
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
                // - `RuntimeTy::RustType` (opaque `$rust_type` field, e.g. the
                //   `_handle` slot on `baml.errors.HostCallable`): wrap the
                //   arc in `Object::RustData` so the BAML→host decoder can
                //   later downcast it back to a `HostValueArc`. No
                //   function signature involved.
                // - `RuntimeTy::Function` (host callable passed as a function
                //   argument): build a `HostClosure` bound to the declared
                //   signature so the call site can invoke it.
                let ty = expected_ty.ok_or_else(|| EngineError::CannotConvert {
                    type_name: "host_value (no declared type in context)".to_string(),
                })?;
                if matches!(peel_to_rust_type(ty), Some(())) {
                    if arc.kind != HostValueKind::Opaque {
                        return Err(EngineError::TypeMismatch {
                            message:
                                "callable host value cannot inhabit an opaque `$rust_type` slot"
                                    .to_string(),
                        });
                    }
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
                if arc.kind != HostValueKind::Callable {
                    return Err(EngineError::TypeMismatch {
                        message: format!(
                            "opaque host value cannot be passed where the callable type `{function_ty}` was declared"
                        ),
                    });
                }
                let (params, ret, throws) = match function_ty {
                    RuntimeTy::Function {
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
                // The host's returned value is validated against `ret` when the
                // call completes. A generic return type erases to
                // `Unknown` at runtime, which the return validator treats
                // as "accept anything" — letting the host inject a value of any
                // type into a position BAML treats as the instantiated type
                // variable. Reject such a callable at bind time rather than
                // admit an unvalidatable return. (This also rejects a genuine
                // bare `-> void` host callable; such a callable must declare a
                // concrete return type.)
                // A top-level `void` callback has one canonical host wire
                // representation: Null. Nested void positions remain invalid
                // (they indicate an erased/unresolved type).
                if !matches!(ret, RuntimeTy::Void { .. }) && ret_ty_has_unvalidatable_position(&ret)
                {
                    return Err(EngineError::TypeMismatch {
                        message: format!(
                            "host callable cannot be bound: its return type `{ret}` contains an \
                             unresolved position, so the host's returned value cannot be validated",
                        ),
                    });
                }
                // `throws` is the callable's declared error contract `E`
                // (`call_host_value<T, E>`). When the parameter pins no
                // concrete error type the throws lowers to a bottom/unit
                // shape: an omitted `throws` becomes `Never` (the function-type
                // lowering's default) and a bare `-> void` throws becomes
                // `Void`. Neither names an error the host is obligated to
                // honor — and the host is foreign code that may surface a
                // native exception regardless (materialized as
                // `baml.errors.HostCallable`). Normalize both to
                // `Unknown` so such a throw is accepted opaquely and an
                // in-BAML `catch` can match it, rather than being rejected as a
                // `HostContractViolation`. Concrete throws (e.g.
                // `throws ParseError`) pass through unchanged and stay enforced.
                let normalized_throws = match throws {
                    RuntimeTy::Void { attr } | RuntimeTy::Never { attr } => {
                        RuntimeTy::Unknown { attr }
                    }
                    other => other,
                };
                // The VM heap stores the callable's signature as `RealizedTy`
                // (`HostClosure`'s fields). A bound host callable's declared
                // function type is realized here; a non-realized position (an
                // unfilled type variable) is a contract violation surfaced as a
                // type mismatch rather than erased.
                let realized_params = params
                    .iter()
                    .map(|param| {
                        Ok(bex_vm_types::RealizedFunctionParamTy {
                            name: param.name.clone(),
                            ty: self.realize_host_ty_with_runtime(
                                &holder.holder().vm,
                                holder.proof(),
                                &param.ty,
                                overlay,
                            )?,
                            mode: param.mode,
                        })
                    })
                    .collect::<Result<Vec<_>, EngineError>>()?;
                let host_closure = bex_vm_types::HostClosure {
                    handle: arc,
                    ret_ty: Box::new(self.realize_host_ty_with_runtime(
                        &holder.holder().vm,
                        holder.proof(),
                        &ret,
                        overlay,
                    )?),
                    throws_ty: Box::new(self.realize_host_ty_with_runtime(
                        &holder.holder().vm,
                        holder.proof(),
                        &normalized_throws,
                        overlay,
                    )?),
                    arity: params.len(),
                    // Capture the declared params (names + optionality) so the VM
                    // can split the call args into positional + supplied-optional
                    // (by name) on dispatch, for the per-bridge argument reshape.
                    params: Box::new(realized_params),
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
        let optional_values = optional_map.to_index_map();
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
            element_type: RuntimeTy::unknown(),
            items: vec![
                BexExternalValue::Array {
                    element_type: RuntimeTy::unknown(),
                    items: positional,
                },
                BexExternalValue::Map {
                    key_type: RuntimeTy::string(),
                    value_type: RuntimeTy::unknown(),
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
    declared_type: &RuntimeTy,
) -> Result<BexExternalValue, EngineError> {
    match declared_type {
        // A nullable union (`T?` == `T | null`) is optionality, not a tagged
        // union: a null value is bare `Null`, and a single non-null member is
        // unwrapped to its bare value (recursing in case that member is itself a
        // real union). This preserves the pre-desugaring behavior where optional
        // values carried no union metadata.
        RuntimeTy::Union(members, attr) if members.iter().any(RuntimeTy::is_null) => {
            if matches!(value, BexExternalValue::Null) {
                return Ok(BexExternalValue::Null);
            }
            let non_null: Vec<RuntimeTy> =
                members.iter().filter(|m| !m.is_null()).cloned().collect();
            match non_null.len() {
                0 => Ok(value),
                1 => maybe_wrap_union(value, &non_null[0]),
                _ => {
                    let selected = find_matching_member(&value, &non_null)?;
                    let metadata = UnionMetadata::new(
                        RuntimeTy::Union(members.clone(), attr.clone()),
                        selected,
                    );
                    Ok(BexExternalValue::Union {
                        value: Box::new(value),
                        metadata,
                    })
                }
            }
        }
        RuntimeTy::Union(members, _) => {
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
    declared_type: &RuntimeTy,
    selected: &RuntimeTy,
) -> Result<BexExternalValue, EngineError> {
    let RuntimeTy::Union(members, _) = declared_type else {
        return Ok(value);
    };
    if members.iter().any(RuntimeTy::is_null) {
        if matches!(value, BexExternalValue::Null) {
            return Ok(value);
        }
        let non_null: Vec<&RuntimeTy> = members.iter().filter(|member| !member.is_null()).collect();
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
/// the matching concrete type in parallel.
///
/// Used to recover a generic method's class type arguments from the *actual*
/// `self` value at a host call: the declared `self` type still mentions the
/// class's type variables (e.g. `Stream<TStream, TFinal>`), while the inbound
/// receiver carries them concretely (e.g. `Stream<null | string, string>`).
/// Zipping the two yields `{TStream -> null | string, TFinal -> string}`, which
/// [`substitute_type_vars`] then applies to the method's declared return type so
/// the host-return conversion sees concrete arms instead of bare type variables.
pub(crate) fn collect_type_var_bindings<N: Clone>(
    declared: &baml_type::RuntimeTy<N>,
    concrete: &baml_type::RuntimeTy<N>,
    out: &mut indexmap::IndexMap<String, baml_type::RuntimeTy<N>>,
) {
    use baml_type::RuntimeTy;
    match (declared, concrete) {
        // A type-var position binds to whatever concrete type sits opposite it.
        // First binding wins (a type var should be consistent across positions).
        (RuntimeTy::TypeVar(name, _), _) => {
            out.entry(name.to_string())
                .or_insert_with(|| concrete.clone());
        }
        (RuntimeTy::Class(_, da, _), RuntimeTy::Class(_, ca, _)) => {
            for (d, c) in da.iter().zip(ca.iter()) {
                collect_type_var_bindings(d, c, out);
            }
        }
        (RuntimeTy::List(d, _), RuntimeTy::List(c, _)) => collect_type_var_bindings(d, c, out),
        (
            RuntimeTy::Map {
                key: dk, value: dv, ..
            },
            RuntimeTy::Map {
                key: ck, value: cv, ..
            },
        ) => {
            collect_type_var_bindings(dk, ck, out);
            collect_type_var_bindings(dv, cv, out);
        }
        (RuntimeTy::Union(dm, _), RuntimeTy::Union(cm, _)) => {
            for (d, c) in dm.iter().zip(cm.iter()) {
                collect_type_var_bindings(d, c, out);
            }
        }
        (RuntimeTy::Future(dv, de, _), RuntimeTy::Future(cv, ce, _)) => {
            collect_type_var_bindings(dv, cv, out);
            collect_type_var_bindings(de, ce, out);
        }
        _ => {}
    }
}

/// Recover type-variable bindings from a live VM type. The declared signature
/// uses wire-name heads, while the concrete side retains identity-carrying
/// `TypeHead`s from the rooted heap object. Keeping the concrete side prevents
/// descriptive handle metadata from becoming authority for generic receivers.
pub(crate) fn collect_live_type_var_bindings<DeclaredHead: Clone, ConcreteHead: Clone>(
    declared: &baml_type::RuntimeTy<DeclaredHead>,
    concrete: &baml_type::RuntimeTy<ConcreteHead>,
    out: &mut indexmap::IndexMap<String, baml_type::RuntimeTy<ConcreteHead>>,
) {
    use baml_type::RuntimeTy;
    match (declared, concrete) {
        (RuntimeTy::TypeVar(name, _), _) => {
            out.entry(name.to_string())
                .or_insert_with(|| concrete.clone());
        }
        (RuntimeTy::Class(_, declared_args, _), RuntimeTy::Class(_, concrete_args, _)) => {
            for (declared, concrete) in declared_args.iter().zip(concrete_args) {
                collect_live_type_var_bindings(declared, concrete, out);
            }
        }
        (RuntimeTy::List(declared, _), RuntimeTy::List(concrete, _)) => {
            collect_live_type_var_bindings(declared, concrete, out);
        }
        (
            RuntimeTy::Map {
                key: declared_key,
                value: declared_value,
                ..
            },
            RuntimeTy::Map {
                key: concrete_key,
                value: concrete_value,
                ..
            },
        ) => {
            collect_live_type_var_bindings(declared_key, concrete_key, out);
            collect_live_type_var_bindings(declared_value, concrete_value, out);
        }
        (RuntimeTy::Union(declared, _), RuntimeTy::Union(concrete, _)) => {
            for (declared, concrete) in declared.iter().zip(concrete) {
                collect_live_type_var_bindings(declared, concrete, out);
            }
        }
        (
            RuntimeTy::Future(declared_value, declared_error, _),
            RuntimeTy::Future(concrete_value, concrete_error, _),
        ) => {
            collect_live_type_var_bindings(declared_value, concrete_value, out);
            collect_live_type_var_bindings(declared_error, concrete_error, out);
        }
        _ => {}
    }
}

/// Replace `TypeVar` leaves named in `bindings` with their concrete types,
/// recursing through container/aggregate positions. Type variables absent from
/// `bindings` (e.g. a method's own, unbound type params) are left as-is.
///
/// This is the fix for the host-driven streaming `TStream`-typevar bug: a
/// generic method's declared return type (e.g. `Stream.next`'s
/// `TStream | Done`) reaches the FFI return conversion with `TStream`
/// unsubstituted, so a concrete partial value matched no union member and the
/// conversion panicked. Substituting from the receiver's bound type args (see
/// [`collect_type_var_bindings`]) makes the concrete arm present.
pub(crate) fn substitute_type_vars<N: Clone>(
    ty: &baml_type::RuntimeTy<N>,
    bindings: &indexmap::IndexMap<String, baml_type::RuntimeTy<N>>,
) -> baml_type::RuntimeTy<N> {
    use baml_type::RuntimeTy;
    if bindings.is_empty() {
        return ty.clone();
    }
    match ty {
        RuntimeTy::TypeVar(name, _) => bindings
            .get(name.as_str())
            .cloned()
            .unwrap_or_else(|| ty.clone()),
        RuntimeTy::Class(tn, args, attr) => RuntimeTy::Class(
            tn.clone(),
            args.iter()
                .map(|a| substitute_type_vars(a, bindings))
                .collect(),
            attr.clone(),
        ),
        RuntimeTy::List(inner, attr) => RuntimeTy::List(
            Box::new(substitute_type_vars(inner, bindings)),
            attr.clone(),
        ),
        RuntimeTy::Map { key, value, attr } => RuntimeTy::Map {
            key: Box::new(substitute_type_vars(key, bindings)),
            value: Box::new(substitute_type_vars(value, bindings)),
            attr: attr.clone(),
        },
        RuntimeTy::Union(members, attr) => RuntimeTy::Union(
            members
                .iter()
                .map(|m| substitute_type_vars(m, bindings))
                .collect(),
            attr.clone(),
        ),
        RuntimeTy::Future(value, error, attr) => RuntimeTy::Future(
            Box::new(substitute_type_vars(value, bindings)),
            Box::new(substitute_type_vars(error, bindings)),
            attr.clone(),
        ),
        // A function-typed parameter (`f: (T) -> R`) carries call type-vars in its
        // params/return/throws. Substitute them so an explicitly-bound closure
        // param materializes against concrete types (J13: `apply[int,int]` binds
        // `f` as `(int) -> int`, not the unvalidatable `(T) -> R`).
        RuntimeTy::Function {
            params,
            ret,
            throws,
            attr,
        } => RuntimeTy::Function {
            params: params
                .iter()
                .map(|p| baml_type::RuntimeFunctionParamTy {
                    name: p.name.clone(),
                    ty: substitute_type_vars(&p.ty, bindings),
                    mode: p.mode,
                })
                .collect(),
            ret: Box::new(substitute_type_vars(ret, bindings)),
            throws: Box::new(substitute_type_vars(throws, bindings)),
            attr: attr.clone(),
        },
        // Other positions (leaves, opaque handles, Interface/projection) don't
        // carry call type-vars in these paths, so they pass through unchanged.
        _ => ty.clone(),
    }
}

/// Return the name of the first unbound `TypeVar` reachable in `ty` (walking the
/// same aggregate positions [`substitute_type_vars`] does), or `None` if the
/// type is fully concrete. After substituting a generic callee's bindings, any
/// `TypeVar` that remains is unbound — the host failed to supply it. The
/// engine rejects such calls (the wire must be fully bound).
pub(crate) fn first_unbound_type_var(ty: &RuntimeTy) -> Option<String> {
    match ty {
        RuntimeTy::TypeVar(name, _) => Some(name.to_string()),
        RuntimeTy::Class(_, args, _) => args.iter().find_map(first_unbound_type_var),
        RuntimeTy::List(inner, _) => first_unbound_type_var(inner),
        RuntimeTy::Map { key, value, .. } => {
            first_unbound_type_var(key).or_else(|| first_unbound_type_var(value))
        }
        RuntimeTy::Union(members, _) => members.iter().find_map(first_unbound_type_var),
        RuntimeTy::Future(value, error, _) => {
            first_unbound_type_var(value).or_else(|| first_unbound_type_var(error))
        }
        _ => None,
    }
}

/// Whether `ty` mentions any `TypeVar` (i.e. the callee is generic at this
/// position). Thin wrapper over [`first_unbound_type_var`].
pub(crate) fn contains_type_var(ty: &RuntimeTy) -> bool {
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
/// host-only sibling (`choose(5, host_obj)` → `int | rust_type`).
pub(crate) fn union_runtime_ty(a: &RuntimeTy, b: &RuntimeTy) -> RuntimeTy {
    RuntimeTy::try_from(baml_type_runtime::union_ty(&Ty::from(a), &Ty::from(b)))
        .expect("union of runtime types is a runtime type")
}

/// Recover `TypeVar(name) -> concrete` bindings from a `(formal, actual)` pair,
/// **union-merging** repeat bindings of the same variable across calls. The
/// `RuntimeTy` seam over the shared [`baml_type_runtime::infer_value_bindings`]
/// — the runtime variant of the TIR unifier, which binds a `Class` arm only when
/// the formal and actual name the same class.
///
/// Contrast [`collect_type_var_bindings`], the *first-wins* self-receiver path,
/// which is intentionally a separate, simpler walk.
///
/// Superseded at the call seam by [`infer_bindings_runtime_checked`] (which
/// solves all arguments together with variance tracking); retained as a
/// best-effort per-pair primitive exercised by the unit tests below.
#[cfg(test)]
pub(crate) fn infer_bindings_runtime(
    formal: &RuntimeTy,
    actual: &RuntimeTy,
    out: &mut indexmap::IndexMap<String, RuntimeTy>,
) {
    let mut bindings: rustc_hash::FxHashMap<baml_type::ParamTy, Ty> =
        rustc_hash::FxHashMap::default();
    baml_type_runtime::infer_value_bindings(&Ty::from(formal), &Ty::from(actual), &mut bindings);
    for (name, ty) in bindings {
        // A binding is always a subterm/union of a runtime-derived actual, so the
        // narrow cannot fail; skip defensively rather than panic if it ever does.
        let Ok(rt) = RuntimeTy::try_from(ty) else {
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
pub(crate) fn infer_bindings_runtime_checked(
    pairs: &[(RuntimeTy, RuntimeTy)],
) -> Result<indexmap::IndexMap<String, RuntimeTy>, String> {
    let mut cons = baml_type_runtime::InferenceConstraints::new();
    for (formal, actual) in pairs {
        cons.record(&Ty::from(formal), &Ty::from(actual));
    }
    let bindings = cons.solve().map_err(|e| e.message)?;
    let mut out = indexmap::IndexMap::new();
    for (name, ty) in bindings {
        // A binding is always a subterm/union of a runtime-derived actual, so the
        // narrow cannot fail; skip defensively rather than panic if it ever does.
        if let Ok(rt) = RuntimeTy::try_from(ty) {
            out.insert(name.to_string(), rt);
        }
    }
    Ok(out)
}

/// Where each `TypeVar` occurs across a generic call's *parameter* types, used
/// to classify still-unbound vars after inference (03c `03c-impl-guide` rules
/// 2 & 4):
///
/// - `value_position` — the var appears in a parameter at a position **not**
///   inside a function-typed sub-term (a bare arg, container element, class/map
///   arg, union member). Such a var, if it came up unbound (empty collection,
///   `null` actual, union-sibling-absorbed, unbound generic), **defaults to
///   `RustType`** and rides opaquely (rule 4).
/// - `closure` — the var appears **inside a function-typed parameter's
///   signature** (its params/return/throws, any depth). A host callable is
///   opaque to BAML, so the var cannot be inferred from it *or* validated
///   against it; it is **poisoned** and must be specified explicitly (rule 2),
///   overriding any value-position evidence it might also have.
///
/// A var in neither set has no parameter occurrence at all (return-only /
/// body-only) ⇒ must-specify (rule 3), left for Gate A.
///
/// `ambiguous_union` is a further must-specify carve-out: a var that is a direct
/// member of a union alongside **another** free `TypeVar` (`f<T,U>(x: T | U |
/// int)`). Such a union has no principled way to split the actual between its
/// free vars, so an unbound one is rejected rather than defaulted to `RustType`
/// (03b J12, distinct from §H's single-`TypeVar`-beside-concrete case).
#[derive(Default)]
pub(crate) struct ParamVarPositions {
    pub value_position: std::collections::HashSet<String>,
    pub closure: std::collections::HashSet<String>,
    pub ambiguous_union: std::collections::HashSet<String>,
}

/// The highest `TypeArgRef(N)` index appearing anywhere in a field template, or
/// `None` if the template references no class type-arg. Used to compute a
/// generic class's arity from its fields.
fn template_max_type_arg_ref<N: Clone>(t: &baml_type::TyTemplate<N>) -> Option<u32> {
    use baml_type::TyTemplate as T;
    match t {
        T::TypeArgRef(n) => Some(*n),
        T::List(inner, _) => template_max_type_arg_ref(inner),
        T::Map { key, value, .. } | T::Future(key, value, _) => template_max_type_arg_ref(key)
            .into_iter()
            .chain(template_max_type_arg_ref(value))
            .max(),
        T::Union(members, _) => members.iter().filter_map(template_max_type_arg_ref).max(),
        T::Class(_, args, _) => args.iter().filter_map(template_max_type_arg_ref).max(),
        T::Interface(_, args, assoc, _) => args
            .iter()
            .chain(assoc.iter().map(|(_, t)| t))
            .filter_map(template_max_type_arg_ref)
            .max(),
        T::Function {
            params,
            ret,
            throws,
            ..
        } => params
            .iter()
            .map(|p| &p.ty)
            .chain([ret.as_ref(), throws.as_ref()])
            .filter_map(template_max_type_arg_ref)
            .max(),
        T::AssociatedTypeProjection {
            base, interface, ..
        } => template_max_type_arg_ref(base)
            .into_iter()
            .chain(
                interface
                    .generics
                    .iter()
                    .chain(interface.associated_types.iter().map(|(_, t)| t))
                    .filter_map(template_max_type_arg_ref),
            )
            .max(),
        // Realized leaves carry no frame ref.
        _ => None,
    }
}

/// Whether a value is a *structural* host-only carrier — an inline
/// `BexExternalValue` (instance / map / list / variant / bytes) as opposed to an
/// opaque handle, host-value, or already-`RustData`. When such a value lands in
/// a `RustType` slot it is stashed verbatim (`OpaqueExternalValue`) rather than
/// materialized into an introspectable VM object, so it round-trips unchanged
/// (03b G2/G3/G4). Handles / host-values / rust-data already have their own
/// opaque round-trip and are left to their existing arms.
fn is_structural_host_only(value: &BexExternalValue) -> bool {
    matches!(
        value,
        BexExternalValue::Instance { .. }
            | BexExternalValue::Map { .. }
            | BexExternalValue::Array { .. }
            | BexExternalValue::Variant { .. }
            | BexExternalValue::Uint8Array(_)
    )
}

/// Classify every `TypeVar` occurring in the declared parameter types into
/// [`ParamVarPositions`]. See that type's docs for the rule mapping.
pub(crate) fn classify_param_var_positions(params: &[RuntimeTy]) -> ParamVarPositions {
    let mut out = ParamVarPositions::default();
    for p in params {
        walk_var_positions(p, false, &mut out);
    }
    out
}

fn walk_var_positions(ty: &RuntimeTy, in_closure: bool, out: &mut ParamVarPositions) {
    match ty {
        RuntimeTy::TypeVar(name, _) => {
            if in_closure {
                out.closure.insert(name.to_string());
            } else {
                out.value_position.insert(name.to_string());
            }
        }
        // Descending into a function-typed position poisons every var reached
        // through it — params, return, and throws alike.
        RuntimeTy::Function {
            params,
            ret,
            throws,
            ..
        } => {
            for p in params {
                walk_var_positions(&p.ty, true, out);
            }
            walk_var_positions(ret, true, out);
            walk_var_positions(throws, true, out);
        }
        RuntimeTy::List(inner, _) => {
            walk_var_positions(inner, in_closure, out);
        }
        RuntimeTy::Map { key, value, .. } => {
            walk_var_positions(key, in_closure, out);
            walk_var_positions(value, in_closure, out);
        }
        RuntimeTy::Union(members, _) => {
            // A union with ≥2 direct `TypeVar` members is un-inferrable (no
            // principled split). Mark those direct members must-specify so the
            // rule-4 default skips them (03b J12).
            let direct_tvs: Vec<&str> = members
                .iter()
                .filter_map(|m| match m {
                    RuntimeTy::TypeVar(name, _) => Some(name.as_str()),
                    _ => None,
                })
                .collect();
            if direct_tvs.len() >= 2 {
                for name in &direct_tvs {
                    out.ambiguous_union.insert(name.to_string());
                }
            }
            for m in members {
                walk_var_positions(m, in_closure, out);
            }
        }
        RuntimeTy::Class(_, args, _) => {
            for a in args {
                walk_var_positions(a, in_closure, out);
            }
        }
        RuntimeTy::Future(value, error, _) => {
            walk_var_positions(value, in_closure, out);
            walk_var_positions(error, in_closure, out);
        }
        _ => {}
    }
}

/// The outcome of synthesizing a `RuntimeTy` from a runtime argument value —
/// the value→type bridge the TIR never needs (it sees typed expressions, not
/// values). Every value yields a `RuntimeTy`: a concrete BAML type when one can
/// be read, else the host-only `rust_type` fallback.
pub(crate) enum SynthTy {
    /// A concrete BAML type was synthesized (Case 1).
    Known(RuntimeTy),
    /// An opaque host value with no BAML type (Case 2) — binds `rust_type` and
    /// rides through as an `Object::RustData` heap handle.
    HostOnly,
}

impl SynthTy {
    /// The `RuntimeTy` to feed [`infer_bindings_runtime`]. `HostOnly` lowers to
    /// `RustType` (the recommended host-only default — round-trips as
    /// `Object::RustData` with no materializer change; see
    /// `convert_external_to_vm_value_with_ty`).
    pub(crate) fn into_runtime_ty(self) -> RuntimeTy {
        match self {
            SynthTy::Known(ty) => ty,
            SynthTy::HostOnly => RuntimeTy::RustType {
                attr: baml_type::TyAttr::default(),
            },
        }
    }
}

/// Union-fold the element/value synths of a collection into a single element
/// type. `None` only when the collection is empty (every value synthesizes to a
/// concrete type or the host-only `rust_type`, so non-empty collections always
/// yield evidence).
fn synth_collection_element<'a>(
    engine: &BexEngine,
    items: impl Iterator<Item = &'a BexExternalValue>,
) -> Option<RuntimeTy> {
    let mut acc: Option<RuntimeTy> = None;
    for item in items {
        let ty = engine.synth_ty_from_value(item).into_runtime_ty();
        acc = Some(match acc {
            None => ty,
            Some(prev) => union_runtime_ty(&prev, &ty),
        });
    }
    acc
}

/// Peel `Optional` and singleton-`Union` wrappers off `ty`, returning the
/// underlying `RuntimeTy::Function` if there is one. Returns `None` if the type is
/// not a function (after peeling).
///
/// Used by `convert_external_to_vm_value_with_ty` to find the function
/// signature behind, e.g., `(int) -> int` and `((int) -> int)?` so that an
/// inbound `BexExternalValue::HostValue` can be bound to it as an
/// `Object::HostClosure`.
/// Returns `Some(())` if `ty` is the runtime representation of
/// `$rust_type` — i.e. `RuntimeTy::RustType` — possibly
/// wrapped in a `Union` (the post-`RuntimeTy::Optional`-removal encoding of
/// `T?` is `RuntimeTy::Union([T, Null], _)`, so nullable forms flow through
/// the union arm). Mirrors [`peel_function_ty`] for the `$rust_type`
/// field shape that a `HostValue` argument can land in.
pub(crate) fn peel_to_rust_type(ty: &RuntimeTy) -> Option<()> {
    if matches!(ty, RuntimeTy::RustType { .. }) {
        return Some(());
    }
    match ty {
        RuntimeTy::Union(members, _) => {
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

pub(crate) fn peel_function_ty(ty: &RuntimeTy) -> Option<&RuntimeTy> {
    match ty {
        RuntimeTy::Function { .. } => Some(ty),
        RuntimeTy::Union(members, _) => {
            // Find the single function member, if any. If there are multiple
            // function members or none, we can't pick deterministically.
            let mut found: Option<&RuntimeTy> = None;
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

/// Find the single list member behind optional/union wrappers and return its
/// declared element type. Ambiguous unions deliberately return `None`.
fn peel_list_element_ty(ty: &RuntimeTy) -> Option<&RuntimeTy> {
    peel_single_container_member(ty, |member| match member {
        RuntimeTy::List(element, _) => Some(element.as_ref()),
        _ => None,
    })
}

/// Find the single map member behind optional/union wrappers and return its
/// declared value type. Ambiguous unions deliberately return `None`.
fn peel_map_value_ty(ty: &RuntimeTy) -> Option<&RuntimeTy> {
    peel_single_container_member(ty, |member| match member {
        RuntimeTy::Map { value, .. } => Some(value.as_ref()),
        _ => None,
    })
}

fn peel_single_container_member<'a>(
    ty: &'a RuntimeTy,
    select: impl Copy + Fn(&'a RuntimeTy) -> Option<&'a RuntimeTy>,
) -> Option<&'a RuntimeTy> {
    if let Some(found) = select(ty) {
        return Some(found);
    }
    let RuntimeTy::Union(members, _) = ty else {
        return None;
    };
    let mut found = None;
    for member in members {
        if let Some(candidate) = peel_single_container_member(member, select) {
            if found.is_some() {
                return None;
            }
            found = Some(candidate);
        }
    }
    found
}

/// Whether a host-callable's declared return type contains a position the
/// host-return validator cannot check, such as `RuntimeTy::Unknown`.
/// Recurses through `Optional` / `List` / `Map`-value /
/// `Union` / `Class`-generic-args so a nested erased position (`(T)[]`,
/// `Box<T>`) is caught too. A host callable with such a return type cannot have
/// its returned value validated, so binding one is rejected.
fn ret_ty_has_unvalidatable_position(ty: &RuntimeTy) -> bool {
    match ty {
        // Unvalidatable: the host's opaque returned value cannot be checked
        // against these declared types (the host-return validator has no
        // positive discriminator for them), so a host could inject a value that
        // violates the declared type. Reject binding such a callable.
        //   - `Unknown`: accept-anything top.
        //   - `TypeVar`/`AssociatedTypeProjection`: faithful (un-erased) generic
        //     positions whose instantiation can't be validated.
        //   - `Interface`: implementation can't be checked at the FFI boundary.
        //   - `Future`: the host cannot produce a VM future, and nothing
        //     validates one.
        RuntimeTy::Unknown { .. }
        | RuntimeTy::TypeVar(..)
        | RuntimeTy::AssociatedTypeProjection { .. }
        | RuntimeTy::Interface(..)
        | RuntimeTy::Future(..) => true,

        // Container positions are validated structurally; recurse so a nested
        // unvalidatable position (`(T)[]`, `Box<T>`, `int | T`) is caught too.
        RuntimeTy::List(elem, _) => ret_ty_has_unvalidatable_position(elem),
        RuntimeTy::Map { value, .. } => ret_ty_has_unvalidatable_position(value),
        RuntimeTy::Union(members, _) => members.iter().any(ret_ty_has_unvalidatable_position),
        RuntimeTy::Class(_, generic_args, _) => {
            generic_args.iter().any(ret_ty_has_unvalidatable_position)
        }

        // Directly validated by the host-return validator.
        RuntimeTy::Null { .. }
        | RuntimeTy::Void { .. }
        | RuntimeTy::Bool { .. }
        | RuntimeTy::Int { .. }
        | RuntimeTy::Float { .. }
        | RuntimeTy::Bigint { .. }
        | RuntimeTy::String { .. }
        | RuntimeTy::Uint8Array { .. }
        | RuntimeTy::Literal(..)
        | RuntimeTy::Enum(..)
        | RuntimeTy::EnumVariant(..)
        | RuntimeTy::Media(..)
        | RuntimeTy::Function { .. } => false,

        // Opaque runtime handles: the declared type is itself opaque, so the
        // host's value has no concrete contract to violate. (Most cannot be a
        // host-callable return type in practice; an unexpanded `TypeAlias` here
        // would be a prior-stage bug, and `-> never` is a callable that only
        // ever throws.)
        RuntimeTy::RustType { .. }
        | RuntimeTy::Type { .. }
        | RuntimeTy::Resource { .. }
        | RuntimeTy::PromptAst { .. }
        | RuntimeTy::TypeAlias(..)
        | RuntimeTy::Never { .. } => false,
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
    members: &[RuntimeTy],
) -> Result<RuntimeTy, EngineError> {
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
            (BexExternalValue::Array { element_type, .. }, RuntimeTy::List(expected, _)) => {
                runtime_ty_structurally_equal(element_type, expected)
            }
            (
                BexExternalValue::Map {
                    key_type,
                    value_type,
                    ..
                },
                RuntimeTy::Map { key, value, .. },
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
        if matches!(member, RuntimeTy::Literal(..)) && value_matches_type(value, member) {
            return Ok(member.clone());
        }
    }
    // A singleton enum-variant arm likewise outranks its broad enum type.
    for member in members {
        if matches!(member, RuntimeTy::EnumVariant(..)) && value_matches_type(value, member) {
            return Ok(member.clone());
        }
    }
    let mut matching: Vec<&RuntimeTy> = Vec::new();
    for member in members.iter().filter(|member| {
        !matches!(
            member,
            RuntimeTy::Literal(..) | RuntimeTy::EnumVariant(..) | RuntimeTy::Unknown { .. }
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
        if matches!(member, RuntimeTy::Unknown { .. }) {
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

/// Select a union member for an inbound node that did not carry `value_type`.
/// Unlike the general VM/output selector above, this must not invent intent for
/// overlapping arms: a literal and its primitive, an enum variant and its enum,
/// or two container/class shapes that both accept the payload require an
/// explicit sparse annotation.
fn find_unannotated_inbound_member_with_aliases(
    value: &BexExternalValue,
    members: &[RuntimeTy],
    aliases: &indexmap::IndexMap<baml_type::TypeName, RuntimeTy>,
    classes: &indexmap::IndexMap<baml_type::TypeName, WireClassDefinition>,
    ambiguity_policy: crate::InboundUnionAmbiguityPolicy,
) -> Result<RuntimeTy, EngineError> {
    let mut matching: Vec<&RuntimeTy> = Vec::new();
    for member in members.iter().filter(|member| {
        !matches!(member, RuntimeTy::Unknown { .. })
            && value_matches_type_with_definitions(value, member, aliases, classes)
    }) {
        if matching
            .iter()
            .all(|matched| !runtime_ty_structurally_equal(matched, member))
        {
            matching.push(member);
        }
    }

    // A nested optional alias can make one null payload match both the alias
    // member and an explicit null member. That is not distinct host intent:
    // unlike an empty container, a bare null already identifies its exact leaf
    // type. Prefer the exact null arm so statically typed SDKs can encode `nil`
    // without attaching the forbidden root-union/optional annotation.
    if matches!(value, BexExternalValue::Null)
        && matching.len() > 1
        && let Some(exact_null) = matching
            .iter()
            .find(|member| runtime_ty_resolves_to_exact_null(member, aliases))
    {
        return Ok((**exact_null).clone());
    }

    match matching.as_slice() {
        [member] => Ok((*member).clone()),
        [] => members
            .iter()
            .find(|member| matches!(member, RuntimeTy::Unknown { .. }))
            .cloned()
            .ok_or_else(|| EngineError::TypeMismatch {
                message: format!(
                    "Value of type '{}' does not match any member of union {:?}",
                    described_value_type(value),
                    members
                ),
            }),
        _ if ambiguity_policy == crate::InboundUnionAmbiguityPolicy::SelectDefault => {
            // Dynamic host values do not carry a generated union wrapper. Use
            // the same specificity preference as normal BAML selection, then
            // fall back to the first declared structural match. This makes an
            // empty Python/TypeScript list deterministic while still letting
            // an exact literal/enum-variant beat its broad parent type.
            matching
                .iter()
                .find(|member| {
                    matches!(member, RuntimeTy::Literal(..) | RuntimeTy::EnumVariant(..))
                })
                .or_else(|| matching.first())
                .map(|member| (**member).clone())
                .ok_or_else(|| EngineError::TypeMismatch {
                    message: "dynamic default selection had no matching union member".to_string(),
                })
        }
        _ => Err(EngineError::TypeMismatch {
            message: format!(
                "value of type `{}` matches multiple union members; add an inbound `value_type` annotation to select one",
                value.type_name()
            ),
        }),
    }
}

fn runtime_ty_resolves_to_exact_null<'a>(
    mut ty: &'a RuntimeTy,
    aliases: &'a indexmap::IndexMap<baml_type::TypeName, RuntimeTy>,
) -> bool {
    // A productive alias chain cannot contain more distinct aliases than the
    // registry. The bound also makes this defensive against an invalid cycle.
    for _ in 0..=aliases.len() {
        match ty {
            RuntimeTy::Null { .. } => return true,
            RuntimeTy::TypeAlias(name, _) if !is_canonical_json_alias(name) => {
                let Some(expanded) = aliases.get(name) else {
                    return false;
                };
                ty = expanded;
            }
            _ => return false,
        }
    }
    false
}

fn type_name_matches_external_name(external_name: &str, type_name: &baml_type::TypeName) -> bool {
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
    // user-package types, so an `Instance` arriving
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
fn value_matches_type(value: &BexExternalValue, ty: &RuntimeTy) -> bool {
    value_matches_type_with_definitions(
        value,
        ty,
        &indexmap::IndexMap::new(),
        &indexmap::IndexMap::new(),
    )
}

/// Look up a class definition in the inbound wire view by its declared name.
///
/// Exact. The view holds only *declared* names — anonymous declarations are
/// filtered out when it is projected, having no name a wire value could carry
/// — and declared names are program-unique, so the old "scan for a unique
/// matching `display_name`" fallback had nothing left to find that this misses.
fn find_inbound_class_definition<'a>(
    classes: &'a indexmap::IndexMap<baml_type::TypeName, WireClassDefinition>,
    type_name: &baml_type::TypeName,
) -> Option<&'a WireClassDefinition> {
    classes.get(type_name)
}

fn map_matches_class_shape(
    entries: &indexmap::IndexMap<String, BexExternalValue>,
    type_name: &baml_type::TypeName,
    aliases: &indexmap::IndexMap<baml_type::TypeName, RuntimeTy>,
    classes: &indexmap::IndexMap<baml_type::TypeName, WireClassDefinition>,
) -> bool {
    let Some(definition) = find_inbound_class_definition(classes, type_name) else {
        // Shape-only fallback. Reached by unit-level callers with no loaded
        // program, and — deliberately — by a real engine for any class the
        // inbound view cannot represent: one that is anonymous, or whose field
        // types name an anonymous declaration. Matching on shape alone is
        // weaker than matching on a definition, but it never asserts a shape
        // the class does not actually have.
        return true;
    };

    // A plain host object has no nominal identity. Its field set and values are
    // therefore the only evidence available for selecting a class union arm.
    // Reject unknown keys, require non-optional fields, and recursively check
    // every supplied value. If two class definitions still accept the same
    // object, the caller reports an ambiguity and asks for a sparse annotation.
    if entries.keys().any(|key| {
        !definition
            .fields
            .iter()
            .any(|field| field.name == *key || field.alias.as_deref() == Some(key.as_str()))
    }) {
        return false;
    }

    definition.fields.iter().all(|field| {
        let value = entries
            .get(&field.name)
            .or_else(|| field.alias.as_ref().and_then(|alias| entries.get(alias)));
        match value {
            Some(value) => {
                value_matches_type_with_definitions(value, &field.field_type, aliases, classes)
            }
            None => {
                field.skip
                    || matches!(
                        &field.field_type,
                        RuntimeTy::Union(members, _) if members.iter().any(RuntimeTy::is_null)
                    )
            }
        }
    })
}

fn value_matches_type_with_definitions(
    value: &BexExternalValue,
    ty: &RuntimeTy,
    aliases: &indexmap::IndexMap<baml_type::TypeName, RuntimeTy>,
    classes: &indexmap::IndexMap<baml_type::TypeName, WireClassDefinition>,
) -> bool {
    if let RuntimeTy::TypeAlias(name, _) = ty
        && !is_canonical_json_alias(name)
        && let Some(expanded) = aliases.get(name)
    {
        return value_matches_type_with_definitions(value, expanded, aliases, classes);
    }

    match (value, ty) {
        // `Unknown` is the engine's "any value matches" sentinel
        // (TypeScript `unknown` semantics — see `baml_type::RuntimeTy::Unknown`).
        // Used by the stdlib generics hardcode in `baml_compiler2_mir::lower`
        // so e.g. `Stream<TStream, TFinal>.next() -> TStream | Done`
        // accepts any partial-stream payload as the `TStream` arm.
        (_, RuntimeTy::Unknown { .. }) => true,
        (BexExternalValue::Null, RuntimeTy::Null { .. }) => true,
        (BexExternalValue::Null, RuntimeTy::Void { .. }) => true,
        (BexExternalValue::Int(_), RuntimeTy::Int { .. }) => true,
        (BexExternalValue::Bigint(_), RuntimeTy::Bigint { .. }) => true,
        (BexExternalValue::Float(_), RuntimeTy::Float { .. }) => true,
        (BexExternalValue::Bool(_), RuntimeTy::Bool { .. }) => true,
        (BexExternalValue::String(_), RuntimeTy::String { .. }) => true,
        // Literal types match their corresponding runtime values
        (BexExternalValue::Int(value), RuntimeTy::Literal(Literal::Int(expected), _, _)) => {
            value == expected
        }
        (BexExternalValue::Bigint(value), RuntimeTy::Literal(Literal::Bigint(expected), _, _)) => {
            value == expected
        }
        (BexExternalValue::Float(value), RuntimeTy::Literal(Literal::Float(expected), _, _)) => {
            float_literal_matches(*value, expected)
        }
        (BexExternalValue::Uint8Array(_), RuntimeTy::Uint8Array { .. }) => true,
        (BexExternalValue::String(value), RuntimeTy::Literal(Literal::String(expected), _, _)) => {
            value.as_str() == expected
        }
        (BexExternalValue::Bool(value), RuntimeTy::Literal(Literal::Bool(expected), _, _)) => {
            value == expected
        }
        (BexExternalValue::Array { items, .. }, RuntimeTy::List(expected_element, _)) => {
            items.iter().all(|item| {
                value_matches_type_with_definitions(item, expected_element, aliases, classes)
            })
        }
        (
            BexExternalValue::Map { entries, .. },
            RuntimeTy::Map {
                value: expected_value,
                ..
            },
        ) => entries.values().all(|value| {
            value_matches_type_with_definitions(value, expected_value, aliases, classes)
        }),
        // A plain host object arrives as a bare `Map`, so use the loaded class
        // definition to test whether its field shape inhabits a `Class` slot.
        // It is promoted to an `Instance` during contextual materialization.
        (BexExternalValue::Map { entries, .. }, RuntimeTy::Class(type_name, _, _)) => {
            map_matches_class_shape(entries, type_name, aliases, classes)
        }
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
            RuntimeTy::Class(tn, expected_args, _),
        ) => {
            (class_name.is_empty() || type_name_matches_external_name(class_name, tn))
                && (type_args.is_empty() || class_type_args_compatible(type_args, expected_args))
        }
        // Media wrapper instances flatten to their canonical portable ADT at
        // the host boundary. That owned value still inhabits the corresponding
        // stdlib wrapper arm (for example `ai.PromptPart`'s
        // `baml.media.Image` member) even though it no longer has class shape.
        (BexExternalValue::Adt(BexExternalAdt::Media(media)), wrapper @ RuntimeTy::Class(..)) => {
            stdlib_media_wrapper_kind(wrapper).is_some_and(|kind| kind == media.kind)
        }
        (BexExternalValue::Variant { enum_name, .. }, RuntimeTy::Enum(tn, _)) => {
            type_name_matches_external_name(enum_name, tn)
        }
        (
            BexExternalValue::Variant {
                enum_name,
                variant_name,
            },
            RuntimeTy::EnumVariant(tn, expected_variant, _),
        ) => {
            type_name_matches_external_name(enum_name, tn)
                && variant_name == expected_variant.as_str()
        }
        (
            BexExternalValue::Adt(BexExternalAdt::Media(media)),
            RuntimeTy::Media(expected_kind, _),
        ) => *expected_kind == baml_type::MediaKind::Generic || media.kind == *expected_kind,
        (BexExternalValue::HostValue(value), RuntimeTy::Function { .. }) => {
            value.kind == bex_external_types::HostValueKind::Callable
        }
        (BexExternalValue::RustData(_), RuntimeTy::RustType { .. }) => true,
        (BexExternalValue::HostValue(value), RuntimeTy::RustType { .. }) => {
            value.kind == bex_external_types::HostValueKind::Opaque
        }
        (BexExternalValue::FunctionRef { .. }, RuntimeTy::Function { .. }) => true,
        (BexExternalValue::Adt(BexExternalAdt::Collector(_)), _) => false,
        (
            BexExternalValue::Adt(BexExternalAdt::Type(_) | BexExternalAdt::TypeDef(_)),
            RuntimeTy::Type { .. },
        ) => true,
        (union_value @ BexExternalValue::Union { metadata, .. }, RuntimeTy::Union(members, _)) => {
            members.iter().any(|member| {
                // Recurse with the ANNOTATED carrier, not the unwrapped
                // payload: sparse inbound annotations are the value's
                // identity for media members (the wrapper's shape never
                // matches `Media` structurally), and the per-member arms
                // below unwrap for every non-annotation-dependent case.
                selected_arm_equal(member, &metadata.selected_option)
                    && value_matches_type_with_definitions(union_value, member, aliases, classes)
            })
        }
        // `value_satisfies_json` peels sparse inbound leaf annotations (the
        // Swift bridge annotates every json scalar leaf) but still rejects
        // genuine union carriers and annotations outside the JSON algebra.
        (union_value @ BexExternalValue::Union { .. }, RuntimeTy::TypeAlias(name, _))
            if is_canonical_json_alias(name) =>
        {
            value_satisfies_json(union_value)
        }
        // A media value crosses the FFI as a class-shaped `{_data: handle}`
        // wrap carrying a sparse `Media(kind)` annotation. The annotation is
        // the value's identity — the payload shape never matches `Media`
        // structurally — so honor it before unwrapping (otherwise media
        // items inside union-typed containers, e.g. `image[]?`, are
        // rejected while the direct-typed path accepts them).
        (BexExternalValue::Union { metadata, .. }, RuntimeTy::Media(expected_kind, _))
            if metadata.is_inbound_type_annotation
                && matches!(
                    &metadata.selected_option,
                    RuntimeTy::Media(kind, _)
                        if *expected_kind == baml_type::MediaKind::Generic
                            || kind == expected_kind
                ) =>
        {
            true
        }
        (BexExternalValue::Union { value, .. }, ty) => {
            value_matches_type_with_definitions(value, ty, aliases, classes)
        }
        (value, RuntimeTy::TypeAlias(name, _)) if is_canonical_json_alias(name) => {
            value_satisfies_json(value)
        }
        // Handle nested unions (including nullable `T | null`) in the type.
        (value, RuntimeTy::Union(members, _)) => members
            .iter()
            .any(|m| value_matches_type_with_definitions(value, m, aliases, classes)),
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
fn class_type_args_compatible(wire_args: &[RuntimeTy], expected_args: &[RuntimeTy]) -> bool {
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

/// A type-arg position that imposes no concrete constraint: an unsubstituted
/// `TypeVar` or the `unknown` sentinel. Such a position can't positively
/// contradict a wire arg, so the structural matcher treats it as a wildcard.
fn is_wildcard_ty(ty: &RuntimeTy) -> bool {
    matches!(ty, RuntimeTy::TypeVar(..) | RuntimeTy::Unknown { .. })
}

/// Structural compatibility of a wire-supplied type against a declared
/// (substituted) type, used to disambiguate generic instances at the FFI
/// boundary without depending on `TyAttr` equality or full subtyping. Lenient:
/// only a *positive* leaf/shape mismatch returns `false`.
///
/// - an expected `TypeVar`/`Unknown` is a wildcard;
/// - two primitives must be the same primitive (this is what separates
///   `Foo<int>` from `Foo<string>`);
/// - matching containers recurse; class names must match, then args recurse;
/// - anything else is treated as compatible (don't over-reject).
fn runtime_ty_compatible(wire: &RuntimeTy, expected: &RuntimeTy) -> bool {
    use RuntimeTy as T;
    match (wire, expected) {
        (_, T::TypeVar(..) | T::Unknown { .. }) => true,
        (T::TypeVar(..) | T::Unknown { .. }, _) => true,
        (T::Int { .. }, T::Int { .. })
        | (T::String { .. }, T::String { .. })
        | (T::Bool { .. }, T::Bool { .. })
        | (T::Float { .. }, T::Float { .. })
        | (T::Null { .. }, T::Null { .. })
        | (T::Bigint { .. }, T::Bigint { .. })
        | (T::Uint8Array { .. }, T::Uint8Array { .. }) => true,
        (T::Media(wire, _), T::Media(expected, _)) => {
            *expected == baml_type::MediaKind::Generic || wire == expected
        }
        // Distinct primitives: a positive mismatch.
        (
            T::Int { .. }
            | T::String { .. }
            | T::Bool { .. }
            | T::Float { .. }
            | T::Null { .. }
            | T::Bigint { .. }
            | T::Uint8Array { .. },
            T::Int { .. }
            | T::String { .. }
            | T::Bool { .. }
            | T::Float { .. }
            | T::Null { .. }
            | T::Bigint { .. }
            | T::Uint8Array { .. },
        ) => false,
        (T::List(w, _), T::List(e, _)) => runtime_ty_compatible(w, e),
        (
            T::Map {
                key: wk, value: wv, ..
            },
            T::Map {
                key: ek, value: ev, ..
            },
        ) => runtime_ty_compatible(wk, ek) && runtime_ty_compatible(wv, ev),
        (T::Class(wn, wa, _), T::Class(en, ea, _)) => {
            wn == en && class_type_args_compatible(wa, ea)
        }
        // Everything else (unions, enums, aliases, opaque, mixed kinds) is
        // treated leniently as compatible.
        _ => true,
    }
}

fn float_literal_matches(value: f64, source: &str) -> bool {
    source
        .parse::<f64>()
        .is_ok_and(|expected| value.to_bits() == expected.to_bits())
}

/// Structurally check a generic call's argument against its now-concrete
/// expected parameter type (Gate B), returning a human-readable mismatch
/// `detail` on a positive failure (the caller frames it with the function name
/// and argument position).
///
/// By the time this runs, Gate A has guaranteed every `TypeVar` is bound, so
/// `expected` is a concrete type and the question is simply "does this value
/// inhabit it?".
///
/// Three kinds of value:
/// - **`Instance`** keeps the stronger comparison: its wire-supplied `type_args`
///   carry generic information the signature can check (a `GenericBox<string>`
///   arriving at a `GenericBox<int>` param is rejected), and an instance
///   smuggling an unbound `TypeVar` in its wire args is rejected (the wire must
///   be fully bound). `caller_specified_types` selects the mode: in explicit /
///   partial mode (`true`) the wire must be fully bound, so an *unbound* instance
///   (empty wire args) is rejected; in pure-inference mode (`false`) BAML has
///   already recovered such an instance's args from its field values (03b G1),
///   so the missing wire args are admitted.
/// - **plain host data** (scalars, lists, maps, enum variants) is checked with
///   the shared `value_satisfies_ty` shape match (via `validate_host_return`) —
///   the same strict, recursive check the *return* path uses. This is the seam
///   the pre-inference version skipped entirely, admitting e.g. a `string`
///   argument into an `int` slot when the binding was caller-specified (03b C4).
///   The shared check is lenient exactly where inference is (it accepts any value
///   against `rust_type`/`unknown`, matches a value against any union arm, and
///   treats an empty container's element position vacuously), so every value
///   whose synthesized type produced a binding still passes.
/// - **opaque / engine-minted typed carriers** (a typed heap handle such as a
///   `Stream` receiver, a host callable, a reflected type / media / collector /
///   prompt, a host-only value, a raw handle) stay lenient — they are either
///   already typed by the engine or ride opaquely through the VM, so a value-shape
///   check isn't meaningful. This matches the pre-inference behavior for every
///   non-`Instance` value; only plain host data is newly checked.
pub(crate) fn check_generic_arg(
    value: &BexExternalValue,
    expected: &RuntimeTy,
    caller_specified_types: bool,
) -> Result<(), String> {
    match value {
        BexExternalValue::Instance { type_args, .. } => {
            if let Some(name) = type_args.iter().find_map(first_unbound_type_var) {
                return Err(format!(
                    "is a generic instance carrying an unbound type variable `{name}`; the host \
                     must send fully-bound generic instances"
                ));
            }
            // Pure-inference mode: an *unbound* generic instance (no wire args)
            // was already reconstructed from its fields and consumed by inference
            // (G1). Admit it rather than demanding wire args inference didn't need.
            if !caller_specified_types && type_args.is_empty() {
                return Ok(());
            }
            // Compare against the expected type, peeling optional/union wrappers
            // so an instance bound for a `T?`/`T | ...` slot still validates
            // against the class member. Lenient where the expected type isn't a
            // concrete class.
            if expected_admits_instance(value, expected) {
                Ok(())
            } else {
                Err(format!(
                    "has runtime type `{}`, which doesn't match the expected type `{expected}`",
                    value.type_name(),
                ))
            }
        }

        // Plain host data — strict structural shape check (the 03b C4 fix).
        BexExternalValue::Null
        | BexExternalValue::Int(_)
        | BexExternalValue::Bigint(_)
        | BexExternalValue::Float(_)
        | BexExternalValue::Bool(_)
        | BexExternalValue::String(_)
        | BexExternalValue::Uint8Array(_)
        | BexExternalValue::Array { .. }
        | BexExternalValue::Map { .. }
        | BexExternalValue::Variant { .. } => {
            bex_external_types::validate_host_return(value, expected).map_err(|_| {
                format!(
                    "has type `{}`, which doesn't match the expected type `{expected}`",
                    value.type_name(),
                )
            })
        }

        // The FFI union wrapper: peel and re-check the carried value, so an opaque
        // inner stays lenient rather than being shape-checked.
        BexExternalValue::Union { value: inner, .. } => {
            check_generic_arg(inner, expected, caller_specified_types)
        }

        // Opaque / engine-minted typed carriers: lenient (see the doc above).
        BexExternalValue::Adt(_)
        | BexExternalValue::RustData(_)
        | BexExternalValue::FunctionRef { .. }
        | BexExternalValue::Handle(_)
        | BexExternalValue::HostValue(_) => Ok(()),
    }
}

/// Whether an `Instance` value is admitted by `expected`, peeling `Union`
/// (covers `Optional`, which lowers to `T | null`) so a class instance matches
/// its union/optional member. Defers the actual class+args comparison to
/// [`value_matches_type`].
fn expected_admits_instance(value: &BexExternalValue, expected: &RuntimeTy) -> bool {
    match expected {
        RuntimeTy::Union(members, _) => members.iter().any(|m| expected_admits_instance(value, m)),
        // Only a `Class` slot can definitively reject an instance; against any
        // other expected shape (TypeVar/unknown/opaque) stay lenient.
        RuntimeTy::Class(..) => value_matches_type(value, expected),
        _ => true,
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
        expected: &RuntimeTy,
    ) -> Result<(), String> {
        // `InboundValue.value_type` is carried transiently as an annotated
        // external union. Coerce its structural payload first so an anonymous
        // class gains its nominal identity/type args and a class-shaped media
        // shell becomes the underlying media ADT. The shared boundary guard
        // has already checked that the annotation is assignable to `expected`;
        // this pass verifies that the payload actually inhabits it and then
        // applies the resolved class schema recursively.
        if matches!(
            value,
            BexExternalValue::Union { metadata, .. }
                if metadata.is_inbound_type_annotation
        ) {
            let coerced = self
                .coerce_inbound_arg(value.clone(), expected)
                .map_err(|error| error.to_string())?;
            return match &coerced {
                BexExternalValue::Union {
                    value: inner,
                    metadata,
                } if metadata.is_inbound_type_annotation => {
                    self.validate_host_return_schema(inner, expected)
                }
                other => self.validate_host_return_schema(other, expected),
            };
        }

        match expected {
            // `unknown` / opaque-any: accept (defensive — concrete at the FFI
            // boundary).
            RuntimeTy::Unknown { .. } => Ok(()),

            // Union (including nullable `T | null`): must satisfy at least one
            // member (schema-aware).
            RuntimeTy::Union(members, _) => {
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

            RuntimeTy::List(inner, _) => match value {
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

            RuntimeTy::Map { value: v_ty, .. } => match value {
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
            RuntimeTy::Class(tn, expected_args, _) => match value {
                BexExternalValue::Instance {
                    class_name, fields, ..
                } => {
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
                            // Compare in the wire's spelling: the host supplied
                            // `expected_args` there. A field whose template names
                            // a declaration the host cannot name is not
                            // host-validatable — materialization reports it.
                            let Ok(template) = class_field
                                .field_template
                                .try_map_heads(&mut bex_vm_types::TypeHead::to_name)
                            else {
                                continue;
                            };
                            let field_ty = template.substitute_symbolic(expected_args);
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
            RuntimeTy::Enum(tn, _) => match value {
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

            RuntimeTy::EnumVariant(tn, expected_variant, _) => match value {
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
                    if variant_name != expected_variant.as_str() {
                        return Err(format!(
                            "host callable returned enum variant `{enum_name}.{variant_name}` \
                             where `{tn}.{expected_variant}` was declared",
                        ));
                    }
                    Ok(())
                }
                other => Err(format!(
                    "host callable returned `{}` where enum variant \
                     `{tn}.{expected_variant}` was declared",
                    other.type_name(),
                )),
            },

            // Function-typed positions are materialized with their declared
            // type context into `Object::HostClosure`, including inside
            // containers and class fields. Keep this validation strict so an
            // opaque host-value carrier cannot masquerade as a callable.
            RuntimeTy::Function { .. } => {
                bex_external_types::validate_host_return(value, expected).map_err(|e| e.to_string())
            }

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
fn resolve_effective_type(value: Value, declared_type: &RuntimeTy) -> &RuntimeTy {
    match declared_type {
        RuntimeTy::Union(members, _) => find_matching_union_member(value, members)
            .unwrap_or_else(|| members.first().unwrap_or(declared_type)),
        _ => declared_type,
    }
}

/// Select a declared interface arm using nominal facts from the live program.
/// The ordinary VM selector runs first so exact literals, variants, containers,
/// and classes retain precedence over interfaces they may implement.
fn find_implemented_interface_union_member<'a>(
    value: Value,
    declared_type: &'a RuntimeTy,
    vm: &BexVm,
    permit: PermitProof<'_>,
) -> Result<Option<&'a RuntimeTy>, EngineError> {
    let RuntimeTy::Union(members, _) = declared_type else {
        return Ok(None);
    };
    if members.iter().any(RuntimeTy::is_null)
        && members.iter().filter(|member| !member.is_null()).count() == 1
    {
        return Ok(None);
    }
    if find_matching_union_member(value, members).is_some() {
        return Ok(None);
    }

    let Some(runtime_ty) = crate::value_runtime_baml_ty(value, permit) else {
        return Ok(None);
    };
    let mut matching: Vec<&'a RuntimeTy> = Vec::new();
    for member in members.iter().filter(|member| {
        matches!(member, RuntimeTy::Interface(..))
            && crate::conversion::anchor_wire_ty(vm, member).is_ok_and(|anchored| {
                baml_type::normalize::is_subtype(runtime_ty.as_ty(), anchored.as_ty(), vm)
            })
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
fn find_matching_union_member(value: Value, members: &[RuntimeTy]) -> Option<&RuntimeTy> {
    let direct = match value.kind() {
        ValueKind::OmittedArg => None,
        ValueKind::Null => members.iter().find(|m| matches!(m, RuntimeTy::Null { .. })),
        ValueKind::Int(value) => members
            .iter()
            .find(|member| {
                matches!(member, RuntimeTy::Literal(Literal::Int(expected), _, _) if value == *expected)
            })
            .or_else(|| {
                members
                    .iter()
                    .find(|member| matches!(member, RuntimeTy::Int { .. }))
            }),
        ValueKind::Bool(value) => members
            .iter()
            .find(|member| {
                matches!(member, RuntimeTy::Literal(Literal::Bool(expected), _, _) if value == *expected)
            })
            .or_else(|| {
                members
                    .iter()
                    .find(|member| matches!(member, RuntimeTy::Bool { .. }))
            }),
        ValueKind::Object(ptr) => {
            let obj = unsafe { ptr.get() };
            match obj {
                Object::Float(value) => members
                    .iter()
                    .find(|member| {
                        matches!(member, RuntimeTy::Literal(Literal::Float(expected), _, _)
                            if float_literal_matches(*value, expected))
                    })
                    .or_else(|| {
                        members
                            .iter()
                            .find(|member| matches!(member, RuntimeTy::Float { .. }))
                    }),
                Object::String(value) => members
                    .iter()
                    .find(|member| {
                        matches!(member, RuntimeTy::Literal(Literal::String(expected), _, _)
                            if value.as_str() == expected)
                    })
                    .or_else(|| {
                        members
                            .iter()
                            .find(|member| matches!(member, RuntimeTy::String { .. }))
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
                        members.iter().find(|m| {
                            matches!(m, RuntimeTy::Class(tn, expected_args, _)
                                if (class.name.declared() == Some(tn)
                                    || (class.type_tag.is_dynamic()
                                        && class.name.overlay_name() == *tn))
                                && (expected_args.is_empty()
                                    || (expected_args.len() == inst.class_type_args.len()
                                        && expected_args
                                            .iter()
                                            .zip(inst.class_type_args.iter())
                                            .all(|(e, a)| {
                                                overlay_wire_ty(a.as_runtime_ty())
                                                    .is_ok_and(|a| *e == a)
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
                        members
                            .iter()
                            .find(|m| {
                                matches!(
                                    (m, actual_variant),
                                    (RuntimeTy::EnumVariant(tn, expected, _), Some(actual))
                                        if enm.name.declared() == Some(tn)
                                            && expected.as_str() == actual
                                )
                            })
                            .or_else(|| {
                                members.iter().find(
                                    |m| matches!(m, RuntimeTy::Enum(tn, _) if enm.name.declared() == Some(tn)),
                                )
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
                    let actual_element = overlay_wire_ty(array.element_ty.as_runtime_ty()).ok()?;
                    members.iter().find(|member| {
                        matches!(member, RuntimeTy::List(expected, _)
                            if runtime_ty_structurally_equal(&actual_element, expected))
                    })
                }
                Object::Map(map) => {
                    let actual_key = overlay_wire_ty(map.key_ty.as_runtime_ty()).ok()?;
                    let actual_value = overlay_wire_ty(map.value_ty.as_runtime_ty()).ok()?;
                    members.iter().find(|member| {
                        matches!(member, RuntimeTy::Map { key, value, .. }
                            if runtime_ty_structurally_equal(&actual_key, key)
                                && runtime_ty_structurally_equal(&actual_value, value))
                    })
                }
                Object::Uint8Array(_) => members
                    .iter()
                    .find(|m| matches!(m, RuntimeTy::Uint8Array { .. })),
                Object::Bigint(value) => members
                    .iter()
                    .find(|member| {
                        matches!(member, RuntimeTy::Literal(Literal::Bigint(expected), _, _)
                            if value.as_ref() == expected)
                    })
                    .or_else(|| {
                        members
                            .iter()
                            .find(|member| matches!(member, RuntimeTy::Bigint { .. }))
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
                | Object::UnscheduledFuture(_)
                | Object::RustData(_)
                | Object::Collector(_)
                | Object::Type(_) => None,
                #[cfg(feature = "heap_debug")]
                Object::Sentinel(_) => None,
            }
        }
    };
    direct.or_else(|| {
        members.iter().find(|member| {
            matches!(member, RuntimeTy::Union(nested, _)
                if find_matching_union_member(value, nested).is_some())
        })
    })
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
                        element_type: bex_external_types::RuntimeTy::Null {
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
                        key_type: bex_external_types::RuntimeTy::String {
                            attr: baml_type::TyAttr::default(),
                        },
                        value_type: bex_external_types::RuntimeTy::Null {
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

                    BexExternalValue::Instance {
                        class_name,
                        type_args: instance
                            .class_type_args
                            .iter()
                            .filter_map(|arg| {
                                overlay_wire_ty(&bex_vm_types::RuntimeTy::from(arg)).ok()
                            })
                            .collect(),
                        fields,
                    }
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
/// 1. **Sparse type annotations:** an inbound node may carry an exact
///    `value_type`. Validate it against this contextual type, then use it as
///    the recursive context for the node's payload.
/// 2. **Containers / classes:** thread the contextual element, map, and class
///    field types recursively instead of requiring type metadata on each
///    payload message.
/// 3. **Class / enum naming:** bind shape-only class/enum payloads to the
///    engine-registered FQN supplied by the contextual type.
/// 4. **Numeric / optional / union coercion:** see `coerce_numeric_to_declared_type`.
impl BexEngine {
    /// Apply the declared type to an inbound value while retaining access to
    /// this program's recursive-alias definitions. Alias references are
    /// nominal in `RuntimeTy`, but their payload shape must still participate
    /// in sparse inbound matching (for example `RecList | null` receiving an
    /// unannotated integer leaf).
    pub(crate) fn coerce_inbound_arg(
        &self,
        value: BexExternalValue,
        ty: &RuntimeTy,
    ) -> Result<BexExternalValue, EngineError> {
        coerce_arg_to_declared_type_with_aliases(
            value,
            ty,
            &self.inbound_alias_view,
            &self.inbound_class_view,
            crate::inbound_config::inbound_union_ambiguity_policy(),
        )
    }
}

/// A class definition as the inbound matcher reads it: name-headed, because it
/// is matched against wire values that carry only names.
#[derive(Clone, Debug)]
pub(crate) struct WireClassDefinition {
    pub fields: Vec<WireClassFieldDefinition>,
}

#[derive(Clone, Debug)]
pub(crate) struct WireClassFieldDefinition {
    pub name: String,
    pub field_type: RuntimeTy,
    /// The serialized key a payload may use instead of `name`.
    pub alias: Option<String>,
    pub skip: bool,
}

/// Project the lane's definition tables into the name-headed views the inbound
/// matcher needs.
///
/// The one place the two spellings meet, and it is a genuine boundary: an
/// inbound value carries a name and nothing else, so matching it can only ever
/// be name-based. Identity still governs everywhere it can — the tables
/// themselves, rendering, and rooting — and an anonymous declaration simply
/// does not appear here, because it has no name a wire value could carry.
///
/// A class projects **all of its fields or none of it**. Admitting a class
/// whose unprojectable fields were dropped would be worse than omitting it:
/// the matcher decides "does this payload fit" by walking `fields`, so a
/// missing field silently stops being required and a payload lacking it
/// matches anyway. Omitting the class instead lands on the shape-only path,
/// which is degraded but does not claim a shape the class does not have.
pub(crate) fn wire_definition_views(
    aliases: &indexmap::IndexMap<::sys_types::DefKey, ::sys_types::SapTy>,
    classes: &indexmap::IndexMap<::sys_types::DefKey, sys_types::ClassDefinition>,
) -> (
    indexmap::IndexMap<baml_type::TypeName, RuntimeTy>,
    indexmap::IndexMap<baml_type::TypeName, WireClassDefinition>,
) {
    let to_wire = |ty: &::sys_types::SapTy| -> Option<RuntimeTy> {
        ty.clone()
            .try_map_heads(&mut |head: &::sys_types::DefKey| head.declared().cloned().ok_or(()))
            .ok()
    };
    let alias_view = aliases
        .iter()
        .filter_map(|(head, ty)| Some((head.declared()?.clone(), to_wire(ty)?)))
        .collect();
    let class_view = classes
        .iter()
        .filter_map(|(head, def)| {
            // All fields or no class — see the doc comment.
            let fields = def
                .fields
                .iter()
                .map(|f| {
                    Some(WireClassFieldDefinition {
                        name: f.name.clone(),
                        field_type: to_wire(&f.field_type)?,
                        alias: f.alias.clone(),
                        skip: f.skip,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            Some((head.declared()?.clone(), WireClassDefinition { fields }))
        })
        .collect();
    (alias_view, class_view)
}

fn runtime_ty_assignable_with_aliases(
    actual: &RuntimeTy,
    expected: &RuntimeTy,
    aliases: &indexmap::IndexMap<baml_type::TypeName, RuntimeTy>,
) -> bool {
    if runtime_ty_structurally_equal(actual, expected) {
        return true;
    }
    if let RuntimeTy::TypeAlias(name, _) = actual
        && let Some(expanded) = aliases.get(name)
    {
        return runtime_ty_assignable_with_aliases(expanded, expected, aliases);
    }
    if let RuntimeTy::TypeAlias(name, _) = expected
        && let Some(expanded) = aliases.get(name)
    {
        return runtime_ty_assignable_with_aliases(actual, expanded, aliases);
    }
    #[expect(
        deprecated,
        reason = "inbound annotation matching has aliases but no VM-backed type facts"
    )]
    baml_type::normalize::is_subtype(
        actual.as_ty(),
        expected.as_ty(),
        &baml_type::normalize::NoFacts,
    )
}

/// An erased generic host object can know its nominal class without knowing
/// every concrete argument (Python's unparameterized Pydantic generic and
/// TypeScript's erased generic are the common cases). With the redundant
/// `InboundClassValue.class_ty` channel removed, a class-shaped `value_type`
/// with omitted/wildcard args carries that nominal discriminator. It may refine
/// to one contextual class, but must not silently select between multiple
/// concrete instantiations in a union.
fn class_annotation_can_refine(annotation: &RuntimeTy, contextual: &RuntimeTy) -> bool {
    let (
        RuntimeTy::Class(annotation_name, annotation_args, _),
        RuntimeTy::Class(contextual_name, contextual_args, _),
    ) = (annotation, contextual)
    else {
        return false;
    };
    annotation_name == contextual_name
        && (annotation_args.is_empty()
            || (annotation_args.len() == contextual_args.len()
                && annotation_args
                    .iter()
                    .zip(contextual_args)
                    .all(|(annotation, contextual)| runtime_ty_compatible(annotation, contextual))))
}

/// The generated host SDKs expose primitive media through the corresponding
/// handle-backed stdlib wrapper class. Some bridges therefore annotate the
/// class-shaped `_data` payload with that wrapper's exact FQN even when the
/// contextual BAML slot is the primitive `image`/`audio`/`video`/`pdf` type.
///
/// Keep this conversion contextual: the same wrapper remains an ordinary
/// nominal class when passed as the receiver of a `baml.media.Image` method.
fn stdlib_media_wrapper_kind(annotation: &RuntimeTy) -> Option<baml_type::MediaKind> {
    let RuntimeTy::Class(name, args, _) = annotation else {
        return None;
    };
    if !args.is_empty() {
        return None;
    }
    baml_type::MediaKind::from_wrapper_class_name(&name.render_dotted(false))
}

fn resolve_runtime_alias<'a>(
    mut ty: &'a RuntimeTy,
    aliases: &'a indexmap::IndexMap<baml_type::TypeName, RuntimeTy>,
) -> Option<&'a RuntimeTy> {
    let mut visited = std::collections::HashSet::new();
    while let RuntimeTy::TypeAlias(name, _) = ty {
        if !visited.insert(name) {
            return None;
        }
        ty = aliases.get(name)?;
    }
    Some(ty)
}

fn stdlib_media_wrapper_matches(
    annotation: &RuntimeTy,
    contextual: &RuntimeTy,
    aliases: &indexmap::IndexMap<baml_type::TypeName, RuntimeTy>,
    allow_generic: bool,
) -> bool {
    let Some(wrapper_kind) = stdlib_media_wrapper_kind(annotation) else {
        return false;
    };
    let Some(contextual) = resolve_runtime_alias(contextual, aliases) else {
        return false;
    };
    let RuntimeTy::Media(contextual_kind, _) = contextual else {
        return false;
    };
    wrapper_kind == *contextual_kind
        || (allow_generic && *contextual_kind == baml_type::MediaKind::Generic)
}

/// Return the concrete primitive media type against which a wrapper's `_data`
/// payload must be checked. In a generic `media` context the wrapper still
/// promises one exact kind, so validate that promise before widening the
/// resulting carrier back to the contextual type.
fn stdlib_media_wrapper_payload_type(
    annotation: &RuntimeTy,
    contextual: &RuntimeTy,
    aliases: &indexmap::IndexMap<baml_type::TypeName, RuntimeTy>,
) -> Option<RuntimeTy> {
    let wrapper_kind = stdlib_media_wrapper_kind(annotation)?;
    let RuntimeTy::Media(contextual_kind, attr) = resolve_runtime_alias(contextual, aliases)?
    else {
        return None;
    };
    (*contextual_kind == wrapper_kind || *contextual_kind == baml_type::MediaKind::Generic)
        .then(|| RuntimeTy::Media(wrapper_kind, attr.clone()))
}

fn refine_class_annotation_args(
    annotation: &[RuntimeTy],
    contextual: &[RuntimeTy],
) -> Vec<RuntimeTy> {
    if annotation.is_empty() {
        return contextual.to_vec();
    }
    annotation
        .iter()
        .zip(contextual)
        .map(|(annotation, contextual)| {
            if is_wildcard_ty(annotation) {
                contextual.clone()
            } else {
                annotation.clone()
            }
        })
        .collect()
}

/// Whether a sparse inbound annotation names an enclosing union rather than
/// one exact selected type. Follow aliases here because the protobuf decoder
/// cannot see program-local alias definitions.
fn inbound_annotation_resolves_to_root_union(
    annotation: &RuntimeTy,
    aliases: &indexmap::IndexMap<baml_type::TypeName, RuntimeTy>,
) -> bool {
    let mut current = annotation;
    let mut visited = std::collections::HashSet::new();
    loop {
        match current {
            RuntimeTy::Union(..) => return true,
            RuntimeTy::TypeAlias(name, _) if visited.insert(name.clone()) => {
                let Some(expanded) = aliases.get(name) else {
                    return false;
                };
                current = expanded;
            }
            _ => return false,
        }
    }
}

#[cfg(test)]
pub(crate) fn coerce_arg_to_declared_type(
    value: BexExternalValue,
    ty: &RuntimeTy,
) -> Result<BexExternalValue, EngineError> {
    coerce_arg_to_declared_type_with_aliases(
        value,
        ty,
        &indexmap::IndexMap::new(),
        &indexmap::IndexMap::new(),
        crate::InboundUnionAmbiguityPolicy::Reject,
    )
}

#[cfg(test)]
fn coerce_arg_to_declared_type_with_policy(
    value: BexExternalValue,
    ty: &RuntimeTy,
    policy: crate::InboundUnionAmbiguityPolicy,
) -> Result<BexExternalValue, EngineError> {
    coerce_arg_to_declared_type_with_aliases(
        value,
        ty,
        &indexmap::IndexMap::new(),
        &indexmap::IndexMap::new(),
        policy,
    )
}

fn coerce_arg_to_declared_type_with_aliases(
    value: BexExternalValue,
    ty: &RuntimeTy,
    aliases: &indexmap::IndexMap<baml_type::TypeName, RuntimeTy>,
    classes: &indexmap::IndexMap<baml_type::TypeName, WireClassDefinition>,
    ambiguity_policy: crate::InboundUnionAmbiguityPolicy,
) -> Result<BexExternalValue, EngineError> {
    // Defense in depth for internally constructed values and aliases. Wire
    // inputs reject a root union in `inbound_to_external`, but an alias can be
    // recognized as union-shaped only after the engine has loaded its body.
    if let BexExternalValue::Union { metadata, .. } = &value
        && metadata.is_inbound_type_annotation
        && inbound_annotation_resolves_to_root_union(&metadata.selected_option, aliases)
    {
        return Err(EngineError::TypeMismatch {
            message: "inbound value_type must identify one exact selected type, not a root union or optional"
                .to_string(),
        });
    }

    // Recursive aliases are nominal in the public type but transparent to the
    // payload matcher. Expanding one layer here makes its body the effective
    // recursive context; recursive references consume payload structure before
    // returning here again, so productive aliases terminate with the value.
    if let RuntimeTy::TypeAlias(name, _) = ty
        && !is_canonical_json_alias(name)
        && let Some(expanded) = aliases.get(name)
    {
        return coerce_arg_to_declared_type_with_aliases(
            value,
            expanded,
            aliases,
            classes,
            ambiguity_policy,
        );
    }

    match (value, ty) {
        (BexExternalValue::Union { value, metadata }, declared @ RuntimeTy::Union(members, _)) => {
            let value_type = &metadata.selected_option;
            let selected_type = members
                .iter()
                .find(|member| selected_arm_equal(member, value_type))
                .or_else(|| {
                    members.iter().find(|member| {
                        runtime_ty_assignable_with_aliases(value_type, member, aliases)
                    })
                })
                // Prefer the exact primitive kind before the generic `media`
                // supertype, independent of union declaration order.
                .or_else(|| {
                    members.iter().find(|member| {
                        stdlib_media_wrapper_matches(value_type, member, aliases, false)
                    })
                })
                .or_else(|| {
                    members.iter().find(|member| {
                        stdlib_media_wrapper_matches(value_type, member, aliases, true)
                    })
                })
                .cloned();
            let selected_type = match selected_type {
                Some(selected) => selected,
                None => {
                    let refinements = members
                        .iter()
                        .filter(|member| class_annotation_can_refine(value_type, member))
                        .collect::<Vec<_>>();
                    match refinements.as_slice() {
                        [selected] => (*selected).clone(),
                        [] => {
                            return Err(EngineError::TypeMismatch {
                                message: format!(
                                    "host value type `{value_type}` is not a member of declared union `{declared}`"
                                ),
                            });
                        }
                        _ => {
                            return Err(EngineError::TypeMismatch {
                                message: format!(
                                    "host class annotation `{value_type}` does not identify one member of declared union `{declared}`; provide concrete generic arguments"
                                ),
                            });
                        }
                    }
                }
            };
            let annotation_coerced = coerce_arg_to_declared_type_with_aliases(
                *value,
                value_type,
                aliases,
                classes,
                ambiguity_policy,
            )?;
            if !value_matches_type_with_definitions(
                &annotation_coerced,
                value_type,
                aliases,
                classes,
            ) {
                return Err(EngineError::TypeMismatch {
                    message: format!(
                        "host-typed payload `{}` does not inhabit `{value_type}`",
                        annotation_coerced.type_name()
                    ),
                });
            }
            let media_payload_type =
                stdlib_media_wrapper_payload_type(value_type, &selected_type, aliases);
            let payload_type = media_payload_type.as_ref().unwrap_or(&selected_type);
            let coerced = coerce_arg_to_declared_type_with_aliases(
                annotation_coerced,
                payload_type,
                aliases,
                classes,
                ambiguity_policy,
            )?;
            if !value_matches_type_with_definitions(&coerced, &selected_type, aliases, classes) {
                return Err(EngineError::TypeMismatch {
                    message: format!(
                        "host-typed payload `{}` does not inhabit contextual type `{selected_type}`",
                        coerced.type_name()
                    ),
                });
            }
            Ok(BexExternalValue::Union {
                value: Box::new(coerced),
                metadata: UnionMetadata::new(declared.clone(), selected_type),
            })
        }
        (BexExternalValue::Union { value, metadata }, declared)
            if metadata.is_inbound_type_annotation =>
        {
            let value_type = &metadata.selected_option;
            let media_payload_type =
                stdlib_media_wrapper_payload_type(value_type, declared, aliases);
            let effective_type = if media_payload_type.is_some() {
                // Validate the payload against its nominal annotation below,
                // then unwrap its sole `_data` field using the primitive
                // contextual type.
                declared
            } else if runtime_ty_assignable_with_aliases(value_type, declared, aliases) {
                value_type
            } else if class_annotation_can_refine(value_type, declared) {
                declared
            } else {
                return Err(EngineError::TypeMismatch {
                    message: format!(
                        "host value type `{value_type}` is not assignable to declared type `{declared}`"
                    ),
                });
            };
            let annotation_coerced = coerce_arg_to_declared_type_with_aliases(
                *value,
                value_type,
                aliases,
                classes,
                ambiguity_policy,
            )?;
            if !value_matches_type_with_definitions(
                &annotation_coerced,
                value_type,
                aliases,
                classes,
            ) {
                return Err(EngineError::TypeMismatch {
                    message: format!(
                        "host-typed payload `{}` does not inhabit `{value_type}`",
                        annotation_coerced.type_name()
                    ),
                });
            }
            let payload_type = media_payload_type.as_ref().unwrap_or(effective_type);
            let coerced = coerce_arg_to_declared_type_with_aliases(
                annotation_coerced,
                payload_type,
                aliases,
                classes,
                ambiguity_policy,
            )?;
            Ok(BexExternalValue::typed(coerced, effective_type.clone()))
        }
        // A declared `baml.json.json` slot receiving a container. Untyped
        // bridge encoders (Go `baml.JSON`, Python dicts/lists, `--json-args`)
        // send containers without a `value_type` annotation, which the wire
        // decoder defaults to a scalar-union element type. Left as-is, the
        // materialized VM map/list would carry that synthesized type and fail
        // runtime type tests such as `match (j) { let m: map<string, json> =>
        // ... }` — diverging from BAML-born `baml.json.parse` values, whose
        // containers carry the `json` alias itself (`serde_to_value`).
        // Re-annotate the container tree with the declared alias so both
        // materialize identically. Values outside the JSON algebra are left
        // unchanged for the standard validation paths to reject.
        (value, declared @ RuntimeTy::TypeAlias(name, _))
            if is_canonical_json_alias(name)
                && matches!(
                    value,
                    BexExternalValue::Array { .. } | BexExternalValue::Map { .. }
                )
                && value_satisfies_json(&value) =>
        {
            Ok(annotate_json_container_types(value, declared))
        }
        (BexExternalValue::Array { items, .. }, RuntimeTy::List(expected_element, _)) => {
            Ok(BexExternalValue::Array {
                element_type: expected_element.as_ref().clone(),
                items: items
                    .into_iter()
                    .map(|item| {
                        coerce_arg_to_declared_type_with_aliases(
                            item,
                            expected_element,
                            aliases,
                            classes,
                            ambiguity_policy,
                        )
                    })
                    .collect::<Result<_, _>>()?,
            })
        }
        (
            BexExternalValue::Map { entries, .. },
            RuntimeTy::Map {
                key,
                value: expected_value,
                ..
            },
        ) => Ok(BexExternalValue::Map {
            key_type: key.as_ref().clone(),
            value_type: expected_value.as_ref().clone(),
            entries: entries
                .into_iter()
                .map(|(name, value)| {
                    coerce_arg_to_declared_type_with_aliases(
                        value,
                        expected_value,
                        aliases,
                        classes,
                        ambiguity_policy,
                    )
                    .map(|value| (name, value))
                })
                .collect::<Result<_, _>>()?,
        }),
        // Python media wrappers use a class-shaped payload solely to carry the
        // `_data` handle, but their sparse annotation is the exact primitive
        // media type (`image`/`audio`/`video`/`pdf`). Unwrap that implementation
        // shell before validating/materializing the annotated node.
        (BexExternalValue::Instance { mut fields, .. }, media_ty @ RuntimeTy::Media(..)) => {
            let data = fields
                .shift_remove(bex_external_types::MEDIA_WRAPPER_DATA_FIELD)
                .ok_or_else(|| EngineError::TypeMismatch {
                    message: format!(
                        "host media payload for `{media_ty}` is missing its `_data` handle"
                    ),
                })?;
            if !fields.is_empty() {
                return Err(EngineError::TypeMismatch {
                    message: format!(
                        "host media payload for `{media_ty}` contains unexpected fields"
                    ),
                });
            }
            let coerced = coerce_arg_to_declared_type_with_aliases(
                data,
                media_ty,
                aliases,
                classes,
                ambiguity_policy,
            )?;
            if !value_matches_type_with_definitions(&coerced, media_ty, aliases, classes) {
                return Err(EngineError::TypeMismatch {
                    message: format!(
                        "host media payload `{}` does not inhabit contextual type `{media_ty}`",
                        coerced.type_name()
                    ),
                });
            }
            Ok(coerced)
        }
        // ── Class / enum naming (incoming only) ──────────────────────────
        (BexExternalValue::Map { entries, .. }, RuntimeTy::Class(type_name, class_args, _)) => {
            Ok(BexExternalValue::Instance {
                class_name: type_name.to_string(),
                type_args: class_args.to_vec(),
                fields: entries,
            })
        }
        (
            BexExternalValue::Instance {
                fields, type_args, ..
            },
            RuntimeTy::Class(type_name, class_args, _),
        ) => {
            // The contextual type is authoritative for nominal identity and
            // generic arguments. A sparse node annotation, when present, has
            // already been checked against that context above.
            if !type_args.is_empty() && !class_type_args_compatible(&type_args, class_args) {
                return Err(EngineError::TypeMismatch {
                    message: format!(
                        "host class type arguments `{type_args:?}` are incompatible with declared class `{type_name}` arguments `{class_args:?}`"
                    ),
                });
            }
            Ok(BexExternalValue::Instance {
                class_name: type_name.to_string(),
                type_args: refine_class_annotation_args(&type_args, class_args),
                fields,
            })
        }
        (BexExternalValue::Variant { variant_name, .. }, RuntimeTy::Enum(type_name, _)) => {
            Ok(BexExternalValue::Variant {
                enum_name: type_name.to_string(),
                variant_name,
            })
        }

        // An unannotated node in a union context is selected from payload
        // shape. Typed bridges reject ambiguous payloads unless they carry
        // `value_type`; dynamic bridges use their registered default policy.
        // An annotation was handled by the union-carrier arm above.
        (value, declared @ RuntimeTy::Union(members, _)) => {
            let value = coerce_numeric_to_declared_type(value, declared)?;
            let selected = find_unannotated_inbound_member_with_aliases(
                &value,
                members,
                aliases,
                classes,
                ambiguity_policy,
            )?;
            let coerced = coerce_arg_to_declared_type_with_aliases(
                value,
                &selected,
                aliases,
                classes,
                ambiguity_policy,
            )?;
            Ok(BexExternalValue::Union {
                value: Box::new(coerced),
                metadata: UnionMetadata::new(declared.clone(), selected),
            })
        }

        // ── Numeric / optional / union ───────────────────────────────────
        (v, ty) => coerce_numeric_to_declared_type(v, ty),
    }
}

/// Rewrite every container annotation in a JSON value tree to the `json`
/// alias itself: lists become `json[]`, maps become `map<string, json>`.
/// Scalars carry no annotation and pass through; sparse inbound leaf
/// annotations (transient `Union` carriers) are recursed through with their
/// metadata intact. The caller has already proven the tree inhabits the JSON
/// algebra (`value_satisfies_json`).
fn annotate_json_container_types(value: BexExternalValue, json_ty: &RuntimeTy) -> BexExternalValue {
    match value {
        BexExternalValue::Array { items, .. } => BexExternalValue::Array {
            element_type: json_ty.clone(),
            items: items
                .into_iter()
                .map(|item| annotate_json_container_types(item, json_ty))
                .collect(),
        },
        BexExternalValue::Map { entries, .. } => BexExternalValue::Map {
            key_type: RuntimeTy::string(),
            value_type: json_ty.clone(),
            entries: entries
                .into_iter()
                .map(|(key, entry)| (key, annotate_json_container_types(entry, json_ty)))
                .collect(),
        },
        BexExternalValue::Union { value, metadata } => BexExternalValue::Union {
            value: Box::new(annotate_json_container_types(*value, json_ty)),
            metadata,
        },
        scalar => scalar,
    }
}

/// Coerce an **outgoing** return value to match the declared return type.
///
/// Handles int↔bigint conversion at the FFI boundary. These conversions exist
/// **only** at the host boundary — the type system does not relate `int` and
/// `bigint` (concrete types are atomic; see `baml_type::normalize`). `int → bigint` and
/// `int → float` widen unconditionally; `bigint → int` succeeds when the value
/// fits in i64, erroring on overflow rather than silently truncating. Unions
/// delegate to member coercion.
///
/// Class / enum naming is intentionally *not* rewritten here — the
/// engine-side FQN (e.g. `user.lorem.MyLorem`) is the authoritative
/// output and stripping it back to the bare display name would break
/// host-side type lookups.
pub(crate) fn coerce_return_to_declared_type(
    value: BexExternalValue,
    ty: &RuntimeTy,
) -> Result<BexExternalValue, EngineError> {
    coerce_numeric_to_declared_type(value, ty)
}

/// Shared numeric / optional / union coercion used by both arg and return
/// paths.
///
/// These conversions exist only at the FFI boundary. The subtype relation
/// (`baml_type::normalize::is_subtype`) is coercion-free and does **not** widen
/// `int` to `bigint` or `float`; the arms below add those widenings (plus a
/// checked `bigint → int` narrowing) only when crossing the host boundary.
fn coerce_numeric_to_declared_type(
    value: BexExternalValue,
    ty: &RuntimeTy,
) -> Result<BexExternalValue, EngineError> {
    match (value, ty) {
        // Int → Bigint widening (FFI boundary only — `int` is not a subtype of
        // `bigint` in the type system).
        (
            BexExternalValue::Int(i),
            RuntimeTy::Bigint { .. } | RuntimeTy::Literal(Literal::Bigint(_), _, _),
        ) => Ok(BexExternalValue::Bigint(num_bigint::BigInt::from(i))),

        // Bigint → Int narrowing: host-supplied bigint must fit in i64, otherwise
        // there is no safe representation in the `int` slot and we reject the
        // call rather than silently truncate.
        (
            BexExternalValue::Bigint(bi),
            RuntimeTy::Int { .. } | RuntimeTy::Literal(Literal::Int(_), _, _),
        ) => i64::try_from(&bi)
            .map(BexExternalValue::Int)
            .map_err(|_| EngineError::TypeMismatch {
                message: format!("bigint value {bi} does not fit in i64"),
            }),

        // Int → Float widening (FFI boundary only — `int` is not a subtype of
        // `float` in the type system). Hosts whose encoders are value-shaped
        // rather than schema-shaped emit integral numbers as ints (Python `7`,
        // JS `Number.isInteger`), so a `float` slot must accept them; without
        // this arm the int rides through unconverted and a declared `-> float`
        // hands the host back an int.
        #[expect(
            clippy::cast_precision_loss,
            reason = "deliberate host-language `float(int)` semantics — may round above 2^53"
        )]
        (
            BexExternalValue::Int(i),
            RuntimeTy::Float { .. } | RuntimeTy::Literal(Literal::Float(_), _, _),
        ) => Ok(BexExternalValue::Float(i as f64)),

        // Union: delegate to member coercion. A value that already inhabits
        // some member is left alone (an `Int` against `int | float` stays
        // `Int`; `Null` against `int | null` is preserved). Otherwise the
        // first member the value coerces into wins — a host int lands on the
        // `bigint` member of `bigint | null` or the `float` member of
        // `float?`. A member coercion error (bigint → int overflow)
        // propagates rather than silently falling through.
        (v, RuntimeTy::Union(members, _)) => {
            if members.iter().any(|m| value_matches_type(&v, m)) {
                return Ok(v);
            }
            // A host int prefers the lossless target: `bigint` (exact for
            // every i64) beats `float` (rounds above 2^53) regardless of
            // member declaration order. An exact `int` member was already
            // taken by the match check above. The partition is stable, so
            // declaration order still breaks ties within each group.
            let prefer_bigint = matches!(&v, BexExternalValue::Int(_));
            let (bigint_members, other_members): (Vec<&RuntimeTy>, Vec<&RuntimeTy>) =
                members.iter().partition(|m| {
                    prefer_bigint
                        && matches!(
                            m,
                            RuntimeTy::Bigint { .. } | RuntimeTy::Literal(Literal::Bigint(_), _, _)
                        )
                });
            for member in bigint_members.into_iter().chain(other_members) {
                let coerced = coerce_numeric_to_declared_type(v.clone(), member)?;
                if value_matches_type(&coerced, member) {
                    return Ok(coerced);
                }
            }
            Ok(v)
        }

        (v, _) => Ok(v),
    }
}

#[cfg(test)]
mod union_container_selection_tests {
    use std::sync::Arc;

    use baml_builtins2::{MediaContent, MediaValue};
    use baml_type::{
        Freshness, FunctionParamMode, MediaKind, Name, RuntimeFunctionParamTy, TyAttr, TypeName,
    };
    use bex_external_types::{HostValueArc, HostValueKind};
    use bex_heap::{BexHeap, Tlab};
    use bex_vm_types::{
        DeclarationName, EnumVariant, Object, Value,
        types::{Class, Enum},
    };

    use super::*;

    fn list(inner: RuntimeTy) -> RuntimeTy {
        RuntimeTy::List(Box::new(inner), TyAttr::default())
    }

    fn map(value: RuntimeTy) -> RuntimeTy {
        RuntimeTy::Map {
            key: Box::new(RuntimeTy::string()),
            value: Box::new(value),
            attr: TyAttr::default(),
        }
    }

    fn string_literal(value: &str) -> RuntimeTy {
        RuntimeTy::Literal(
            Literal::String(value.to_string()),
            Freshness::Regular,
            TyAttr::default(),
        )
    }

    fn float_literal(value: &str) -> RuntimeTy {
        RuntimeTy::Literal(
            Literal::Float(value.to_string()),
            Freshness::Regular,
            TyAttr::default(),
        )
    }

    fn media_ty(kind: MediaKind) -> RuntimeTy {
        RuntimeTy::Media(kind, TyAttr::default())
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

    fn media_wrapper_ty(kind: MediaKind) -> RuntimeTy {
        let name = match kind {
            MediaKind::Image => "baml.media.Image",
            MediaKind::Audio => "baml.media.Audio",
            MediaKind::Video => "baml.media.Video",
            MediaKind::Pdf => "baml.media.Pdf",
            MediaKind::Generic => panic!("generic media has no stdlib wrapper class"),
        };
        RuntimeTy::Class(
            TypeName::from_dotted_path(name),
            Box::new([]),
            TyAttr::default(),
        )
    }

    fn media_wrapper_value(kind: MediaKind) -> BexExternalValue {
        let RuntimeTy::Class(name, ..) = media_wrapper_ty(kind) else {
            unreachable!()
        };
        BexExternalValue::Instance {
            class_name: name.to_string(),
            type_args: vec![],
            fields: indexmap::IndexMap::from([("_data".to_string(), media_value(kind))]),
        }
    }

    fn callback_ty(param_freshness: Freshness) -> RuntimeTy {
        RuntimeTy::Function {
            params: Box::new([RuntimeFunctionParamTy {
                name: Some(Name::new("status")),
                ty: RuntimeTy::Literal(
                    Literal::String("draft".to_string()),
                    param_freshness,
                    TyAttr::default(),
                ),
                mode: FunctionParamMode::Required,
            }]),
            ret: Box::new(RuntimeTy::int()),
            throws: Box::new(RuntimeTy::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        }
    }

    fn json_ty() -> RuntimeTy {
        RuntimeTy::TypeAlias(
            TypeName::from_dotted_path("baml.json.json"),
            TyAttr::default(),
        )
    }

    #[test]
    fn canonical_json_alias_matches_values_and_selected_union_arms() {
        let json = json_ty();
        let image = media_ty(MediaKind::Image);
        let mut entries = indexmap::IndexMap::new();
        entries.insert("value".to_string(), BexExternalValue::Int(7));
        let object = BexExternalValue::Map {
            key_type: RuntimeTy::string(),
            value_type: RuntimeTy::unknown(),
            entries,
        };
        assert!(value_matches_type(&object, &json));
        assert!(!value_matches_type(
            &BexExternalValue::Float(f64::INFINITY),
            &json
        ));
        let forged = BexExternalValue::union(
            BexExternalValue::String("json-shaped payload".into()),
            [RuntimeTy::bigint(), RuntimeTy::string()],
            RuntimeTy::bigint(),
        );
        assert!(!value_matches_type(&forged, &json));

        let declared = RuntimeTy::union([json.clone(), image]);
        let selected = BexExternalValue::union(
            object,
            [json.clone(), media_ty(MediaKind::Image)],
            json.clone(),
        );
        let coerced = coerce_arg_to_declared_type(selected, &declared).unwrap();
        let BexExternalValue::Union { metadata, .. } = coerced else {
            panic!("selected JSON union metadata was lost")
        };
        assert!(runtime_ty_structurally_equal(
            &metadata.selected_option,
            &json
        ));
    }

    #[test]
    fn canonical_json_alias_accepts_leaf_annotated_inbound_trees() {
        // The Swift bridge annotates every json scalar leaf with a sparse
        // inbound `value_type` (a transient `Union` carrier). Such trees must
        // satisfy `json` and re-annotate their containers exactly like
        // untyped trees; annotations outside the JSON algebra must not.
        let json = json_ty();
        let mut entries = indexmap::IndexMap::new();
        entries.insert(
            "type".to_string(),
            BexExternalValue::typed(BexExternalValue::String("ok".into()), RuntimeTy::string()),
        );
        let object = BexExternalValue::Map {
            key_type: RuntimeTy::string(),
            value_type: RuntimeTy::unknown(),
            entries,
        };
        assert!(value_matches_type(&object, &json));

        let coerced = coerce_arg_to_declared_type(object, &json).unwrap();
        let BexExternalValue::Map {
            value_type,
            entries,
            ..
        } = coerced
        else {
            panic!("annotated JSON object must stay a map")
        };
        assert!(runtime_ty_structurally_equal(&value_type, &json));
        let BexExternalValue::Union { metadata, .. } = &entries["type"] else {
            panic!("leaf annotation must be preserved")
        };
        assert!(metadata.is_inbound_type_annotation);

        // A leaf annotated outside the JSON algebra keeps the tree non-JSON.
        let mut bigint_entries = indexmap::IndexMap::new();
        bigint_entries.insert(
            "huge".to_string(),
            BexExternalValue::typed(
                BexExternalValue::Bigint(num_bigint::BigInt::from(1)),
                RuntimeTy::bigint(),
            ),
        );
        let bigint_object = BexExternalValue::Map {
            key_type: RuntimeTy::string(),
            value_type: RuntimeTy::unknown(),
            entries: bigint_entries,
        };
        assert!(!value_matches_type(&bigint_object, &json));
    }

    #[test]
    fn canonical_json_alias_reannotates_untyped_inbound_containers() {
        // Untyped bridges (Go `baml.JSON`, Python dicts, `--json-args`) send
        // containers without element annotations; the wire decoder synthesizes
        // a scalar-union element type. A declared `json` slot must rewrite
        // those to the alias itself so the materialized VM containers pass
        // `match (j) { let m: map<string, json> => ... }` exactly like
        // BAML-born `baml.json.parse` values.
        let json = json_ty();
        let nested_list = BexExternalValue::Array {
            element_type: RuntimeTy::unknown(),
            items: vec![BexExternalValue::Int(1)],
        };
        let mut nested_entries = indexmap::IndexMap::new();
        nested_entries.insert("inner".to_string(), nested_list);
        let object = BexExternalValue::Map {
            key_type: RuntimeTy::string(),
            value_type: RuntimeTy::unknown(),
            entries: nested_entries,
        };

        let coerced = coerce_arg_to_declared_type(object, &json).unwrap();
        let BexExternalValue::Map {
            key_type,
            value_type,
            entries,
        } = coerced
        else {
            panic!("JSON object must stay a map")
        };
        assert!(runtime_ty_structurally_equal(
            &key_type,
            &RuntimeTy::string()
        ));
        assert!(runtime_ty_structurally_equal(&value_type, &json));
        let BexExternalValue::Array { element_type, .. } = &entries["inner"] else {
            panic!("nested JSON list must stay a list")
        };
        assert!(runtime_ty_structurally_equal(element_type, &json));

        // A tree outside the JSON algebra is left untouched for the standard
        // validation paths to reject.
        let mut binary_entries = indexmap::IndexMap::new();
        binary_entries.insert(
            "bytes".to_string(),
            BexExternalValue::Uint8Array(vec![1, 2]),
        );
        let binary = BexExternalValue::Map {
            key_type: RuntimeTy::string(),
            value_type: RuntimeTy::unknown(),
            entries: binary_entries,
        };
        let untouched = coerce_arg_to_declared_type(binary, &json).unwrap();
        let BexExternalValue::Map { value_type, .. } = untouched else {
            panic!("non-JSON map must stay a map")
        };
        assert!(runtime_ty_structurally_equal(
            &value_type,
            &RuntimeTy::unknown()
        ));
    }

    #[test]
    fn literal_matching_checks_value_and_prefers_exact_literal() {
        let draft = string_literal("draft");
        let published = string_literal("published");
        let value = BexExternalValue::String("draft".into());
        assert!(value_matches_type(&value, &draft));
        assert!(!value_matches_type(&value, &published));
        assert_eq!(
            find_matching_member(&value, &[RuntimeTy::string(), draft.clone()]).unwrap(),
            draft
        );
    }

    #[test]
    fn enum_variant_matching_checks_value_and_prefers_exact_variant() {
        let mood = TypeName::from_dotted_path("user.callbacks.Mood");
        let happy = RuntimeTy::EnumVariant(mood.clone(), Name::new("HAPPY"), TyAttr::default());
        let sad = RuntimeTy::EnumVariant(mood.clone(), Name::new("SAD"), TyAttr::default());
        let broad = RuntimeTy::Enum(mood.clone(), TyAttr::default());
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
    fn vm_enum_variant_selection_prefers_exact_variant_then_broad_enum() {
        let mood = TypeName::from_dotted_path("user.callbacks.Mood");
        let mut tlab = Tlab::new(BexHeap::new(Vec::new()));
        let enum_ptr = tlab.alloc(Object::Enum(Box::new(Enum {
            type_tag: baml_type::typetag::TypeTag::from_i64(200),
            name: bex_vm_types::DeclarationName::Declared(mood.clone()),
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
            ty_attr: TyAttr::default(),
            owner: bex_vm_types::HeapPtr::null(),
        })));
        let happy = RuntimeTy::EnumVariant(mood.clone(), Name::new("HAPPY"), TyAttr::default());
        let broad = RuntimeTy::Enum(mood, TyAttr::default());
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
            name: DeclarationName,
            type_tag: baml_type::typetag::TypeTag,
        ) -> bex_vm_types::HeapPtr {
            tlab.alloc(Object::Class(Box::new(Class {
                name,
                fields: Vec::new(),
                description: None,
                alias: None,
                docstring: None,
                other: indexmap::IndexMap::new(),
                type_tag,
                ty_attr: TyAttr::default(),
                has_cleanup: false,
                generic_param_count: 0,
                owner: bex_vm_types::HeapPtr::null(),
            })))
        }

        let mut tlab = Tlab::new(BexHeap::new(Vec::new()));
        let dynamic_name = Name::new("Hs7Collision");
        let dynamic_class = alloc_class(
            &mut tlab,
            DeclarationName::Anonymous(dynamic_name.clone()),
            baml_type::typetag::TypeTag::fresh_dynamic(),
        );
        let dynamic_value = Value::object(tlab.alloc_instance(dynamic_class, Vec::new()));

        let done_name = TypeName::from_dotted_path("ai.stream.Done");
        let done_class = alloc_class(
            &mut tlab,
            DeclarationName::Declared(done_name.clone()),
            baml_type::typetag::TypeTag::of_head(&done_name.render_dotted(false)),
        );
        let done_value = Value::object(tlab.alloc_instance(done_class, Vec::new()));

        let partial_arm = RuntimeTy::Union(
            Box::new([
                RuntimeTy::Class(
                    TypeName::local(dynamic_name),
                    Box::new([]),
                    TyAttr::default(),
                ),
                RuntimeTy::Null {
                    attr: TyAttr::default(),
                },
            ]),
            TyAttr::default(),
        );
        let done_arm = RuntimeTy::Class(done_name, Box::new([]), TyAttr::default());
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
    fn enum_variant_selected_arm_has_exact_structural_identity() {
        let mood = TypeName::from_dotted_path("user.callbacks.Mood");
        let happy = RuntimeTy::EnumVariant(mood.clone(), Name::new("HAPPY"), TyAttr::default());
        let another_happy =
            RuntimeTy::EnumVariant(mood.clone(), Name::new("HAPPY"), TyAttr::default());
        let sad = RuntimeTy::EnumVariant(mood, Name::new("SAD"), TyAttr::default());
        assert!(runtime_ty_structurally_equal(&happy, &another_happy));
        assert!(!runtime_ty_structurally_equal(&happy, &sad));
    }

    #[test]
    fn float_literal_matching_preserves_negative_zero_and_decimal_precision() {
        let negative_zero = float_literal("-0.0");
        let precise = float_literal("1.2345678901234567e-300");
        assert!(value_matches_type(
            &BexExternalValue::Float(-0.0),
            &negative_zero
        ));
        assert!(!value_matches_type(
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
                &[RuntimeTy::float(), negative_zero.clone()]
            )
            .unwrap(),
            negative_zero
        );
    }

    #[test]
    fn host_typed_overlapping_arm_uses_contextual_union() {
        let draft = string_literal("draft");
        let declared = RuntimeTy::union([RuntimeTy::string(), draft]);
        // The inbound wire carries only the exact value type. Its transient
        // carrier therefore has no copy of `declared`; coercion must validate
        // the type against the contextual union and reconstruct that union.
        let selected = BexExternalValue::typed(
            BexExternalValue::String("draft".into()),
            RuntimeTy::string(),
        );
        let coerced = coerce_arg_to_declared_type(selected, &declared).unwrap();
        let BexExternalValue::Union { metadata, .. } = coerced else {
            panic!("selected union metadata was lost")
        };
        assert_eq!(metadata.union_type, declared);
        assert_eq!(metadata.selected_option, RuntimeTy::string());
    }

    #[test]
    fn host_root_union_annotation_is_rejected_before_arm_matching() {
        let declared = RuntimeTy::union([RuntimeTy::int(), RuntimeTy::string()]);
        let typed = BexExternalValue::typed(BexExternalValue::Int(7), declared.clone());

        let error = coerce_arg_to_declared_type(typed, &declared).unwrap_err();
        assert!(matches!(
            error,
            EngineError::TypeMismatch { message }
                if message.contains("must identify one exact selected type")
        ));
    }

    #[test]
    fn host_alias_to_root_union_annotation_is_rejected() {
        let alias_name = TypeName::from_dotted_path("user.aliases.Choice");
        let alias = RuntimeTy::TypeAlias(alias_name.clone(), TyAttr::default());
        let mut aliases = indexmap::IndexMap::new();
        aliases.insert(
            alias_name,
            RuntimeTy::union([RuntimeTy::int(), RuntimeTy::string()]),
        );
        let typed = BexExternalValue::typed(BexExternalValue::Int(7), alias);

        let error = coerce_arg_to_declared_type_with_aliases(
            typed,
            &RuntimeTy::int(),
            &aliases,
            &indexmap::IndexMap::new(),
            crate::InboundUnionAmbiguityPolicy::Reject,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            EngineError::TypeMismatch { message }
                if message.contains("must identify one exact selected type")
        ));
    }

    #[test]
    fn host_type_annotation_rejects_type_outside_contextual_union() {
        let declared = RuntimeTy::union([RuntimeTy::int(), RuntimeTy::string()]);
        let typed = BexExternalValue::typed(BexExternalValue::Bool(true), RuntimeTy::bool());
        let error = coerce_arg_to_declared_type(typed, &declared).unwrap_err();
        assert!(matches!(
            error,
            EngineError::TypeMismatch { message }
                if message.contains("bool") && message.contains("declared union")
        ));
    }

    #[test]
    fn host_typed_literal_is_assignable_to_non_union_primitive() {
        let draft = string_literal("draft");
        let typed =
            BexExternalValue::typed(BexExternalValue::String("draft".into()), draft.clone());
        let coerced = coerce_arg_to_declared_type(typed, &RuntimeTy::string()).unwrap();
        let BexExternalValue::Union { metadata, .. } = coerced else {
            panic!("typed-value metadata was lost")
        };
        assert_eq!(metadata.selected_option, draft);
    }

    #[test]
    fn host_type_annotation_rejects_wrong_non_union_type() {
        let typed = BexExternalValue::typed(BexExternalValue::Int(7), RuntimeTy::int());
        let error = coerce_arg_to_declared_type(typed, &RuntimeTy::string()).unwrap_err();
        assert!(matches!(
            error,
            EngineError::TypeMismatch { message }
                if message.contains("int") && message.contains("string")
        ));
    }

    #[test]
    fn nominal_generic_class_annotation_refines_from_context() {
        let name = TypeName::from_dotted_path("user.generics.Box");
        let nominal = RuntimeTy::Class(name.clone(), Box::new([]), TyAttr::default());
        let declared = RuntimeTy::Class(name, Box::new([RuntimeTy::int()]), TyAttr::default());
        let typed = BexExternalValue::typed(
            BexExternalValue::Instance {
                class_name: String::new(),
                type_args: vec![],
                fields: indexmap::IndexMap::new(),
            },
            nominal,
        );

        let coerced = coerce_arg_to_declared_type(typed, &declared).unwrap();
        let BexExternalValue::Union { metadata, value } = coerced else {
            panic!("expected sparse type carrier")
        };
        assert!(runtime_ty_structurally_equal(
            &metadata.selected_option,
            &declared
        ));
        let BexExternalValue::Instance { type_args, .. } = *value else {
            panic!("expected class payload")
        };
        assert_eq!(type_args, vec![RuntimeTy::int()]);
    }

    #[test]
    fn nominal_generic_class_annotation_cannot_choose_concrete_union_arm() {
        let name = TypeName::from_dotted_path("user.generics.Box");
        let nominal = RuntimeTy::Class(name.clone(), Box::new([]), TyAttr::default());
        let declared = RuntimeTy::union([
            RuntimeTy::Class(
                name.clone(),
                Box::new([RuntimeTy::int()]),
                TyAttr::default(),
            ),
            RuntimeTy::Class(name, Box::new([RuntimeTy::string()]), TyAttr::default()),
        ]);
        let typed = BexExternalValue::typed(
            BexExternalValue::Instance {
                class_name: String::new(),
                type_args: vec![],
                fields: indexmap::IndexMap::new(),
            },
            nominal,
        );

        let error = coerce_arg_to_declared_type(typed, &declared).unwrap_err();
        assert!(matches!(
            error,
            EngineError::TypeMismatch { message }
                if message.contains("does not identify one member")
                    && message.contains("concrete generic arguments")
        ));
    }

    #[test]
    fn host_typed_literal_selects_broader_contextual_union_arm() {
        let draft = string_literal("draft");
        let declared = RuntimeTy::union([RuntimeTy::string(), RuntimeTy::int()]);
        let typed = BexExternalValue::typed(BexExternalValue::String("draft".into()), draft);
        let coerced = coerce_arg_to_declared_type(typed, &declared).unwrap();
        let BexExternalValue::Union { metadata, .. } = coerced else {
            panic!("selected union metadata was lost")
        };
        assert_eq!(metadata.union_type, declared);
        assert_eq!(metadata.selected_option, RuntimeTy::string());
    }

    #[test]
    fn sparse_nested_hint_selects_only_the_ambiguous_empty_child() {
        let int_list = list(RuntimeTy::int());
        let string_list = list(RuntimeTy::string());
        let child_type = RuntimeTy::union([int_list.clone(), string_list]);
        let value = BexExternalValue::Array {
            element_type: RuntimeTy::unknown(),
            items: vec![BexExternalValue::typed(
                BexExternalValue::Array {
                    element_type: RuntimeTy::unknown(),
                    items: vec![],
                },
                int_list.clone(),
            )],
        };

        let coerced = coerce_arg_to_declared_type(value, &list(child_type)).unwrap();
        let BexExternalValue::Array { items, .. } = coerced else {
            panic!("expected outer list")
        };
        let BexExternalValue::Union { metadata, value } = &items[0] else {
            panic!("expected selected child union")
        };
        assert_eq!(metadata.selected_option, int_list);
        assert!(
            matches!(value.as_ref(), BexExternalValue::Array { items, .. } if items.is_empty())
        );
    }

    #[test]
    fn unannotated_recursive_alias_uses_contextual_payload_shape() {
        let alias_name = TypeName::from_dotted_path("user.aliases.RecList");
        let alias = RuntimeTy::TypeAlias(alias_name.clone(), TyAttr::default());
        let alias_body = RuntimeTy::union([RuntimeTy::int(), list(alias.clone())]);
        let mut aliases = indexmap::IndexMap::new();
        aliases.insert(alias_name, alias_body);

        let declared = RuntimeTy::union([
            alias.clone(),
            RuntimeTy::Null {
                attr: TyAttr::default(),
            },
        ]);
        let value = BexExternalValue::Array {
            element_type: RuntimeTy::unknown(),
            items: vec![BexExternalValue::Int(6)],
        };

        let coerced = coerce_arg_to_declared_type_with_aliases(
            value,
            &declared,
            &aliases,
            &indexmap::IndexMap::new(),
            crate::InboundUnionAmbiguityPolicy::Reject,
        )
        .unwrap();
        let BexExternalValue::Union { metadata, value } = coerced else {
            panic!("expected the nullable union carrier")
        };
        assert!(runtime_ty_structurally_equal(
            &metadata.selected_option,
            &alias
        ));
        let BexExternalValue::Union { metadata, value } = *value else {
            panic!("expected the recursive alias body union carrier")
        };
        assert!(matches!(metadata.selected_option, RuntimeTy::List(..)));
        let BexExternalValue::Array { items, .. } = *value else {
            panic!("expected the recursive list payload")
        };
        assert!(matches!(items.as_slice(), [BexExternalValue::Union { .. }]));
    }

    #[test]
    fn unannotated_null_prefers_exact_null_over_nested_optional_alias() {
        let alias_name = TypeName::from_dotted_path("user.aliases.OptionalState");
        let alias = RuntimeTy::TypeAlias(alias_name.clone(), TyAttr::default());
        let alias_body = RuntimeTy::optional(RuntimeTy::Enum(
            TypeName::from_dotted_path("user.State"),
            TyAttr::default(),
        ));
        let mut aliases = indexmap::IndexMap::new();
        aliases.insert(alias_name, alias_body);

        // This is the runtime shape of `OptionalState?`: the alias already
        // contains null, but applying `?` outside the opaque alias introduces
        // another explicit null member.
        let declared = RuntimeTy::union([alias, RuntimeTy::null()]);
        let coerced = coerce_arg_to_declared_type_with_aliases(
            BexExternalValue::Null,
            &declared,
            &aliases,
            &indexmap::IndexMap::new(),
            crate::InboundUnionAmbiguityPolicy::Reject,
        )
        .unwrap();

        let BexExternalValue::Union { value, metadata } = coerced else {
            panic!("expected the outer optional union carrier")
        };
        assert!(metadata.selected_option.is_null());
        assert!(matches!(*value, BexExternalValue::Null));
    }

    #[test]
    fn unannotated_nested_container_uses_payload_shape() {
        let int_list = list(RuntimeTy::int());
        let string_list = list(RuntimeTy::string());
        let child_type = RuntimeTy::union([int_list, string_list.clone()]);
        let value = BexExternalValue::Array {
            element_type: RuntimeTy::unknown(),
            items: vec![BexExternalValue::Array {
                element_type: RuntimeTy::unknown(),
                items: vec![BexExternalValue::String("hello".into())],
            }],
        };

        let coerced = coerce_arg_to_declared_type(value, &list(child_type)).unwrap();
        let BexExternalValue::Array { items, .. } = coerced else {
            panic!("expected outer list")
        };
        let BexExternalValue::Union { metadata, .. } = &items[0] else {
            panic!("expected selected child union")
        };
        assert_eq!(metadata.selected_option, string_list);
    }

    #[test]
    fn unannotated_ambiguous_empty_child_requires_value_type() {
        let child_type = RuntimeTy::union([list(RuntimeTy::int()), list(RuntimeTy::string())]);
        let value = BexExternalValue::Array {
            element_type: RuntimeTy::unknown(),
            items: vec![BexExternalValue::Array {
                element_type: RuntimeTy::unknown(),
                items: vec![],
            }],
        };

        let error = coerce_arg_to_declared_type(value, &list(child_type)).unwrap_err();
        assert!(matches!(
            error,
            EngineError::TypeMismatch { message }
                if message.contains("multiple union members")
                    && message.contains("value_type")
        ));
    }

    #[test]
    fn unannotated_literal_vs_primitive_requires_value_type() {
        let declared = RuntimeTy::union([RuntimeTy::string(), string_literal("draft")]);
        let error =
            coerce_arg_to_declared_type(BexExternalValue::String("draft".into()), &declared)
                .unwrap_err();
        assert!(matches!(
            error,
            EngineError::TypeMismatch { message }
                if message.contains("multiple union members")
                    && message.contains("value_type")
        ));
    }

    #[test]
    fn dynamic_default_selects_first_matching_empty_container_arm() {
        let int_list = list(RuntimeTy::int());
        let string_list = list(RuntimeTy::string());
        let declared = RuntimeTy::union([int_list.clone(), string_list]);
        let value = BexExternalValue::Array {
            element_type: RuntimeTy::unknown(),
            items: vec![],
        };

        let coerced = coerce_arg_to_declared_type_with_policy(
            value,
            &declared,
            crate::InboundUnionAmbiguityPolicy::SelectDefault,
        )
        .unwrap();
        let BexExternalValue::Union { metadata, value } = coerced else {
            panic!("expected selected union")
        };
        assert!(runtime_ty_structurally_equal(
            &metadata.selected_option,
            &int_list
        ));
        assert!(matches!(
            *value,
            BexExternalValue::Array {
                ref element_type,
                ref items,
            } if items.is_empty()
                && runtime_ty_structurally_equal(element_type, &RuntimeTy::int())
        ));
    }

    #[test]
    fn dynamic_default_prefers_exact_literal_over_broad_primitive() {
        let literal = string_literal("draft");
        let declared = RuntimeTy::union([RuntimeTy::string(), literal.clone()]);

        let coerced = coerce_arg_to_declared_type_with_policy(
            BexExternalValue::String("draft".into()),
            &declared,
            crate::InboundUnionAmbiguityPolicy::SelectDefault,
        )
        .unwrap();
        let BexExternalValue::Union { metadata, value } = coerced else {
            panic!("expected selected union")
        };
        assert!(runtime_ty_structurally_equal(
            &metadata.selected_option,
            &literal
        ));
        assert!(matches!(*value, BexExternalValue::String(ref value) if value == "draft"));
    }

    #[test]
    fn unannotated_structurally_duplicate_members_select_first_canonical_arm() {
        let declared = RuntimeTy::Union(
            Box::new([RuntimeTy::int(), RuntimeTy::int(), RuntimeTy::string()]),
            TyAttr::default(),
        );

        let coerced = coerce_arg_to_declared_type(BexExternalValue::Int(7), &declared).unwrap();
        let BexExternalValue::Union { metadata, value } = coerced else {
            panic!("expected selected union")
        };
        assert!(runtime_ty_structurally_equal(
            &metadata.selected_option,
            &RuntimeTy::int()
        ));
        assert!(matches!(*value, BexExternalValue::Int(7)));

        let RuntimeTy::Union(members, _) = &declared else {
            unreachable!()
        };
        let outbound_selected = find_matching_member(&BexExternalValue::Int(7), members).unwrap();
        assert!(runtime_ty_structurally_equal(
            &outbound_selected,
            &RuntimeTy::int()
        ));
    }

    #[test]
    fn host_typed_callable_selects_structurally_equal_function_arm() {
        let declared_arm = callback_ty(Freshness::Regular);
        let wire_arm = callback_ty(Freshness::Fresh);
        assert_ne!(declared_arm, wire_arm);
        assert!(runtime_ty_structurally_equal(&declared_arm, &wire_arm));

        let declared = RuntimeTy::union([declared_arm.clone(), RuntimeTy::string()]);
        let typed = BexExternalValue::typed(
            BexExternalValue::HostValue(HostValueArc::new(41, HostValueKind::Callable)),
            wire_arm,
        );
        let coerced = coerce_arg_to_declared_type(typed, &declared).unwrap();
        let BexExternalValue::Union { metadata, .. } = coerced else {
            panic!("selected callable union metadata was lost")
        };
        assert_eq!(metadata.union_type, declared);
        assert_eq!(metadata.selected_option, declared_arm);

        let opaque = BexExternalValue::HostValue(HostValueArc::new(42, HostValueKind::Opaque));
        assert!(!value_matches_type(&opaque, &metadata.selected_option));
    }

    #[test]
    fn media_matching_is_kind_specific_and_generic_accepts_every_kind() {
        let image = media_value(MediaKind::Image);
        assert!(value_matches_type(&image, &media_ty(MediaKind::Image)));
        assert!(value_matches_type(&image, &media_ty(MediaKind::Generic)));
        assert!(!value_matches_type(&image, &media_ty(MediaKind::Audio)));

        assert!(runtime_ty_compatible(
            &media_ty(MediaKind::Image),
            &media_ty(MediaKind::Generic)
        ));
        assert!(!runtime_ty_compatible(
            &media_ty(MediaKind::Image),
            &media_ty(MediaKind::Audio)
        ));
        assert!(runtime_ty_structurally_equal(
            &media_ty(MediaKind::Image),
            &media_ty(MediaKind::Image)
        ));
        assert!(!runtime_ty_structurally_equal(
            &media_ty(MediaKind::Image),
            &media_ty(MediaKind::Generic)
        ));
    }

    #[test]
    fn portable_media_selects_the_nested_prompt_part_wrapper_union() {
        let media_part = RuntimeTy::union([
            media_wrapper_ty(MediaKind::Image),
            media_wrapper_ty(MediaKind::Audio),
            media_wrapper_ty(MediaKind::Video),
            media_wrapper_ty(MediaKind::Pdf),
        ]);
        let prompt_part = RuntimeTy::union([RuntimeTy::string(), media_part.clone()]);

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
        let inner = RuntimeTy::union([
            RuntimeTy::int(),
            RuntimeTy::string(),
            RuntimeTy::float(),
            RuntimeTy::bool(),
        ]);
        let optional = RuntimeTy::Union(
            Box::new([inner.clone(), RuntimeTy::null()]),
            TyAttr::default(),
        );

        let wrapped =
            wrap_selected_union_member(BexExternalValue::String("alias".into()), &optional, &inner)
                .unwrap();
        let BexExternalValue::Union { metadata, .. } = wrapped else {
            panic!("expected the inner union envelope")
        };
        assert!(runtime_ty_structurally_equal(&metadata.union_type, &inner));
        assert_eq!(metadata.selected_option, RuntimeTy::string());
    }

    #[test]
    fn stdlib_media_wrapper_annotation_coerces_only_in_primitive_context() {
        let wrapper_ty = media_wrapper_ty(MediaKind::Image);
        let typed_wrapper =
            BexExternalValue::typed(media_wrapper_value(MediaKind::Image), wrapper_ty.clone());

        let coerced =
            coerce_arg_to_declared_type(typed_wrapper, &media_ty(MediaKind::Image)).unwrap();
        let BexExternalValue::Union { value, metadata } = coerced else {
            panic!("expected sparse type carrier")
        };
        assert!(runtime_ty_structurally_equal(
            &metadata.selected_option,
            &media_ty(MediaKind::Image)
        ));
        assert!(matches!(
            *value,
            BexExternalValue::Adt(BexExternalAdt::Media(ref media))
                if media.kind == MediaKind::Image
        ));

        let nominal = coerce_arg_to_declared_type(
            BexExternalValue::typed(media_wrapper_value(MediaKind::Image), wrapper_ty.clone()),
            &wrapper_ty,
        )
        .unwrap();
        let BexExternalValue::Union { value, metadata } = nominal else {
            panic!("expected sparse type carrier")
        };
        assert!(runtime_ty_structurally_equal(
            &metadata.selected_option,
            &wrapper_ty
        ));
        assert!(matches!(*value, BexExternalValue::Instance { .. }));
    }

    #[test]
    fn generic_media_still_validates_the_wrappers_concrete_kind() {
        let typed_wrapper = BexExternalValue::typed(
            media_wrapper_value(MediaKind::Audio),
            media_wrapper_ty(MediaKind::Image),
        );

        let error =
            coerce_arg_to_declared_type(typed_wrapper, &media_ty(MediaKind::Generic)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("does not inhabit contextual type `image`"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn local_class_cannot_spoof_a_stdlib_media_wrapper_fqn() {
        let local_spoof = RuntimeTy::Class(
            TypeName::new(
                Name::new("user"),
                vec![Name::new("baml"), Name::new("media")],
                Name::new("Image"),
            ),
            Box::new([]),
            TyAttr::default(),
        );
        let RuntimeTy::Class(name, ..) = &local_spoof else {
            unreachable!()
        };
        assert_eq!(name.display_name().as_str(), "baml.media.Image");
        assert_eq!(name.render_dotted(false), "user.baml.media.Image");
        assert_eq!(stdlib_media_wrapper_kind(&local_spoof), None);

        let typed_spoof =
            BexExternalValue::typed(media_wrapper_value(MediaKind::Image), local_spoof);
        let error =
            coerce_arg_to_declared_type(typed_spoof, &media_ty(MediaKind::Image)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("is not assignable to declared type `image`"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn stdlib_media_wrapper_selects_alias_union_arm_and_retains_alias_metadata() {
        let alias_name = TypeName::from_dotted_path("user.aliases.ImageAlias");
        let alias = RuntimeTy::TypeAlias(alias_name.clone(), TyAttr::default());
        let mut aliases = indexmap::IndexMap::new();
        aliases.insert(alias_name, media_ty(MediaKind::Image));
        let declared = RuntimeTy::union([alias.clone(), RuntimeTy::string()]);
        let typed_wrapper = BexExternalValue::typed(
            media_wrapper_value(MediaKind::Image),
            media_wrapper_ty(MediaKind::Image),
        );

        let coerced = coerce_arg_to_declared_type_with_aliases(
            typed_wrapper,
            &declared,
            &aliases,
            &indexmap::IndexMap::new(),
            crate::InboundUnionAmbiguityPolicy::Reject,
        )
        .unwrap();
        let BexExternalValue::Union { value, metadata } = coerced else {
            panic!("expected selected union carrier")
        };
        assert!(runtime_ty_structurally_equal(
            &metadata.selected_option,
            &alias
        ));
        assert!(matches!(
            *value,
            BexExternalValue::Adt(BexExternalAdt::Media(ref media))
                if media.kind == MediaKind::Image
        ));
    }

    #[test]
    fn stdlib_media_wrapper_alias_resolution_rejects_cycles() {
        let first_name = TypeName::from_dotted_path("user.aliases.First");
        let second_name = TypeName::from_dotted_path("user.aliases.Second");
        let first = RuntimeTy::TypeAlias(first_name.clone(), TyAttr::default());
        let second = RuntimeTy::TypeAlias(second_name.clone(), TyAttr::default());
        let mut aliases = indexmap::IndexMap::new();
        aliases.insert(first_name, second);
        aliases.insert(second_name, first.clone());

        assert!(!stdlib_media_wrapper_matches(
            &media_wrapper_ty(MediaKind::Image),
            &first,
            &aliases,
            true,
        ));
    }

    #[test]
    fn stdlib_image_wrapper_selects_primitive_arm_over_user_image_class() {
        let primitive_image = media_ty(MediaKind::Image);
        let user_image = RuntimeTy::Class(
            TypeName::from_dotted_path("user.media.Image"),
            Box::new([]),
            TyAttr::default(),
        );
        let declared = RuntimeTy::union([primitive_image.clone(), user_image]);
        let typed_wrapper = BexExternalValue::typed(
            media_wrapper_value(MediaKind::Image),
            media_wrapper_ty(MediaKind::Image),
        );

        let coerced = coerce_arg_to_declared_type(typed_wrapper, &declared).unwrap();
        let BexExternalValue::Union { value, metadata } = coerced else {
            panic!("expected selected union carrier")
        };
        assert!(runtime_ty_structurally_equal(
            &metadata.selected_option,
            &primitive_image
        ));
        assert!(matches!(
            *value,
            BexExternalValue::Adt(BexExternalAdt::Media(ref media))
                if media.kind == MediaKind::Image
        ));
    }

    #[test]
    fn host_selected_media_arm_survives_argument_coercion() {
        let image = media_ty(MediaKind::Image);
        let audio = media_ty(MediaKind::Audio);
        let declared = RuntimeTy::union([image.clone(), audio.clone()]);
        let selected = BexExternalValue::union(
            media_value(MediaKind::Image),
            [image.clone(), audio],
            image.clone(),
        );
        let coerced = coerce_arg_to_declared_type(selected, &declared).unwrap();
        let BexExternalValue::Union { metadata, .. } = coerced else {
            panic!("selected media union metadata was lost")
        };
        assert!(runtime_ty_structurally_equal(
            &metadata.selected_option,
            &image
        ));
    }

    #[test]
    fn host_selected_metatype_arm_survives_argument_coercion() {
        let metatype = RuntimeTy::type_type();
        let declared = RuntimeTy::union([metatype.clone(), RuntimeTy::string()]);
        let selected = BexExternalValue::union(
            BexExternalValue::Adt(BexExternalAdt::Type(RuntimeTy::int())),
            [metatype.clone(), RuntimeTy::string()],
            metatype.clone(),
        );
        let coerced = coerce_arg_to_declared_type(selected, &declared).unwrap();
        let BexExternalValue::Union { metadata, .. } = coerced else {
            panic!("selected metatype union metadata was lost")
        };
        assert!(runtime_ty_structurally_equal(
            &metadata.selected_option,
            &metatype
        ));
    }

    #[test]
    fn structural_type_identity_preserves_nested_optional_arms() {
        let int_list = list(RuntimeTy::int());
        let optional_int_list = list(RuntimeTy::optional(RuntimeTy::int()));
        assert!(!runtime_ty_structurally_equal(
            &int_list,
            &optional_int_list
        ));
        assert!(!runtime_ty_structurally_equal(
            &map(RuntimeTy::int()),
            &map(RuntimeTy::optional(RuntimeTy::int()))
        ));
    }

    #[test]
    fn empty_array_uses_its_declared_element_type_to_select_union_arm() {
        let int_list = list(RuntimeTy::int());
        let string_list = list(RuntimeTy::string());
        let value = BexExternalValue::Array {
            element_type: RuntimeTy::string(),
            items: Vec::new(),
        };
        let wrapped = maybe_wrap_union(
            value,
            &RuntimeTy::Union(Box::new([int_list, string_list.clone()]), TyAttr::default()),
        )
        .unwrap();
        let BexExternalValue::Union { metadata, .. } = wrapped else {
            panic!("expected union wrapper")
        };
        assert_eq!(metadata.selected_option, string_list);
    }

    #[test]
    fn empty_map_uses_its_declared_value_type_to_select_union_arm() {
        let int_map = map(RuntimeTy::int());
        let string_map = map(RuntimeTy::string());
        let value = BexExternalValue::Map {
            key_type: RuntimeTy::string(),
            value_type: RuntimeTy::string(),
            entries: indexmap::IndexMap::new(),
        };
        let wrapped = maybe_wrap_union(
            value,
            &RuntimeTy::Union(Box::new([int_map, string_map.clone()]), TyAttr::default()),
        )
        .unwrap();
        let BexExternalValue::Union { metadata, .. } = wrapped else {
            panic!("expected union wrapper")
        };
        assert_eq!(metadata.selected_option, string_map);
    }
}

#[cfg(test)]
mod peel_to_rust_type_tests {
    use baml_type::TyAttr;

    use super::*;

    /// `RuntimeTy::RustType` — the canonical `$rust_type` shape.
    fn rust_type() -> RuntimeTy {
        RuntimeTy::RustType {
            attr: TyAttr::default(),
        }
    }

    #[test]
    fn direct_rust_type_matches() {
        assert_eq!(peel_to_rust_type(&rust_type()), Some(()));
    }

    #[test]
    fn optional_rust_type_peels_through() {
        // `RuntimeTy::optional(RustType)` lowers to `RuntimeTy::Union([RustType, Null])`
        // post-`RuntimeTy::Optional`-removal; the union arm in `peel_to_rust_type`
        // picks the single RustType member.
        let ty = RuntimeTy::optional(rust_type());
        assert_eq!(peel_to_rust_type(&ty), Some(()));
    }

    #[test]
    fn nested_optional_rust_type_peels_through() {
        // `T??` collapses to `T?` per `RuntimeTy::optional`'s idempotence rule
        // (a union already containing `null` is returned unchanged), so
        // this is effectively the same shape as the single-optional case
        // — still a single non-null member that peels.
        let ty = RuntimeTy::optional(RuntimeTy::optional(rust_type()));
        assert_eq!(peel_to_rust_type(&ty), Some(()));
    }

    #[test]
    fn singleton_union_with_rust_type_and_null_matches() {
        // `RustType | null` — only one non-null arm so the peel
        // unambiguously picks `RustType`.
        let ty = RuntimeTy::Union(
            Box::new([
                rust_type(),
                RuntimeTy::Null {
                    attr: TyAttr::default(),
                },
            ]),
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
        let ty = RuntimeTy::Union(
            Box::new([
                rust_type(),
                RuntimeTy::String {
                    attr: TyAttr::default(),
                },
            ]),
            TyAttr::default(),
        );
        assert_eq!(peel_to_rust_type(&ty), Some(()));
    }

    #[test]
    fn union_with_two_rust_type_arms_is_ambiguous() {
        // `RustType | RustType` — two arms peel to the target. The
        // function rejects to avoid silently picking one.
        let ty = RuntimeTy::Union(Box::new([rust_type(), rust_type()]), TyAttr::default());
        assert_eq!(peel_to_rust_type(&ty), None);
    }

    #[test]
    fn plain_string_does_not_match() {
        assert_eq!(
            peel_to_rust_type(&RuntimeTy::String {
                attr: TyAttr::default()
            }),
            None,
        );
    }

    #[test]
    fn unrelated_opaque_does_not_match() {
        // A different opaque leaf type — e.g. `ai.Prompt` — must
        // not be confused with `$rust_type`.
        let ty = RuntimeTy::PromptAst {
            attr: TyAttr::default(),
        };
        assert_eq!(peel_to_rust_type(&ty), None);
    }

    #[test]
    fn optional_of_unrelated_type_does_not_match() {
        let ty = RuntimeTy::optional(RuntimeTy::String {
            attr: TyAttr::default(),
        });
        assert_eq!(peel_to_rust_type(&ty), None);
    }

    #[test]
    fn union_with_no_rust_type_arm_does_not_match() {
        let ty = RuntimeTy::Union(
            Box::new([
                RuntimeTy::String {
                    attr: TyAttr::default(),
                },
                RuntimeTy::Int {
                    attr: TyAttr::default(),
                },
            ]),
            TyAttr::default(),
        );
        assert_eq!(peel_to_rust_type(&ty), None);
    }
}

#[cfg(test)]
mod peel_function_ty_tests {
    use baml_type::{RuntimeFunctionParamTy, TyAttr};

    use super::*;

    /// `(int) -> string` — the canonical concrete function shape.
    fn fn_ty() -> RuntimeTy {
        RuntimeTy::Function {
            params: Box::new([RuntimeFunctionParamTy::required(
                None,
                RuntimeTy::Int {
                    attr: TyAttr::default(),
                },
            )]),
            ret: Box::new(RuntimeTy::String {
                attr: TyAttr::default(),
            }),
            throws: Box::new(RuntimeTy::Void {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        }
    }

    /// A second, distinct function shape — `() -> int` — used to verify the
    /// uniqueness rule rejects two function members in a union.
    fn other_fn_ty() -> RuntimeTy {
        RuntimeTy::Function {
            params: Box::new([]),
            ret: Box::new(RuntimeTy::Int {
                attr: TyAttr::default(),
            }),
            throws: Box::new(RuntimeTy::Void {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        }
    }

    #[test]
    fn direct_function_returns_itself() {
        let ty = fn_ty();
        let peeled = peel_function_ty(&ty).expect("must peel a direct Function");
        assert!(matches!(peeled, RuntimeTy::Function { .. }));
    }

    #[test]
    fn optional_function_peels_through() {
        // `RuntimeTy::optional(fn)` lowers to `RuntimeTy::Union([fn, Null])`; the union
        // arm in `peel_function_ty` picks the single function member.
        let ty = RuntimeTy::optional(fn_ty());
        let peeled = peel_function_ty(&ty).expect("Union<fn, Null> must peel");
        assert!(matches!(peeled, RuntimeTy::Function { .. }));
    }

    #[test]
    fn nested_optional_function_peels_through() {
        // `RuntimeTy::optional` is idempotent — `T??` collapses to `T?` — so this
        // is effectively the same shape as the single-optional case.
        let ty = RuntimeTy::optional(RuntimeTy::optional(fn_ty()));
        assert!(peel_function_ty(&ty).is_some());
    }

    #[test]
    fn union_with_single_function_arm_peels_through() {
        // `((int) -> string) | null` — only one function member.
        let ty = RuntimeTy::Union(
            Box::new([
                fn_ty(),
                RuntimeTy::Null {
                    attr: TyAttr::default(),
                },
            ]),
            TyAttr::default(),
        );
        assert!(peel_function_ty(&ty).is_some());
    }

    #[test]
    fn union_with_function_plus_non_function_arm_peels_through() {
        // `((int) -> string) | string` — exactly one function member.
        let ty = RuntimeTy::Union(
            Box::new([
                fn_ty(),
                RuntimeTy::String {
                    attr: TyAttr::default(),
                },
            ]),
            TyAttr::default(),
        );
        assert!(peel_function_ty(&ty).is_some());
    }

    #[test]
    fn union_with_two_distinct_function_arms_is_ambiguous() {
        // `((int) -> string) | (() -> int)` — two function members.
        // The peel rejects to avoid silently picking one. Pins the
        // determinism contract of the helper.
        let ty = RuntimeTy::Union(Box::new([fn_ty(), other_fn_ty()]), TyAttr::default());
        assert!(peel_function_ty(&ty).is_none());
    }

    #[test]
    fn plain_string_does_not_match() {
        let ty = RuntimeTy::String {
            attr: TyAttr::default(),
        };
        assert!(peel_function_ty(&ty).is_none());
    }

    #[test]
    fn optional_of_non_function_does_not_match() {
        let ty = RuntimeTy::optional(RuntimeTy::String {
            attr: TyAttr::default(),
        });
        assert!(peel_function_ty(&ty).is_none());
    }

    #[test]
    fn union_with_no_function_arm_does_not_match() {
        let ty = RuntimeTy::Union(
            Box::new([
                RuntimeTy::String {
                    attr: TyAttr::default(),
                },
                RuntimeTy::Int {
                    attr: TyAttr::default(),
                },
            ]),
            TyAttr::default(),
        );
        assert!(peel_function_ty(&ty).is_none());
    }

    #[test]
    fn empty_union_does_not_match() {
        let ty = RuntimeTy::Union(Box::new([]), TyAttr::default());
        assert!(peel_function_ty(&ty).is_none());
    }
}

#[cfg(test)]
mod inference_unifier_tests {
    //! Unit tests for the `RuntimeTy` generic unifier (01a/01b): `union_runtime_ty`
    //! and `infer_bindings_runtime`. These mirror the TIR's `union_ty` /
    //! `infer_bindings_inner` semantics — union-merge, `null`-strip, no arm
    //! routing (00b3 G5).
    use baml_type::{Name, TyAttr};

    use super::*;

    fn tv(name: &str) -> RuntimeTy {
        RuntimeTy::TypeVar(
            baml_type::ParamTy::new(0, Name::new(name)),
            TyAttr::default(),
        )
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
        RuntimeTy::RustType {
            attr: TyAttr::default(),
        }
    }
    fn never() -> RuntimeTy {
        RuntimeTy::Never {
            attr: TyAttr::default(),
        }
    }
    fn list(inner: RuntimeTy) -> RuntimeTy {
        RuntimeTy::List(Box::new(inner), TyAttr::default())
    }
    fn map(value: RuntimeTy) -> RuntimeTy {
        RuntimeTy::Map {
            key: Box::new(string()),
            value: Box::new(value),
            attr: TyAttr::default(),
        }
    }
    fn union(members: Vec<RuntimeTy>) -> RuntimeTy {
        RuntimeTy::Union(members.into(), TyAttr::default())
    }
    fn class(name: &str, args: Vec<RuntimeTy>) -> RuntimeTy {
        RuntimeTy::Class(
            baml_type::TypeName::local(Name::new(name)),
            args.into(),
            TyAttr::default(),
        )
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
        // matching compile-time inference. In practice the synth layer never
        // feeds `unknown` here — a no-evidence value becomes host-only
        // `rust_type` instead — so this binding does not arise end-to-end.
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
    fn infer_nullable_null_actual_binds_null() {
        // maybe_id(None) ⇒ T = null (TIR-faithful, T33).
        assert_eq!(
            infer(&union(vec![tv("T"), null()]), &null()).get("T"),
            Some(&null())
        );
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

#[cfg(test)]
mod union_media_annotation_tests {
    use super::*;

    /// An inbound media value annotated `image` must satisfy a DECLARED
    /// union containing that media kind (`image | string`). The wrapper
    /// payload's shape never matches `Media` structurally — the sparse
    /// annotation is the value's identity — so the union-member check must
    /// recurse with the annotated carrier, not the unwrapped payload.
    #[test]
    fn annotated_media_matches_declared_media_or_string_union() {
        let media_ty = RuntimeTy::Media(baml_type::MediaKind::Image, baml_type::TyAttr::default());
        let declared = RuntimeTy::Union(
            Box::new([
                media_ty.clone(),
                RuntimeTy::String {
                    attr: baml_type::TyAttr::default(),
                },
            ]),
            baml_type::TyAttr::default(),
        );
        let wrapper = BexExternalValue::Instance {
            class_name: "baml.media.Image".to_string(),
            type_args: vec![],
            fields: indexmap::IndexMap::new(),
        };
        let annotated = BexExternalValue::typed(wrapper, media_ty);
        assert!(value_matches_type_with_definitions(
            &annotated,
            &declared,
            &indexmap::IndexMap::new(),
            &indexmap::IndexMap::new(),
        ));

        // control: a string-annotated payload still matches through the
        // generic unwrap path
        let annotated_string = BexExternalValue::typed(
            BexExternalValue::String("hi".into()),
            RuntimeTy::String {
                attr: baml_type::TyAttr::default(),
            },
        );
        assert!(value_matches_type_with_definitions(
            &annotated_string,
            &declared,
            &indexmap::IndexMap::new(),
            &indexmap::IndexMap::new(),
        ));
    }
}
