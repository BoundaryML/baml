//! BEP-066 reflection kind views over `Object::Type`.

use std::sync::Arc;

use baml_compiler_diagnostics::{
    Diagnostic, DiagnosticId, DiagnosticPhase,
    runtime_type::{self, DuplicateMemberKind, InvalidIdentifierKind, SerializedKeyContainer},
};
use bex_heap::TlabHolder;
use bex_vm_types::types::{
    Class, ClassField, DynTypeDefs, DynWitnessDef, Enum, EnumVariant, InterfaceDef, MethodImpl,
    Object, PortableClassDef, PortableClassFieldDef, PortableEnumDef, PortableEnumVariantDef,
    PortableMetadata, PortableTypeDef, RuntimeImplRule, RuntimeTypeProvenance, TypeValue, Value,
};
use indexmap::IndexMap;

use super::{
    BamlClassReflectArrayType, BamlClassReflectClassType, BamlClassReflectEnumType,
    BamlClassReflectFunctionType, BamlClassReflectInterfaceImplementation,
    BamlClassReflectInterfaceType, BamlClassReflectLiteralType, BamlClassReflectMapType,
    BamlClassReflectPrimitiveType, BamlClassReflectUnionType, BamlClassTypeValue,
    BamlNamespaceReflectArray, BamlNamespaceReflectClass, BamlNamespaceReflectEnum,
    BamlNamespaceReflectFunction, BamlNamespaceReflectInterface, BamlNamespaceReflectLiteral,
    BamlNamespaceReflectMap, BamlNamespaceReflectPrimitive, BamlNamespaceReflectUnion,
    PackageBamlImpl, copy,
};
use crate::BexVm;

impl BexVm {
    /// Copy a type's reachable nominal schemas into a pointer-free host
    /// carrier. The package owner is deliberately not represented.
    pub fn export_portable_type_def(&self, type_value: &TypeValue) -> PortableTypeDef {
        let mut class_ptrs = type_value
            .defs()
            .classes
            .values()
            .copied()
            .collect::<Vec<_>>();
        let mut enum_ptrs = type_value
            .defs()
            .enums
            .values()
            .copied()
            .collect::<Vec<_>>();
        if !type_value.owner.is_null()
            && let Object::Package(package) = self.get_object(type_value.owner)
        {
            for ptr in package.classes.values().copied() {
                if !class_ptrs.contains(&ptr)
                    && !matches!(self.get_object(ptr), Object::Class(class) if class.name.name().as_str().ends_with("$stream"))
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
        let metadata = |description: &Option<String>,
                        alias: &Option<String>,
                        docstring: &Option<String>,
                        other: &IndexMap<String, String>| PortableMetadata {
            description: description.clone(),
            alias: alias.clone(),
            docstring: docstring.clone(),
            other: other.clone(),
        };
        let classes = class_ptrs
            .into_iter()
            .filter_map(|ptr| {
                let Object::Class(class) = self.get_object(ptr) else {
                    return None;
                };
                Some(PortableClassDef {
                    name: class.name.clone(),
                    fields: class
                        .fields
                        .iter()
                        .map(|field| PortableClassFieldDef {
                            name: field.name.clone(),
                            ty: field.field_type.clone(),
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
                let Object::Enum(enm) = self.get_object(ptr) else {
                    return None;
                };
                Some(PortableEnumDef {
                    name: enm.name.clone(),
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
            root: type_value.ty.clone().into(),
            classes,
            enums,
            witnesses: type_value.defs().witnesses.clone(),
        }
    }

    /// Materialize an inbound portable definition as fresh declarations.
    /// Names remain definition keys; the declarations are new, so they are
    /// distinct from any same-named declaration the program already has.
    pub fn materialize_portable_type_def(
        &mut self,
        definition: PortableTypeDef,
    ) -> Result<TypeValue, String> {
        let root = baml_type::RealizedTy::try_from(&definition.root)
            .map_err(|error| format!("host type definition root is not realized: {error}"))?;
        for class in &definition.classes {
            for field in &class.fields {
                baml_type::RealizedTy::try_from(&field.ty).map_err(|error| {
                    format!(
                        "host type definition field `{}.{}` is not realized: {error}",
                        class.name, field.name
                    )
                })?;
            }
        }
        let mut defs = DynTypeDefs {
            classes: IndexMap::new(),
            enums: IndexMap::new(),
            witnesses: definition.witnesses,
        };
        for class in &definition.classes {
            let ptr = self.tlab.alloc(Object::Class(Box::new(Class {
                name: class.name.clone(),
                fields: class
                    .fields
                    .iter()
                    .map(|field| ClassField {
                        name: field.name.clone(),
                        field_type: field.ty.clone(),
                        field_template: baml_type::TyTemplate::from(
                            baml_type::RealizedTy::try_from(&field.ty)
                                .expect("portable fields validated as realized"),
                        ),
                        description: field.metadata.description.clone(),
                        alias: field.metadata.alias.clone(),
                        docstring: field.metadata.docstring.clone(),
                        other: field.metadata.other.clone(),
                        skip: field.skip,
                        runtime_type: None,
                    })
                    .collect(),
                description: class.metadata.description.clone(),
                alias: class.metadata.alias.clone(),
                docstring: class.metadata.docstring.clone(),
                other: class.metadata.other.clone(),
                // A counter tag, never content-addressed from the wire name: a
                // payload naming a compiled FQN must not take the compiled
                // class's identity (it would land on that class's jump-table
                // and virtual-field switch arms). The name is display data.
                type_tag: baml_type::typetag::TypeTag::fresh_dynamic(),
                ty_attr: baml_type::TyAttr::default(),
                has_cleanup: false,
                generic_param_count: class.generic_param_count,
                runtime_type: None,
            })));
            defs.classes.insert(class.name.clone(), ptr);
            self.dynamic_dispatch
                .register_class(class.name.clone(), ptr);
        }
        for enm in &definition.enums {
            let ptr = self.tlab.alloc(Object::Enum(Box::new(Enum {
                name: enm.name.clone(),
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
                type_tag: baml_type::typetag::TypeTag::fresh_dynamic(),
                ty_attr: baml_type::TyAttr::default(),
                runtime_type: None,
            })));
            defs.enums.insert(enm.name.clone(), ptr);
        }

        for (_, ptr) in defs.classes.clone() {
            let Object::Class(class) = self.get_object_mut(ptr) else {
                unreachable!()
            };
            class.runtime_type = Some(RuntimeTypeProvenance {
                defs: defs.clone(),
                owner: bex_vm_types::HeapPtr::null(),
            });
        }
        for (_, ptr) in defs.enums.clone() {
            let Object::Enum(enm) = self.get_object_mut(ptr) else {
                unreachable!()
            };
            enm.runtime_type = Some(RuntimeTypeProvenance {
                defs: defs.clone(),
                owner: bex_vm_types::HeapPtr::null(),
            });
        }
        Ok(TypeValue::with_defs(root, defs))
    }
}

impl BamlNamespaceReflectArray for PackageBamlImpl {}

#[derive(Clone, Debug)]
struct InterfaceWitness {
    interface_ptr: bex_vm_types::HeapPtr,
    interface_ty: baml_type::RealizedTy,
    field_links: IndexMap<baml_type::Name, baml_type::Name>,
}

pub(super) struct WitnessField {
    pub(super) name: String,
    pub(super) ty: baml_type::RealizedTy,
}

pub(super) struct ValidatedClassWitness {
    interface_ptr: bex_vm_types::HeapPtr,
    interface_args: Vec<baml_type::RealizedTy>,
    interface_assoc: Vec<(baml_type::Name, baml_type::RealizedTy)>,
    field_links: Vec<u32>,
    pub(super) definition: DynWitnessDef,
}

fn witness_state(vm: &BexVm, value: Value) -> Result<InterfaceWitness, String> {
    let instance = vm
        .as_instance(&value)
        .map_err(|_| "implementations must contain reflect.interface.Implementation values")?;
    let Object::Class(class) = vm.get_object(instance.class) else {
        unreachable!("Instance.class must point to Object::Class")
    };
    if class.name.to_string() != "baml.reflect.interface.Implementation" {
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
                    baml_type::RealizedTy::Interface(name, _, _, _)
                        if name == &required.name
                )
            });
            if !present {
                diagnostics.push(compiler_diagnostic(
                    DiagnosticId::MissingRequiredInterface,
                    format!(
                        "interface witness for `{}` requires a witness for `{}`",
                        interface.name.display_name(),
                        required.name.display_name()
                    ),
                ));
            }
        }
        let mut physical_links = Vec::with_capacity(interface.fields.len());
        let mut tuple_links = Vec::with_capacity(interface.fields.len());
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
            tuple_links.push((required.name.clone(), class_field_name.clone()));
        }
        let (interface_name, interface_args, interface_assoc) = match &witness.interface_ty {
            baml_type::RealizedTy::Interface(name, args, assoc, _) => {
                (name.clone(), args.clone(), assoc.clone())
            }
            _ => unreachable!("implementation() only creates interface witnesses"),
        };
        validated.push(ValidatedClassWitness {
            interface_ptr: witness.interface_ptr,
            interface_args: interface_args.clone(),
            interface_assoc: interface_assoc.clone(),
            field_links: physical_links,
            definition: DynWitnessDef {
                interface: interface_name,
                interface_args,
                associated_types: interface_assoc,
                field_links: tuple_links,
            },
        });
    }
    validated
}

pub(super) fn register_class_witnesses(
    vm: &mut BexVm,
    class_ptr: bex_vm_types::HeapPtr,
    ty: &baml_type::RealizedTy,
    witnesses: Vec<ValidatedClassWitness>,
) {
    for witness in witnesses {
        let Object::Interface(interface) = vm.get_object(witness.interface_ptr) else {
            unreachable!("witness interface validated before allocation")
        };
        let mut methods = IndexMap::new();
        let for_ty_pattern = baml_type::TyTemplate::from(ty.clone());
        let mut default_frame = vec![for_ty_pattern.clone()];
        default_frame.extend(
            witness
                .interface_args
                .iter()
                .cloned()
                .map(baml_type::TyTemplate::from),
        );
        default_frame.extend(
            witness
                .interface_assoc
                .iter()
                .map(|(_, ty)| baml_type::TyTemplate::from(ty.clone())),
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
                .map(baml_type::TyTemplate::from)
                .collect(),
            interface_assoc: witness
                .interface_assoc
                .into_iter()
                .map(|(name, ty)| (name, baml_type::TyTemplate::from(ty)))
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

impl BamlNamespaceReflectClass for PackageBamlImpl {
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
        let mut child_defs = DynTypeDefs::default();
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
            child_defs.merge_from(row.type_value.defs());
            witness_fields.push(WitnessField {
                name: field_name.to_string(),
                ty: row.type_value.ty.clone(),
            });
            class_fields.push(ClassField {
                name: field_name.to_string(),
                field_type: row.type_value.ty.clone().into(),
                field_template: baml_type::TyTemplate::from(row.type_value.ty.clone()),
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
            return Err(crate::errors::VmRustFnError::Thrown(
                alloc_compilation_error(vm, &diagnostics),
            ));
        }

        let type_name = baml_type::QualifiedTypeName::runtime_local(
            baml_type::Name::new(class_name),
            vm.tlab.heap().next_synthetic_name_id(),
        );
        child_defs.witnesses = witnesses
            .iter()
            .map(|witness| witness.definition.clone())
            .collect();
        let class_ptr = vm.tlab.alloc(Object::Class(Box::new(Class {
            name: type_name.clone(),
            fields: class_fields,
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            type_tag: baml_type::typetag::TypeTag::fresh_dynamic(),
            ty_attr: baml_type::TyAttr::default(),
            has_cleanup: false,
            generic_param_count: 0,
            runtime_type: Some(RuntimeTypeProvenance {
                defs: child_defs.clone(),
                owner: bex_vm_types::HeapPtr::null(),
            }),
        })));
        let ty = baml_type::RealizedTy::Class(
            type_name.clone(),
            Vec::new(),
            baml_type::TyAttr::default(),
        );
        child_defs.classes.insert(type_name.clone(), class_ptr);
        vm.dynamic_dispatch.register_class(type_name, class_ptr);
        register_class_witnesses(vm, class_ptr, &ty, witnesses);
        Ok(Value::object(
            vm.tlab.alloc_type(TypeValue::with_defs(ty, child_defs)),
        ))
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
            crate::errors::VmRustFnError::Thrown(alloc_compilation_error(
                vm,
                &[compiler_diagnostic(DiagnosticId::TypeMismatch, message)],
            ))
        };
        let Some(instance_ptr) = value.as_object_ptr() else {
            return Err(fail(
                vm,
                format!(
                    "reflect.class.get_field expected a class instance, got {}",
                    vm.type_of(value)
                ),
            ));
        };
        let (field_value, class_name) = {
            let Object::Instance(instance) = vm.get_object(instance_ptr) else {
                return Err(fail(
                    vm,
                    format!(
                        "reflect.class.get_field expected a class instance, got {}",
                        vm.type_of(value)
                    ),
                ));
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
        let expected = vm
            .current_call_type_args()
            .first()
            .cloned()
            .unwrap_or_else(baml_type::RealizedTy::unknown);
        let matches = crate::type_match::value_matches_template(
            vm,
            field_value,
            &baml_type::TyTemplate::from(expected.clone()),
            &[],
        )
        .map_err(crate::errors::VmRustFnError::InternalError)?;
        if !matches {
            let got = vm.value_concrete_ty(field_value).map_or_else(
                || "unknown".to_string(),
                |ty| baml_type::RealizedTy::from(ty).to_string(),
            );
            return Err(fail(
                vm,
                format!("field `{class_name}.{name}` has type `{got}`, expected `{expected}`"),
            ));
        }
        Ok(field_value)
    }
}
impl BamlNamespaceReflectEnum for PackageBamlImpl {
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
        copy::reflect::r#enum::Value { name, meta }.to_value(vm)
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
            return Err(crate::errors::VmRustFnError::Thrown(
                alloc_compilation_error(vm, &diagnostics),
            ));
        }

        let type_name = baml_type::QualifiedTypeName::runtime_local(
            baml_type::Name::new(enum_name),
            vm.tlab.heap().next_synthetic_name_id(),
        );
        let enum_ptr = vm.tlab.alloc(Object::Enum(Box::new(Enum {
            name: type_name.clone(),
            variants,
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            type_tag: baml_type::typetag::TypeTag::fresh_dynamic(),
            ty_attr: baml_type::TyAttr::default(),
            runtime_type: Some(RuntimeTypeProvenance {
                defs: DynTypeDefs::default(),
                owner: bex_vm_types::HeapPtr::null(),
            }),
        })));
        let ty = baml_type::RealizedTy::Enum(type_name.clone(), baml_type::TyAttr::default());
        let defs = DynTypeDefs::with_enum(type_name, enum_ptr);
        let type_ptr = vm.tlab.alloc_type(TypeValue::with_defs(ty, defs));
        Ok(Value::object(type_ptr))
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
impl BamlNamespaceReflectFunction for PackageBamlImpl {}
impl BamlClassReflectInterfaceImplementation for PackageBamlImpl {
    fn field(
        vm: &mut BexVm,
        implementation: &Value,
        interface_field: &bex_str::BexStr,
        class_field: Option<&bex_str::BexStr>,
    ) -> Result<Value, crate::errors::VmRustFnError> {
        let mut witness = witness_state(vm, *implementation).map_err(|message| {
            crate::errors::VmRustFnError::Thrown(alloc_compilation_error(
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
            return Err(crate::errors::VmRustFnError::Thrown(
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
            return Err(crate::errors::VmRustFnError::Thrown(
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
        Ok(copy::reflect::interface::Implementation {
            _handle: Arc::new(witness),
        }
        .to_value(vm))
    }
}

impl BamlNamespaceReflectInterface for PackageBamlImpl {
    fn implementation(vm: &mut BexVm) -> Result<Value, crate::errors::VmRustFnError> {
        let Some(interface_ty) = vm.current_call_type_args().first().cloned() else {
            unreachable!("implementation<I> receives one runtime type argument")
        };
        let baml_type::RealizedTy::Interface(interface_name, _, _, _) = &interface_ty else {
            return Err(crate::errors::VmRustFnError::Thrown(
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
        let interface_ptr = vm.lookup_interface(interface_name).unwrap_or_else(|| {
            unreachable!("statically checked interface `{interface_name}` must be loaded")
        });
        let Object::Interface(interface) = vm.get_object(interface_ptr) else {
            unreachable!("lookup_interface returns Object::Interface")
        };
        if let Some(required) = interface
            .methods
            .iter()
            .find(|method| method.default.is_none())
        {
            let interface_name = interface.name.display_name().to_string();
            let required_name = required.name.to_string();
            return Err(crate::errors::VmRustFnError::Thrown(
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
        Ok(copy::reflect::interface::Implementation {
            _handle: Arc::new(InterfaceWitness {
                interface_ptr,
                interface_ty,
                field_links: IndexMap::new(),
            }),
        }
        .to_value(vm))
    }
}
impl BamlNamespaceReflectLiteral for PackageBamlImpl {
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
            baml_type::RealizedTy::Literal(
                literal,
                baml_type::Freshness::Regular,
                baml_type::TyAttr::default(),
            ),
            DynTypeDefs::default(),
        )
    }
}
impl BamlNamespaceReflectMap for PackageBamlImpl {
    fn new(vm: &mut BexVm, key: &Value, value: &Value) -> Value {
        let key = reflected_type_value(vm, *key);
        let value = reflected_type_value(vm, *value);
        let mut defs = key.defs().clone();
        defs.merge_from(value.defs());
        alloc_runtime_composite(
            vm,
            baml_type::RealizedTy::Map {
                key: Box::new(key.ty),
                value: Box::new(value.ty),
                attr: baml_type::TyAttr::default(),
            },
            defs,
        )
    }
}
impl BamlNamespaceReflectPrimitive for PackageBamlImpl {}
impl BamlNamespaceReflectUnion for PackageBamlImpl {
    fn new(vm: &mut BexVm, types: &[Value]) -> Result<Value, crate::errors::VmRustFnError> {
        if types.is_empty() {
            let diagnostic = runtime_type::runtime_empty_union().with_phase(DiagnosticPhase::Hir);
            return Err(crate::errors::VmRustFnError::Thrown(
                alloc_compilation_error(vm, &[diagnostic]),
            ));
        }
        let mut members = Vec::with_capacity(types.len());
        let mut defs = DynTypeDefs::default();
        for value in types {
            let type_value = reflected_type_value(vm, *value);
            members.push(type_value.ty.clone());
            defs.merge_from(type_value.defs());
        }
        Ok(alloc_runtime_composite(
            vm,
            baml_type::RealizedTy::Union(members, baml_type::TyAttr::default()),
            defs,
        ))
    }
}

fn reflected_ty(vm: &BexVm, value: Value) -> baml_type::RealizedTy {
    super::type_class::type_value_ty(vm, value)
        .unwrap_or_else(|| unreachable!("kind method receiver must be Object::Type"))
}

/// Realize an interface field declaration against the exact interface
/// instantiation captured by a witness. Interface metadata deliberately keeps
/// generic parameters and `Self.Assoc` projections symbolic; construction is the
/// point where C-10 requires those positions to become invariant concrete types.
fn realize_witness_field_type(
    ty: &baml_type::RuntimeTy,
    interface: &InterfaceDef,
    witnessed: &baml_type::RealizedTy,
) -> Result<baml_type::RealizedTy, String> {
    let baml_type::RealizedTy::Interface(name, args, assoc, _) = witnessed else {
        unreachable!("implementation() only captures interface types")
    };
    if name != &interface.name {
        return Err("was captured for a different interface".into());
    }
    let substituted = substitute_witness_field_type(ty, interface, args, assoc)?;
    baml_type::RealizedTy::try_from(substituted)
        .map_err(|_| "depends on an unsupported open type".into())
}

fn substitute_witness_field_type(
    ty: &baml_type::RuntimeTy,
    interface: &InterfaceDef,
    args: &[baml_type::RealizedTy],
    assoc: &[(baml_type::Name, baml_type::RealizedTy)],
) -> Result<baml_type::RuntimeTy, String> {
    use baml_type::RuntimeTy;

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
            && projection_interface.name == interface.name =>
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
            name.clone(),
            type_args
                .iter()
                .map(|arg| substitute_witness_field_type(arg, interface, args, assoc))
                .collect::<Result<Vec<_>, _>>()?,
            attr.clone(),
        )),
        RuntimeTy::Interface(name, type_args, bindings, attr) => Ok(RuntimeTy::Interface(
            name.clone(),
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
            interface: Box::new(baml_type::RuntimeInterface::new(
                projection_interface.name.clone(),
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
        other => baml_type::RealizedTy::try_from(other.clone())
            .map(RuntimeTy::from)
            .map_err(|_| "depends on an unsupported open type".into()),
    }
}

fn reflected_type_value(vm: &BexVm, value: Value) -> TypeValue {
    let ptr = value
        .as_object_ptr()
        .unwrap_or_else(|| unreachable!("type argument must be Object::Type"));
    let Object::Type(type_value) = vm.get_object(ptr) else {
        unreachable!("type argument must be Object::Type")
    };
    (**type_value).clone()
}

fn alloc_runtime_composite(vm: &mut BexVm, ty: baml_type::RealizedTy, defs: DynTypeDefs) -> Value {
    Value::object(vm.tlab.alloc_type(TypeValue::with_defs(ty, defs)))
}

pub(super) struct ReflectedTypeRow {
    pub(super) type_value: TypeValue,
    pub(super) alias: Option<String>,
    pub(super) description: Option<String>,
    pub(super) docstring: Option<String>,
    pub(super) other: IndexMap<String, String>,
}

pub(super) fn reflected_type_row(vm: &BexVm, value: Value) -> Result<ReflectedTypeRow, String> {
    let Some(ptr) = value.as_object_ptr() else {
        return Err("class fields must be type values or reflect.WithMeta<type> rows".into());
    };
    match vm.get_object(ptr) {
        Object::Type(type_value) => Ok(ReflectedTypeRow {
            type_value: (**type_value).clone(),
            alias: None,
            description: None,
            docstring: None,
            other: IndexMap::new(),
        }),
        Object::Instance(instance) => {
            let Object::Class(class) = vm.get_object(instance.class) else {
                unreachable!("Instance.class must point to Object::Class")
            };
            if class.name.to_string() != "baml.reflect.WithMeta" {
                return Err(
                    "class fields must be type values or reflect.WithMeta<type> rows".into(),
                );
            }
            let type_value = reflected_type_value(vm, instance.load_field(0));
            let optional_string = |index| {
                let value = instance.load_field(index);
                if value.is_null() {
                    Ok(None)
                } else {
                    vm.as_string(&value)
                        .map(|value| Some(value.to_string()))
                        .map_err(|_| {
                            "reflect.WithMeta string field has an invalid value".to_string()
                        })
                }
            };
            let other = vm
                .as_map(&instance.load_field(4))
                .map_err(|_| "reflect.WithMeta.other must be map<string, string>".to_string())?
                .to_index_map()
                .iter()
                .map(|(key, value)| {
                    vm.as_string(value)
                        .map(|value| (key.to_string(), value.to_string()))
                        .map_err(|_| {
                            "reflect.WithMeta.other must be map<string, string>".to_string()
                        })
                })
                .collect::<Result<IndexMap<_, _>, _>>()?;
            Ok(ReflectedTypeRow {
                type_value,
                alias: optional_string(1)?,
                description: optional_string(2)?,
                docstring: optional_string(3)?,
                other,
            })
        }
        _ => Err("class fields must be type values or reflect.WithMeta<type> rows".into()),
    }
}

fn reflected_class(vm: &BexVm, value: Value) -> (bex_vm_types::Class, Vec<baml_type::RealizedTy>) {
    let ptr = value
        .as_object_ptr()
        .unwrap_or_else(|| unreachable!("class.Type receiver must be Object::Type"));
    let Object::Type(type_value) = vm.get_object(ptr) else {
        unreachable!("class.Type receiver must be Object::Type")
    };
    let baml_type::RealizedTy::Class(name, args, _) = &type_value.ty else {
        unreachable!("class.Type receiver must wrap a class type")
    };
    let ptr = type_value
        .defs()
        .classes
        .get(name)
        .copied()
        .or_else(|| {
            if type_value.owner.as_ptr().is_null() {
                None
            } else {
                let Object::Package(package) = vm.get_object(type_value.owner) else {
                    unreachable!("runtime type owner must be a Package")
                };
                package.classes.values().find(|ptr| {
                    matches!(vm.get_object(**ptr), Object::Class(class) if class.name == *name)
                }).copied()
            }
        })
        .or_else(|| vm.lookup_type(name))
        .unwrap_or_else(|| unreachable!("reflected class {name} must be loaded"));
    let Object::Class(class) = vm.get_object(ptr) else {
        unreachable!("reflected class name resolved to a non-class")
    };
    ((**class).clone(), args.clone())
}

fn reflected_enum(vm: &BexVm, value: Value) -> bex_vm_types::Enum {
    let ptr = value
        .as_object_ptr()
        .unwrap_or_else(|| unreachable!("enum.Type receiver must be Object::Type"));
    let Object::Type(type_value) = vm.get_object(ptr) else {
        unreachable!("enum.Type receiver must be Object::Type")
    };
    let baml_type::RealizedTy::Enum(name, _) = &type_value.ty else {
        unreachable!("enum.Type receiver must wrap an enum type")
    };
    let ptr = type_value
        .defs()
        .enums
        .get(name)
        .copied()
        .or_else(|| vm.lookup_type(name))
        .unwrap_or_else(|| unreachable!("reflected enum {name} must be loaded"));
    let Object::Enum(enm) = vm.get_object(ptr) else {
        unreachable!("reflected enum name resolved to a non-enum")
    };
    (**enm).clone()
}

fn opt_string(vm: &mut BexVm, value: Option<&str>) -> Value {
    value.map_or(Value::NULL, |s| Value::object(vm.alloc_string(s)))
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
        baml_type::RealizedTy::string(),
        baml_type::RealizedTy::string(),
        entries,
    ));
    let alias = opt_string(vm, alias);
    let description = opt_string(vm, description);
    let docstring = opt_string(vm, docstring);
    copy::reflect::Meta {
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
                copy::reflect::Span {
                    file,
                    start: i64::from(*start),
                    end: i64::from(*end),
                }
                .to_value(vm)
            });
            copy::reflect::Diagnostic {
                code,
                span,
                message,
            }
            .to_value(vm)
        })
        .collect();
    let diagnostic_ty = baml_type::RealizedTy::Class(
        baml_type::QualifiedTypeName::from_dotted_path("baml.reflect.Diagnostic"),
        vec![],
        baml_type::TyAttr::default(),
    );
    let diagnostics = Value::object(vm.alloc_array(diagnostic_ty, values));
    let message = Value::object(vm.alloc_string(message));
    let class = vm.resolve_class("baml.reflect.errors.CompilationError");
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
            if class.name.to_string() != "baml.reflect.enum.Value" {
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
            if meta_class.name.to_string() != "baml.reflect.Meta" {
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
    ($trait_name:ident) => {
        fn as_type(_vm: &BexVm, r#type: &Value) -> Value {
            *r#type
        }
    };
}

impl BamlClassReflectClassType for PackageBamlImpl {
    impl_as_type!(BamlClassReflectClassType);

    fn fields(vm: &mut BexVm, r#type: &Value) -> Vec<Value> {
        let (class, args) = reflected_class(vm, *r#type);
        class
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
                    Value::object(vm.alloc_static_type(ty))
                };
                let meta = alloc_meta(
                    vm,
                    field.alias.as_deref(),
                    field.description.as_deref(),
                    field.docstring.as_deref(),
                    &field.other,
                );
                copy::reflect::class::Field { name, r#type, meta }.to_value(vm)
            })
            .collect()
    }

    fn meta(vm: &mut BexVm, r#type: &Value) -> Value {
        let (class, _) = reflected_class(vm, *r#type);
        alloc_meta(
            vm,
            class.alias.as_deref(),
            class.description.as_deref(),
            class.docstring.as_deref(),
            &class.other,
        )
    }
}

impl BamlClassReflectEnumType for PackageBamlImpl {
    impl_as_type!(BamlClassReflectEnumType);

    fn values(vm: &mut BexVm, r#type: &Value) -> Vec<Value> {
        let enm = reflected_enum(vm, *r#type);
        enm.variants
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
                copy::reflect::r#enum::Value { name, meta }.to_value(vm)
            })
            .collect()
    }

    fn meta(vm: &mut BexVm, r#type: &Value) -> Value {
        let enm = reflected_enum(vm, *r#type);
        alloc_meta(
            vm,
            enm.alias.as_deref(),
            enm.description.as_deref(),
            enm.docstring.as_deref(),
            &enm.other,
        )
    }
}

impl BamlClassReflectUnionType for PackageBamlImpl {
    impl_as_type!(BamlClassReflectUnionType);

    fn member_types(vm: &mut BexVm, r#type: &Value) -> Vec<Value> {
        let baml_type::RealizedTy::Union(members, _) = reflected_ty(vm, *r#type) else {
            unreachable!("union.Type receiver must wrap a union type")
        };
        members
            .into_iter()
            .map(|ty| Value::object(vm.alloc_static_type(ty)))
            .collect()
    }
}

impl BamlClassReflectArrayType for PackageBamlImpl {
    impl_as_type!(BamlClassReflectArrayType);

    fn element_type(vm: &mut BexVm, r#type: &Value) -> Value {
        let baml_type::RealizedTy::List(element, _) = reflected_ty(vm, *r#type) else {
            unreachable!("array.Type receiver must wrap an array type")
        };
        Value::object(vm.alloc_static_type(*element))
    }
}

impl BamlClassReflectMapType for PackageBamlImpl {
    impl_as_type!(BamlClassReflectMapType);

    fn key_type(vm: &mut BexVm, r#type: &Value) -> Value {
        let baml_type::RealizedTy::Map { key, .. } = reflected_ty(vm, *r#type) else {
            unreachable!("map.Type receiver must wrap a map type")
        };
        Value::object(vm.alloc_static_type(*key))
    }

    fn value_type(vm: &mut BexVm, r#type: &Value) -> Value {
        let baml_type::RealizedTy::Map { value, .. } = reflected_ty(vm, *r#type) else {
            unreachable!("map.Type receiver must wrap a map type")
        };
        Value::object(vm.alloc_static_type(*value))
    }
}

impl BamlClassReflectFunctionType for PackageBamlImpl {
    impl_as_type!(BamlClassReflectFunctionType);

    fn params(vm: &mut BexVm, r#type: &Value) -> Vec<Value> {
        let baml_type::RealizedTy::Function { params, .. } = reflected_ty(vm, *r#type) else {
            unreachable!("function.Type receiver must wrap a function type")
        };
        params
            .into_iter()
            .map(|param| {
                let name = opt_string(vm, param.name.as_ref().map(baml_type::Name::as_str));
                let optional = param.is_optional();
                let r#type = Value::object(vm.alloc_static_type(param.ty));
                copy::reflect::function::Parameter {
                    name,
                    r#type,
                    optional,
                }
                .to_value(vm)
            })
            .collect()
    }

    fn return_type(vm: &mut BexVm, r#type: &Value) -> Value {
        let baml_type::RealizedTy::Function { ret, .. } = reflected_ty(vm, *r#type) else {
            unreachable!("function.Type receiver must wrap a function type")
        };
        Value::object(vm.alloc_static_type(*ret))
    }
}

impl BamlClassReflectInterfaceType for PackageBamlImpl {
    impl_as_type!(BamlClassReflectInterfaceType);

    fn implemented_by(vm: &BexVm, r#type: &Value, other: &Value) -> bool {
        <PackageBamlImpl as BamlClassTypeValue>::implemented_by(vm, r#type, other)
    }
}

impl BamlClassReflectLiteralType for PackageBamlImpl {
    impl_as_type!(BamlClassReflectLiteralType);
}

impl BamlClassReflectPrimitiveType for PackageBamlImpl {
    impl_as_type!(BamlClassReflectPrimitiveType);
}
