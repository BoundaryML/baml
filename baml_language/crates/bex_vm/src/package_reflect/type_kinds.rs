//! BEP-066 reflection kind views over `Object::Type`.

use std::sync::Arc;

use baml_compiler_diagnostics::{
    Diagnostic, DiagnosticId, DiagnosticPhase,
    runtime_type::{self, DuplicateMemberKind, InvalidIdentifierKind, SerializedKeyContainer},
};
use bex_heap::TlabHolder;
use bex_vm_types::types::{
    Class, ClassField, Enum, EnumVariant, InterfaceDef, MethodImpl, Object, PortableTypeDef,
    RuntimeImplRule, TypeValue, Value,
};
use indexmap::IndexMap;

use super::{
    BamlClassArrayReflectTypeView_for_Type, BamlClassArrayType,
    BamlClassClassReflectTypeView_for_Type, BamlClassClassType,
    BamlClassEnumReflectTypeView_for_Type, BamlClassEnumType,
    BamlClassFunctionReflectTypeView_for_Type, BamlClassFunctionType,
    BamlClassInterfaceImplementation, BamlClassInterfaceReflectTypeView_for_Type,
    BamlClassInterfaceType, BamlClassLiteralReflectTypeView_for_Type,
    BamlClassMapReflectTypeView_for_Type, BamlClassMapType,
    BamlClassPrimitiveReflectTypeView_for_Type, BamlClassType,
    BamlClassUnionReflectTypeView_for_Type, BamlClassUnionType, BamlNamespaceArray,
    BamlNamespaceArrayReflect, BamlNamespaceClass, BamlNamespaceClassReflect, BamlNamespaceEnum,
    BamlNamespaceEnumReflect, BamlNamespaceFunction, BamlNamespaceFunctionReflect,
    BamlNamespaceInterface, BamlNamespaceInterfaceReflect, BamlNamespaceLiteral,
    BamlNamespaceLiteralReflect, BamlNamespaceMap, BamlNamespaceMapReflect, BamlNamespacePrimitive,
    BamlNamespacePrimitiveReflect, BamlNamespaceUnion, BamlNamespaceUnionReflect,
    PackageReflectImpl, copy,
};
use crate::BexVm;

impl BexVm {
    /// Create the declarations a host asked for, and return the type naming them.
    ///
    /// This is the one inbound path that *creates* a declaration, and it is
    /// legitimate: the host is not shipping a definition to be reconstructed, it
    /// is driving BAML to declare one over FFI. BAML allocates it, owns it, and
    /// keeps it — so the resulting heads point at declarations this engine made,
    /// exactly like every other runtime declaration.
    ///
    /// A request describes its graph by name because names are all it has: a
    /// class being authored has no head to refer to until it exists. Those names
    /// are resolved only *within the request* — nothing consults a global
    /// registry, so a request naming a compiled FQN cannot capture the compiled
    /// declaration's identity.
    pub fn materialize_portable_type_def(
        &mut self,
        definition: &PortableTypeDef,
    ) -> Result<TypeValue, String> {
        // Allocate every declaration first, field-less, so members that name
        // each other have something to point at. Same ordering constraint the
        // runtime class builder has, for the same reason.
        let mut declared = IndexMap::new();
        for class in &definition.classes {
            let type_tag = baml_type::typetag::TypeTag::fresh_dynamic();
            let ptr = self.tlab.alloc(Object::Class(Box::new(Class {
                // The wire qtn is the request's internal linking spelling; the
                // declaration it authors is anonymous — item name only.
                name: bex_vm_types::DeclarationName::Anonymous(class.name.name().clone()),
                fields: Vec::new(),
                description: class.metadata.description.clone(),
                alias: class.metadata.alias.clone(),
                docstring: class.metadata.docstring.clone(),
                other: class.metadata.other.clone(),
                type_tag,
                ty_attr: baml_type::TyAttr::default(),
                has_cleanup: false,
                generic_param_count: class.generic_param_count,
                owner: bex_vm_types::HeapPtr::null(),
            })));
            declared.insert(
                class.name.clone(),
                bex_vm_types::TypeHead::new(ptr, type_tag),
            );
        }
        for enm in &definition.enums {
            let type_tag = baml_type::typetag::TypeTag::fresh_dynamic();
            let ptr = self.tlab.alloc(Object::Enum(Box::new(Enum {
                name: bex_vm_types::DeclarationName::Anonymous(enm.name.name().clone()),
                variants: enm
                    .variants
                    .iter()
                    .map(|variant| EnumVariant {
                        name: variant.name.clone(),
                        description: variant.metadata.description.clone(),
                        alias: variant.metadata.alias.clone(),
                        docstring: variant.metadata.docstring.clone(),
                        other: variant.metadata.other.clone(),
                        skip: variant.skip,
                    })
                    .collect(),
                description: enm.metadata.description.clone(),
                alias: enm.metadata.alias.clone(),
                docstring: enm.metadata.docstring.clone(),
                other: enm.metadata.other.clone(),
                type_tag,
                ty_attr: baml_type::TyAttr::default(),
                owner: bex_vm_types::HeapPtr::null(),
            })));
            declared.insert(enm.name.clone(), bex_vm_types::TypeHead::new(ptr, type_tag));
        }

        // Anchor every name the request used: a member of this request, or a
        // declaration the program already has. Anything else is a request for a
        // type that does not exist, which is an error rather than a stand-in.
        let anchor = |vm: &Self, name: &baml_type::TypeName| -> Result<_, String> {
            declared
                .get(name)
                .copied()
                .or_else(|| vm.declaration_head(name))
                .ok_or_else(|| format!("host type definition names unknown type `{name}`"))
        };

        let root = definition
            .root
            .try_map_heads(&mut |name: &baml_type::TypeName| anchor(self, name))
            .and_then(|ty| {
                bex_vm_types::RealizedTy::try_from(ty)
                    .map_err(|e| format!("host type definition root is not realized: {e}"))
            })?;

        for class in &definition.classes {
            let mut fields = Vec::with_capacity(class.fields.len());
            for field in &class.fields {
                let ty = field
                    .ty
                    .try_map_heads(&mut |name: &baml_type::TypeName| anchor(self, name))
                    .and_then(|ty| {
                        bex_vm_types::RealizedTy::try_from(ty).map_err(|e| {
                            format!(
                                "host type definition field `{}.{}` is not realized: {e}",
                                class.name, field.name
                            )
                        })
                    })?;
                fields.push(ClassField {
                    name: field.name.clone(),
                    field_type: bex_vm_types::RuntimeTy::from(ty.clone()),
                    field_template: bex_vm_types::TyTemplate::from(ty),
                    description: field.metadata.description.clone(),
                    alias: field.metadata.alias.clone(),
                    docstring: field.metadata.docstring.clone(),
                    other: field.metadata.other.clone(),
                    skip: field.skip,
                    runtime_type: None,
                });
            }
            let head = declared[&class.name];
            let Object::Class(declaration) = self.get_object_mut(head.ptr()) else {
                unreachable!("a just-allocated class changed variant")
            };
            declaration.fields = fields;
        }

        Ok(TypeValue::new(root))
    }
}

impl BamlNamespaceArray for PackageReflectImpl {}

#[derive(Clone, Debug)]
struct InterfaceWitness {
    interface_ptr: bex_vm_types::HeapPtr,
    interface_ty: bex_vm_types::RealizedTy,
    field_links: IndexMap<baml_type::Name, baml_type::Name>,
}

pub(super) struct WitnessField {
    pub(super) name: String,
    pub(super) ty: bex_vm_types::RealizedTy,
}

pub(super) struct ValidatedClassWitness {
    interface_ptr: bex_vm_types::HeapPtr,
    interface_args: Vec<bex_vm_types::RealizedTy>,
    interface_assoc: Vec<(baml_type::Name, bex_vm_types::RealizedTy)>,
    field_links: Vec<u32>,
}

fn witness_state(vm: &BexVm, value: Value) -> Result<InterfaceWitness, String> {
    let instance = vm
        .as_instance(&value)
        .map_err(|_| "implementations must contain reflect.interface.Implementation values")?;
    let Object::Class(class) = vm.get_object(instance.class) else {
        unreachable!("Instance.class must point to Object::Class")
    };
    if class.name.to_string() != "reflect.interface.Implementation" {
        return Err("implementations must contain reflect.interface.Implementation values".into());
    }
    vm.as_rust_data::<InterfaceWitness>(&instance.load_field(0))
        .cloned()
        .map_err(|_| "invalid reflect.interface.Implementation handle".into())
}

pub(super) fn validate_class_witnesses(
    vm: &BexVm,
    class_fields: &[WitnessField],
    implementations: &[Value],
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<ValidatedClassWitness> {
    let mut witnesses = Vec::with_capacity(implementations.len());
    for implementation in implementations {
        match witness_state(vm, *implementation) {
            Ok(witness) => witnesses.push(witness),
            Err(message) => {
                diagnostics.push(compiler_diagnostic(DiagnosticId::TypeMismatch, message));
            }
        }
    }

    // All aggregate witness checks happen before allocating the class/type
    // value (C-12, Fail-Before-Type).
    //
    // BUG: only intra-batch duplicates are rejected. A witness for `I` on a
    // class that a static blanket rule (`implement<T extends Bound> I for T`)
    // already covers is not detected; the resolver tries the static slice first
    // and returns on the first match, so such a witness is silently shadowed
    // rather than rejected. Coherence (TYPE_SYSTEM.md, "Interface Coherence")
    // says at most one implementation per (type, interface) — this should fail
    // closed at registration by probing `type_implements` for the fresh class
    // against the static rules before allocating.
    let mut unique_witnesses = std::collections::HashSet::new();
    for witness in &witnesses {
        if !unique_witnesses.insert(witness.interface_ty.clone()) {
            diagnostics.push(compiler_diagnostic(
                DiagnosticId::OverlappingImplements,
                format!(
                    "duplicate structural witness for `{}`",
                    witness.interface_ty
                ),
            ));
        }
    }
    let mut validated = Vec::with_capacity(witnesses.len());
    for witness in &witnesses {
        let Object::Interface(interface) = vm.get_object(witness.interface_ptr) else {
            diagnostics.push(compiler_diagnostic(
                DiagnosticId::TypeMismatch,
                "witness interface is not loaded".into(),
            ));
            continue;
        };
        for required in &interface.requires {
            let present = witnesses.iter().any(|candidate| {
                matches!(
                    &candidate.interface_ty,
                    bex_vm_types::RealizedTy::Interface(name, _, _, _)
                        if name == &required.name
                )
            });
            if !present {
                diagnostics.push(compiler_diagnostic(
                    DiagnosticId::MissingRequiredInterface,
                    format!(
                        "interface witness for `{}` requires a witness for `{}`",
                        interface.name.display_name(),
                        baml_type::HeadDisplay::head_display_name(&required.name)
                    ),
                ));
            }
        }
        let mut physical_links = Vec::with_capacity(interface.fields.len());
        for required in &interface.fields {
            let Some(class_field_name) = witness.field_links.get(&required.name) else {
                diagnostics.push(compiler_diagnostic(
                    DiagnosticId::TypeMismatch,
                    format!(
                        "interface witness for `{}` is missing required field `{}`",
                        interface.name.display_name(),
                        required.name
                    ),
                ));
                continue;
            };
            let Some((slot, class_field)) = class_fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.name == class_field_name.as_str())
            else {
                diagnostics.push(compiler_diagnostic(
                    DiagnosticId::TypeMismatch,
                    format!(
                        "interface field `{}.{}` links to missing class field `{}`",
                        interface.name.display_name(),
                        required.name,
                        class_field_name
                    ),
                ));
                continue;
            };
            let required_ty =
                match realize_witness_field_type(&required.ty, interface, &witness.interface_ty) {
                    Ok(ty) => ty,
                    Err(message) => {
                        diagnostics.push(compiler_diagnostic(
                            DiagnosticId::TypeMismatch,
                            format!(
                                "interface field `{}.{}` {message}",
                                interface.name.display_name(),
                                required.name
                            ),
                        ));
                        continue;
                    }
                };
            if class_field.ty != required_ty {
                diagnostics.push(compiler_diagnostic(
                    DiagnosticId::TypeMismatch,
                    format!(
                        "interface field `{}.{}` requires `{required_ty}`, but class field `{}` has `{}`",
                        interface.name.display_name(),
                        required.name,
                        class_field_name,
                        class_field.ty
                    ),
                ));
                continue;
            }
            physical_links.push(u32::try_from(slot).expect("class field count fits u32"));
        }
        let (interface_args, interface_assoc) = match &witness.interface_ty {
            bex_vm_types::RealizedTy::Interface(_, args, assoc, _) => (args.clone(), assoc.clone()),
            _ => unreachable!("implementation() only creates interface witnesses"),
        };
        validated.push(ValidatedClassWitness {
            interface_ptr: witness.interface_ptr,
            interface_args,
            interface_assoc,
            field_links: physical_links,
        });
    }
    validated
}

pub(super) fn register_class_witnesses(
    vm: &mut BexVm,
    class_ptr: bex_vm_types::HeapPtr,
    ty: &bex_vm_types::RealizedTy,
    witnesses: Vec<ValidatedClassWitness>,
) {
    for witness in witnesses {
        let Object::Interface(interface) = vm.get_object(witness.interface_ptr) else {
            unreachable!("witness interface validated before allocation")
        };
        let mut methods = IndexMap::new();
        let for_ty_pattern = bex_vm_types::TyTemplate::from(ty.clone());
        // The default-body frame is `[Self, iface args..]` — associated types
        // are not frame slots; the body's `Self.X` projection templates reduce
        // through this rule's `interface_assoc` bindings at realization.
        let mut default_frame = vec![for_ty_pattern.clone()];
        default_frame.extend(
            witness
                .interface_args
                .iter()
                .cloned()
                .map(bex_vm_types::TyTemplate::from),
        );
        for method in &interface.methods {
            // A witness supplies fields only; every method comes from the
            // interface's default body, which the loader bound to a pointer.
            // (The gate above rejected any interface with a required method.)
            if method.default.is_none() {
                continue;
            }
            let callee = method.default_fn;
            debug_assert!(
                !callee.is_null(),
                "interface `{}` default `{}` was pooled but never bound",
                interface.name,
                method.name
            );
            methods.insert(
                method.name.clone(),
                MethodImpl {
                    fqn: callee,
                    frame: default_frame.clone(),
                },
            );
        }
        // The witness is an ordinary heap `Object::ImplRule` — the resolver
        // borrows it exactly like a package-owned rule and the collector keeps
        // its `interface_head`/`methods[].fqn` current — so the side table
        // holds only a pointer to it, never a copy.
        let rule = vm.tlab.alloc(Object::ImplRule(Box::new(RuntimeImplRule {
            interface_head: witness.interface_ptr,
            for_ty_pattern,
            generic_param_bounds: Vec::new(),
            interface_args: witness
                .interface_args
                .into_iter()
                .map(bex_vm_types::TyTemplate::from)
                .collect(),
            interface_assoc: witness
                .interface_assoc
                .into_iter()
                .map(|(name, ty)| (name, bex_vm_types::TyTemplate::from(ty)))
                .collect(),
            methods,
            field_links: witness.field_links.into_boxed_slice(),
        })));
        vm.dynamic_dispatch.register_rule(
            witness.interface_ptr,
            crate::package_load::DynRuleEntry {
                class: class_ptr,
                rule,
            },
        );
    }
}

impl BamlNamespaceClass for PackageReflectImpl {
    fn _new(
        vm: &mut BexVm,
        name: &bex_str::BexStr,
        fields: &IndexMap<bex_str::BexStr, Value>,
        implementations: &[Value],
    ) -> Result<Value, crate::errors::VmRustFnError> {
        let class_name = name.as_str();
        let mut diagnostics = Vec::new();
        if !is_baml_identifier(class_name) {
            diagnostics.push(
                runtime_type::invalid_identifier(InvalidIdentifierKind::Class, class_name)
                    .with_phase(DiagnosticPhase::Hir),
            );
        }

        let mut class_fields = Vec::with_capacity(fields.len());
        let mut witness_fields = Vec::with_capacity(fields.len());
        let mut seen_serialized_keys = std::collections::HashSet::new();
        for (field_name, value) in fields {
            if !is_baml_identifier(field_name.as_str()) {
                diagnostics.push(
                    runtime_type::invalid_identifier(
                        InvalidIdentifierKind::Field,
                        &format!("{class_name}.{field_name}"),
                    )
                    .with_phase(DiagnosticPhase::Hir),
                );
            }
            let row = match reflected_type_row(vm, *value) {
                Ok(row) => row,
                Err(message) => {
                    diagnostics.push(compiler_diagnostic(DiagnosticId::TypeMismatch, message));
                    continue;
                }
            };
            let serialized_key = row.alias.as_deref().unwrap_or(field_name.as_str());
            if !seen_serialized_keys.insert(serialized_key.to_string()) {
                diagnostics.push(
                    runtime_type::duplicate_serialized_key(
                        serialized_key,
                        SerializedKeyContainer::Class,
                    )
                    .with_phase(DiagnosticPhase::Hir),
                );
            }
            witness_fields.push(WitnessField {
                name: field_name.to_string(),
                ty: row.type_value.ty.clone(),
            });
            class_fields.push(ClassField {
                name: field_name.to_string(),
                field_type: row.type_value.ty.clone().into(),
                field_template: bex_vm_types::TyTemplate::from(row.type_value.ty.clone()),
                description: row.description,
                alias: row.alias,
                docstring: row.docstring,
                other: row.other,
                skip: false,
                runtime_type: Some(row.type_value),
            });
        }

        let witnesses =
            validate_class_witnesses(vm, &witness_fields, implementations, &mut diagnostics);
        if !diagnostics.is_empty() {
            return Err(crate::errors::VmRustFnError::thrown_fresh(
                alloc_compilation_error(vm, &diagnostics),
            ));
        }

        let type_tag = baml_type::typetag::TypeTag::fresh_dynamic();
        let class_ptr = vm.tlab.alloc(Object::Class(Box::new(Class {
            name: bex_vm_types::DeclarationName::Anonymous(baml_type::Name::new(class_name)),
            fields: class_fields,
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            type_tag,
            ty_attr: baml_type::TyAttr::default(),
            has_cleanup: false,
            generic_param_count: 0,
            owner: bex_vm_types::HeapPtr::null(),
        })));
        // The head is built off the declaration that was just allocated, so the
        // type reaches it directly — there is no table to consult and no name
        // to re-resolve.
        let ty = bex_vm_types::RealizedTy::Class(
            bex_vm_types::TypeHead::new(class_ptr, type_tag),
            Vec::new(),
            baml_type::TyAttr::default(),
        );
        register_class_witnesses(vm, class_ptr, &ty, witnesses);
        Ok({
            let ty_value = Value::object(vm.tlab.alloc_type(TypeValue::new(ty)));
            alloc_kind_view(vm, baml_type::type_kind::TypeKind::Class, ty_value)
        })
    }

    fn builder(vm: &mut BexVm, name: &bex_str::BexStr) -> Value {
        super::runtime_class_builder::alloc_builder(vm, name.as_str())
    }

    fn get_field(
        vm: &mut BexVm,
        value: &Value,
        name: &bex_str::BexStr,
    ) -> Result<Value, crate::errors::VmRustFnError> {
        let fail = |vm: &mut BexVm, message: String| {
            crate::errors::VmRustFnError::thrown_fresh(alloc_compilation_error(
                vm,
                &[compiler_diagnostic(DiagnosticId::TypeMismatch, message)],
            ))
        };
        let fail_non_instance = |vm: &mut BexVm| {
            let diagnostic = runtime_type::expected_class_instance(
                "reflect.class.get_field",
                &vm.type_of(value).to_string(),
            )
            .with_phase(DiagnosticPhase::Hir);
            crate::errors::VmRustFnError::thrown_fresh(alloc_compilation_error(vm, &[diagnostic]))
        };
        let Some(instance_ptr) = value.as_object_ptr() else {
            return Err(fail_non_instance(vm));
        };
        let (field_value, class_name) = {
            let Object::Instance(instance) = vm.get_object(instance_ptr) else {
                return Err(fail_non_instance(vm));
            };
            let Object::Class(class) = vm.get_object(instance.class) else {
                unreachable!("Instance.class must point to Object::Class")
            };
            let Some(index) = class
                .fields
                .iter()
                .position(|field| field.name == name.as_str())
            else {
                return Err(fail(
                    vm,
                    format!(
                        "class `{}` has no field `{name}`",
                        class.name.display_name()
                    ),
                ));
            };
            (
                instance.load_field(index),
                class.name.display_name().to_string(),
            )
        };
        // The caller's `T`. Erasing a missing one to `unknown` would make the
        // membership test below vacuously true, admitting a field read at any
        // type; an absent type argument is a frame-seeding bug, not a value to
        // stand in for.
        let Some(expected) = vm.current_call_type_args().first().cloned() else {
            return Err(crate::errors::VmRustFnError::InternalError(
                bex_vm_types::errors::VmInternalError::MissingNativeFunction {
                    name: "reflect.class.get_field: missing type argument".to_string(),
                },
            ));
        };
        let matches = crate::type_match::value_matches_template(
            vm,
            field_value,
            &bex_vm_types::TyTemplate::from(expected.clone()),
            &[],
        )
        .map_err(crate::errors::VmRustFnError::InternalError)?;
        if !matches {
            let got = vm.value_concrete_ty(field_value).map_or_else(
                || "unknown".to_string(),
                |ty| bex_vm_types::RealizedTy::from(ty).to_string(),
            );
            return Err(fail(
                vm,
                format!("field `{class_name}.{name}` has type `{got}`, expected `{expected}`"),
            ));
        }
        Ok(field_value)
    }
}
impl BamlNamespaceEnum for PackageReflectImpl {
    fn value(
        vm: &mut BexVm,
        name: &bex_str::BexStr,
        alias: Option<&bex_str::BexStr>,
        description: Option<&bex_str::BexStr>,
        docstring: Option<&bex_str::BexStr>,
        other: Option<&IndexMap<bex_str::BexStr, Value>>,
    ) -> Value {
        let other = other.map_or_else(IndexMap::new, |other| string_map(vm, other));
        let meta = alloc_meta(
            vm,
            alias.map(bex_str::BexStr::as_str),
            description.map(bex_str::BexStr::as_str),
            docstring.map(bex_str::BexStr::as_str),
            &other,
        );
        let name = Value::object(vm.alloc_string(name.clone()));
        copy::r#enum::Value { name, meta }.to_value(vm)
    }

    fn new(
        vm: &mut BexVm,
        name: &bex_str::BexStr,
        values: &[Value],
    ) -> Result<Value, crate::errors::VmRustFnError> {
        let enum_name = name.as_str();
        let mut diagnostics = Vec::new();
        if !is_baml_identifier(enum_name) {
            diagnostics.push(
                runtime_type::invalid_identifier(InvalidIdentifierKind::Enum, enum_name)
                    .with_phase(DiagnosticPhase::Hir),
            );
        }

        let mut variants = Vec::with_capacity(values.len());
        for value in values {
            match enum_row(vm, *value) {
                Ok(variant) => variants.push(variant),
                Err(message) => {
                    diagnostics.push(compiler_diagnostic(DiagnosticId::TypeMismatch, message));
                }
            }
        }

        for variant in &variants {
            if !is_baml_identifier(&variant.name) {
                diagnostics.push(
                    runtime_type::invalid_identifier(
                        InvalidIdentifierKind::EnumVariant,
                        &format!("{}.{}", enum_name, variant.name),
                    )
                    .with_phase(DiagnosticPhase::Hir),
                );
            }
        }

        let mut seen_names = std::collections::HashSet::new();
        for variant in &variants {
            if !seen_names.insert(variant.name.as_str()) {
                diagnostics.push(
                    runtime_type::duplicate_member(
                        DuplicateMemberKind::Variant,
                        enum_name,
                        &variant.name,
                    )
                    .with_phase(DiagnosticPhase::Hir),
                );
            }
        }

        let mut seen_keys = std::collections::HashSet::new();
        for variant in &variants {
            let key = variant.alias.as_deref().unwrap_or(&variant.name);
            if !seen_keys.insert(key) {
                diagnostics.push(
                    runtime_type::duplicate_serialized_key(key, SerializedKeyContainer::Enum)
                        .with_phase(DiagnosticPhase::Hir),
                );
            }
        }

        if !diagnostics.is_empty() {
            return Err(crate::errors::VmRustFnError::thrown_fresh(
                alloc_compilation_error(vm, &diagnostics),
            ));
        }

        let type_tag = baml_type::typetag::TypeTag::fresh_dynamic();
        let enum_ptr = vm.tlab.alloc(Object::Enum(Box::new(Enum {
            name: bex_vm_types::DeclarationName::Anonymous(baml_type::Name::new(enum_name)),
            variants,
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            type_tag,
            ty_attr: baml_type::TyAttr::default(),
            owner: bex_vm_types::HeapPtr::null(),
        })));
        let ty = bex_vm_types::RealizedTy::Enum(
            bex_vm_types::TypeHead::new(enum_ptr, type_tag),
            baml_type::TyAttr::default(),
        );
        Ok({
            let ty_value = Value::object(vm.tlab.alloc_type(TypeValue::new(ty)));
            alloc_kind_view(vm, baml_type::type_kind::TypeKind::Enum, ty_value)
        })
    }

    fn get_value(
        vm: &mut BexVm,
        value: &Value,
    ) -> Result<bex_str::BexStr, crate::errors::VmRustFnError> {
        let Some(ptr) = value.as_object_ptr() else {
            return Err(invalid_enum_value(vm, *value));
        };
        let Object::Variant(variant) = vm.get_object(ptr) else {
            return Err(invalid_enum_value(vm, *value));
        };
        let Object::Enum(enm) = vm.get_object(variant.enm) else {
            unreachable!("Variant.enm must point to Object::Enum")
        };
        let name = enm
            .variants
            .get(variant.index)
            .unwrap_or_else(|| unreachable!("variant index validated at allocation"))
            .name
            .clone();
        Ok(bex_str::BexStr::from(name))
    }
}
impl BamlNamespaceFunction for PackageReflectImpl {}
impl BamlClassInterfaceImplementation for PackageReflectImpl {
    fn field(
        vm: &mut BexVm,
        implementation: &Value,
        interface_field: &bex_str::BexStr,
        class_field: Option<&bex_str::BexStr>,
    ) -> Result<Value, crate::errors::VmRustFnError> {
        let mut witness = witness_state(vm, *implementation).map_err(|message| {
            crate::errors::VmRustFnError::thrown_fresh(alloc_compilation_error(
                vm,
                &[compiler_diagnostic(DiagnosticId::TypeMismatch, message)],
            ))
        })?;
        let Object::Interface(interface) = vm.get_object(witness.interface_ptr) else {
            unreachable!("implementation witness captures Object::Interface")
        };
        let field_name = baml_type::Name::new(interface_field.as_str());
        if !interface
            .fields
            .iter()
            .any(|field| field.name == field_name)
        {
            let interface_name = interface.name.display_name().to_string();
            return Err(crate::errors::VmRustFnError::thrown_fresh(
                alloc_compilation_error(
                    vm,
                    &[compiler_diagnostic(
                        DiagnosticId::NoSuchField,
                        format!("interface `{interface_name}` has no field `{interface_field}`"),
                    )],
                ),
            ));
        }
        if witness.field_links.contains_key(&field_name) {
            let interface_name = interface.name.display_name().to_string();
            return Err(crate::errors::VmRustFnError::thrown_fresh(
                alloc_compilation_error(
                    vm,
                    &[compiler_diagnostic(
                        DiagnosticId::DuplicateField,
                        format!(
                            "interface witness for `{interface_name}` links field `{interface_field}` more than once"
                        ),
                    )],
                ),
            ));
        }
        let class_field = baml_type::Name::new(
            class_field.map_or(interface_field.as_str(), bex_str::BexStr::as_str),
        );
        witness.field_links.insert(field_name, class_field);
        Ok(copy::interface::Implementation {
            _handle: Arc::new(witness),
        }
        .to_value(vm))
    }
}

impl BamlNamespaceInterface for PackageReflectImpl {
    fn implementation(vm: &mut BexVm) -> Result<Value, crate::errors::VmRustFnError> {
        let Some(interface_ty) = vm.current_call_type_args().first().cloned() else {
            unreachable!("implementation<I> receives one runtime type argument")
        };
        let bex_vm_types::RealizedTy::Interface(interface_head, _, _, _) = &interface_ty else {
            return Err(crate::errors::VmRustFnError::thrown_fresh(
                alloc_compilation_error(
                    vm,
                    &[compiler_diagnostic(
                        DiagnosticId::TypeMismatch,
                        format!(
                            "reflect.interface.implementation expects an interface type, got `{interface_ty}`"
                        ),
                    )],
                ),
            ));
        };
        // The head is the canonical `Object::Interface` pointer, so there is
        // nothing to look up.
        debug_assert!(interface_head.is_resolved());
        let interface_ptr = interface_head.ptr();
        let Object::Interface(interface) = vm.get_object(interface_ptr) else {
            unreachable!("an interface type's head points at Object::Interface")
        };
        if let Some(required) = interface
            .methods
            .iter()
            .find(|method| method.default.is_none())
        {
            let interface_name = interface.name.display_name().to_string();
            let required_name = required.name.to_string();
            return Err(crate::errors::VmRustFnError::thrown_fresh(
                alloc_compilation_error(
                    vm,
                    &[compiler_diagnostic(
                        DiagnosticId::TypeMismatch,
                        format!(
                            "interface `{interface_name}` cannot be witnessed structurally because required method `{required_name}` has no default body; use `reflect.Package.compile` with an `implements` block"
                        ),
                    )],
                ),
            ));
        }
        Ok(copy::interface::Implementation {
            _handle: Arc::new(InterfaceWitness {
                interface_ptr,
                interface_ty,
                field_links: IndexMap::new(),
            }),
        }
        .to_value(vm))
    }
}
impl BamlNamespaceLiteral for PackageReflectImpl {
    fn new(vm: &mut BexVm, value: &Value) -> Value {
        let literal = if let Some(value) = value.as_int() {
            baml_type::Literal::Int(value)
        } else if let Some(value) = value.as_bool() {
            baml_type::Literal::Bool(value)
        } else if let Ok(value) = vm.as_string(value) {
            baml_type::Literal::String(value.to_string())
        } else {
            unreachable!("literal.new argument checked by native glue")
        };
        alloc_runtime_composite(
            vm,
            baml_type::type_kind::TypeKind::Literal,
            bex_vm_types::RealizedTy::Literal(
                literal,
                baml_type::Freshness::Regular,
                baml_type::TyAttr::default(),
            ),
        )
    }
}
impl BamlNamespaceMap for PackageReflectImpl {
    fn new(vm: &mut BexVm, key: &Value, value: &Value) -> Value {
        let key = reflected_type_value(vm, *key);
        let value = reflected_type_value(vm, *value);
        alloc_runtime_composite(
            vm,
            baml_type::type_kind::TypeKind::Map,
            bex_vm_types::RealizedTy::Map {
                key: Box::new(key.ty),
                value: Box::new(value.ty),
                attr: baml_type::TyAttr::default(),
            },
        )
    }
}
impl BamlNamespacePrimitive for PackageReflectImpl {}
impl BamlNamespaceUnion for PackageReflectImpl {
    fn new(vm: &mut BexVm, types: &[Value]) -> Result<Value, crate::errors::VmRustFnError> {
        if types.is_empty() {
            let diagnostic = runtime_type::runtime_empty_union().with_phase(DiagnosticPhase::Hir);
            return Err(crate::errors::VmRustFnError::thrown_fresh(
                alloc_compilation_error(vm, &[diagnostic]),
            ));
        }
        let members = types
            .iter()
            .map(|value| reflected_type_value(vm, *value).ty)
            .collect();
        Ok(alloc_runtime_composite(
            vm,
            baml_type::type_kind::TypeKind::Union,
            bex_vm_types::RealizedTy::Union(members, baml_type::TyAttr::default()),
        ))
    }
}

fn reflected_ty(
    vm: &BexVm,
    view: Value,
    expected: baml_type::type_kind::TypeKind,
) -> Result<bex_vm_types::RealizedTy, crate::errors::VmRustFnError> {
    let ty_value = view_type_value(vm, view, expected)?;
    Ok(super::type_class::type_value_ty(vm, ty_value)
        .unwrap_or_else(|| unreachable!("view_type_value returns an Object::Type")))
}

/// Realize an interface field declaration against the exact interface
/// instantiation captured by a witness. Interface metadata deliberately keeps
/// generic parameters and `Self.Assoc` projections symbolic; construction is the
/// point where C-10 requires those positions to become invariant concrete types.
fn realize_witness_field_type(
    ty: &bex_vm_types::RuntimeTy,
    interface: &InterfaceDef,
    witnessed: &bex_vm_types::RealizedTy,
) -> Result<bex_vm_types::RealizedTy, String> {
    let bex_vm_types::RealizedTy::Interface(head, args, assoc, _) = witnessed else {
        unreachable!("implementation() only captures interface types")
    };
    if head.tag() != interface.type_tag {
        return Err("was captured for a different interface".into());
    }
    let substituted = substitute_witness_field_type(ty, interface, args, assoc)?;
    bex_vm_types::RealizedTy::try_from(substituted)
        .map_err(|_| "depends on an unsupported open type".into())
}

fn substitute_witness_field_type(
    ty: &bex_vm_types::RuntimeTy,
    interface: &InterfaceDef,
    args: &[bex_vm_types::RealizedTy],
    assoc: &[(baml_type::Name, bex_vm_types::RealizedTy)],
) -> Result<bex_vm_types::RuntimeTy, String> {
    use bex_vm_types::RuntimeTy;

    match ty {
        RuntimeTy::TypeVar(param, _) => {
            let Some((index, _)) = interface
                .args
                .iter()
                .enumerate()
                .find(|(_, (name, _))| name.as_str() == param.as_str())
            else {
                return Err(format!("references unbound type parameter `{param}`"));
            };
            args.get(index)
                .cloned()
                .map(RuntimeTy::from)
                .ok_or_else(|| format!("is missing interface type argument `{param}`"))
        }
        RuntimeTy::AssociatedTypeProjection {
            base,
            interface: projection_interface,
            member,
            ..
        } if matches!(&**base, RuntimeTy::TypeVar(param, _) if param.as_str() == "Self")
            && projection_interface.name.tag() == interface.type_tag =>
        {
            assoc
                .iter()
                .find(|(name, _)| name == member)
                .map(|(_, ty)| RuntimeTy::from(ty.clone()))
                .ok_or_else(|| format!("is missing associated binding `{member}`"))
        }
        RuntimeTy::List(inner, attr) => Ok(RuntimeTy::List(
            Box::new(substitute_witness_field_type(
                inner, interface, args, assoc,
            )?),
            attr.clone(),
        )),
        RuntimeTy::Map { key, value, attr } => Ok(RuntimeTy::Map {
            key: Box::new(substitute_witness_field_type(key, interface, args, assoc)?),
            value: Box::new(substitute_witness_field_type(
                value, interface, args, assoc,
            )?),
            attr: attr.clone(),
        }),
        RuntimeTy::Union(members, attr) => Ok(RuntimeTy::Union(
            members
                .iter()
                .map(|member| substitute_witness_field_type(member, interface, args, assoc))
                .collect::<Result<Vec<_>, _>>()?,
            attr.clone(),
        )),
        RuntimeTy::Class(name, type_args, attr) => Ok(RuntimeTy::Class(
            *name,
            type_args
                .iter()
                .map(|arg| substitute_witness_field_type(arg, interface, args, assoc))
                .collect::<Result<Vec<_>, _>>()?,
            attr.clone(),
        )),
        RuntimeTy::Interface(name, type_args, bindings, attr) => Ok(RuntimeTy::Interface(
            *name,
            type_args
                .iter()
                .map(|arg| substitute_witness_field_type(arg, interface, args, assoc))
                .collect::<Result<Vec<_>, _>>()?,
            bindings
                .iter()
                .map(|(name, ty)| {
                    Ok((
                        name.clone(),
                        substitute_witness_field_type(ty, interface, args, assoc)?,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?,
            attr.clone(),
        )),
        RuntimeTy::Function {
            params,
            ret,
            throws,
            attr,
        } => Ok(RuntimeTy::Function {
            params: params
                .iter()
                .map(|param| {
                    Ok(baml_type::RuntimeFunctionParamTy {
                        name: param.name.clone(),
                        ty: substitute_witness_field_type(&param.ty, interface, args, assoc)?,
                        mode: param.mode,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
            ret: Box::new(substitute_witness_field_type(ret, interface, args, assoc)?),
            throws: Box::new(substitute_witness_field_type(
                throws, interface, args, assoc,
            )?),
            attr: attr.clone(),
        }),
        RuntimeTy::Future(value, error, attr) => Ok(RuntimeTy::Future(
            Box::new(substitute_witness_field_type(
                value, interface, args, assoc,
            )?),
            Box::new(substitute_witness_field_type(
                error, interface, args, assoc,
            )?),
            attr.clone(),
        )),
        RuntimeTy::AssociatedTypeProjection {
            base,
            interface: projection_interface,
            member,
            attr,
        } => Ok(RuntimeTy::AssociatedTypeProjection {
            base: Box::new(substitute_witness_field_type(base, interface, args, assoc)?),
            interface: Box::new(bex_vm_types::RuntimeInterface::new(
                projection_interface.name,
                projection_interface
                    .generics
                    .iter()
                    .map(|ty| substitute_witness_field_type(ty, interface, args, assoc))
                    .collect::<Result<Vec<_>, _>>()?,
                projection_interface
                    .associated_types
                    .iter()
                    .map(|(name, ty)| {
                        Ok((
                            name.clone(),
                            substitute_witness_field_type(ty, interface, args, assoc)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            )),
            member: member.clone(),
            attr: attr.clone(),
        }),
        // If the full subtree contains no symbolic positions, preserve it
        // exactly (including attributes) without rebuilding every realized leaf.
        other => bex_vm_types::RealizedTy::try_from(other.clone())
            .map(RuntimeTy::from)
            .map_err(|_| "depends on an unsupported open type".into()),
    }
}

pub(super) fn reflected_type_value(vm: &BexVm, value: Value) -> TypeValue {
    let ptr = value
        .as_object_ptr()
        .unwrap_or_else(|| unreachable!("type argument must be Object::Type"));
    let Object::Type(type_value) = vm.get_object(ptr) else {
        unreachable!("type argument must be Object::Type")
    };
    (**type_value).clone()
}

/// Allocate a runtime composite type and wrap it in `kind`'s view: the
/// composite constructors (`literal.new`, `map.new`, `union.new`) declare the
/// kind views as their return types.
fn alloc_runtime_composite(
    vm: &mut BexVm,
    kind: baml_type::type_kind::TypeKind,
    ty: bex_vm_types::RealizedTy,
) -> Value {
    let ty_value = Value::object(vm.tlab.alloc_type(TypeValue::new(ty)));
    alloc_kind_view(vm, kind, ty_value)
}

pub(super) struct ReflectedTypeRow {
    pub(super) type_value: TypeValue,
    pub(super) alias: Option<String>,
    pub(super) description: Option<String>,
    pub(super) docstring: Option<String>,
    pub(super) other: IndexMap<String, String>,
}

/// A `reflect.WithMeta<T>` row read without interpreting its payload.
///
/// `T` is `type` for an ordinary field and `reflect.class.PendingType` for a
/// recursive one, so the payload stays a raw `Value` and each caller decides
/// what it accepts.
pub(super) struct WithMetaRow {
    pub(super) payload: Value,
    pub(super) alias: Option<String>,
    pub(super) description: Option<String>,
    pub(super) docstring: Option<String>,
    pub(super) other: IndexMap<String, String>,
}

/// Read `value` as a `reflect.WithMeta` wrapper. `None` when it is not one.
pub(super) fn with_meta_row(vm: &BexVm, value: Value) -> Option<Result<WithMetaRow, String>> {
    let Object::Instance(instance) = vm.get_object(value.as_object_ptr()?) else {
        return None;
    };
    let Object::Class(class) = vm.get_object(instance.class) else {
        unreachable!("Instance.class must point to Object::Class")
    };
    if class.name.to_string() != "reflect.WithMeta" {
        return None;
    }
    let optional_string = |index| {
        let value = instance.load_field(index);
        if value.is_null() {
            Ok(None)
        } else {
            vm.as_string(&value)
                .map(|value| Some(value.to_string()))
                .map_err(|_| "reflect.WithMeta string field has an invalid value".to_string())
        }
    };
    let read = || {
        let other = vm
            .as_map(&instance.load_field(4))
            .map_err(|_| "reflect.WithMeta.other must be map<string, string>".to_string())?
            .to_index_map()
            .iter()
            .map(|(key, value)| {
                vm.as_string(value)
                    .map(|value| (key.to_string(), value.to_string()))
                    .map_err(|_| "reflect.WithMeta.other must be map<string, string>".to_string())
            })
            .collect::<Result<IndexMap<_, _>, _>>()?;
        Ok(WithMetaRow {
            payload: instance.load_field(0),
            alias: optional_string(1)?,
            description: optional_string(2)?,
            docstring: optional_string(3)?,
            other,
        })
    };
    Some(read())
}

pub(super) fn reflected_type_row(vm: &BexVm, value: Value) -> Result<ReflectedTypeRow, String> {
    const EXPECTED: &str =
        "class fields must be type values or reflect.WithMeta<reflect.Type> rows";
    let Some(ptr) = value.as_object_ptr() else {
        return Err(EXPECTED.into());
    };
    if let Object::Type(type_value) = vm.get_object(ptr) {
        return Ok(ReflectedTypeRow {
            type_value: (**type_value).clone(),
            alias: None,
            description: None,
            docstring: None,
            other: IndexMap::new(),
        });
    }
    let row = with_meta_row(vm, value).ok_or_else(|| EXPECTED.to_string())??;
    let Some(Object::Type(type_value)) = row.payload.as_object_ptr().map(|ptr| vm.get_object(ptr))
    else {
        return Err(EXPECTED.into());
    };
    Ok(ReflectedTypeRow {
        type_value: (**type_value).clone(),
        alias: row.alias,
        description: row.description,
        docstring: row.docstring,
        other: row.other,
    })
}

/// Allocate an instance of `kind`'s view class wrapping `type_value` (an
/// `Object::Type`) in its `_ty` field.
pub(crate) fn alloc_kind_view(
    vm: &mut BexVm,
    kind: baml_type::type_kind::TypeKind,
    type_value: Value,
) -> Value {
    debug_assert!(matches!(
        type_value.as_object_ptr().map(|ptr| vm.get_object(ptr)),
        Some(Object::Type(_))
    ));
    let name = kind.class_name();
    let class = vm.declaration_head(&name).unwrap_or_else(|| {
        unreachable!("reflection kind class `{name}` is declared by the stdlib")
    });
    Value::object(vm.alloc_instance(class.ptr(), vec![type_value]))
}

/// Enforce a view's kind invariant on its `_ty` value: `_ty` is private by
/// invariant, so a held type that does not classify as `expected` means user
/// code overwrote the field — a BAML panic, never an answer computed from a
/// state the API rules out. Returns `ty_value` (an `Object::Type`) on success.
pub(crate) fn check_kind_invariant(
    vm: &BexVm,
    ty_value: Value,
    expected: baml_type::type_kind::TypeKind,
) -> Result<Value, crate::errors::VmRustFnError> {
    let invariant_panic = |actual: &str| {
        crate::errors::VmRustFnError::Panic(bex_vm_types::errors::VmPanic::UserPanic {
            message: format!(
                "reflect.{}.Type `_ty` holds a {actual} type; the field is private to \
                 reflection and must keep its view's kind",
                expected.namespace(),
            ),
        })
    };
    let Some(ty_ptr) = ty_value.as_object_ptr() else {
        return Err(invariant_panic("non-`reflect.Type`"));
    };
    let Object::Type(type_value) = vm.get_object(ty_ptr) else {
        return Err(invariant_panic("non-`reflect.Type`"));
    };
    let actual = baml_type::type_kind::classify_type(&type_value.ty);
    if actual != expected {
        return Err(invariant_panic(&format!("{}-kind", actual.namespace())));
    }
    Ok(ty_value)
}

/// If `value` is an instance of one of the nine kind views, its `_ty` value
/// (no kind check — callers that accept `reflect.TypeView` accept any kind).
pub(crate) fn as_view_type_value(vm: &BexVm, value: Value) -> Option<Value> {
    let ptr = value.as_object_ptr()?;
    let Object::Instance(instance) = vm.get_object(ptr) else {
        return None;
    };
    let is_view = baml_type::type_kind::TypeKind::ALL.iter().any(|kind| {
        vm.declaration_head(&kind.class_name())
            .is_some_and(|head| head.ptr() == instance.class)
    });
    is_view.then(|| instance.fields[0].load())
}

/// The `Object::Type` value a kind-view instance wraps (`_ty`, its sole
/// declared field), after enforcing the kind invariant.
pub(crate) fn view_type_value(
    vm: &BexVm,
    view: Value,
    expected: baml_type::type_kind::TypeKind,
) -> Result<Value, crate::errors::VmRustFnError> {
    // The receiver shape is guaranteed by static dispatch on the view class.
    let ptr = view
        .as_object_ptr()
        .unwrap_or_else(|| unreachable!("kind-view receiver must be an instance"));
    let Object::Instance(instance) = vm.get_object(ptr) else {
        unreachable!("kind-view receiver must be an Object::Instance")
    };
    check_kind_invariant(vm, instance.fields[0].load(), expected)
}

fn reflected_class(
    vm: &BexVm,
    view: Value,
) -> Result<(bex_vm_types::Class, Vec<bex_vm_types::RealizedTy>), crate::errors::VmRustFnError> {
    let ty_value = view_type_value(vm, view, baml_type::type_kind::TypeKind::Class)?;
    let ptr = ty_value
        .as_object_ptr()
        .unwrap_or_else(|| unreachable!("view_type_value returns an Object::Type"));
    let Object::Type(type_value) = vm.get_object(ptr) else {
        unreachable!("view_type_value returns an Object::Type")
    };
    let bex_vm_types::RealizedTy::Class(head, args, _) = &type_value.ty else {
        unreachable!("a Class-classified type is RealizedTy::Class")
    };
    debug_assert!(head.is_resolved());
    let Object::Class(class) = vm.get_object(head.ptr()) else {
        unreachable!("a class type's head points at Object::Class")
    };
    Ok(((**class).clone(), args.clone()))
}

fn reflected_enum(
    vm: &BexVm,
    view: Value,
) -> Result<bex_vm_types::Enum, crate::errors::VmRustFnError> {
    let ty_value = view_type_value(vm, view, baml_type::type_kind::TypeKind::Enum)?;
    let ptr = ty_value
        .as_object_ptr()
        .unwrap_or_else(|| unreachable!("view_type_value returns an Object::Type"));
    let Object::Type(type_value) = vm.get_object(ptr) else {
        unreachable!("view_type_value returns an Object::Type")
    };
    let bex_vm_types::RealizedTy::Enum(head, _) = &type_value.ty else {
        unreachable!("an Enum-classified type is RealizedTy::Enum")
    };
    debug_assert!(head.is_resolved());
    let Object::Enum(enm) = vm.get_object(head.ptr()) else {
        unreachable!("an enum type's head points at Object::Enum")
    };
    Ok((**enm).clone())
}

fn opt_string(vm: &mut BexVm, value: Option<&str>) -> Value {
    value.map_or(Value::NULL, |s| Value::object(vm.alloc_string(s)))
}

/// Read a native `map<string, string>` argument into owned rows.
pub(super) fn string_map_rows(
    vm: &BexVm,
    other: Option<&IndexMap<bex_str::BexStr, Value>>,
) -> IndexMap<String, String> {
    other
        .into_iter()
        .flatten()
        .map(|(key, value)| {
            let value = vm
                .as_string(value)
                .expect("map<string, string> value checked by native glue");
            (key.to_string(), value.to_string())
        })
        .collect()
}

/// Pair `payload` with schema metadata as a `reflect.WithMeta` row. `payload`
/// is a `type` value for `reflect.Type.meta` and a pending reference for
/// `reflect.class.PendingType.meta`.
pub(super) fn alloc_with_meta(
    vm: &mut BexVm,
    payload: Value,
    alias: Option<&str>,
    description: Option<&str>,
    docstring: Option<&str>,
    other: &IndexMap<String, String>,
) -> Value {
    let mut entries = IndexMap::with_capacity(other.len());
    for (key, value) in other {
        entries.insert(
            bex_str::BexStr::from(key.as_str()),
            Value::object(vm.alloc_string(value.as_str())),
        );
    }
    let other = Value::object(vm.alloc_map(
        baml_type::RealizedTy::string(),
        baml_type::RealizedTy::string(),
        entries,
    ));
    let alias = opt_string(vm, alias);
    let description = opt_string(vm, description);
    let docstring = opt_string(vm, docstring);
    copy::WithMeta {
        ty: payload,
        alias,
        description,
        docstring,
        other,
    }
    .to_value(vm)
}

fn alloc_meta(
    vm: &mut BexVm,
    alias: Option<&str>,
    description: Option<&str>,
    docstring: Option<&str>,
    other: &IndexMap<String, String>,
) -> Value {
    let mut entries = IndexMap::with_capacity(other.len());
    for (key, value) in other {
        entries.insert(
            bex_str::BexStr::from(key.as_str()),
            Value::object(vm.alloc_string(value.as_str())),
        );
    }
    let other = Value::object(vm.alloc_map(
        bex_vm_types::RealizedTy::string(),
        bex_vm_types::RealizedTy::string(),
        entries,
    ));
    let alias = opt_string(vm, alias);
    let description = opt_string(vm, description);
    let docstring = opt_string(vm, docstring);
    copy::Meta {
        alias,
        description,
        docstring,
        other,
    }
    .to_value(vm)
}

fn string_map(
    vm: &mut BexVm,
    values: &IndexMap<bex_str::BexStr, Value>,
) -> IndexMap<String, String> {
    values
        .iter()
        .map(|(key, value)| {
            let value = vm
                .as_string(value)
                .expect("map<string, string> value checked by native glue")
                .to_string();
            (key.to_string(), value)
        })
        .collect()
}

pub(super) fn is_baml_identifier(value: &str) -> bool {
    runtime_type::is_baml_identifier(value)
}

pub(crate) fn compiler_diagnostic(id: DiagnosticId, message: String) -> Diagnostic {
    Diagnostic::error(id, message).with_phase(DiagnosticPhase::Hir)
}

pub(crate) fn alloc_compilation_error(vm: &mut BexVm, diagnostics: &[Diagnostic]) -> Value {
    alloc_compilation_error_with_span(vm, diagnostics, None)
}

fn alloc_compilation_error_with_span(
    vm: &mut BexVm,
    diagnostics: &[Diagnostic],
    span: Option<&(String, u32, u32)>,
) -> Value {
    let message = diagnostics
        .first()
        .map_or("runtime schema validation failed", |diagnostic| {
            diagnostic.message.as_str()
        });
    let values = diagnostics
        .iter()
        .map(|diagnostic| {
            let code = Value::object(vm.alloc_string(diagnostic.code()));
            let message = Value::object(vm.alloc_string(diagnostic.message.as_str()));
            let span = span.map_or(Value::NULL, |(file, start, end)| {
                let file = Value::object(vm.alloc_string(file.as_str()));
                copy::Span {
                    file,
                    start: i64::from(*start),
                    end: i64::from(*end),
                }
                .to_value(vm)
            });
            copy::Diagnostic {
                code,
                span,
                message,
            }
            .to_value(vm)
        })
        .collect();
    let diagnostic_qtn = baml_type::QualifiedTypeName::from_dotted_path("reflect.Diagnostic");
    let diagnostic_ty = bex_vm_types::RealizedTy::Class(
        vm.declaration_head(&diagnostic_qtn)
            .unwrap_or_else(|| unreachable!("`reflect.Diagnostic` is declared by the stdlib")),
        vec![],
        baml_type::TyAttr::default(),
    );
    let diagnostics = Value::object(vm.alloc_array(diagnostic_ty, values));
    let message = Value::object(vm.alloc_string(message));
    let class = vm.resolve_class("reflect.errors.CompilationError");
    Value::object(vm.alloc_instance(class, vec![message, diagnostics]))
}

fn enum_row(vm: &BexVm, value: Value) -> Result<EnumVariant, String> {
    let Some(ptr) = value.as_object_ptr() else {
        return Err("reflect.enum.new values must be strings or reflect.enum.Value rows".into());
    };
    match vm.get_object(ptr) {
        Object::String(name) => Ok(EnumVariant {
            name: name.to_string(),
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            skip: false,
        }),
        Object::Instance(instance) => {
            let Object::Class(class) = vm.get_object(instance.class) else {
                unreachable!("Instance.class must point to Object::Class")
            };
            if class.name.to_string() != "reflect.enum.Value" {
                return Err(
                    "reflect.enum.new values must be strings or reflect.enum.Value rows".into(),
                );
            }
            let name = vm
                .as_string(&instance.load_field(0))
                .map_err(|_| "reflect.enum.Value.name must be a string")?
                .to_string();
            let meta_value = instance.load_field(1);
            let Some(meta_ptr) = meta_value.as_object_ptr() else {
                return Err("reflect.enum.Value.meta must be reflect.Meta".into());
            };
            let Object::Instance(meta) = vm.get_object(meta_ptr) else {
                return Err("reflect.enum.Value.meta must be reflect.Meta".into());
            };
            let Object::Class(meta_class) = vm.get_object(meta.class) else {
                unreachable!("Instance.class must point to Object::Class")
            };
            if meta_class.name.to_string() != "reflect.Meta" {
                return Err("reflect.enum.Value.meta must be reflect.Meta".into());
            }
            let optional_string = |index| {
                let value = meta.load_field(index);
                if value.is_null() {
                    Ok(None)
                } else {
                    vm.as_string(&value)
                        .map(|value| Some(value.to_string()))
                        .map_err(|_| "reflect.Meta string field has an invalid value".to_string())
                }
            };
            let other = vm
                .as_map(&meta.load_field(3))
                .map_err(|_| "reflect.Meta.other must be map<string, string>".to_string())?
                .to_index_map()
                .iter()
                .map(|(key, value)| {
                    vm.as_string(value)
                        .map(|value| (key.to_string(), value.to_string()))
                        .map_err(|_| "reflect.Meta.other must be map<string, string>".to_string())
                })
                .collect::<Result<IndexMap<_, _>, _>>()?;
            Ok(EnumVariant {
                name,
                alias: optional_string(0)?,
                description: optional_string(1)?,
                docstring: optional_string(2)?,
                other,
                skip: false,
            })
        }
        _ => Err("reflect.enum.new values must be strings or reflect.enum.Value rows".into()),
    }
}

fn invalid_enum_value(vm: &BexVm, value: Value) -> crate::errors::VmRustFnError {
    crate::errors::VmRustFnError::BamlError(crate::errors::VmBamlError::InvalidArgument {
        message: format!(
            "reflect.enum.get_value expects an enum value, got {}",
            vm.type_of(&value)
        ),
    })
}

macro_rules! impl_as_type {
    ($kind:ident, $view_ns:ident) => {
        #[expect(
            clippy::used_underscore_items,
            reason = "the generated view accessor is named for the `_ty` field it reads"
        )]
        fn as_type(
            vm: &BexVm,
            r#type: &super::view::$view_ns::Type<'_>,
        ) -> Result<Value, crate::errors::VmRustFnError> {
            check_kind_invariant(vm, r#type._ty(), baml_type::type_kind::TypeKind::$kind)
        }
    };
}

impl BamlClassClassType for PackageReflectImpl {
    fn fields(vm: &mut BexVm, r#type: &Value) -> Result<Vec<Value>, crate::errors::VmRustFnError> {
        let (class, args) = reflected_class(vm, *r#type)?;
        Ok(class
            .fields
            .iter()
            .map(|field| {
                let name = Value::object(vm.alloc_string(field.name.as_str()));
                let r#type = if let Some(type_value) = &field.runtime_type {
                    Value::object(vm.tlab.alloc_type(type_value.clone()))
                } else {
                    let ty = field
                        .field_template
                        .substitute(&args, vm)
                        .unwrap_or_else(|err| {
                            unreachable!("emitted class field template must realize: {err}")
                        });
                    Value::object(vm.alloc_type(bex_vm_types::types::TypeValue::new(ty)))
                };
                let meta = alloc_meta(
                    vm,
                    field.alias.as_deref(),
                    field.description.as_deref(),
                    field.docstring.as_deref(),
                    &field.other,
                );
                copy::class::Field {
                    name,
                    r#type,
                    meta,
                    _owner: Value::NULL,
                }
                .to_value(vm)
            })
            .collect())
    }

    fn meta(vm: &mut BexVm, r#type: &Value) -> Result<Value, crate::errors::VmRustFnError> {
        let (class, _) = reflected_class(vm, *r#type)?;
        Ok(alloc_meta(
            vm,
            class.alias.as_deref(),
            class.description.as_deref(),
            class.docstring.as_deref(),
            &class.other,
        ))
    }
}

impl BamlClassEnumType for PackageReflectImpl {
    fn values(vm: &mut BexVm, r#type: &Value) -> Result<Vec<Value>, crate::errors::VmRustFnError> {
        let enm = reflected_enum(vm, *r#type)?;
        Ok(enm
            .variants
            .iter()
            .map(|variant| {
                let name = Value::object(vm.alloc_string(variant.name.as_str()));
                let meta = alloc_meta(
                    vm,
                    variant.alias.as_deref(),
                    variant.description.as_deref(),
                    variant.docstring.as_deref(),
                    &variant.other,
                );
                copy::r#enum::Value { name, meta }.to_value(vm)
            })
            .collect())
    }

    fn meta(vm: &mut BexVm, r#type: &Value) -> Result<Value, crate::errors::VmRustFnError> {
        let enm = reflected_enum(vm, *r#type)?;
        Ok(alloc_meta(
            vm,
            enm.alias.as_deref(),
            enm.description.as_deref(),
            enm.docstring.as_deref(),
            &enm.other,
        ))
    }
}

impl BamlClassUnionType for PackageReflectImpl {
    fn member_types(
        vm: &mut BexVm,
        r#type: &Value,
    ) -> Result<Vec<Value>, crate::errors::VmRustFnError> {
        let ty = reflected_ty(vm, *r#type, baml_type::type_kind::TypeKind::Union)?;
        let bex_vm_types::RealizedTy::Union(members, _) = ty else {
            unreachable!("a Union-classified type is RealizedTy::Union")
        };
        Ok(members
            .into_iter()
            .map(|ty| Value::object(vm.alloc_type(bex_vm_types::types::TypeValue::new(ty))))
            .collect())
    }
}

impl BamlClassArrayType for PackageReflectImpl {
    fn element_type(vm: &mut BexVm, r#type: &Value) -> Result<Value, crate::errors::VmRustFnError> {
        let ty = reflected_ty(vm, *r#type, baml_type::type_kind::TypeKind::Array)?;
        let bex_vm_types::RealizedTy::List(element, _) = ty else {
            unreachable!("an Array-classified type is RealizedTy::List")
        };
        Ok(Value::object(
            vm.alloc_type(bex_vm_types::types::TypeValue::new(*element)),
        ))
    }
}

impl BamlClassMapType for PackageReflectImpl {
    fn key_type(vm: &mut BexVm, r#type: &Value) -> Result<Value, crate::errors::VmRustFnError> {
        let ty = reflected_ty(vm, *r#type, baml_type::type_kind::TypeKind::Map)?;
        let bex_vm_types::RealizedTy::Map { key, .. } = ty else {
            unreachable!("a Map-classified type is RealizedTy::Map")
        };
        Ok(Value::object(
            vm.alloc_type(bex_vm_types::types::TypeValue::new(*key)),
        ))
    }

    fn value_type(vm: &mut BexVm, r#type: &Value) -> Result<Value, crate::errors::VmRustFnError> {
        let ty = reflected_ty(vm, *r#type, baml_type::type_kind::TypeKind::Map)?;
        let bex_vm_types::RealizedTy::Map { value, .. } = ty else {
            unreachable!("a Map-classified type is RealizedTy::Map")
        };
        Ok(Value::object(
            vm.alloc_type(bex_vm_types::types::TypeValue::new(*value)),
        ))
    }
}

impl BamlClassFunctionType for PackageReflectImpl {
    fn params(vm: &mut BexVm, r#type: &Value) -> Result<Vec<Value>, crate::errors::VmRustFnError> {
        let ty = reflected_ty(vm, *r#type, baml_type::type_kind::TypeKind::Function)?;
        let bex_vm_types::RealizedTy::Function { params, .. } = ty else {
            unreachable!("a Function-classified type is RealizedTy::Function")
        };
        Ok(params
            .into_iter()
            .map(|param| {
                let name = opt_string(vm, param.name.as_ref().map(baml_type::Name::as_str));
                let optional = param.is_optional();
                let r#type =
                    Value::object(vm.alloc_type(bex_vm_types::types::TypeValue::new(param.ty)));
                copy::function::Parameter {
                    name,
                    r#type,
                    optional,
                }
                .to_value(vm)
            })
            .collect())
    }

    fn return_type(vm: &mut BexVm, r#type: &Value) -> Result<Value, crate::errors::VmRustFnError> {
        let ty = reflected_ty(vm, *r#type, baml_type::type_kind::TypeKind::Function)?;
        let bex_vm_types::RealizedTy::Function { ret, .. } = ty else {
            unreachable!("a Function-classified type is RealizedTy::Function")
        };
        Ok(Value::object(
            vm.alloc_type(bex_vm_types::types::TypeValue::new(*ret)),
        ))
    }
}

impl BamlClassInterfaceType for PackageReflectImpl {
    #[expect(
        clippy::used_underscore_items,
        reason = "the generated view accessor is named for the `_ty` field it reads"
    )]
    fn implemented_by(
        vm: &BexVm,
        r#type: &super::view::interface::Type<'_>,
        other: &Value,
    ) -> Result<bool, crate::errors::VmRustFnError> {
        let ty_value =
            check_kind_invariant(vm, r#type._ty(), baml_type::type_kind::TypeKind::Interface)?;
        Ok(<PackageReflectImpl as BamlClassType>::implemented_by(
            vm, &ty_value, other,
        ))
    }
}

// The `implements reflect.TypeView` blocks' `as_type` bodies — impl-block
// methods, so codegen emits one `…ReflectTypeView_for_Type` trait per view
// class (plus the aggregating `BamlNamespace…Reflect` supertraits).

impl BamlClassArrayReflectTypeView_for_Type for PackageReflectImpl {
    impl_as_type!(Array, array);
}

impl BamlClassClassReflectTypeView_for_Type for PackageReflectImpl {
    impl_as_type!(Class, class);
}

impl BamlClassEnumReflectTypeView_for_Type for PackageReflectImpl {
    impl_as_type!(Enum, r#enum);
}

impl BamlClassFunctionReflectTypeView_for_Type for PackageReflectImpl {
    impl_as_type!(Function, function);
}

impl BamlClassInterfaceReflectTypeView_for_Type for PackageReflectImpl {
    impl_as_type!(Interface, interface);
}

impl BamlClassLiteralReflectTypeView_for_Type for PackageReflectImpl {
    impl_as_type!(Literal, literal);
}

impl BamlClassMapReflectTypeView_for_Type for PackageReflectImpl {
    impl_as_type!(Map, map);
}

impl BamlClassPrimitiveReflectTypeView_for_Type for PackageReflectImpl {
    impl_as_type!(Primitive, primitive);
}

impl BamlClassUnionReflectTypeView_for_Type for PackageReflectImpl {
    impl_as_type!(Union, union);
}

impl BamlNamespaceArrayReflect for PackageReflectImpl {}

impl BamlNamespaceClassReflect for PackageReflectImpl {}

impl BamlNamespaceEnumReflect for PackageReflectImpl {}

impl BamlNamespaceFunctionReflect for PackageReflectImpl {}

impl BamlNamespaceInterfaceReflect for PackageReflectImpl {}

impl BamlNamespaceLiteralReflect for PackageReflectImpl {}

impl BamlNamespaceMapReflect for PackageReflectImpl {}

impl BamlNamespacePrimitiveReflect for PackageReflectImpl {}

impl BamlNamespaceUnionReflect for PackageReflectImpl {}
