//! BEP-066 reflection kind views over `Object::Type`.

use std::sync::Arc;

use baml_compiler_diagnostics::{
    Diagnostic, DiagnosticId, DiagnosticIdentifierKind, DiagnosticMessageKind, DiagnosticPhase,
    Severity,
    runtime_type::{self, DuplicateMemberKind, InvalidIdentifierKind, SerializedKeyContainer},
};
use bex_heap::TlabHolder;
use bex_vm_types::types::{
    Class, ClassField, Enum, EnumVariant, InterfaceDef, Object, RuntimeImplRule, TypeValue, Value,
};
use indexmap::IndexMap;

use super::{
    BamlClassArray, BamlClassClass, BamlClassEnum, BamlClassFunction, BamlClassInterface,
    BamlClassInterfaceImplementation, BamlClassLiteral, BamlClassMap,
    BamlClassReflectTypeView_for_Array, BamlClassReflectTypeView_for_Class,
    BamlClassReflectTypeView_for_Enum, BamlClassReflectTypeView_for_Function,
    BamlClassReflectTypeView_for_Interface, BamlClassReflectTypeView_for_Literal,
    BamlClassReflectTypeView_for_Map, BamlClassReflectTypeView_for_Primitive,
    BamlClassReflectTypeView_for_Union, BamlClassUnion, BamlNamespaceClass, BamlNamespaceInterface,
    BamlNamespaceReflect, PackageReflectImpl, copy,
};
use crate::{BexVm, package_load::StdlibClass};

#[derive(Clone, Debug)]
struct InterfaceWitness {
    interface_ptr: bex_vm_types::HeapPtr,
    interface_ty: bex_vm_types::RealizedTy,
    field_links: IndexMap<baml_type::Name, baml_type::Name>,
}

impl bex_vm_types::BexRustData for InterfaceWitness {
    fn measure(&self, meter: &mut bex_vm_types::Meter) {
        meter.bytes(
            self.field_links.capacity() * size_of::<(baml_type::Name, baml_type::Name, usize)>(),
        );
    }
}

pub(super) struct WitnessField {
    pub(super) name: String,
    pub(super) ty: bex_vm_types::RealizedTy,
}

pub(super) struct ValidatedClassWitness {
    interface_ptr: bex_vm_types::HeapPtr,
    interface_args: Box<[bex_vm_types::RealizedTy]>,
    interface_assoc: Box<[(baml_type::Name, bex_vm_types::RealizedTy)]>,
    field_links: Vec<u32>,
}

fn witness_state(vm: &BexVm, value: Value) -> Result<InterfaceWitness, String> {
    let instance = vm
        .as_instance(&value)
        .map_err(|_| "implementations must contain reflect.interface.Implementation values")?;
    let Object::Class(class) = vm.get_object(instance.class) else {
        unreachable!("Instance.class must point to Object::Class")
    };
    if !vm
        .stdlib_heads()
        .is_class(StdlibClass::InterfaceImplementation, class.type_tag)
    {
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
    let witnesses = implementations
        .iter()
        .map(|implementation| witness_state(vm, *implementation))
        .collect();
    validate_witness_states(vm, class_fields, witnesses, diagnostics)
}

/// [`validate_class_witnesses`] over witnesses already read out of their
/// `reflect.interface.Implementation` values; an `Err` is a value that was
/// not one.
fn validate_witness_states(
    vm: &BexVm,
    class_fields: &[WitnessField],
    states: Vec<Result<InterfaceWitness, String>>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<ValidatedClassWitness> {
    let mut witnesses = Vec::with_capacity(states.len());
    for state in states {
        match state {
            Ok(witness) => witnesses.push(witness),
            Err(message) => {
                diagnostics.push(compiler_diagnostic(DiagnosticId::TypeMismatch, message));
            }
        }
    }

    // Shape and field checks happen before allocating the class/type value
    // (C-12, Fail-Before-Type). Coherence against the rules that already exist
    // needs the fresh class's identity and the whole proposed batch, so
    // `prepare_class_witnesses` checks it once the class is allocated and
    // before anything about it is published.
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
                    bex_vm_types::RealizedTy::Interface(name, _, _)
                        if name == &required.name
                )
            });
            if !present {
                diagnostics.push(compiler_diagnostic(
                    DiagnosticId::MissingRequiredInterface,
                    format!(
                        "interface witness for `{}` requires a witness for `{}`",
                        interface.name().display_name(),
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
                        interface.name().display_name(),
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
                        interface.name().display_name(),
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
                                interface.name().display_name(),
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
                        interface.name().display_name(),
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
            bex_vm_types::RealizedTy::Interface(_, args, assoc) => (args.clone(), assoc.clone()),
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

/// Turn the validated witnesses of the fresh class `ty` into impl rules and
/// check coherence for the whole batch against the rules that already exist,
/// without publishing anything: a witness the static world already covers
/// (`implements<T extends Gate> Pick for T` plus a `Gate` witness makes a
/// `Pick` witness an overlap) is rejected here, never silently shadowed.
pub(super) fn prepare_class_witnesses(
    vm: &BexVm,
    ty: &bex_vm_types::RealizedTy,
    witnesses: Vec<ValidatedClassWitness>,
) -> Result<Vec<RuntimeImplRule>, Diagnostic> {
    let goals: Vec<_> = witnesses
        .iter()
        .map(|witness| (witness.interface_ptr, witness.interface_args.clone()))
        .collect();
    let rules: Vec<_> = witnesses
        .into_iter()
        .map(|witness| RuntimeImplRule {
            interface_head: witness.interface_ptr,
            for_ty_pattern: bex_vm_types::TyTemplate::from(ty.clone()),
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
            // A witness supplies fields only, so its method table is EMPTY:
            // every method is the interface's default body, adopted at
            // resolution (`ImplResolver::rule_method_impl` falls back to the
            // interface's bound `default_fn` with the `[Self, iface args..]`
            // frame). The required-method gate at witness construction (the
            // "cannot be witnessed structurally" rejection) already excluded
            // any interface with a bodyless required method.
            methods: IndexMap::new(),
            field_links: witness.field_links.into_boxed_slice(),
        })
        .collect();
    let resolver = crate::package_baml::resolve::ImplResolver::new(vm).with_staged_rules(&rules);
    for (interface, args) in goals {
        resolver
            .check_sole_implementation(ty, interface, &args)
            .map_err(|message| compiler_diagnostic(DiagnosticId::OverlappingImplements, message))?;
    }
    Ok(rules)
}

/// Publish prepared witness rules for `class_ptr`. Each rule becomes an
/// ordinary heap `Object::ImplRule` — the resolver borrows it exactly like a
/// package-owned rule and the collector keeps its `interface_head`/
/// `methods[].fqn` current — held by the class itself, as the witnesses of
/// its [`Owner::Anonymous`](bex_vm_types::types::Owner::Anonymous): a
/// declaration with no package holds what a package would hold for it. The
/// dynamic dispatch table then only *finds* the rules; it holds no strong
/// reference, and the rules live exactly as long as the class does —
/// including while only an instance still retains it.
pub(super) fn register_class_witnesses(
    vm: &mut BexVm,
    class_ptr: bex_vm_types::HeapPtr,
    rules: Vec<RuntimeImplRule>,
) {
    if rules.is_empty() {
        return;
    }
    let entries: Vec<_> = rules
        .into_iter()
        .map(|rule| {
            let interface = rule.interface_head;
            let rule = vm.tlab.alloc(Object::ImplRule(Box::new(rule)));
            (interface, rule)
        })
        .collect();
    let Object::Class(class) = vm.get_object(class_ptr) else {
        unreachable!("witnessed class placeholder changed variant")
    };
    class
        .owner
        .set_witnesses(entries.iter().map(|(_, rule)| *rule).collect());
    for (interface, rule) in entries {
        vm.dynamic_dispatch
            .register_rule(interface, class_ptr, rule);
    }
}

/// One field of a class about to be minted, or the diagnostic that rejects it.
type FieldRow = (String, Result<ReflectedTypeRow, Diagnostic>);

/// Mint a closed runtime class from its field rows and structural witnesses,
/// reporting every rejected field and witness together.
fn mint_class(
    vm: &mut BexVm,
    class_name: &str,
    fields: Vec<FieldRow>,
    witnesses: Vec<Result<InterfaceWitness, String>>,
) -> Result<Value, crate::errors::VmRustFnError> {
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
    for (field_name, row) in fields {
        if !is_baml_identifier(&field_name) {
            diagnostics.push(
                runtime_type::invalid_identifier(
                    InvalidIdentifierKind::Field,
                    &format!("{class_name}.{field_name}"),
                )
                .with_phase(DiagnosticPhase::Hir),
            );
        }
        let row = match row {
            Ok(row) => row,
            Err(diagnostic) => {
                diagnostics.push(diagnostic);
                continue;
            }
        };
        let serialized_key = row.alias.as_deref().unwrap_or(&field_name);
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
            name: field_name.clone(),
            ty: row.type_value.ty.clone(),
        });
        class_fields.push(ClassField {
            name: field_name,
            field_type: row.type_value.ty.clone().into(),
            field_template: bex_vm_types::TyTemplate::from(row.type_value.ty.clone()),
            description: row.description,
            alias: row.alias,
            docstring: row.docstring,
            other: row.other,
            skip: false,
            stream_done: false,
            must_exist: false,
            runtime_type: Some(row.type_value),
        });
    }

    let witnesses = validate_witness_states(vm, &witness_fields, witnesses, &mut diagnostics);
    if !diagnostics.is_empty() {
        return Err(crate::errors::VmRustFnError::thrown_fresh(
            alloc_compilation_error(vm, &diagnostics),
        ));
    }

    let type_tag = baml_type::typetag::TypeTag::fresh_dynamic();
    let class_ptr = vm.tlab.alloc(Object::Class(Box::new(Class {
        path: bex_vm_types::types::LocalName::new(Vec::new(), baml_type::Name::new(class_name)),
        fields: class_fields,
        description: None,
        alias: None,
        docstring: None,
        other: IndexMap::new(),
        stream_done: false,
        declared_crossing: baml_type::ClassCrossing::Fields,
        type_tag,
        cleanup: None,
        methods: IndexMap::new(),
        generic_param_count: 0,
        owner: bex_vm_types::types::Owner::anonymous(),
        telemetry_definition: bex_vm_types::DefinitionCell::default(),
    })));
    // The head is built off the declaration that was just allocated, so the
    // type reaches it directly — there is no table to consult and no name
    // to re-resolve.
    let ty = bex_vm_types::RealizedTy::Class(
        bex_vm_types::TypeHead::new(class_ptr, type_tag),
        Box::new([]),
    );
    let rules = prepare_class_witnesses(vm, &ty, witnesses).map_err(|diagnostic| {
        crate::errors::VmRustFnError::thrown_fresh(alloc_compilation_error(vm, &[diagnostic]))
    })?;
    register_class_witnesses(vm, class_ptr, rules);
    let ty_value = Value::object(vm.tlab.alloc_type(TypeValue::new(ty)));
    Ok(alloc_kind_view(
        vm,
        baml_type::type_kind::TypeKind::Class,
        ty_value,
    ))
}

/// Why a structural witness cannot implement `interface`, if it cannot: a
/// witness supplies fields only, so every method needs a default body.
fn structural_witness_blocker(interface: &InterfaceDef) -> Option<String> {
    let required = interface
        .methods
        .iter()
        .find(|method| method.default.is_none())?;
    Some(format!(
        "interface `{}` cannot be witnessed structurally because required method `{}` has no default body; use `reflect.Package.compile` with an `implements` block",
        interface.name().display_name(),
        required.name
    ))
}

/// `interface` and every interface it requires, transitively, each realized
/// at the instantiation its requirer names. Base-first: a required interface
/// precedes the interface that requires it, required interfaces keep their
/// `requires` order, and an interface reached twice appears once, at its
/// first position.
fn required_interface_closure(
    vm: &BexVm,
    interface: bex_vm_types::RealizedTy,
) -> Result<Vec<bex_vm_types::RealizedTy>, String> {
    fn visit(
        vm: &BexVm,
        interface_ty: bex_vm_types::RealizedTy,
        seen: &mut std::collections::HashSet<bex_vm_types::RealizedTy>,
        closure: &mut Vec<bex_vm_types::RealizedTy>,
    ) -> Result<(), String> {
        if !seen.insert(interface_ty.clone()) {
            return Ok(());
        }
        let bex_vm_types::RealizedTy::Interface(head, args, assoc) = &interface_ty else {
            unreachable!("the closure walk visits only interfaces")
        };
        let Object::Interface(interface) = vm.get_object(head.ptr()) else {
            unreachable!("an interface type's head points at Object::Interface")
        };
        for required in &interface.requires {
            let required = realize_required_interface(vm, required, interface, args, assoc)?;
            visit(vm, required, seen, closure)?;
        }
        closure.push(interface_ty);
        Ok(())
    }

    let mut closure = Vec::new();
    visit(
        vm,
        interface,
        &mut std::collections::HashSet::new(),
        &mut closure,
    )?;
    Ok(closure)
}

/// Realize a `requires` target of `interface`, instantiated at `args` and
/// `assoc`. Every associated type of the target must be pinned: a runtime
/// class has no declaration to infer one from.
fn realize_required_interface(
    vm: &BexVm,
    required: &bex_vm_types::RuntimeInterface,
    interface: &InterfaceDef,
    args: &[bex_vm_types::RealizedTy],
    assoc: &[(baml_type::Name, bex_vm_types::RealizedTy)],
) -> Result<bex_vm_types::RealizedTy, String> {
    let Object::Interface(target) = vm.get_object(required.name.ptr()) else {
        unreachable!("a `requires` head points at Object::Interface")
    };
    let describe = || {
        format!(
            "interface `{}` requires `{}`, which",
            interface.name().display_name(),
            target.name().display_name()
        )
    };
    let realize = |ty: &bex_vm_types::RuntimeTy| {
        substitute_witness_field_type(ty, interface, args, assoc)
            .and_then(|ty| {
                bex_vm_types::RealizedTy::try_from(ty)
                    .map_err(|_| "depends on an unsupported open type".to_string())
            })
            .map_err(|message| format!("{} {message}", describe()))
    };
    let generics = required
        .generics
        .iter()
        .map(realize)
        .collect::<Result<Box<[_]>, _>>()?;
    let mut bindings = required
        .associated_types
        .iter()
        .map(|(name, ty)| Ok((name.clone(), realize(ty)?)))
        .collect::<Result<Vec<_>, String>>()?;
    if let Some((member, _)) = target
        .assoc
        .iter()
        .find(|(member, _)| !bindings.iter().any(|(name, _)| name == member))
    {
        return Err(format!(
            "{} leaves associated type `{member}` unbound",
            describe()
        ));
    }
    bindings.sort_by(|(left, _), (right, _)| left.cmp(right));
    Ok(bex_vm_types::RealizedTy::Interface(
        required.name,
        generics,
        bindings.into(),
    ))
}

/// What a class implementing every interface of a `requires` closure takes
/// from those interfaces.
struct InterfaceMembers {
    /// Each interface's fields in declaration order; a field two interfaces
    /// declare at one type appears once.
    rows: Vec<FieldRow>,
    /// Each inherited field's name, and the interface that declares it first.
    declared_by: IndexMap<String, String>,
    /// One witness per interface, linking each interface field to the class
    /// field of its name.
    witnesses: Vec<Result<InterfaceWitness, String>>,
}

/// Collect the [`InterfaceMembers`] of `closure`. `Err` when an interface
/// cannot be implemented structurally, one of its field types cannot be
/// realized, or two interfaces declare one field at different types.
fn interface_closure_members(
    vm: &BexVm,
    closure: Vec<bex_vm_types::RealizedTy>,
) -> Result<InterfaceMembers, String> {
    let mut rows = Vec::new();
    // Each field taken so far: its type and the interface it came from.
    let mut taken = IndexMap::<baml_type::Name, (bex_vm_types::RealizedTy, String)>::new();
    let mut witnesses = Vec::with_capacity(closure.len());
    for interface_ty in closure {
        let bex_vm_types::RealizedTy::Interface(head, _, _) = &interface_ty else {
            unreachable!("the closure holds only interfaces")
        };
        let interface_ptr = head.ptr();
        let Object::Interface(interface) = vm.get_object(interface_ptr) else {
            unreachable!("an interface type's head points at Object::Interface")
        };
        if let Some(message) = structural_witness_blocker(interface) {
            return Err(message);
        }
        let interface_name = interface.name().display_name().to_string();
        let mut field_links = IndexMap::with_capacity(interface.fields.len());
        for field in &interface.fields {
            let ty = realize_witness_field_type(&field.ty, interface, &interface_ty).map_err(
                |message| {
                    format!(
                        "interface field `{interface_name}.{}` {message}",
                        field.name
                    )
                },
            )?;
            field_links.insert(field.name.clone(), field.name.clone());
            match taken.get(&field.name) {
                None => {
                    rows.push((
                        field.name.to_string(),
                        Ok(ReflectedTypeRow {
                            type_value: TypeValue::new(ty.clone()),
                            alias: None,
                            description: None,
                            docstring: field.docstring.clone(),
                            other: IndexMap::new(),
                        }),
                    ));
                    taken.insert(field.name.clone(), (ty, interface_name.clone()));
                }
                Some((first_ty, _)) if *first_ty == ty => {}
                Some((first_ty, first_interface)) => {
                    return Err(format!(
                        "interfaces `{first_interface}` and `{interface_name}` both declare field `{}`, with different types `{first_ty}` and `{ty}`",
                        field.name
                    ));
                }
            }
        }
        witnesses.push(Ok(InterfaceWitness {
            interface_ptr,
            interface_ty,
            field_links,
        }));
    }
    Ok(InterfaceMembers {
        rows,
        declared_by: taken
            .into_iter()
            .map(|(field, (_, interface))| (field.to_string(), interface))
            .collect(),
        witnesses,
    })
}

impl BamlClassClass for PackageReflectImpl {
    fn _new(
        vm: &mut BexVm,
        name: &bex_str::BexStr,
        fields: &IndexMap<bex_str::BexStr, Value>,
        implementations: &[Value],
    ) -> Result<Value, crate::errors::VmRustFnError> {
        let rows = fields
            .iter()
            .map(|(field_name, value)| {
                let row = reflected_type_row(vm, *value)
                    .map_err(|message| compiler_diagnostic(DiagnosticId::TypeMismatch, message));
                (field_name.to_string(), row)
            })
            .collect();
        let witnesses = implementations
            .iter()
            .map(|implementation| witness_state(vm, *implementation))
            .collect();
        mint_class(vm, name.as_str(), rows, witnesses)
    }

    fn _extends(
        vm: &mut BexVm,
        name: &bex_str::BexStr,
        fields: &IndexMap<bex_str::BexStr, Value>,
    ) -> Result<Value, crate::errors::VmRustFnError> {
        let Some(interface_ty) = vm.current_call_type_args().first().cloned() else {
            unreachable!("extends<I> receives one runtime type argument")
        };
        let members = if matches!(interface_ty, bex_vm_types::RealizedTy::Interface(..)) {
            required_interface_closure(vm, interface_ty)
                .and_then(|closure| interface_closure_members(vm, closure))
        } else {
            Err(format!(
                "reflect.Class.extends expects an interface type, got `{interface_ty}`"
            ))
        };
        let InterfaceMembers {
            mut rows,
            declared_by,
            witnesses,
        } = members.map_err(|message| {
            crate::errors::VmRustFnError::thrown_fresh(alloc_compilation_error(
                vm,
                &[compiler_diagnostic(DiagnosticId::TypeMismatch, message)],
            ))
        })?;
        for (field_name, value) in fields {
            let row = if let Some(interface) = declared_by.get(field_name.as_str()) {
                Err(compiler_diagnostic(
                    DiagnosticId::DuplicateField,
                    format!(
                        "duplicate field `{name}.{field_name}`: interface `{interface}` already declares it"
                    ),
                ))
            } else {
                reflected_type_row(vm, *value)
                    .map_err(|message| compiler_diagnostic(DiagnosticId::TypeMismatch, message))
            };
            rows.push((field_name.to_string(), row));
        }
        mint_class(vm, name.as_str(), rows, witnesses)
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
                "reflect.Class.get_field",
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
                        class.name().display_name()
                    ),
                ));
            };
            (
                instance.load_field(index),
                class.name().display_name().to_string(),
            )
        };
        // The caller's `T`. Erasing a missing one to `unknown` would make the
        // membership test below vacuously true, admitting a field read at any
        // type; an absent type argument is a frame-seeding bug, not a value to
        // stand in for.
        let Some(expected) = vm.current_call_type_args().first().cloned() else {
            return Err(crate::errors::VmRustFnError::InternalError(
                bex_vm_types::errors::VmInternalError::MissingNativeFunction {
                    name: "reflect.Class.get_field: missing type argument".to_string(),
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

    fn instantiate(
        vm: &mut BexVm,
        fields: &IndexMap<bex_str::BexStr, Value>,
    ) -> Result<Value, crate::errors::VmRustFnError> {
        // The caller's `T`, as in `get_field`: a missing one is a
        // frame-seeding bug.
        let Some(class_ty) = vm.current_call_type_args().first().cloned() else {
            return Err(crate::errors::VmRustFnError::InternalError(
                bex_vm_types::errors::VmInternalError::MissingNativeFunction {
                    name: "reflect.Class.instantiate: missing type argument".to_string(),
                },
            ));
        };
        let bex_vm_types::RealizedTy::Class(head, type_args) = &class_ty else {
            return Err(invalid_argument(
                vm,
                format!("reflect.Class.instantiate expects a class type, got `{class_ty}`"),
            ));
        };
        debug_assert!(head.is_resolved());
        let class_ptr = head.ptr();
        let Object::Class(class) = vm.get_object(class_ptr) else {
            unreachable!("a class type's head points at Object::Class")
        };
        // Refuse what the compiler refuses for a class literal: a reflection
        // kind view, whose values only reflection creates, and (below) a
        // class holding native state.
        if baml_type::type_kind::TypeKind::ALL
            .iter()
            .any(|kind| vm.lang_declaration(kind.class()) == Some(class_ptr))
        {
            return Err(invalid_argument(
                vm,
                format!(
                    "reflect.Class.instantiate cannot build `{class_ty}`: reflection kinds are obtained from a type value"
                ),
            ));
        }
        let param_count = class.generic_param_count;
        if type_args.len() != param_count {
            return Err(invalid_argument(
                vm,
                format!(
                    "reflect.Class.instantiate expects {param_count} type arguments for `{class_ty}`, got {}",
                    type_args.len()
                ),
            ));
        }
        // Each declared field with its type realized against `T`'s arguments,
        // and whether a missing value may stand in as `null`.
        let program: &BexVm = vm;
        let declared: Vec<(String, bex_vm_types::RealizedTy, bool)> = class
            .fields
            .iter()
            .map(|field| {
                let ty = program.realize_field_ty(&field.field_template, type_args);
                let nullable = baml_type::accepts_null::accepts_null(ty.as_ty(), &|head| {
                    baml_type::normalize::TypeContext::alias_def(program, head)
                });
                (field.name.clone(), ty, nullable)
            })
            .collect();
        // Only the class's own functions create `$rust_type` state, so a
        // class literal cannot build such a class either. Refuse it here with
        // that reason, before the field check below reports a bare mismatch.
        if let Some((name, ..)) = declared
            .iter()
            .find(|(_, ty, _)| matches!(ty, bex_vm_types::RealizedTy::RustType))
        {
            return Err(invalid_argument(
                vm,
                format!(
                    "reflect.Class.instantiate cannot build `{class_ty}`: its field `{name}` holds state that only the class's own functions create"
                ),
            ));
        }
        // Values are stored exactly as given: the membership test only, never
        // `call_any`'s int-to-float widening, which would allocate a new value.
        let mut values = Vec::with_capacity(declared.len());
        for (name, ty, nullable) in &declared {
            let value = match fields.get(name.as_str()) {
                Some(&value) if field_value_fits(vm, value, ty) => value,
                Some(&value) => {
                    let got = match vm.value_concrete_ty(value) {
                        Some(got) => {
                            let got = bex_vm_types::RealizedTy::from(got);
                            let hint = container_hint(vm, &got, ty);
                            format!("`{got}`{hint}")
                        }
                        None => "a value with no BAML type".to_string(),
                    };
                    return Err(invalid_argument(
                        vm,
                        format!(
                            "reflect.Class.instantiate: field `{class_ty}.{name}` expects `{ty}`, got {got}"
                        ),
                    ));
                }
                None if *nullable => Value::NULL,
                None => {
                    return Err(invalid_argument(
                        vm,
                        format!(
                            "reflect.Class.instantiate: missing field `{class_ty}.{name}` of type `{ty}`"
                        ),
                    ));
                }
            };
            values.push(value);
        }
        if let Some(key) = fields
            .keys()
            .find(|key| !declared.iter().any(|(name, ..)| name == key.as_str()))
        {
            return Err(invalid_argument(
                vm,
                format!("reflect.Class.instantiate: class `{class_ty}` has no field `{key}`"),
            ));
        }
        Ok(Value::object(vm.tlab.alloc_instance_with_type_args(
            class_ptr,
            type_args.clone(),
            values,
        )))
    }

    fn fields(vm: &mut BexVm, r#type: &Value) -> Result<Vec<Value>, crate::errors::VmRustFnError> {
        Ok(reflected_class_fields(vm, *r#type)?
            .into_iter()
            .map(|(field, type_value)| {
                let name = Value::object(vm.alloc_string(field.name.as_str()));
                let r#type = Value::object(vm.tlab.alloc_type(type_value));
                let meta = alloc_meta(
                    vm,
                    field.alias.as_deref(),
                    field.description.as_deref(),
                    field.docstring.as_deref(),
                    &field.other,
                    field.skip,
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

    fn metadata(vm: &mut BexVm, r#type: &Value) -> Result<Value, crate::errors::VmRustFnError> {
        let (class, _) = reflected_class(vm, *r#type)?;
        Ok(alloc_meta(
            vm,
            class.alias.as_deref(),
            class.description.as_deref(),
            class.docstring.as_deref(),
            &class.other,
            false,
        ))
    }
}
impl BamlClassEnum for PackageReflectImpl {
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
            false,
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
            path: bex_vm_types::types::LocalName::new(Vec::new(), baml_type::Name::new(enum_name)),
            variants,
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            type_tag,
            owner: bex_vm_types::types::Owner::anonymous(),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
        })));
        let ty = bex_vm_types::RealizedTy::Enum(bex_vm_types::TypeHead::new(enum_ptr, type_tag));
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

    fn value_of(
        vm: &mut BexVm,
        name: &bex_str::BexStr,
    ) -> Result<Value, crate::errors::VmRustFnError> {
        let Some(enum_ty) = vm.current_call_type_args().first().cloned() else {
            return Err(crate::errors::VmRustFnError::InternalError(
                bex_vm_types::errors::VmInternalError::MissingNativeFunction {
                    name: "reflect.Enum.value_of: missing type argument".to_string(),
                },
            ));
        };
        // A single variant's type (`Team.Technical`) is a literal type, not an
        // enum, and is refused here like any other non-enum.
        let bex_vm_types::RealizedTy::Enum(head) = &enum_ty else {
            return Err(invalid_argument(
                vm,
                format!("reflect.Enum.value_of expects an enum type, got `{enum_ty}`"),
            ));
        };
        debug_assert!(head.is_resolved());
        let enum_ptr = head.ptr();
        let Object::Enum(enm) = vm.get_object(enum_ptr) else {
            unreachable!("an enum type's head points at Object::Enum")
        };
        if let Some(index) = enm
            .variants
            .iter()
            .position(|variant| variant.name == name.as_str())
        {
            return Ok(Value::object(vm.alloc_variant(enum_ptr, index)));
        }
        let message = match enm
            .variants
            .iter()
            .find(|variant| variant.alias.as_deref() == Some(name.as_str()))
        {
            Some(aliased) => format!(
                "reflect.Enum.value_of: enum `{enum_ty}` has no variant `{name}`; `{name}` is the alias of `{}`",
                aliased.name
            ),
            None => format!("reflect.Enum.value_of: enum `{enum_ty}` has no variant `{name}`"),
        };
        Err(invalid_argument(vm, message))
    }

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
                    variant.skip,
                );
                copy::r#enum::Value { name, meta }.to_value(vm)
            })
            .collect())
    }

    fn metadata(vm: &mut BexVm, r#type: &Value) -> Result<Value, crate::errors::VmRustFnError> {
        let enm = reflected_enum(vm, *r#type)?;
        Ok(alloc_meta(
            vm,
            enm.alias.as_deref(),
            enm.description.as_deref(),
            enm.docstring.as_deref(),
            &enm.other,
            false,
        ))
    }
}
impl BamlNamespaceClass for PackageReflectImpl {}

impl BamlNamespaceInterface for PackageReflectImpl {}

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
            let interface_name = interface.name().display_name().to_string();
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
            let interface_name = interface.name().display_name().to_string();
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

impl BamlClassInterface for PackageReflectImpl {
    fn implementation(vm: &mut BexVm) -> Result<Value, crate::errors::VmRustFnError> {
        let Some(interface_ty) = vm.current_call_type_args().first().cloned() else {
            unreachable!("implementation<I> receives one runtime type argument")
        };
        let bex_vm_types::RealizedTy::Interface(interface_head, _, _) = &interface_ty else {
            return Err(crate::errors::VmRustFnError::thrown_fresh(
                alloc_compilation_error(
                    vm,
                    &[compiler_diagnostic(
                        DiagnosticId::TypeMismatch,
                        format!(
                            "reflect.Interface.implementation expects an interface type, got `{interface_ty}`"
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
        if let Some(message) = structural_witness_blocker(interface) {
            return Err(crate::errors::VmRustFnError::thrown_fresh(
                alloc_compilation_error(
                    vm,
                    &[compiler_diagnostic(DiagnosticId::TypeMismatch, message)],
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
impl BamlClassLiteral for PackageReflectImpl {
    fn new(vm: &mut BexVm, value: &Value) -> Value {
        let literal = if let Some(value) = value.as_int() {
            baml_type::Literal::Int(value)
        } else if let Some(value) = value.as_bool() {
            baml_type::Literal::Bool(value)
        } else if let Ok(value) = vm.as_bigint(value) {
            baml_type::Literal::Bigint((**value).clone())
        } else if let Ok(value) = vm.as_string(value) {
            baml_type::Literal::String(value.to_string())
        } else {
            unreachable!("reflect.Literal.new argument checked by native glue")
        };
        alloc_runtime_composite(
            vm,
            baml_type::type_kind::TypeKind::Literal,
            bex_vm_types::RealizedTy::Literal(literal, baml_type::Freshness::Regular),
        )
    }

    fn value(vm: &mut BexVm, r#type: &Value) -> Result<Value, crate::errors::VmRustFnError> {
        let ty = reflected_ty(vm, *r#type, baml_type::type_kind::TypeKind::Literal)?;
        match ty {
            bex_vm_types::RealizedTy::Literal(literal, _) => Ok(match literal {
                baml_type::Literal::String(s) => Value::object(vm.alloc_string(s)),
                baml_type::Literal::Int(n) => Value::int(n),
                baml_type::Literal::Bool(b) => Value::bool(b),
                baml_type::Literal::Bigint(n) => vm.try_alloc_bigint(Arc::new(n))?,
                baml_type::Literal::Float(s) => Value::object(
                    vm.alloc_float(
                        s.parse()
                            .expect("a float literal has a valid representation"),
                    ),
                ),
            }),
            bex_vm_types::RealizedTy::EnumVariant(head, name) => {
                let Object::Enum(enm) = vm.get_object(head.ptr()) else {
                    unreachable!("an enum variant type's head points at Object::Enum")
                };
                let index = enm
                    .variants
                    .iter()
                    .position(|variant| variant.name == name)
                    .expect("an enum variant type names a declared variant");
                Ok(Value::object(vm.alloc_variant(head.ptr(), index)))
            }
            _ => unreachable!("a Literal-classified type is a literal or enum variant"),
        }
    }
}

impl BamlClassMap for PackageReflectImpl {
    fn _new(vm: &mut BexVm, key: &Value, value: &Value) -> Value {
        let key = reflected_type_value(vm, *key);
        let value = reflected_type_value(vm, *value);
        alloc_runtime_composite(
            vm,
            baml_type::type_kind::TypeKind::Map,
            bex_vm_types::RealizedTy::Map {
                key: Box::new(key.ty),
                value: Box::new(value.ty),
            },
        )
    }

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
impl BamlClassUnion for PackageReflectImpl {
    fn _new(vm: &mut BexVm, types: &[Value]) -> Result<Value, crate::errors::VmRustFnError> {
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
            bex_vm_types::RealizedTy::Union(members),
        ))
    }

    fn member_types(
        vm: &mut BexVm,
        r#type: &Value,
    ) -> Result<Vec<Value>, crate::errors::VmRustFnError> {
        let ty = reflected_ty(vm, *r#type, baml_type::type_kind::TypeKind::Union)?;
        let bex_vm_types::RealizedTy::Union(members) = ty else {
            unreachable!("a Union-classified type is RealizedTy::Union")
        };
        Ok(members
            .into_iter()
            .map(|ty| Value::object(vm.alloc_type(bex_vm_types::types::TypeValue::new(ty))))
            .collect())
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
    let bex_vm_types::RealizedTy::Interface(head, args, assoc) = witnessed else {
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
        RuntimeTy::TypeVar(param) => {
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
        } if matches!(&**base, RuntimeTy::TypeVar(param) if param.as_str() == "Self")
            && projection_interface.name.tag() == interface.type_tag =>
        {
            assoc
                .iter()
                .find(|(name, _)| name == member)
                .map(|(_, ty)| RuntimeTy::from(ty.clone()))
                .ok_or_else(|| format!("is missing associated binding `{member}`"))
        }
        RuntimeTy::List(inner) => Ok(RuntimeTy::List(Box::new(substitute_witness_field_type(
            inner, interface, args, assoc,
        )?))),
        RuntimeTy::Map { key, value } => Ok(RuntimeTy::Map {
            key: Box::new(substitute_witness_field_type(key, interface, args, assoc)?),
            value: Box::new(substitute_witness_field_type(
                value, interface, args, assoc,
            )?),
        }),
        RuntimeTy::Union(members) => Ok(RuntimeTy::Union(
            members
                .iter()
                .map(|member| substitute_witness_field_type(member, interface, args, assoc))
                .collect::<Result<Box<[_]>, _>>()?,
        )),
        RuntimeTy::Class(name, type_args) => Ok(RuntimeTy::Class(
            *name,
            type_args
                .iter()
                .map(|arg| substitute_witness_field_type(arg, interface, args, assoc))
                .collect::<Result<Box<[_]>, _>>()?,
        )),
        RuntimeTy::Interface(name, type_args, bindings) => Ok(RuntimeTy::Interface(
            *name,
            type_args
                .iter()
                .map(|arg| substitute_witness_field_type(arg, interface, args, assoc))
                .collect::<Result<Box<[_]>, _>>()?,
            bindings
                .iter()
                .map(|(name, ty)| {
                    Ok((
                        name.clone(),
                        substitute_witness_field_type(ty, interface, args, assoc)?,
                    ))
                })
                .collect::<Result<Box<[_]>, String>>()?,
        )),
        RuntimeTy::Function {
            params,
            ret,
            throws,
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
                .collect::<Result<Box<[_]>, String>>()?,
            ret: Box::new(substitute_witness_field_type(ret, interface, args, assoc)?),
            throws: Box::new(substitute_witness_field_type(
                throws, interface, args, assoc,
            )?),
        }),
        RuntimeTy::Future(value, error) => Ok(RuntimeTy::Future(
            Box::new(substitute_witness_field_type(
                value, interface, args, assoc,
            )?),
            Box::new(substitute_witness_field_type(
                error, interface, args, assoc,
            )?),
        )),
        RuntimeTy::AssociatedTypeProjection {
            base,
            interface: projection_interface,
            member,
        } => Ok(RuntimeTy::AssociatedTypeProjection {
            base: Box::new(substitute_witness_field_type(base, interface, args, assoc)?),
            interface: Box::new(bex_vm_types::RuntimeInterface::new(
                projection_interface.name,
                projection_interface
                    .generics
                    .iter()
                    .map(|ty| substitute_witness_field_type(ty, interface, args, assoc))
                    .collect::<Result<Box<[_]>, _>>()?,
                projection_interface
                    .associated_types
                    .iter()
                    .map(|(name, ty)| {
                        Ok((
                            name.clone(),
                            substitute_witness_field_type(ty, interface, args, assoc)?,
                        ))
                    })
                    .collect::<Result<Box<[_]>, String>>()?,
            )),
            member: member.clone(),
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
/// composite constructors (`Literal.new`, `Map.new`, `Union.new`) declare the
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
    if !vm
        .stdlib_heads()
        .is_class(StdlibClass::WithMeta, class.type_tag)
    {
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
            .as_string_map(&instance.load_field(4))
            .map_err(|_| "reflect.WithMeta.other must be map<string, string>".to_string())?
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
    let class = vm.lang_head(kind.class()).unwrap_or_else(|| {
        unreachable!(
            "reflection kind class `{}` is declared by the stdlib",
            kind.class()
        )
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
                "{} `_ty` holds a {actual} type; the field is private to \
                 reflection and must keep its view's kind",
                expected.class(),
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
        return Err(invariant_panic(&format!("{}-kind", actual.name())));
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
    let is_view = baml_type::type_kind::TypeKind::ALL
        .iter()
        .any(|kind| vm.lang_declaration(kind.class()) == Some(instance.class));
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
) -> Result<(bex_vm_types::Class, Box<[bex_vm_types::RealizedTy]>), crate::errors::VmRustFnError> {
    let ty_value = view_type_value(vm, view, baml_type::type_kind::TypeKind::Class)?;
    let ptr = ty_value
        .as_object_ptr()
        .unwrap_or_else(|| unreachable!("view_type_value returns an Object::Type"));
    let Object::Type(type_value) = vm.get_object(ptr) else {
        unreachable!("view_type_value returns an Object::Type")
    };
    let bex_vm_types::RealizedTy::Class(head, args) = &type_value.ty else {
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
    let bex_vm_types::RealizedTy::Enum(head) = &type_value.ty else {
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
///
/// `WithMeta<T>` is generic, so the row carries its type argument like any
/// other generic instance: `reflect.Type` or `reflect.class.PendingType`.
/// The generated `copy::WithMeta::to_value` allocates without type
/// arguments, and BAML code that tests a row's type (a `match` over
/// `TypeView | WithMeta<Type>`) reads them.
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
    let type_arg = match payload.as_object_ptr().map(|ptr| vm.get_object(ptr)) {
        Some(Object::Type(_)) => bex_vm_types::RealizedTy::Type,
        _ => {
            let pending = vm
                .lang_head(crate::lang_items::REFLECT_CLASS_PENDING_TYPE)
                .unwrap_or_else(|| unreachable!("the stdlib declares reflect.class.PendingType"));
            bex_vm_types::RealizedTy::Class(pending, Box::new([]))
        }
    };
    let class = vm.lang_object(crate::lang_items::REFLECT_WITH_META);
    Value::object(vm.tlab.alloc_instance_with_type_args(
        class,
        Box::new([type_arg]),
        vec![payload, alias, description, docstring, other],
    ))
}

fn alloc_meta(
    vm: &mut BexVm,
    alias: Option<&str>,
    description: Option<&str>,
    docstring: Option<&str>,
    other: &IndexMap<String, String>,
    skip: bool,
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
        skip,
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
            let source_span = span.map(|(file, start, end)| bex_vm_types::RuntimeSourceSpan {
                file: file.clone(),
                start: usize::try_from(*start).expect("source offsets fit usize"),
                end: usize::try_from(*end).expect("source offsets fit usize"),
            });
            let highlights = diagnostic
                .message_highlights
                .iter()
                .map(|highlight| bex_vm_types::RuntimeDiagnosticHighlight {
                    start: highlight.start,
                    end: highlight.end,
                    kind: match highlight.kind {
                        DiagnosticMessageKind::Identifier(DiagnosticIdentifierKind::Type) => {
                            bex_vm_types::RuntimeDiagnosticHighlightKind::IdentifierType
                        }
                        DiagnosticMessageKind::Identifier(DiagnosticIdentifierKind::Function) => {
                            bex_vm_types::RuntimeDiagnosticHighlightKind::IdentifierFunction
                        }
                        DiagnosticMessageKind::Identifier(DiagnosticIdentifierKind::Field) => {
                            bex_vm_types::RuntimeDiagnosticHighlightKind::IdentifierField
                        }
                        DiagnosticMessageKind::Identifier(DiagnosticIdentifierKind::Variable) => {
                            bex_vm_types::RuntimeDiagnosticHighlightKind::IdentifierVariable
                        }
                        DiagnosticMessageKind::Identifier(
                            DiagnosticIdentifierKind::EnumVariant,
                        ) => bex_vm_types::RuntimeDiagnosticHighlightKind::IdentifierEnumVariant,
                        DiagnosticMessageKind::Identifier(DiagnosticIdentifierKind::Attribute) => {
                            bex_vm_types::RuntimeDiagnosticHighlightKind::IdentifierAttribute
                        }
                        DiagnosticMessageKind::TypeExpression => {
                            bex_vm_types::RuntimeDiagnosticHighlightKind::TypeExpression
                        }
                        DiagnosticMessageKind::Code => {
                            bex_vm_types::RuntimeDiagnosticHighlightKind::Code
                        }
                    },
                })
                .collect();
            let runtime = bex_vm_types::RuntimeCompileDiagnostic {
                code: diagnostic.code().to_string(),
                message: diagnostic.message_with_primary_label().into_owned(),
                severity: match diagnostic.severity {
                    Severity::Error => bex_vm_types::RuntimeDiagnosticSeverity::Error,
                    Severity::Warning => bex_vm_types::RuntimeDiagnosticSeverity::Warning,
                    Severity::Info => bex_vm_types::RuntimeDiagnosticSeverity::Info,
                },
                span: source_span,
                details: Some(Box::new(bex_vm_types::RuntimeDiagnosticDetails {
                    headline: diagnostic.message.clone(),
                    primary_label: diagnostic
                        .annotations
                        .iter()
                        .find(|annotation| annotation.is_primary)
                        .and_then(|annotation| annotation.message.clone()),
                    phase: match diagnostic.phase {
                        DiagnosticPhase::Parse => bex_vm_types::RuntimeDiagnosticPhase::Parse,
                        DiagnosticPhase::Hir => bex_vm_types::RuntimeDiagnosticPhase::Hir,
                        DiagnosticPhase::Validation => {
                            bex_vm_types::RuntimeDiagnosticPhase::Validation
                        }
                        DiagnosticPhase::Type => bex_vm_types::RuntimeDiagnosticPhase::Type,
                    },
                    message_highlights: highlights,
                    annotations: Vec::new(),
                    related_info: Vec::new(),
                })),
            };
            super::reflect::diagnostic_value(vm, &runtime)
        })
        .collect();
    let diagnostic_ty = bex_vm_types::RealizedTy::Class(
        vm.lang_head(crate::lang_items::REFLECT_DIAGNOSTIC)
            .unwrap_or_else(|| unreachable!("`reflect.Diagnostic` is declared by the stdlib")),
        Box::new([]),
    );
    let diagnostics = Value::object(vm.alloc_array(diagnostic_ty, values));
    let message = Value::object(vm.alloc_string(message));
    let class = vm.lang_object(crate::lang_items::REFLECT_ERRORS_COMPILATION_ERROR);
    Value::object(vm.alloc_instance(class, vec![message, diagnostics]))
}

fn enum_row(vm: &BexVm, value: Value) -> Result<EnumVariant, String> {
    let Some(ptr) = value.as_object_ptr() else {
        return Err("reflect.Enum.new values must be strings or reflect.enum.Value rows".into());
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
            if !vm
                .stdlib_heads()
                .is_class(StdlibClass::EnumValue, class.type_tag)
            {
                return Err(
                    "reflect.Enum.new values must be strings or reflect.enum.Value rows".into(),
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
            if !vm
                .stdlib_heads()
                .is_class(StdlibClass::Meta, meta_class.type_tag)
            {
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
                .as_string_map(&meta.load_field(3))
                .map_err(|_| "reflect.Meta.other must be map<string, string>".to_string())?
                .iter()
                .map(|(key, value)| {
                    vm.as_string(value)
                        .map(|value| (key.to_string(), value.to_string()))
                        .map_err(|_| "reflect.Meta.other must be map<string, string>".to_string())
                })
                .collect::<Result<IndexMap<_, _>, _>>()?;
            // A row read back from `values()` keeps its skip flag, so
            // rebuilding an enum from reflected rows does not unskip a variant.
            let skip = meta.load_field(4).as_bool().unwrap_or(false);
            Ok(EnumVariant {
                name,
                alias: optional_string(0)?,
                description: optional_string(1)?,
                docstring: optional_string(2)?,
                other,
                skip,
            })
        }
        _ => Err("reflect.Enum.new values must be strings or reflect.enum.Value rows".into()),
    }
}

fn invalid_enum_value(vm: &mut BexVm, value: Value) -> crate::errors::VmRustFnError {
    let message = format!(
        "reflect.Enum.get_value expects an enum value, got {}",
        vm.type_of(&value)
    );
    invalid_argument(vm, message)
}

/// Whether `value` fits the field type `expected` in
/// `reflect.Class.instantiate`: the membership test `call_any` uses, but
/// failing CLOSED where `call_any` fails open. A value with no BAML type, such
/// as an opaque native handle, fits only a field that accepts anything
/// (`unknown`). The instance outlives the call, so a handle let into a
/// `string` field would fail later, at a use no `catch` can see.
fn field_value_fits(vm: &BexVm, value: Value, expected: &bex_vm_types::RealizedTy) -> bool {
    let expected = expected.as_ty();
    crate::type_match::value_is_member(vm, value, expected).unwrap_or_else(|| {
        baml_type::normalize::TypeContext::is_subtype(vm, &bex_vm_types::Ty::Unknown, expected)
    })
}

/// The fix `reflect.Class.instantiate` names when a list or map misses its
/// field's type: the field takes a container of the same kind, so only the
/// element, key or value types differ. A container keeps the types it was
/// built with, so a literal written inside `fields` is typed by its own
/// elements and `[]` is `unknown[]`. Empty when the mismatch is anything else.
fn container_hint(
    vm: &BexVm,
    got: &bex_vm_types::RealizedTy,
    expected: &bex_vm_types::RealizedTy,
) -> String {
    let (list, rule) = match got {
        bex_vm_types::RealizedTy::List(_) => {
            (true, "a list keeps the element type it was built with")
        }
        bex_vm_types::RealizedTy::Map { .. } => (
            false,
            "a map keeps the key and value types it was built with",
        ),
        _ => return String::new(),
    };
    let mut wanted = Vec::new();
    same_kind_containers(vm, expected.as_ty(), list, &mut Vec::new(), &mut wanted);
    if wanted.is_empty() {
        return String::new();
    }
    let wanted = wanted
        .iter()
        .map(|ty| format!("`{ty}`"))
        .collect::<Vec<_>>()
        .join(" or ");
    format!(": {rule}; build it as {wanted} first")
}

/// The lists (`list`) or maps of `ty` and of its union members, through the
/// type aliases not in `within`.
fn same_kind_containers(
    vm: &BexVm,
    ty: &bex_vm_types::Ty,
    list: bool,
    within: &mut Vec<bex_vm_types::TypeHead>,
    out: &mut Vec<bex_vm_types::Ty>,
) {
    match ty {
        bex_vm_types::Ty::List(_) if list => out.push(ty.clone()),
        bex_vm_types::Ty::Map { .. } if !list => out.push(ty.clone()),
        bex_vm_types::Ty::Union(members) => {
            for member in members {
                same_kind_containers(vm, member, list, within, out);
            }
        }
        bex_vm_types::Ty::TypeAlias(head) if !within.contains(head) => {
            within.push(*head);
            if let Some(definition) = baml_type::normalize::TypeContext::alias_def(vm, head) {
                same_kind_containers(vm, &definition, list, within, out);
            }
        }
        _ => {}
    }
}

/// Throw `reflect.errors.InvalidArgument { message }`: how a reflection
/// native reports an argument or receiver it cannot accept. `reflect` does
/// not depend on `baml`, so its natives never throw a `baml.errors` value.
pub(super) fn invalid_argument(
    vm: &mut BexVm,
    message: impl Into<bex_str::BexStr>,
) -> crate::errors::VmRustFnError {
    let message = Value::object(vm.alloc_string(message));
    crate::errors::VmRustFnError::thrown_fresh(
        copy::errors::InvalidArgument { message }.to_value(vm),
    )
}

macro_rules! impl_as_type {
    ($kind:ident) => {
        #[expect(
            clippy::used_underscore_items,
            reason = "the generated view accessor is named for the `_ty` field it reads"
        )]
        fn as_type(
            vm: &BexVm,
            r#type: &super::view::$kind<'_>,
        ) -> Result<Value, crate::errors::VmRustFnError> {
            check_kind_invariant(vm, r#type._ty(), baml_type::type_kind::TypeKind::$kind)
        }
    };
}

/// A reflected class's fields in declaration order, each paired with its type
/// realized against the view's type arguments.
pub(super) fn reflected_class_fields(
    vm: &BexVm,
    view: Value,
) -> Result<Vec<(ClassField, TypeValue)>, crate::errors::VmRustFnError> {
    let (class, args) = reflected_class(vm, view)?;
    Ok(class
        .fields
        .into_iter()
        .map(|field| {
            let type_value = if let Some(type_value) = &field.runtime_type {
                type_value.clone()
            } else {
                let ty = field
                    .field_template
                    .substitute(&args, vm)
                    .unwrap_or_else(|err| {
                        unreachable!("emitted class field template must realize: {err}")
                    });
                TypeValue::new(ty)
            };
            (field, type_value)
        })
        .collect())
}

impl BamlClassArray for PackageReflectImpl {
    fn element_type(vm: &mut BexVm, r#type: &Value) -> Result<Value, crate::errors::VmRustFnError> {
        let ty = reflected_ty(vm, *r#type, baml_type::type_kind::TypeKind::Array)?;
        let bex_vm_types::RealizedTy::List(element) = ty else {
            unreachable!("an Array-classified type is RealizedTy::List")
        };
        Ok(Value::object(
            vm.alloc_type(bex_vm_types::types::TypeValue::new(*element)),
        ))
    }
}

impl BamlClassFunction for PackageReflectImpl {
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

// The `implements root.TypeView` blocks' `as_type` bodies — impl-block
// methods, so codegen emits one `…ReflectTypeView_for_<Kind>` trait per view
// class (plus the aggregating `BamlNamespaceReflect` supertrait): the native
// key spells the written `root.` as the package's fixed name
// (`baml_base::native_key_spelling`).

impl BamlClassReflectTypeView_for_Array for PackageReflectImpl {
    impl_as_type!(Array);
}

impl BamlClassReflectTypeView_for_Class for PackageReflectImpl {
    impl_as_type!(Class);
}

impl BamlClassReflectTypeView_for_Enum for PackageReflectImpl {
    impl_as_type!(Enum);
}

impl BamlClassReflectTypeView_for_Function for PackageReflectImpl {
    impl_as_type!(Function);
}

impl BamlClassReflectTypeView_for_Interface for PackageReflectImpl {
    impl_as_type!(Interface);
}

impl BamlClassReflectTypeView_for_Literal for PackageReflectImpl {
    impl_as_type!(Literal);
}

impl BamlClassReflectTypeView_for_Map for PackageReflectImpl {
    impl_as_type!(Map);
}

impl BamlClassReflectTypeView_for_Primitive for PackageReflectImpl {
    impl_as_type!(Primitive);
}

impl BamlClassReflectTypeView_for_Union for PackageReflectImpl {
    impl_as_type!(Union);
}

impl BamlNamespaceReflect for PackageReflectImpl {}

#[cfg(test)]
mod registration_tests {
    use std::sync::{Arc, atomic::AtomicBool};

    use bex_vm_types::{RealizedTy, RootHaverExt, TypeHead};

    use super::*;

    /// BAML observes that a witnessed class keeps working after
    /// `baml.sys.collect_garbage()`; only Rust can observe *how* — the rule
    /// is reachable from the class alone, with no root of its own. The blanket
    /// is real compiler output, not a mock rule.
    fn vm() -> BexVm {
        BexVm::from_program(
            baml_test_support::compile_source(
                r#"
interface Gate {}
interface Pick {}
implements<T extends Gate> Pick for T {}
class Fresh {}
"#,
            ),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap()
    }

    fn head(vm: &BexVm, name: &str) -> TypeHead {
        vm.declaration_head(&baml_type::TypeName::from_dotted_path(&format!(
            "user.{name}"
        )))
        .unwrap()
    }

    #[test]
    fn literal_value_supports_internal_float_literals() {
        let mut vm = vm();
        let literal = alloc_runtime_composite(
            &mut vm,
            baml_type::type_kind::TypeKind::Literal,
            RealizedTy::Literal(
                baml_type::Literal::Float("-1.25".into()),
                baml_type::Freshness::Regular,
            ),
        );
        let value = <PackageReflectImpl as BamlClassLiteral>::value(&mut vm, &literal)
            .expect("a literal view has a value");
        let Object::Float(value) = vm.get_object(value.as_object_ptr().unwrap()) else {
            panic!("expected a float value")
        };
        assert_eq!(value.to_bits(), (-1.25_f64).to_bits());
    }

    #[test]
    fn literal_value_rejects_a_view_with_the_wrong_kind() {
        let mut vm = vm();
        let forged = alloc_runtime_composite(
            &mut vm,
            baml_type::type_kind::TypeKind::Literal,
            RealizedTy::Int,
        );
        let error = <PackageReflectImpl as BamlClassLiteral>::value(&mut vm, &forged)
            .expect_err("the literal view must retain its kind invariant");
        assert!(matches!(error, crate::errors::VmRustFnError::Panic(_)));
    }

    /// A fresh anonymous class, as `reflect.Class.new` allocates one.
    fn fresh_class(vm: &mut BexVm) -> TypeHead {
        let Object::Class(class) = vm.get_object(head(vm, "Fresh").ptr()) else {
            unreachable!()
        };
        let mut class = (**class).clone();
        class.type_tag = baml_type::typetag::TypeTag::fresh_dynamic();
        // A minted class belongs to no package, so its name is its path alone.
        class.owner = bex_vm_types::types::Owner::anonymous();
        let tag = class.type_tag;
        TypeHead::new(vm.tlab.alloc(Object::Class(Box::new(class))), tag)
    }

    fn witness(vm: &BexVm, name: &str) -> ValidatedClassWitness {
        ValidatedClassWitness {
            interface_ptr: head(vm, name).ptr(),
            interface_args: Box::new([]),
            interface_assoc: Box::new([]),
            field_links: Vec::new(),
        }
    }

    fn class_ty(head: TypeHead) -> RealizedTy {
        RealizedTy::Class(head, Box::new([]))
    }

    #[test]
    #[expect(
        unsafe_code,
        reason = "standalone stop-the-world dynamic registration test"
    )]
    fn registered_rules_are_owned_by_the_class_without_extra_gc_roots() {
        let mut vm = vm();
        let class = fresh_class(&mut vm);
        let rules =
            prepare_class_witnesses(&vm, &class_ty(class), vec![witness(&vm, "Pick")]).unwrap();
        register_class_witnesses(&mut vm, class.ptr(), rules);
        let rule = vm.dynamic_dispatch.rules_for_class(class.ptr())[0];

        // Root only the class, as an instance would. A major collection moves
        // every survivor, so the rule appears in the forwarding map iff the
        // class kept it alive.
        let heap = Arc::clone(&vm.heap);
        let mut index = crate::package_load::DynDispatchRoot::new(Arc::clone(&vm.dynamic_dispatch));
        let (_, _, forwarding) = unsafe { heap.collect_garbage(&[class.ptr()]) };
        assert!(forwarding.contains_key(&rule), "the class owns its rule");
        vm.forward_roots(&mut bex_vm_types::ForwardRoots::new(&forwarding, &*heap));
        index.forward_roots(&mut bex_vm_types::ForwardRoots::new(&forwarding, &*heap));
        assert_eq!(index.tables.rule_count(), 1);
        let moved = TypeHead::new(forwarding[&class.ptr()], class.tag());
        assert!(
            crate::package_baml::resolve::ImplResolver::new(&vm).type_implements(
                &class_ty(moved),
                head(&vm, "Pick"),
                &[],
                &[]
            )
        );

        // Nothing reaches the class any more: the rule goes with it.
        drop(vm);
        let (_, _, forwarding) = unsafe { heap.collect_garbage(&[]) };
        index.forward_roots(&mut bex_vm_types::ForwardRoots::new(&forwarding, &*heap));
        assert_eq!(index.tables.rule_count(), 0);
    }
}
