//! Value conversion functions between VM and external types.
//!
//! This module contains all the conversion logic for transforming values
//! between the VM representation (`Value`, `Object`) and the external
//! representation (`BexValue`, `BexExternalValue`).

use ::bex_heap::{BexValue, PermitProof};
use ::bex_vm_types::{HeapPtr, Object, Value, ValueKind, float_order};
use baml_type::{Literal, Ty};
use bex_external_types::{
    BexExternalAdt, BexExternalValue, RuntimeTy, UnionMetadata, is_canonical_json_alias,
    runtime_ty_structurally_equal, selected_arm_equal, value_satisfies_json,
};
use bex_vm::BexVm;

use crate::{BexEngine, EngineError};

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
    to_wire_ty_with(ty, &mut bex_vm_types::TypeHead::to_name)
}

pub(crate) fn to_wire_ty_with(
    ty: &bex_vm_types::RuntimeTy,
    name: &mut impl FnMut(
        &bex_vm_types::TypeHead,
    ) -> Result<baml_type::TypeName, bex_vm_types::UnnameableHead>,
) -> Result<RuntimeTy, EngineError> {
    ty.try_map_heads(name)
        .map_err(|head| EngineError::TypeMismatch {
            message: format!("type names an unnameable declaration: {head}"),
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
pub(crate) enum InboundDeclarationKind {
    Any,
    Class,
    Enum,
}

/// The host proxy kind for a live stdlib capability: the class IS the
/// stdlib's own declaration, by identity. A declaration's display name is
/// never an identity — a user package spelled `ai`, or a runtime package
/// compiling the same local spelling under a fresh head, renders the same
/// name — so nothing here reads one.
fn trusted_stdlib_capability_kind(
    stdlib_heads: &bex_vm::package_load::StdlibHeads,
    class: &bex_vm_types::Class,
) -> Option<bex_external_types::TaggedHeapHandleKind> {
    Some(match stdlib_heads.capability(class.type_tag)? {
        bex_vm::package_load::StdlibCapability::FunctionSpec => {
            bex_external_types::TaggedHeapHandleKind::FunctionSpec
        }
        bex_vm::package_load::StdlibCapability::Stream => {
            bex_external_types::TaggedHeapHandleKind::Stream
        }
    })
}

/// The runtime declarations one call may name by their wire spelling: the
/// heads of the type values a host passed as type arguments. A `BamlTy` head
/// is a bare name on the wire, so a type argument that names a runtime
/// declaration resolves through these before the program's own declarations.
#[derive(Clone, Copy)]
pub(crate) struct InboundRuntimeOverlay<'a> {
    runtime_named_objects: Option<&'a indexmap::IndexMap<String, HeapPtr>>,
}

impl<'a> InboundRuntimeOverlay<'a> {
    pub(crate) fn new(runtime_named_objects: &'a indexmap::IndexMap<String, HeapPtr>) -> Self {
        Self {
            runtime_named_objects: Some(runtime_named_objects),
        }
    }

    /// A call that passed no type values names no runtime declaration.
    pub(crate) const fn none() -> Self {
        Self {
            runtime_named_objects: None,
        }
    }
}

impl BexEngine {
    /// Resolve one inbound overlay spelling through the same per-call cascade
    /// used for external instances and variants, then through the VM's declared
    /// package surface. Keeping this lookup shared prevents container/type
    /// metadata from drifting from value allocation again.
    pub(crate) fn resolve_inbound_declaration(
        &self,
        vm: &BexVm,
        name: &str,
        overlay: InboundRuntimeOverlay<'_>,
        kind: InboundDeclarationKind,
    ) -> Option<HeapPtr> {
        if let Some(ptr) = overlay.runtime_named_objects.and_then(|objects| {
            objects
                .get(name)
                .or_else(|| resolve_named_object(objects, name))
        }) {
            return Some(*ptr);
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
            return Some(*ptr);
        }

        vm.declaration_head(&baml_type::TypeName::from_dotted_path(name))
            .map(bex_vm_types::TypeHead::ptr)
    }

    pub(crate) fn anchor_wire_ty_with_runtime(
        &self,
        vm: &BexVm,
        ty: &RuntimeTy,
        overlay: InboundRuntimeOverlay<'_>,
    ) -> Result<bex_vm_types::RuntimeTy, EngineError> {
        ty.try_map_heads(&mut |name: &baml_type::TypeName| {
            let spelling = name.to_string();
            let ptr = self
                .resolve_inbound_declaration(vm, &spelling, overlay, InboundDeclarationKind::Any)
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
            Object::Class(class) => class.owner.package(),
            Object::Enum(enm) => enm.owner.package(),
            _ => None,
        })
        .collect::<Vec<_>>();
    owners.dedup();
    for owner in owners {
        let Object::Package(package) = (unsafe { owner.get() }) else {
            continue;
        };
        // A package contributes its whole surface.
        for ptr in package.classes.values().copied() {
            if !class_ptrs.contains(&ptr) {
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
            RuntimeTy::Union(members) => find_matching_union_member(value, members),
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
                    RuntimeTy::List(elem_ty) => elem_ty.as_ref(),
                    _ => &RuntimeTy::Null,
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
                    _ => (&RuntimeTy::String, &RuntimeTy::Null),
                };

                if !anchor_wire_ty(vm, key_type)
                    .is_ok_and(|key| baml_type::normalize::is_string_key(key.as_ty(), vm))
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
                let capability_kind = trusted_stdlib_capability_kind(&self.stdlib_heads, class);
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
            Object::Bigint(bi) => Ok(BexExternalValue::Bigint((**bi).clone())),
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
            Object::Tombstone => Object::tombstone_reached(),
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
    /// names the class admission resolves. Opaque host values — and anything else with
    /// no readable BAML type, including empty containers — are `HostOnly`
    /// (Case 2, → `rust_type`).
    pub(crate) fn synth_ty_from_value(&self, value: &BexExternalValue) -> SynthTy {
        match value {
            BexExternalValue::Int(_) => SynthTy::Known(RuntimeTy::int()),
            BexExternalValue::Bigint(_) => SynthTy::Known(RuntimeTy::Bigint),
            BexExternalValue::Float(_) => SynthTy::Known(RuntimeTy::float()),
            BexExternalValue::Bool(_) => SynthTy::Known(RuntimeTy::bool()),
            // A `String` value widens to `string`, never a `Literal` (00b3
            // T2/T45 — `identity("hi")` binds `T = string`).
            BexExternalValue::String(_) => SynthTy::Known(RuntimeTy::string()),
            BexExternalValue::Uint8Array(_) => SynthTy::Known(RuntimeTy::Uint8Array),
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
                let elem =
                    synth_collection_element(self, items.iter()).unwrap_or(RuntimeTy::RustType);
                SynthTy::Known(RuntimeTy::List(Box::new(elem)))
            }
            BexExternalValue::Map { entries, .. } => {
                // A map always inhabits `map<string, _>`: every wire entry is
                // string-keyed, so the key is always `string`. When the values
                // carry no evidence (a genuinely empty `{}`, or values that are
                // themselves evidence-free), the value type is the host-only
                // `rust_type` — an empty map binds `Map<string, rust_type>`,
                // never leaving the value TypeVar for Gate A.
                let value_ty =
                    synth_collection_element(self, entries.values()).unwrap_or(RuntimeTy::RustType);
                SynthTy::Known(RuntimeTy::Map {
                    key: Box::new(RuntimeTy::string()),
                    value: Box::new(value_ty),
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
                        SynthTy::Known(RuntimeTy::Class(tn, type_args.clone().into()))
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
                    Some(tn) => SynthTy::Known(RuntimeTy::Enum(tn)),
                    None => SynthTy::HostOnly,
                }
            }
            // A reflected type passed as a value inhabits the `type` metatype.
            BexExternalValue::Adt(BexExternalAdt::Type(_) | BexExternalAdt::TypeDef(_)) => {
                SynthTy::Known(RuntimeTy::Type)
            }
            // A tagged handle's wire type is annotation-only. Call paths that
            // hold a heap permit resolve the rooted object and supply its live
            // TypeHead; inference without that proof must treat it as opaque.
            BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle { .. }) => SynthTy::HostOnly,
            // Media is a concrete leaf BAML type (`image`/`audio`/...): its kind
            // is readable from the value, so `identity<T>(img)` binds T to the
            // real media type rather than the host-only `rust_type` catch-all.
            BexExternalValue::Adt(BexExternalAdt::Media(media)) => {
                SynthTy::Known(RuntimeTy::Media(media.kind))
            }
            // A rendered prompt inhabits `ai.Prompt` — bind T to that rather than
            // falling into the host-only catch-all below.
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
    /// declared slot's name. `None` if no such class is registered.
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
        let unknown = || RuntimeTy::Unknown;
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
            RuntimeTy::Class(formal_name, formal_args),
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
                    return RuntimeTy::Class(tn, args.into());
                }
            }
        }
        self.synth_ty_from_value(value).into_runtime_ty()
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
        RuntimeTy::Union(members) if members.iter().any(RuntimeTy::is_null) => {
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
                    let metadata = UnionMetadata::new(RuntimeTy::Union(members.clone()), selected);
                    Ok(BexExternalValue::Union {
                        value: Box::new(value),
                        metadata,
                    })
                }
            }
        }
        RuntimeTy::Union(members) => {
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
    let RuntimeTy::Union(members) = declared_type else {
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
/// class's type variables (e.g. `Stream<T>`), while the inbound
/// receiver carries them concretely (e.g. `Stream<string>`).
/// Zipping the two yields `{T -> string}`, which
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
        (RuntimeTy::TypeVar(name), _) => {
            out.entry(name.to_string())
                .or_insert_with(|| concrete.clone());
        }
        (RuntimeTy::Class(_, da), RuntimeTy::Class(_, ca)) => {
            for (d, c) in da.iter().zip(ca.iter()) {
                collect_type_var_bindings(d, c, out);
            }
        }
        (RuntimeTy::List(d), RuntimeTy::List(c)) => collect_type_var_bindings(d, c, out),
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
        (RuntimeTy::Union(dm), RuntimeTy::Union(cm)) => {
            for (d, c) in dm.iter().zip(cm.iter()) {
                collect_type_var_bindings(d, c, out);
            }
        }
        (RuntimeTy::Future(dv, de), RuntimeTy::Future(cv, ce)) => {
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
        (RuntimeTy::TypeVar(name), _) => {
            out.entry(name.to_string())
                .or_insert_with(|| concrete.clone());
        }
        (RuntimeTy::Class(_, declared_args), RuntimeTy::Class(_, concrete_args)) => {
            for (declared, concrete) in declared_args.iter().zip(concrete_args) {
                collect_live_type_var_bindings(declared, concrete, out);
            }
        }
        (RuntimeTy::List(declared), RuntimeTy::List(concrete)) => {
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
        (RuntimeTy::Union(declared), RuntimeTy::Union(concrete)) => {
            for (declared, concrete) in declared.iter().zip(concrete) {
                collect_live_type_var_bindings(declared, concrete, out);
            }
        }
        (
            RuntimeTy::Future(declared_value, declared_error),
            RuntimeTy::Future(concrete_value, concrete_error),
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
/// This is the fix for the host-driven streaming type-var bug: a
/// generic method's declared return type (e.g. `Stream.next`'s
/// `T | Done`) reaches the FFI return conversion with `T`
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
        RuntimeTy::TypeVar(name) => bindings
            .get(name.as_str())
            .cloned()
            .unwrap_or_else(|| ty.clone()),
        RuntimeTy::Class(tn, args) => RuntimeTy::Class(
            tn.clone(),
            args.iter()
                .map(|a| substitute_type_vars(a, bindings))
                .collect(),
        ),
        RuntimeTy::List(inner) => RuntimeTy::List(Box::new(substitute_type_vars(inner, bindings))),
        RuntimeTy::Map { key, value } => RuntimeTy::Map {
            key: Box::new(substitute_type_vars(key, bindings)),
            value: Box::new(substitute_type_vars(value, bindings)),
        },
        RuntimeTy::Union(members) => RuntimeTy::Union(
            members
                .iter()
                .map(|m| substitute_type_vars(m, bindings))
                .collect(),
        ),
        RuntimeTy::Future(value, error) => RuntimeTy::Future(
            Box::new(substitute_type_vars(value, bindings)),
            Box::new(substitute_type_vars(error, bindings)),
        ),
        // A function-typed parameter (`f: (T) -> R`) carries call type-vars in its
        // params/return/throws. Substitute them so an explicitly-bound closure
        // param materializes against concrete types (J13: `apply[int,int]` binds
        // `f` as `(int) -> int`, not the unvalidatable `(T) -> R`).
        RuntimeTy::Function {
            params,
            ret,
            throws,
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
        RuntimeTy::TypeVar(name) => Some(name.to_string()),
        RuntimeTy::Class(_, args) => args.iter().find_map(first_unbound_type_var),
        RuntimeTy::List(inner) => first_unbound_type_var(inner),
        RuntimeTy::Map { key, value, .. } => {
            first_unbound_type_var(key).or_else(|| first_unbound_type_var(value))
        }
        RuntimeTy::Union(members) => members.iter().find_map(first_unbound_type_var),
        RuntimeTy::Future(value, error) => {
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
#[expect(
    deprecated,
    reason = "fact-free by necessity: a per-pair primitive with no VM to supply facts"
)]
pub(crate) fn infer_bindings_runtime(
    formal: &RuntimeTy,
    actual: &RuntimeTy,
    out: &mut indexmap::IndexMap<String, RuntimeTy>,
) {
    let mut bindings: rustc_hash::FxHashMap<baml_type::ParamTy, Ty<baml_type::TypeName>> =
        rustc_hash::FxHashMap::default();
    baml_type_runtime::infer_value_bindings(
        &Ty::from(formal),
        &Ty::from(actual),
        &mut bindings,
        &baml_type::normalize::NoFacts,
    );
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
#[expect(
    deprecated,
    reason = "fact-free by necessity: inference runs at the host entry boundary, before any VM exists to supply facts"
)]
pub(crate) fn infer_bindings_runtime_checked(
    pairs: &[(RuntimeTy, RuntimeTy)],
) -> Result<indexmap::IndexMap<String, RuntimeTy>, String> {
    let mut cons = baml_type_runtime::InferenceConstraints::new();
    for (formal, actual) in pairs {
        cons.record(
            &Ty::from(formal),
            &Ty::from(actual),
            &baml_type::normalize::NoFacts,
        );
    }
    let bindings = cons
        .solve(&baml_type::normalize::NoFacts)
        .map_err(|e| e.message)?;
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
        T::List(inner) => template_max_type_arg_ref(inner),
        T::Map { key, value, .. } | T::Future(key, value) => template_max_type_arg_ref(key)
            .into_iter()
            .chain(template_max_type_arg_ref(value))
            .max(),
        T::Union(members) => members.iter().filter_map(template_max_type_arg_ref).max(),
        T::Class(_, args) => args.iter().filter_map(template_max_type_arg_ref).max(),
        T::Interface(_, args, assoc) => args
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
        RuntimeTy::TypeVar(name) => {
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
        RuntimeTy::List(inner) => {
            walk_var_positions(inner, in_closure, out);
        }
        RuntimeTy::Map { key, value, .. } => {
            walk_var_positions(key, in_closure, out);
            walk_var_positions(value, in_closure, out);
        }
        RuntimeTy::Union(members) => {
            // A union with ≥2 direct `TypeVar` members is un-inferrable (no
            // principled split). Mark those direct members must-specify so the
            // rule-4 default skips them (03b J12).
            let direct_tvs: Vec<&str> = members
                .iter()
                .filter_map(|m| match m {
                    RuntimeTy::TypeVar(name) => Some(name.as_str()),
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
        RuntimeTy::Class(_, args) => {
            for a in args {
                walk_var_positions(a, in_closure, out);
            }
        }
        RuntimeTy::Future(value, error) => {
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
    /// `BexEngine::land_engine_value`).
    pub(crate) fn into_runtime_ty(self) -> RuntimeTy {
        match self {
            SynthTy::Known(ty) => ty,
            SynthTy::HostOnly => RuntimeTy::RustType,
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

/// The standard library's class for a rendered prompt.
pub(crate) const PROMPT_CLASS: &str = "ai.Prompt";

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
            (BexExternalValue::Array { element_type, .. }, RuntimeTy::List(expected)) => {
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
            RuntimeTy::Literal(..) | RuntimeTy::EnumVariant(..) | RuntimeTy::Unknown
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
        if matches!(member, RuntimeTy::Unknown) {
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
pub(crate) fn described_value_type(value: &BexExternalValue) -> String {
    match value {
        BexExternalValue::Instance { class_name, .. } => format!("instance of `{class_name}`"),
        BexExternalValue::Variant { enum_name, .. } => format!("variant of `{enum_name}`"),
        other => other.type_name().to_string(),
    }
}

pub(crate) fn type_name_matches_external_name(
    external_name: &str,
    type_name: &baml_type::TypeName,
) -> bool {
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
    // user-package types, so an `Instance` admitted to a declared class
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
fn value_matches_type(value: &BexExternalValue, ty: &RuntimeTy) -> bool {
    match (value, ty) {
        // `Unknown` is the engine's "any value matches" sentinel
        // (TypeScript `unknown` semantics — see `baml_type::RuntimeTy::Unknown`).
        // Used by the stdlib generics hardcode in `baml_compiler2_mir::lower`
        // so e.g. `Stream<T>.next() -> T | Done`
        // accepts any partial-stream payload as the `T` arm.
        (_, RuntimeTy::Unknown) => true,
        (BexExternalValue::Null, RuntimeTy::Null) => true,
        (BexExternalValue::Int(_), RuntimeTy::Int) => true,
        (BexExternalValue::Bigint(_), RuntimeTy::Bigint) => true,
        (BexExternalValue::Float(_), RuntimeTy::Float) => true,
        (BexExternalValue::Bool(_), RuntimeTy::Bool) => true,
        (BexExternalValue::String(_), RuntimeTy::String) => true,
        // Literal types match their corresponding runtime values
        (BexExternalValue::Int(value), RuntimeTy::Literal(Literal::Int(expected), _)) => {
            value == expected
        }
        (BexExternalValue::Bigint(value), RuntimeTy::Literal(Literal::Bigint(expected), _)) => {
            value == expected
        }
        (BexExternalValue::Float(value), RuntimeTy::Literal(Literal::Float(expected), _)) => {
            float_literal_matches(*value, expected)
        }
        (BexExternalValue::Uint8Array(_), RuntimeTy::Uint8Array) => true,
        (BexExternalValue::String(value), RuntimeTy::Literal(Literal::String(expected), _)) => {
            value.as_str() == expected
        }
        (BexExternalValue::Bool(value), RuntimeTy::Literal(Literal::Bool(expected), _)) => {
            value == expected
        }
        (BexExternalValue::Array { items, .. }, RuntimeTy::List(expected_element)) => items
            .iter()
            .all(|item| value_matches_type(item, expected_element)),
        (
            BexExternalValue::Map { entries, .. },
            RuntimeTy::Map {
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
            RuntimeTy::Class(tn, expected_args),
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
        (BexExternalValue::Variant { enum_name, .. }, RuntimeTy::Enum(tn)) => {
            type_name_matches_external_name(enum_name, tn)
        }
        (
            BexExternalValue::Variant {
                enum_name,
                variant_name,
            },
            RuntimeTy::EnumVariant(tn, expected_variant),
        ) => {
            type_name_matches_external_name(enum_name, tn)
                && variant_name == expected_variant.as_str()
        }
        (BexExternalValue::Adt(BexExternalAdt::Media(media)), RuntimeTy::Media(expected_kind)) => {
            *expected_kind == baml_type::MediaKind::Generic || media.kind == *expected_kind
        }
        (BexExternalValue::HostValue(value), RuntimeTy::Function { .. }) => {
            value.kind == bex_external_types::HostValueKind::Callable
        }
        (BexExternalValue::RustData(_), RuntimeTy::RustType) => true,
        (BexExternalValue::HostValue(value), RuntimeTy::RustType) => {
            value.kind == bex_external_types::HostValueKind::Opaque
        }
        (BexExternalValue::FunctionRef { .. }, RuntimeTy::Function { .. }) => true,
        (
            BexExternalValue::Adt(BexExternalAdt::Type(_) | BexExternalAdt::TypeDef(_)),
            RuntimeTy::Type,
        ) => true,
        (union_value @ BexExternalValue::Union { metadata, .. }, RuntimeTy::Union(members)) => {
            members.iter().any(|member| {
                // Recurse with the ANNOTATED carrier, not the unwrapped
                // payload: sparse inbound annotations are the value's
                // identity for media members (the wrapper's shape never
                // matches `Media` structurally), and the per-member arms
                // below unwrap for every non-annotation-dependent case.
                selected_arm_equal(member, &metadata.selected_option)
                    && value_matches_type(union_value, member)
            })
        }
        // `value_satisfies_json` peels sparse inbound leaf annotations (the
        // Swift bridge annotates every json scalar leaf) but still rejects
        // genuine union carriers and annotations outside the JSON algebra.
        (union_value @ BexExternalValue::Union { .. }, RuntimeTy::TypeAlias(name))
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
        (BexExternalValue::Union { metadata, .. }, RuntimeTy::Media(expected_kind))
            if metadata.is_inbound_type_annotation
                && matches!(
                    &metadata.selected_option,
                    RuntimeTy::Media(kind)
                        if *expected_kind == baml_type::MediaKind::Generic
                            || kind == expected_kind
                ) =>
        {
            true
        }
        (BexExternalValue::Union { value, .. }, ty) => value_matches_type(value, ty),
        (value, RuntimeTy::TypeAlias(name)) if is_canonical_json_alias(name) => {
            value_satisfies_json(value)
        }
        // Handle nested unions (including nullable `T | null`) in the type.
        (value, RuntimeTy::Union(members)) => members.iter().any(|m| value_matches_type(value, m)),
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
pub(crate) fn class_type_args_compatible(
    wire_args: &[RuntimeTy],
    expected_args: &[RuntimeTy],
) -> bool {
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
    matches!(ty, RuntimeTy::TypeVar(..) | RuntimeTy::Unknown)
}

/// Structural compatibility of a wire-supplied type against a declared
/// (substituted) type, used to disambiguate generic instances at the FFI
/// boundary without depending on full subtyping. Lenient:
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
        (_, T::TypeVar(..) | T::Unknown) => true,
        (T::TypeVar(..) | T::Unknown, _) => true,
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
fn resolve_effective_type(value: Value, declared_type: &RuntimeTy) -> &RuntimeTy {
    match declared_type {
        RuntimeTy::Union(members) => find_matching_union_member(value, members)
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
    let RuntimeTy::Union(members) = declared_type else {
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
        ValueKind::Null => members.iter().find(|m| matches!(m, RuntimeTy::Null)),
        ValueKind::Int(value) => members
            .iter()
            .find(|member| {
                matches!(member, RuntimeTy::Literal(Literal::Int(expected), _) if value == *expected)
            })
            .or_else(|| {
                members
                    .iter()
                    .find(|member| matches!(member, RuntimeTy::Int))
            }),
        ValueKind::Bool(value) => members
            .iter()
            .find(|member| {
                matches!(member, RuntimeTy::Literal(Literal::Bool(expected), _) if value == *expected)
            })
            .or_else(|| {
                members
                    .iter()
                    .find(|member| matches!(member, RuntimeTy::Bool))
            }),
        ValueKind::Object(ptr) => {
            let obj = unsafe { ptr.get() };
            match obj {
                Object::Float(value) => members
                    .iter()
                    .find(|member| {
                        matches!(member, RuntimeTy::Literal(Literal::Float(expected), _)
                            if float_literal_matches(*value, expected))
                    })
                    .or_else(|| {
                        members
                            .iter()
                            .find(|member| matches!(member, RuntimeTy::Float))
                    }),
                Object::String(value) => members
                    .iter()
                    .find(|member| {
                        matches!(member, RuntimeTy::Literal(Literal::String(expected), _)
                            if value.as_str() == expected)
                    })
                    .or_else(|| {
                        members
                            .iter()
                            .find(|member| matches!(member, RuntimeTy::String))
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
                            matches!(m, RuntimeTy::Class(tn, expected_args)
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
                                    (RuntimeTy::EnumVariant(tn, expected), Some(actual))
                                        if enm.name.declared() == Some(tn)
                                            && expected.as_str() == actual
                                )
                            })
                            .or_else(|| {
                                members.iter().find(
                                    |m| matches!(m, RuntimeTy::Enum(tn) if enm.name.declared() == Some(tn))
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
                        matches!(member, RuntimeTy::List(expected)
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
                    .find(|m| matches!(m, RuntimeTy::Uint8Array)),
                Object::Bigint(value) => members
                    .iter()
                    .find(|member| {
                        matches!(member, RuntimeTy::Literal(Literal::Bigint(expected), _)
                            if value.as_ref() == expected)
                    })
                    .or_else(|| {
                        members
                            .iter()
                            .find(|member| matches!(member, RuntimeTy::Bigint))
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
            matches!(member, RuntimeTy::Union(nested)
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
pub(crate) fn stdlib_media_wrapper_kind(annotation: &RuntimeTy) -> Option<baml_type::MediaKind> {
    let RuntimeTy::Class(name, args) = annotation else {
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
        DeclarationName, EnumVariant, MediaContent, MediaValue, Object, Value,
        types::{Class, Enum},
    };

    use super::*;

    fn list(inner: RuntimeTy) -> RuntimeTy {
        RuntimeTy::List(Box::new(inner))
    }

    fn map(value: RuntimeTy) -> RuntimeTy {
        RuntimeTy::Map {
            key: Box::new(RuntimeTy::string()),
            value: Box::new(value),
        }
    }

    fn string_literal(value: &str) -> RuntimeTy {
        RuntimeTy::Literal(Literal::String(value.to_string()), Freshness::Regular)
    }

    fn float_literal(value: &str) -> RuntimeTy {
        RuntimeTy::Literal(Literal::Float(value.to_string()), Freshness::Regular)
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
        RuntimeTy::Class(TypeName::from_dotted_path(name), Box::new([]))
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
            owner: bex_vm_types::types::Owner::anonymous(),
        })));
        let happy = RuntimeTy::EnumVariant(mood.clone(), Name::new("HAPPY"));
        let broad = RuntimeTy::Enum(mood);
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
                stream_done: false,
                type_tag,
                has_cleanup: false,
                methods: indexmap::IndexMap::new(),
                generic_param_count: 0,
                owner: bex_vm_types::types::Owner::anonymous(),
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
            baml_type::typetag::TypeTag::of_static_index(0),
        );
        let done_value = Value::object(tlab.alloc_instance(done_class, Vec::new()));

        let partial_arm = RuntimeTy::Union(Box::new([
            RuntimeTy::Class(TypeName::local(dynamic_name), Box::new([])),
            RuntimeTy::Null,
        ]));
        let done_arm = RuntimeTy::Class(done_name, Box::new([]));
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
        let optional = RuntimeTy::Union(Box::new([inner.clone(), RuntimeTy::null()]));

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
    fn empty_array_uses_its_declared_element_type_to_select_union_arm() {
        let int_list = list(RuntimeTy::int());
        let string_list = list(RuntimeTy::string());
        let value = BexExternalValue::Array {
            element_type: RuntimeTy::string(),
            items: Vec::new(),
        };
        let wrapped = maybe_wrap_union(
            value,
            &RuntimeTy::Union(Box::new([int_list, string_list.clone()])),
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
            &RuntimeTy::Union(Box::new([int_map, string_map.clone()])),
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
            find_matching_member(&value, &[RuntimeTy::string(), draft.clone()]).unwrap(),
            draft
        );
    }

    #[test]
    fn enum_variant_matching_checks_value_and_prefers_exact_variant() {
        let mood = TypeName::from_dotted_path("user.callbacks.Mood");
        let happy = RuntimeTy::EnumVariant(mood.clone(), Name::new("HAPPY"));
        let sad = RuntimeTy::EnumVariant(mood.clone(), Name::new("SAD"));
        let broad = RuntimeTy::Enum(mood.clone());
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
                &[RuntimeTy::float(), negative_zero.clone()]
            )
            .unwrap(),
            negative_zero
        );
    }

    #[test]
    fn structurally_duplicate_members_select_the_first() {
        let declared = RuntimeTy::Union(Box::new([
            RuntimeTy::int(),
            RuntimeTy::int(),
            RuntimeTy::string(),
        ]));

        let RuntimeTy::Union(members) = &declared else {
            unreachable!()
        };
        let outbound_selected = find_matching_member(&BexExternalValue::Int(7), members).unwrap();
        assert!(runtime_ty_structurally_equal(
            &outbound_selected,
            &RuntimeTy::int()
        ));
    }

    #[test]
    fn enum_variant_selected_arm_has_exact_structural_identity() {
        let mood = TypeName::from_dotted_path("user.callbacks.Mood");
        let happy = RuntimeTy::EnumVariant(mood.clone(), Name::new("HAPPY"));
        let another_happy = RuntimeTy::EnumVariant(mood.clone(), Name::new("HAPPY"));
        let sad = RuntimeTy::EnumVariant(mood, Name::new("SAD"));
        assert!(runtime_ty_structurally_equal(&happy, &another_happy));
        assert!(!runtime_ty_structurally_equal(&happy, &sad));
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
}

#[cfg(test)]
mod inference_unifier_tests {
    //! Unit tests for the `RuntimeTy` generic unifier (01a/01b): `union_runtime_ty`
    //! and `infer_bindings_runtime`. These mirror the TIR's `union_ty` /
    //! `infer_bindings_inner` semantics — union-merge, `null`-strip, no arm
    //! routing (00b3 G5).
    use baml_type::Name;

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
