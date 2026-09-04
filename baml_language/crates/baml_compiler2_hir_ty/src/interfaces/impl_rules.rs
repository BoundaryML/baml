use std::collections::HashMap;

use baml_base::{Name, Span, TyAttr};
use baml_compiler2_hir::{contributions::Definition, package::PackageId};
use baml_type::{
    ParamTy, QualifiedTypeName, Ty,
    normalize::TypeContext,
    pattern_overlap::TypeVarBoundsMap,
    unify::{TypeBindings, substitute_ty},
};
use baml_type_runtime::contains_typevar;

use super::{
    InterfaceImplOrigin, LowerScope, interface_declared_param_bounds,
    lower_interface_associated_bindings, lower_ref_in, lower_ref_in_at, match_ty_patterns,
    normalized_arg_implements_bound, resolve_ref_to_interface,
};
use crate::{diagnostics::TirTypeError, lower::qualify_def};

/// Fully-resolved data for one `implements` block, keyed by its stable
/// [`ImplLoc`](baml_compiler2_hir::loc::ImplLoc).
///
/// Every impl — in-body or out-of-body — normalizes to the same *free* shape
/// here: an in-body `implements I {…}` inside `class C<T>` resolves exactly as
/// `implement<T> I for C<T>`. The in-body/out-of-body distinction survives only
/// as `origin`, which is diagnostic metadata and MUST NOT drive resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplData<'db> {
    /// The implemented interface's resolved head identity.
    pub interface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    /// The interface's generic input args (`<int>` in `Container<int>`).
    pub interface_args: Box<[Ty]>,
    /// The resolved implementor pattern (may carry `Ty::TypeVar`s).
    pub for_ty_pattern: Ty,
    /// Generic params with their interface bounds (BEP-044).
    pub generic_params: Vec<(ParamTy, Vec<baml_type::Interface>)>,
    /// Diagnostics produced while resolving this impl, each paired with the
    /// span-free [`ImplDiagnosticLocation`] it originated from. Never dropped.
    pub diagnostics: Vec<(TirTypeError, ImplDiagnosticLocation)>,
    /// The impl body's own method overrides, as stable function ids.
    pub methods: Vec<baml_compiler2_hir::loc::FunctionLoc<'db>>,
    /// Interface associated-type bindings supplied by this impl body
    /// (`type Item = int`), resolved.
    pub associated_types: Vec<(Name, Ty)>,
    /// Interface-field → class-field links declared in the block.
    pub field_links: Vec<(Name, Name)>,
    /// In-body vs out-of-body provenance. Diagnostic metadata ONLY.
    pub origin: InterfaceImplOrigin,
}

/// # Safety
///
/// `ImplData<'db>` holds Salsa interned locs with a db-tied lifetime, so it
/// can't auto-derive `salsa::Update`; `maybe_update` uses `PartialEq` for
/// proper early-cutoff.
#[allow(unsafe_code)]
unsafe impl salsa::Update for ImplData<'_> {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        // SAFETY: `old_pointer` is valid, aligned, and Salsa-owned.
        #[allow(unsafe_code)]
        let old = unsafe { &*old_pointer };
        if old == &new_value {
            false
        } else {
            #[allow(unsafe_code)]
            unsafe {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            true
        }
    }
}

/// Where in an `implements` block a diagnostic originated. Span-free
/// (Salsa-stable); check.rs maps it to a source range via [`impl_data_source_map`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImplDiagnosticLocation {
    /// The interface-target type expr (`implement <here> for T`). Also covers
    /// associated-type bindings, which are part of the interface reference.
    InterfaceTarget,
    /// The for-target type expr (`implement I for <here>`). Absent for in-body
    /// impls; falls back to the block.
    ForTarget,
    /// A generic bound (`<T extends <here>>`). Bounds carry no source span, so
    /// this resolves to the whole-block span.
    Bound,
    /// An override method in the block, by name — resolved via
    /// [`ImplDataSourceMap::method_spans`] to the span of *every* override with
    /// that name. Falls back to the whole block.
    Method(Name),
    /// The interface-field side of a `field as class_field` link, by
    /// interface-field name.
    InterfaceFieldLink(Name),
    /// The class-field side of a `field as class_field` link, by class-field name.
    ClassFieldLink(Name),
    /// A `type Name = …` associated-type binding in the block, by binding name.
    AssociatedBinding(Name),
}

/// Spans for an `implements` block, split out of [`ImplData`] for Salsa
/// early-cutoff (semantic resolution must not re-run on whitespace edits).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplDataSourceMap {
    /// The span coherence attributes a conflict to: the interface-target span
    /// for in-body impls, the whole-block span for out-of-body impls.
    pub impl_span: Span,
    /// Span of the interface-target type expr (`implement <here> for T`).
    pub interface_target_span: Span,
    /// Span of the for-target type expr; `None` for in-body impls.
    pub for_target_span: Option<Span>,
    /// Override-method name → the span of *every* override with that name.
    pub method_spans: HashMap<Name, Vec<Span>>,
    /// Interface-field name → the span of the interface-field side of *every*
    /// `field as class_field` link with that name.
    pub interface_field_link_spans: HashMap<Name, Vec<Span>>,
    /// Class-field name → the span of the class-field side of *every* link.
    pub class_field_link_spans: HashMap<Name, Vec<Span>>,
    /// Associated-binding name → the name span of *every* `type Name = …` binding.
    pub associated_binding_spans: HashMap<Name, Vec<Span>>,
}

/// The qualified name of a resolved interface loc (head identity for building a
/// `Ty::Interface`). Always `Some` for a genuine loc; the `Option` shape is kept
/// for its many callers.
pub fn interface_loc_qtn<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
) -> Option<QualifiedTypeName> {
    let data = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
    Some(qualify_def(
        db,
        Definition::Interface(iface_loc),
        &data.name,
    ))
}

/// Why [`impl_data`] could not produce an [`ImplData`].
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub enum ImplDataError {
    /// The implements target does not name an interface. The diagnostics
    /// lowered before the failure ride along so check.rs still surfaces them.
    InterfaceUnresolved {
        diagnostics: Vec<(TirTypeError, ImplDiagnosticLocation)>,
    },
    /// The impl block, its class, or the interface declaration was missing from
    /// the item tree (internal invariant).
    Malformed,
    /// The impl header references its own resolution: **concrete projections in
    /// an impl header are illegal** (the salsa cycle converges here).
    CyclicHeader,
}

/// Lower one generic param's bounds to its interface constraints, pushing both
/// the lowering diagnostics and the non-interface-bound (E0142) diagnostics
/// into `diags`. `store` is the arena the bound ids index (the declaring item's
/// `type_refs`); `generic_param_names` are the in-scope type-var names so a
/// bound naming a sibling param doesn't read as an unresolved type.
pub(crate) fn lower_generic_param_interface_bounds<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    bounds: &[baml_compiler2_hir::type_ref::TypeRefId],
    pkg_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    ns: &[Name],
    generic_param_names: &[ParamTy],
    diags: &mut Vec<TirTypeError>,
) -> Vec<baml_type::Interface> {
    let mut ifaces = Vec::new();
    for &bound in bounds {
        let scope = LowerScope {
            db,
            package_items: pkg_items,
            ns_context: ns,
            generic_params: generic_param_names,
            bounds: &TypeVarBoundsMap::default(),
            self_ty: None,
        };
        let ty = lower_ref_in_at(
            &scope,
            store,
            bound,
            crate::lower::TypePosition::ConstraintHead,
            diags,
        );
        match ty {
            // BEP-062: `reflect.AnyFunction` is legal only as a value type; as a
            // bound it is rejected and contributes no constraint.
            Ty::Interface(qtn, ..) if qtn.is_reflect_root_type("AnyFunction") => {
                diags.push(TirTypeError::BuiltinInterfaceNotABound { interface: qtn });
            }
            Ty::Interface(qtn, generics, assoc, _) => {
                ifaces.push(baml_type::Interface {
                    name: qtn,
                    generics,
                    associated_types: assoc,
                });
            }
            // Already diagnosed by lowering the bound expression itself.
            Ty::Error { .. } | Ty::Unknown { .. } => {}
            // BEP-044 requires bounds to be interfaces (E0142).
            other => diags.push(TirTypeError::GenericBoundNotInterface { bound: other }),
        }
    }
    ifaces
}

/// The fallback for a self-referential [`impl_data`] computation.
fn impl_data_cycle_result<'db>(
    _db: &'db dyn baml_compiler2_ppir::Db,
    _id: salsa::Id,
    _impl_loc: baml_compiler2_hir::loc::ImplLoc<'db>,
) -> Result<ImplData<'db>, ImplDataError> {
    Err(ImplDataError::CyclicHeader)
}

/// Resolve one `implements` block to its [`ImplData`]. All of this impl's
/// diagnostics are owned here — in [`ImplData::diagnostics`] on success, or the
/// `InterfaceUnresolved` payload on failure — and surfaced at the impl's span
/// by check.rs.
#[salsa::tracked(returns(ref), cycle_result = impl_data_cycle_result)]
pub fn impl_data<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    impl_loc: baml_compiler2_hir::loc::ImplLoc<'db>,
) -> Result<ImplData<'db>, ImplDataError> {
    use baml_compiler2_ppir::item_data::{ImplSubjectData, function_data, impl_block_data};

    let file = impl_loc.file(db);
    let block = impl_block_data(db, impl_loc);

    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let ns = &pkg_info.namespace_path;

    // Normalize in-body → free: an in-body impl's generics are the class's and
    // its for-type is the class applied to its own params as type vars.
    let (
        generic_param_names,
        for_ty_pattern,
        generic_params,
        for_target_diags,
        bound_diags,
        origin,
    ) = match &block.subject {
        ImplSubjectData::InClass { class, out_of_body } => {
            let class_data = baml_compiler2_ppir::item_data::class_data(db, *class);
            let generic_param_names = crate::lower::class_generic_frame(db, *class);
            let class_qtn = qualify_def(db, Definition::Class(*class), &class_data.name);
            let for_ty = Ty::Class(
                class_qtn.clone(),
                generic_param_names
                    .iter()
                    .map(|p| Ty::TypeVar(p.clone(), TyAttr::default()))
                    .collect(),
                TyAttr::default(),
            );
            // An in-body impl's generics ARE the class's; the class declaration
            // owns its bounds' diagnostics, so they are lowered into a
            // discarded sink here.
            let mut class_bound_diags = Vec::new();
            let generic_params: Vec<(ParamTy, Vec<baml_type::Interface>)> = generic_param_names
                .iter()
                .zip(class_data.generic_params.iter())
                .map(|(name, declared)| {
                    let ifaces = lower_generic_param_interface_bounds(
                        db,
                        &class_data.type_refs,
                        &declared.bounds,
                        pkg_items,
                        ns,
                        &generic_param_names,
                        &mut class_bound_diags,
                    );
                    (name.clone(), ifaces)
                })
                .collect();
            (
                generic_param_names,
                for_ty,
                generic_params,
                Vec::new(),
                Vec::new(),
                // A simple `implement I for C` is merged onto `C` (`InClass`
                // subject) but written out-of-body — its origin stays `OutOfBody`.
                if *out_of_body {
                    InterfaceImplOrigin::OutOfBody
                } else {
                    InterfaceImplOrigin::InBodyClass { class_qtn }
                },
            )
        }
        ImplSubjectData::Free {
            for_target,
            generics,
        } => {
            let names: Vec<Name> = generics.iter().map(|g| g.name.clone()).collect();
            let generic_param_names = crate::lower::impl_frame(db, impl_loc);
            let mut for_target_diags = Vec::new();
            let for_ty = lower_ref_in(
                &LowerScope {
                    db,
                    package_items: pkg_items,
                    ns_context: ns,
                    generic_params: &generic_param_names,
                    bounds: &TypeVarBoundsMap::default(),
                    self_ty: None,
                },
                &block.type_refs,
                *for_target,
                &mut for_target_diags,
            );
            let mut bound_diags = Vec::new();
            // `implements<T, T> …` — a duplicate impl generic is a declaration error.
            for (idx, name) in names.iter().enumerate() {
                if names[..idx].contains(name) {
                    bound_diags.push(TirTypeError::DuplicateGenericParam { name: name.clone() });
                }
            }
            let generic_params = generics
                .iter()
                .zip(generic_param_names.iter())
                .map(|(g, param)| {
                    let ifaces = lower_generic_param_interface_bounds(
                        db,
                        &block.type_refs,
                        &g.bounds,
                        pkg_items,
                        ns,
                        &generic_param_names,
                        &mut bound_diags,
                    );
                    (param.clone(), ifaces)
                })
                .collect();
            (
                generic_param_names,
                for_ty,
                generic_params,
                for_target_diags,
                bound_diags,
                InterfaceImplOrigin::OutOfBody,
            )
        }
    };

    let mut interface_target_diags = Vec::new();
    // The target is a constraint head: it pins only its written inline bindings.
    let lowered_interface = lower_ref_in_at(
        &LowerScope {
            db,
            package_items: pkg_items,
            ns_context: ns,
            generic_params: &generic_param_names,
            bounds: &TypeVarBoundsMap::default(),
            self_ty: None,
        },
        &block.type_refs,
        block.interface_target,
        crate::lower::TypePosition::ConstraintHead,
        &mut interface_target_diags,
    );
    let interface_args = if let Ty::Interface(_, args, _, _) = &lowered_interface {
        args.clone()
    } else {
        Box::new([])
    };

    // Resolve the interface head to its loc *after* lowering, so a bad interface
    // target still surfaces its diagnostics.
    let Some(iface_loc) =
        resolve_ref_to_interface(db, &block.type_refs, block.interface_target, pkg_items, ns)
    else {
        let head_diags: Vec<_> = match &lowered_interface {
            Ty::Class(qtn, ..) | Ty::Enum(qtn, ..) => vec![(
                TirTypeError::ImplTargetNotInterface {
                    name: qtn.name().clone(),
                },
                ImplDiagnosticLocation::InterfaceTarget,
            )],
            _ => interface_target_diags
                .into_iter()
                .map(|e| (e, ImplDiagnosticLocation::InterfaceTarget))
                .collect(),
        };
        let diagnostics = head_diags
            .into_iter()
            .chain(
                for_target_diags
                    .into_iter()
                    .map(|e| (e, ImplDiagnosticLocation::ForTarget)),
            )
            .chain(
                bound_diags
                    .into_iter()
                    .map(|e| (e, ImplDiagnosticLocation::Bound)),
            )
            .collect();
        return Err(ImplDataError::InterfaceUnresolved { diagnostics });
    };
    let iface_data = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);

    let mut assoc_diags = Vec::new();
    // The impl's own generic bounds, so a `T.member` projection in a binding value
    // (`type Item = T.Elem`) resolves through the impl generic's declared bound.
    let impl_bounds: TypeVarBoundsMap = generic_params.iter().cloned().collect();
    let associated_types = lower_interface_associated_bindings(
        db,
        iface_loc,
        iface_data,
        &interface_args,
        &for_ty_pattern,
        &block.type_refs,
        &block.associated_type_bindings,
        pkg_items,
        ns,
        &generic_param_names,
        &impl_bounds,
        &mut assoc_diags,
    );

    // Conformance, computed alongside lowering: E0126/E0113/E0115 plus field-link
    // and associated-binding hygiene. (Signature conformance is phase 5.)
    let mut conformance_diags: Vec<(TirTypeError, ImplDiagnosticLocation)> = Vec::new();
    if let Some(iface_qtn) = interface_loc_qtn(db, iface_loc) {
        // BEP-062 (E0153): `reflect.AnyFunction`'s conformance is compiler-derived;
        // a written impl is rejected outright.
        if iface_qtn.is_reflect_root_type("AnyFunction") {
            conformance_diags.push((
                TirTypeError::BuiltinInterfaceNotImplementable {
                    interface: iface_qtn.clone(),
                },
                ImplDiagnosticLocation::InterfaceTarget,
            ));
        }
        if matches!(origin, InterfaceImplOrigin::OutOfBody) && !iface_data.fields.is_empty() {
            conformance_diags.push((
                TirTypeError::OutOfBodyImplementsFieldInterface {
                    interface: iface_qtn.clone(),
                },
                ImplDiagnosticLocation::InterfaceTarget,
            ));
        }
        let override_names: Vec<&Name> = block
            .methods
            .iter()
            .map(|loc| &function_data(db, *loc).name)
            .collect();
        let default_names: Vec<&Name> = iface_data
            .default_methods
            .iter()
            .map(|loc| &function_data(db, *loc).name)
            .collect();
        // E0113: a required method the impl neither provides nor the interface defaults.
        for required in &iface_data.required_methods {
            let provided = override_names.iter().any(|n| **n == required.name)
                || default_names.iter().any(|n| **n == required.name);
            if !provided {
                conformance_diags.push((
                    TirTypeError::MissingInterfaceMethod {
                        interface: iface_qtn.clone(),
                        method: required.name.clone(),
                    },
                    ImplDiagnosticLocation::InterfaceTarget,
                ));
            }
        }
        // E0115: an override matching no required or default method. Reported
        // once per name.
        for (idx, &name) in override_names.iter().enumerate() {
            if override_names[..idx].contains(&name) {
                continue;
            }
            let is_member = iface_data.required_methods.iter().any(|m| m.name == *name)
                || default_names.iter().any(|n| **n == *name);
            if !is_member {
                conformance_diags.push((
                    TirTypeError::UnknownInterfaceMember {
                        interface: iface_qtn.clone(),
                        member: name.clone(),
                    },
                    ImplDiagnosticLocation::Method(name.clone()),
                ));
            }
        }
        // Field-side conformance, in-body class impls only.
        if let InterfaceImplOrigin::InBodyClass { class_qtn } = &origin
            && let ImplSubjectData::InClass { class, .. } = &block.subject
            && (!block.field_links.is_empty() || !iface_data.fields.is_empty())
        {
            let class_fields = crate::lower::resolve_class_fields(db, *class);
            let is_iface_field =
                |name: &Name| iface_data.fields.iter().any(|fld| fld.name == *name);
            let is_class_field = |name: &Name| class_fields.iter().any(|(n, _, _)| n == name);

            // Interface-field side of each link, deduped by interface-field name.
            // E0130: linked more than once. E0128: unknown interface field.
            for (idx, link) in block.field_links.iter().enumerate() {
                let iface_field = &link.interface_field;
                if block.field_links[..idx]
                    .iter()
                    .any(|l| l.interface_field == *iface_field)
                {
                    continue;
                }
                if block.field_links[idx + 1..]
                    .iter()
                    .any(|l| l.interface_field == *iface_field)
                {
                    conformance_diags.push((
                        TirTypeError::DuplicateInterfaceFieldLink {
                            interface: iface_qtn.clone(),
                            field: iface_field.clone(),
                        },
                        ImplDiagnosticLocation::InterfaceFieldLink(iface_field.clone()),
                    ));
                }
                if !is_iface_field(iface_field) {
                    conformance_diags.push((
                        TirTypeError::UnknownInterfaceFieldLink {
                            interface: iface_qtn.clone(),
                            field: iface_field.clone(),
                        },
                        ImplDiagnosticLocation::InterfaceFieldLink(iface_field.clone()),
                    ));
                }
            }
            // Class-field side (E0129), only for links whose interface field is
            // valid; deduped by class-field name among eligible links.
            for (idx, link) in block.field_links.iter().enumerate() {
                if !is_iface_field(&link.interface_field) {
                    continue;
                }
                let class_field = &link.class_field;
                if block.field_links[..idx]
                    .iter()
                    .any(|l| l.class_field == *class_field && is_iface_field(&l.interface_field))
                {
                    continue;
                }
                if !is_class_field(class_field) {
                    conformance_diags.push((
                        TirTypeError::UnknownClassFieldInInterfaceLink {
                            class: class_qtn.name().clone(),
                            interface: iface_qtn.clone(),
                            field: class_field.clone(),
                        },
                        ImplDiagnosticLocation::ClassFieldLink(class_field.clone()),
                    ));
                }
            }
            // E0124: every interface field must be covered by a same-named class
            // field or an explicit link.
            for iface_field in &iface_data.fields {
                let linked = block
                    .field_links
                    .iter()
                    .any(|fl| fl.interface_field == iface_field.name);
                if !linked && !is_class_field(&iface_field.name) {
                    conformance_diags.push((
                        TirTypeError::MissingInterfaceField {
                            interface: iface_qtn.clone(),
                            field: iface_field.name.clone(),
                        },
                        ImplDiagnosticLocation::InterfaceTarget,
                    ));
                }
            }
        }

        // Associated-type binding hygiene, name-based.
        let is_assoc = |name: &Name| iface_data.associated_types.iter().any(|a| a.name == *name);
        for (idx, binding) in block.associated_type_bindings.iter().enumerate() {
            if block.associated_type_bindings[..idx]
                .iter()
                .any(|b| b.name == binding.name)
            {
                continue;
            }
            // Duplicate: the same associated type is bound by a later sibling too.
            if block.associated_type_bindings[idx + 1..]
                .iter()
                .any(|b| b.name == binding.name)
            {
                conformance_diags.push((
                    TirTypeError::DuplicateAssociatedTypeBinding {
                        interface: iface_qtn.clone(),
                        name: binding.name.clone(),
                    },
                    ImplDiagnosticLocation::AssociatedBinding(binding.name.clone()),
                ));
            }
            // Unknown: names no associated type of the interface.
            if !is_assoc(&binding.name) {
                conformance_diags.push((
                    TirTypeError::UnknownAssociatedTypeBinding {
                        interface: iface_qtn.clone(),
                        name: binding.name.clone(),
                    },
                    ImplDiagnosticLocation::AssociatedBinding(binding.name.clone()),
                ));
            }
        }
        // Missing: an associated type with no default and no binding.
        for assoc in &iface_data.associated_types {
            if assoc.default.is_none()
                && !block
                    .associated_type_bindings
                    .iter()
                    .any(|b| b.name == assoc.name)
            {
                conformance_diags.push((
                    TirTypeError::MissingImplAssociatedTypeBinding {
                        interface: iface_qtn.clone(),
                        name: assoc.name.clone(),
                    },
                    ImplDiagnosticLocation::InterfaceTarget,
                ));
            }
        }
        // Bindings written on the `implements` target are rejected — the
        // block's `type Name = …` is the only binding site.
        if let baml_compiler2_hir::type_ref::TypeRefKind::Path {
            associated_type_bindings,
            ..
        } = &block.type_refs[block.interface_target].kind
            && !associated_type_bindings.is_empty()
        {
            conformance_diags.push((
                TirTypeError::AssociatedTypeBindingsOnImplementsTarget {
                    interface: iface_qtn,
                },
                ImplDiagnosticLocation::InterfaceTarget,
            ));
        }
    }

    // Tag each diagnostic with its origin. Deterministic order for stable output.
    let diagnostics: Vec<(TirTypeError, ImplDiagnosticLocation)> = interface_target_diags
        .into_iter()
        .map(|e| (e, ImplDiagnosticLocation::InterfaceTarget))
        .chain(
            assoc_diags
                .into_iter()
                .map(|e| (e, ImplDiagnosticLocation::InterfaceTarget)),
        )
        .chain(conformance_diags)
        .chain(
            for_target_diags
                .into_iter()
                .map(|e| (e, ImplDiagnosticLocation::ForTarget)),
        )
        .chain(
            bound_diags
                .into_iter()
                .map(|e| (e, ImplDiagnosticLocation::Bound)),
        )
        .collect();

    let methods = block.methods.clone();
    let field_links = block
        .field_links
        .iter()
        .map(|fl| (fl.interface_field.clone(), fl.class_field.clone()))
        .collect();

    Ok(ImplData {
        interface: iface_loc,
        interface_args,
        for_ty_pattern,
        generic_params,
        diagnostics,
        methods,
        associated_types,
        field_links,
        origin,
    })
}

/// Span sidecar for [`impl_data`] (early-cutoff split).
#[salsa::tracked(returns(ref))]
pub fn impl_data_source_map<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    impl_loc: baml_compiler2_hir::loc::ImplLoc<'db>,
) -> ImplDataSourceMap {
    use baml_compiler2_ppir::item_data::{
        ImplSubjectData, function_data, function_source_map, impl_block_data, impl_block_source_map,
    };

    let file_id = impl_loc.file(db).file_id(db);
    let block = impl_block_data(db, impl_loc);
    let spans = impl_block_source_map(db, impl_loc);
    let interface_target_range = spans.type_refs.span(block.interface_target);
    // In-body impls attribute to the interface-target span; out-of-body to the
    // whole block span.
    let (impl_range, for_target_span) = match &block.subject {
        ImplSubjectData::InClass { .. } => (interface_target_range, None),
        ImplSubjectData::Free { for_target, .. } => (
            spans.span,
            Some(Span::new(file_id, spans.type_refs.span(*for_target))),
        ),
    };
    // Group all override spans by name so a same-named duplicate marks every occurrence.
    let mut method_spans: HashMap<Name, Vec<Span>> = HashMap::new();
    for &func_loc in &block.methods {
        method_spans
            .entry(function_data(db, func_loc).name.clone())
            .or_default()
            .push(Span::new(file_id, function_source_map(db, func_loc).span));
    }
    // Group each field link's endpoint spans by name.
    let mut interface_field_link_spans: HashMap<Name, Vec<Span>> = HashMap::new();
    let mut class_field_link_spans: HashMap<Name, Vec<Span>> = HashMap::new();
    for (link, link_spans) in block.field_links.iter().zip(&spans.field_links) {
        interface_field_link_spans
            .entry(link.interface_field.clone())
            .or_default()
            .push(Span::new(file_id, link_spans.interface_field_span));
        class_field_link_spans
            .entry(link.class_field.clone())
            .or_default()
            .push(Span::new(file_id, link_spans.class_field_span));
    }
    // Group each `type Name = …` binding's name span by name.
    let mut associated_binding_spans: HashMap<Name, Vec<Span>> = HashMap::new();
    for (binding, binding_spans) in block
        .associated_type_bindings
        .iter()
        .zip(&spans.associated_type_bindings)
    {
        associated_binding_spans
            .entry(binding.name.clone())
            .or_default()
            .push(Span::new(file_id, binding_spans.name_span));
    }
    ImplDataSourceMap {
        impl_span: Span::new(file_id, impl_range),
        interface_target_span: Span::new(file_id, interface_target_range),
        for_target_span,
        method_spans,
        interface_field_link_spans,
        class_field_link_spans,
        associated_binding_spans,
    }
}

// ── The normalized interface-method signature (TIR's `InterfaceMethodSpec`) ──

/// One type slot in a method signature — a written type, or one of the two
/// slots the declaration leaves implicit.
#[derive(Debug, Clone, Copy)]
enum SigTypeRef {
    Id(baml_compiler2_hir::type_ref::TypeRefId),
    /// The `self` receiver, desugared to `self: Self` — lowers as the `Self` path.
    SelfReceiver,
    /// No written type — lowers to `unknown`.
    Missing,
}

struct InterfaceMethodParam {
    name: Name,
    ty: SigTypeRef,
}

/// A normalized interface-method signature: default and required methods both
/// reduce to this so one builder produces the `Ty::Function`.
pub(crate) struct InterfaceMethodSpec<'db> {
    /// The arena every signature slot indexes: the elaborated store for a
    /// default method, the interface's own store for a required one.
    sig_refs: &'db baml_compiler2_hir::type_ref::TypeRefStore,
    /// The arena the `generics` bound ids index (the declaration store).
    bound_refs: &'db baml_compiler2_hir::type_ref::TypeRefStore,
    args: Vec<InterfaceMethodParam>,
    kwargs: Vec<InterfaceMethodParam>,
    return_type: SigTypeRef,
    throws: SigTypeRef,
    /// Method generic params with their interface-bound *conjunction*.
    generics: Vec<baml_compiler2_ppir::item_data::GenericParamData>,
}

impl<'db> InterfaceMethodSpec<'db> {
    pub(crate) fn from_default(
        db: &'db dyn baml_compiler2_ppir::Db,
        func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
    ) -> Self {
        let sig = baml_compiler2_ppir::item_data::elaborated_function_data(db, func_loc);
        let func_data = baml_compiler2_ppir::item_data::function_data(db, func_loc);
        let (args, kwargs) = split_params(sig.params.iter().map(|p| {
            // The implicit `self` receiver: name "self" with no declared type
            // (elaboration synthesizes a `Missing` node for it).
            let is_self = p.name.as_str() == "self"
                && matches!(
                    sig.type_refs[p.type_ref].kind,
                    baml_compiler2_hir::type_ref::TypeRefKind::Missing
                );
            (
                is_self,
                p.has_default,
                p.name.clone(),
                SigTypeRef::Id(p.type_ref),
            )
        }));
        // `user_generic_params` is the elaborated view of the same declaration
        // order `generic_params` lowers, so the two stay parallel.
        let generics = func_data.generic_params.clone();
        Self {
            sig_refs: &sig.type_refs,
            bound_refs: &func_data.type_refs,
            args,
            kwargs,
            return_type: sig.return_type.map_or(SigTypeRef::Missing, SigTypeRef::Id),
            throws: sig.throws.map_or(SigTypeRef::Missing, SigTypeRef::Id),
            generics,
        }
    }

    pub(crate) fn from_required(
        iface_data: &'db baml_compiler2_ppir::item_data::InterfaceData<'db>,
        sig: &baml_compiler2_ppir::item_data::InterfaceMethodSigData,
    ) -> Self {
        let (args, kwargs) = split_params(sig.params.iter().map(|p| {
            let is_self = p.name.as_str() == "self" && p.type_ref.is_none();
            let ty = p.type_ref.map_or(SigTypeRef::Missing, SigTypeRef::Id);
            (is_self, p.has_default, p.name.clone(), ty)
        }));
        let generics = sig.generic_params.clone();
        Self {
            sig_refs: &iface_data.type_refs,
            bound_refs: &iface_data.type_refs,
            args,
            kwargs,
            return_type: sig.return_type.map_or(SigTypeRef::Missing, SigTypeRef::Id),
            throws: sig.throws.map_or(SigTypeRef::Missing, SigTypeRef::Id),
            generics,
        }
    }

    pub(crate) fn generic_param_names(&self) -> Vec<Name> {
        self.generics.iter().map(|g| g.name.clone()).collect()
    }

    pub(crate) fn generic_bounds(&self) -> &[baml_compiler2_ppir::item_data::GenericParamData] {
        &self.generics
    }

    pub(crate) fn bound_store(&self) -> &'db baml_compiler2_hir::type_ref::TypeRefStore {
        self.bound_refs
    }

    /// Lower this normalized signature to a `Ty::Function` through `scope`
    /// (which resolves `Self`, `Self.Assoc`, and the in-scope generics). Still
    /// a template over its free type variables.
    pub(crate) fn to_function_ty(
        &self,
        scope: &LowerScope<'_, '_>,
        diags: &mut Vec<TirTypeError>,
    ) -> Ty {
        // The two implicit slots, synthesized into a scratch arena so they
        // lower through the same road as written types.
        let mut scratch = baml_compiler2_hir::type_ref::TypeRefBuilder::new();
        let self_id = scratch.alloc_synthetic(baml_compiler2_hir::type_ref::TypeRefKind::Path {
            segments: vec![Name::new("Self")],
            generic_args: Box::new([]),
            associated_type_bindings: Box::new([]),
        });
        let unknown_id =
            scratch.alloc_synthetic(baml_compiler2_hir::type_ref::TypeRefKind::Missing);
        let (scratch_store, _) = scratch.finish();
        let lower = |slot: SigTypeRef, diags: &mut Vec<TirTypeError>| match slot {
            SigTypeRef::Id(id) => lower_ref_in(scope, self.sig_refs, id, diags),
            SigTypeRef::SelfReceiver => lower_ref_in(scope, &scratch_store, self_id, diags),
            SigTypeRef::Missing => lower_ref_in(scope, &scratch_store, unknown_id, diags),
        };

        let params = self
            .args
            .iter()
            .map(|p| (p, baml_type::FunctionParamMode::Required))
            .chain(
                self.kwargs
                    .iter()
                    .map(|p| (p, baml_type::FunctionParamMode::Optional)),
            )
            .map(|(p, mode)| baml_type::FunctionParamTy {
                name: Some(p.name.clone()),
                ty: lower(p.ty, diags),
                mode,
            })
            .collect();
        Ty::Function {
            params,
            ret: Box::new(lower(self.return_type, diags)),
            throws: Box::new(lower(self.throws, diags)),
            attr: TyAttr::default(),
        }
    }
}

/// Split `(is_self, has_default, name, ty)` tuples into positional args and
/// keyword/optional kwargs. The `self` receiver desugars to `self: Self`.
fn split_params(
    params: impl Iterator<Item = (bool, bool, Name, SigTypeRef)>,
) -> (Vec<InterfaceMethodParam>, Vec<InterfaceMethodParam>) {
    let mut args = Vec::new();
    let mut kwargs = Vec::new();
    for (is_self, has_default, name, ty) in params {
        let ty = if is_self {
            SigTypeRef::SelfReceiver
        } else {
            ty
        };
        let param = InterfaceMethodParam { name, ty };
        if has_default {
            kwargs.push(param);
        } else {
            args.push(param);
        }
    }
    (args, kwargs)
}

/// A method spec's generic parameters paired with their lowered interface-bound
/// *conjunction* (declaration order), for comparing an override's bounds
/// against the interface method's. A non-interface / unresolved conjunct is
/// dropped (already diagnosed at its own declaration).
fn method_generic_bound_interfaces<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    ns: &[Name],
    scope_generics: &[ParamTy],
    spec: &InterfaceMethodSpec<'_>,
) -> Vec<(Name, Vec<baml_type::Interface>)> {
    let empty = TypeVarBoundsMap::default();
    spec.generic_bounds()
        .iter()
        .map(|declared| {
            let conjunction = declared
                .bounds
                .iter()
                .filter_map(|&id| {
                    let mut d = Vec::new();
                    let scope = LowerScope {
                        db,
                        package_items: pkg_items,
                        ns_context: ns,
                        generic_params: scope_generics,
                        bounds: &empty,
                        self_ty: None,
                    };
                    lower_ref_in_at(
                        &scope,
                        spec.bound_store(),
                        id,
                        crate::lower::TypePosition::ConstraintHead,
                        &mut d,
                    )
                    .as_interface()
                })
                .collect();
            (declared.name.clone(), conjunction)
        })
        .collect()
}

/// The RFC-2451 covered-rule outcome for an out-of-body impl.
enum OrphanOutcome {
    Ok,
    UncoveredParam(Name),
    NoLocalType,
}

/// RFC-2451 "covered" rule (BEP-044): an out-of-body impl of a foreign
/// interface is allowed only if — scanning `[T, args..]` left to right — a type
/// local to `current_package` appears before any *uncovered* type parameter.
fn orphan_check(
    current_package: &Name,
    iface_qtn: &QualifiedTypeName,
    for_ty: &Ty,
    iface_args: &[Ty],
) -> OrphanOutcome {
    if iface_qtn.package() == current_package {
        return OrphanOutcome::Ok;
    }
    for input in std::iter::once(for_ty).chain(iface_args.iter()) {
        match input {
            Ty::Class(tn, ..) | Ty::Enum(tn, ..) if tn.package() == current_package => {
                return OrphanOutcome::Ok;
            }
            Ty::TypeVar(param, _) => {
                return OrphanOutcome::UncoveredParam(param.name().clone());
            }
            _ => {}
        }
    }
    OrphanOutcome::NoLocalType
}

/// Realize an interface-scoped type (a method signature or a `requires`-clause
/// interface) through `lower`, treating `Self` as a *rigid type variable* bound
/// to `self_bound` — the interface being implemented — and substituting
/// `Self -> receiver` (plus `bindings`) *last*. Unqualified `Self.member`
/// therefore lowers to a symbolic `(Self as I).member` projection and only
/// collapses to the receiver's realization *after* substitution.
#[expect(clippy::too_many_arguments)]
fn realize_with_symbolic_self<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    package_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    ns_context: &[Name],
    base_generics: &[ParamTy],
    self_param: &ParamTy,
    base_bounds: &TypeVarBoundsMap,
    self_bound: &baml_type::Interface,
    receiver: &Ty,
    bindings: &TypeBindings,
    lower: impl FnOnce(&LowerScope<'_, 'db>) -> Ty,
) -> Ty {
    // `Self` joins the in-scope generics with the implemented interface as its
    // bound, so a `Self.member` projection resolves through that bound's closure.
    let mut generics = base_generics.to_vec();
    if !generics.contains(self_param) {
        generics.push(self_param.clone());
    }
    let mut bounds = base_bounds.clone();
    bounds.insert(self_param.clone(), vec![self_bound.clone()]);
    let lowered = lower(&LowerScope {
        db,
        package_items,
        ns_context,
        generic_params: &generics,
        bounds: &bounds,
        self_ty: Some(Ty::TypeVar(self_param.clone(), TyAttr::default())),
    });
    // Substitute the receiver for `Self` last: `(Self as I).member` becomes
    // `(receiver as I).member`, which `normalize` reduces against the receiver's impl.
    let mut substitution = bindings.clone();
    substitution.insert(self_param.clone(), receiver.clone());
    substitute_ty(&lowered, &substitution)
}

/// Phase-5 signature/type conformance for one `implements` block. Runs strictly
/// downstream of [`impl_data`] + associated-type resolution, so the canonical
/// type algebra is fully determined here. It validates that field and method
/// *types* match the interface's declared types (E0116 / E0120); the name-based
/// membership checks live in [`impl_data`].
#[salsa::tracked(returns(ref))]
pub fn validate_impl_signatures<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    impl_loc: baml_compiler2_hir::loc::ImplLoc<'db>,
) -> Vec<(TirTypeError, ImplDiagnosticLocation)> {
    use baml_compiler2_ppir::item_data::{
        ImplSubjectData, function_data, impl_block_data, interface_data,
    };

    let mut diags = Vec::new();
    let data = match impl_data(db, impl_loc).as_ref() {
        Ok(data) => data,
        // A cyclic header can't carry its own diagnostic — re-detect and
        // surface it here so the user's impl doesn't silently vanish.
        Err(ImplDataError::CyclicHeader) => {
            return vec![(
                TirTypeError::CyclicImplHeader,
                ImplDiagnosticLocation::ForTarget,
            )];
        }
        // BUG: `ImplData::interface` is a SOURCE `InterfaceLoc`, so an impl of
        // a MOUNTED interface always lands here and every header diagnostic
        // below is skipped — E0138, E0135, the orphan rule and signature
        // conformance alike. `implement dep.I for true | false` is therefore
        // accepted in silence, while the identical block against a local
        // interface reports E0138. Not a soundness hole today (the header's
        // own validity decision still withholds the facts from selection, so
        // nothing dispatches to it) but a real diagnostic gap; it needs the
        // mounted-interface impl validation slice.
        Err(ImplDataError::InterfaceUnresolved { .. } | ImplDataError::Malformed) => return diags,
    };
    let Some(iface_qtn) = interface_loc_qtn(db, data.interface) else {
        return diags;
    };
    let file = impl_loc.file(db);
    let block = impl_block_data(db, impl_loc);
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let current_package = pkg_info.package.clone();
    let pkg_id = PackageId::new(db, pkg_info.package);

    // The canonical algebra context: hir_ty's fact oracle carrying the impl's
    // own param env (TIR's `GlobalTypeContext` role).
    let res_ctx = crate::package_interface::package_resolution_context(db, pkg_id);
    let aliases = super::package_resolved_aliases(db, pkg_id);
    let bounds: TypeVarBoundsMap = data.generic_params.iter().cloned().collect();
    let ctx = crate::facts::Facts::with_bounds(db, bounds.clone().into_iter().collect());

    let iface_data = interface_data(db, data.interface);
    let iface_self_param = super::interface_self_param(db, data.interface);
    let iface_generic_params = crate::lower::interface_declared_params(db, data.interface);
    let iface_pkg_info =
        baml_compiler2_hir::file_package::file_package(db, data.interface.file(db));
    let iface_pkg_items =
        baml_compiler2_ppir::package_items(db, PackageId::new(db, iface_pkg_info.package.clone()));

    // ── E0116: field-type conformance (in-body impls). ──
    if !iface_data.fields.is_empty()
        && matches!(data.origin, InterfaceImplOrigin::InBodyClass { .. })
        && let ImplSubjectData::InClass { class, .. } = &block.subject
    {
        let class_fields = crate::lower::resolve_class_fields(db, *class);
        let iface_field_bounds = interface_declared_param_bounds(db, data.interface);
        // Realize the interface's declared field types at the impl's interface args.
        let iface_bindings =
            baml_type::unify::bind_type_vars(&iface_generic_params, &data.interface_args);
        // A field type may name `Self.Item`; realize it symbolically and
        // substitute `Self -> for-type` last.
        let self_bound =
            baml_type::Interface::new(iface_qtn.clone(), data.interface_args.clone(), Box::new([]));
        for iface_field in &iface_data.fields {
            // The satisfying class field: explicit link, else same name. Absent → E0124.
            let class_field_name = block
                .field_links
                .iter()
                .find(|fl| fl.interface_field == iface_field.name)
                .map_or(&iface_field.name, |fl| &fl.class_field);
            let Some((_, class_field_ty, _)) = class_fields
                .iter()
                .find(|(name, _, _)| name == class_field_name)
            else {
                continue;
            };
            let mut lower_diags = Vec::new();
            let declared = realize_with_symbolic_self(
                db,
                iface_pkg_items,
                &iface_pkg_info.namespace_path,
                &iface_generic_params,
                &iface_self_param,
                &iface_field_bounds,
                &self_bound,
                &data.for_ty_pattern,
                &iface_bindings,
                |scope| {
                    lower_ref_in(
                        scope,
                        &iface_data.type_refs,
                        iface_field.type_ref,
                        &mut lower_diags,
                    )
                },
            );
            // Field types are invariant — the class field must be the same type.
            if !baml_type::normalize::equivalent(&declared, class_field_ty, &ctx) {
                diags.push((
                    TirTypeError::InterfaceFieldTypeMismatch {
                        interface: iface_qtn.clone(),
                        field: iface_field.name.clone(),
                        expected: declared,
                        got: class_field_ty.clone(),
                    },
                    ImplDiagnosticLocation::InterfaceTarget,
                ));
            }
        }
    }

    // ── Impl-header gates (out-of-body only). ──
    if matches!(data.origin, InterfaceImplOrigin::OutOfBody) {
        // E0138: the for-target must be a single concrete impl subject. The
        // verdict comes from the header's ONE validity decision
        // (`impl_facts`), which also withholds the facts — so an impl this
        // diagnostic rejects is invisible to every consumer, and coherence
        // cannot re-derive concreteness on a different spelling and disagree.
        if let crate::impls::ImplHeaderResolution::NotImplementor { target, .. } =
            crate::impls::impl_facts(db, impl_loc)
        {
            diags.push((
                TirTypeError::ImplTargetNotConcrete {
                    target: target.clone(),
                },
                ImplDiagnosticLocation::ForTarget,
            ));
        }
        // E0135: every declared generic param must be determined by the
        // for-type or interface args. The list comes from the header's ONE
        // validity decision (`impl_facts`), which also POISONS the impl —
        // an undetermined param means the impl resolves nowhere, so the
        // diagnostic and the unresolvability can never drift.
        if let crate::impls::ImplHeaderResolution::Poisoned { unconstrained } =
            crate::impls::impl_facts(db, impl_loc)
        {
            for name in unconstrained {
                diags.push((
                    TirTypeError::UnconstrainedImplTypeParam { name: name.clone() },
                    ImplDiagnosticLocation::Bound,
                ));
            }
        }
        // E0139: orphan rule (RFC-2451 covered).
        match orphan_check(
            &current_package,
            &iface_qtn,
            &data.for_ty_pattern,
            &data.interface_args,
        ) {
            OrphanOutcome::Ok => {}
            OrphanOutcome::UncoveredParam(name) => diags.push((
                TirTypeError::ImplViolatesOrphanRule {
                    interface: iface_qtn.clone(),
                    uncovered_param: Some(name),
                },
                ImplDiagnosticLocation::InterfaceTarget,
            )),
            OrphanOutcome::NoLocalType => diags.push((
                TirTypeError::ImplViolatesOrphanRule {
                    interface: iface_qtn.clone(),
                    uncovered_param: None,
                },
                ImplDiagnosticLocation::InterfaceTarget,
            )),
        }
    }

    // ── E0120: method-signature conformance. An override conforms iff its
    // lowered `Ty::Function` is a subtype of the interface method's realized
    // one. `Self` = for-type on both sides; the interface's generics bind to
    // `interface_args`. ──
    let for_ty = data.for_ty_pattern.clone();
    let impl_generic_names: Vec<ParamTy> =
        data.generic_params.iter().map(|(n, _)| n.clone()).collect();
    let iface_bindings =
        baml_type::unify::bind_type_vars(&iface_generic_params, &data.interface_args);
    let iface_bounds = interface_declared_param_bounds(db, data.interface);
    let self_bound =
        baml_type::Interface::new(iface_qtn.clone(), data.interface_args.clone(), Box::new([]));
    let no_bindings = TypeBindings::default();

    for &method_loc in &data.methods {
        let method_name = function_data(db, method_loc).name.clone();

        // A `$rust_io_function` (sys-op) override with method-level generics
        // can't be dispatched through interface (virtual) dispatch; reject at
        // declaration.
        if !function_data(db, method_loc).generic_params.is_empty()
            && matches!(
                baml_compiler2_ppir::function_body(db, method_loc).as_ref(),
                baml_compiler2_hir::body::FunctionBody::Builtin(
                    baml_compiler2_ast::BuiltinKind::Io
                )
            )
        {
            diags.push((
                TirTypeError::GenericSysOpMethodInInterfaceImpl {
                    interface: iface_qtn.clone(),
                    method: method_name.clone(),
                },
                ImplDiagnosticLocation::Method(method_name.clone()),
            ));
            continue;
        }

        // The interface method this override targets.
        let iface_spec = if let Some(sig) = iface_data
            .required_methods
            .iter()
            .find(|m| m.name == method_name)
        {
            InterfaceMethodSpec::from_required(iface_data, sig)
        } else if let Some(&default_loc) = iface_data
            .default_methods
            .iter()
            .find(|loc| function_data(db, **loc).name == method_name)
        {
            InterfaceMethodSpec::from_default(db, default_loc)
        } else {
            continue; // unknown member → E0115 (impl_data)
        };

        let mut d = Vec::new();
        // The override's function type: Self = for-type; impl + method generics
        // in scope. Its declared throws is replaced by the effective throws.
        let impl_spec = InterfaceMethodSpec::from_default(db, method_loc);
        let impl_scope_generics =
            super::append_params(&impl_generic_names, &impl_spec.generic_param_names());
        let impl_method_params = &impl_scope_generics[impl_generic_names.len()..];
        let mut impl_fn = realize_with_symbolic_self(
            db,
            &res_ctx.own_items,
            &pkg_info.namespace_path,
            &impl_scope_generics,
            &iface_self_param,
            &bounds,
            &self_bound,
            &for_ty,
            &no_bindings,
            |scope| impl_spec.to_function_ty(scope, &mut d),
        );
        if let Ty::Function { throws, .. } = &mut impl_fn {
            **throws = crate::callable::callable_throws(db, method_loc).0.clone();
        }

        // The interface method's function type, realized at `interface_args`.
        let iface_scope_generics =
            super::append_params(&iface_generic_params, &iface_spec.generic_param_names());
        let iface_method_params = &iface_scope_generics[iface_generic_params.len()..];
        let method_generic_arity_matches = impl_method_params.len() == iface_method_params.len();
        let mut iface_method_bindings = iface_bindings.clone();
        if method_generic_arity_matches {
            iface_method_bindings.extend(iface_method_params.iter().zip(impl_method_params).map(
                |(iface_param, impl_param)| {
                    (
                        iface_param.clone(),
                        Ty::TypeVar(impl_param.clone(), TyAttr::default()),
                    )
                },
            ));
        }
        let iface_fn = realize_with_symbolic_self(
            db,
            iface_pkg_items,
            &iface_pkg_info.namespace_path,
            &iface_scope_generics,
            &iface_self_param,
            &iface_bounds,
            &self_bound,
            &for_ty,
            &iface_method_bindings,
            |scope| iface_spec.to_function_ty(scope, &mut d),
        );

        if !method_generic_arity_matches
            || !baml_type::normalize::is_subtype(&impl_fn, &iface_fn, &ctx)
        {
            diags.push((
                TirTypeError::InterfaceMethodSignatureMismatch {
                    interface: iface_qtn.clone(),
                    method: method_name.clone(),
                    expected: iface_fn,
                    got: impl_fn,
                },
                ImplDiagnosticLocation::Method(method_name.clone()),
            ));
        }

        // An override may not add a generic bound the interface method does not
        // declare (Rust's E0276). Checked positionally: each override bound must
        // be entailed by the interface method's bound at the same position.
        let impl_bounds = method_generic_bound_interfaces(
            db,
            &res_ctx.own_items,
            &pkg_info.namespace_path,
            &impl_scope_generics,
            &impl_spec,
        );
        let iface_method_bounds = method_generic_bound_interfaces(
            db,
            iface_pkg_items,
            &iface_pkg_info.namespace_path,
            &iface_scope_generics,
            &iface_spec,
        )
        .into_iter()
        .map(|(name, bounds)| {
            let bounds: Vec<_> = bounds
                .into_iter()
                .map(|bound| bound.map_tys(|ty| substitute_ty(ty, &iface_method_bindings)))
                .collect();
            (name, bounds)
        })
        .collect::<Vec<_>>();
        for (i, (param, impl_conjunction)) in impl_bounds.iter().enumerate() {
            let iface_conjunction = iface_method_bounds
                .get(i)
                .map(|(_, b)| b.as_slice())
                .unwrap_or(&[]);
            for impl_bound in impl_conjunction {
                let entailed = iface_conjunction.iter().any(|iface_bound| {
                    super::carried_bound_satisfies(&ctx, iface_bound, impl_bound)
                        || ctx.interface_requires(iface_bound, impl_bound)
                });
                if !entailed {
                    diags.push((
                        TirTypeError::InterfaceMethodAddsGenericBound {
                            interface: iface_qtn.clone(),
                            method: method_name.clone(),
                            param: param.clone(),
                            bound: impl_bound.clone(),
                        },
                        ImplDiagnosticLocation::Method(method_name.clone()),
                    ));
                }
            }
        }
    }

    // ── E0125: the for-type must implement each interface the implemented one
    // `requires`. ──
    {
        // Diagnostics from re-lowering the clause are dropped on purpose: the
        // interface's own declaration owns them.
        let mut d = Vec::new();
        for &required_ref in &iface_data.requires {
            // A `requires` clause may project `Self.member`; realize it with
            // `Self` bound to the implemented interface and `Self -> for_ty` last.
            let required = realize_with_symbolic_self(
                db,
                iface_pkg_items,
                &iface_pkg_info.namespace_path,
                &iface_generic_params,
                &iface_self_param,
                &iface_bounds,
                &self_bound,
                &for_ty,
                &iface_bindings,
                |scope| {
                    lower_ref_in_at(
                        scope,
                        &iface_data.type_refs,
                        required_ref,
                        crate::lower::TypePosition::ConstraintHead,
                        &mut d,
                    )
                },
            );
            // Reduce any `Self.member` projection in the realized obligation so
            // the associated pins below are concrete.
            let required = baml_type::normalize::normalize(&required, &ctx);
            let Ty::Interface(qtn, generics, assoc, _) = &required else {
                continue;
            };
            // An ill-formed clause realizes to an obligation carrying a recovery
            // sentinel — a cascade off an error already reported at the
            // interface declaration.
            if baml_type_runtime::contains_error_recovery(&required) {
                continue;
            }
            let required_iface = baml_type::Interface {
                name: qtn.clone(),
                generics: generics.clone(),
                associated_types: assoc.clone(),
            };
            if !implements_interface(db, &for_ty, &required_iface, aliases, |a, b| {
                baml_type::normalize::is_subtype(a, b, &ctx)
            }) {
                diags.push((
                    TirTypeError::MissingRequiredInterface {
                        interface: iface_qtn.clone(),
                        required: required_iface,
                    },
                    ImplDiagnosticLocation::InterfaceTarget,
                ));
            }
        }
    }

    // ── Associated-type binding bound satisfaction: an explicit `type Name = V`
    // binding must *implement* the interface's declared bound for `Name`. ──
    {
        let target_iface = baml_type::Interface {
            name: iface_qtn.clone(),
            generics: data.interface_args.clone(),
            associated_types: data.associated_types.clone().into(),
        };
        for binding in &block.associated_type_bindings {
            let Some((_, binding_ty)) = data
                .associated_types
                .iter()
                .find(|(n, _)| *n == binding.name)
            else {
                continue;
            };
            let normalized = baml_type::normalize::normalize(binding_ty, &ctx);
            for bound in ctx.associated_type_bound(&target_iface, binding.name.clone()) {
                if !normalized_arg_implements_bound(&ctx, &normalized, &bound) {
                    diags.push((
                        TirTypeError::AssociatedTypeBindingViolatesBound {
                            interface: iface_qtn.clone(),
                            name: binding.name.clone(),
                            binding: binding_ty.clone(),
                            bound,
                        },
                        ImplDiagnosticLocation::AssociatedBinding(binding.name.clone()),
                    ));
                }
            }
        }
    }

    diags
}

/// One `implements` block resolved for a specific *realized* `(interface, type)`
/// pair, returned by [`get_implements_block`].
pub struct ResolvedImpl<'db> {
    /// The unique impl block — coherence guarantees at most one per realized
    /// `(interface, type)`.
    pub impl_loc: baml_compiler2_hir::loc::ImplLoc<'db>,
    /// The impl's own generic params bound to the realized type arguments.
    pub bindings: TypeBindings,
}

/// Every `implements` block id declared in a package, as stable `ImplLoc`s.
#[salsa::tracked(returns(ref))]
pub fn package_impl_locs<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_id: PackageId<'db>,
) -> Vec<baml_compiler2_hir::loc::ImplLoc<'db>> {
    let mut out = Vec::new();
    // Scan only the package's own files (`package_files`), so edits to
    // another root's file set never invalidate this query.
    for file in baml_compiler2_hir::package::package_files(db, pkg_id) {
        // `file_impls` yields the blocks in source order, so the resolver's
        // "first full match" is reproducible.
        out.extend(
            baml_compiler2_ppir::item_data::file_impls(db, *file)
                .iter()
                .copied(),
        );
    }
    out
}

/// Recursion budget for verifying generic bounds: a bounded blanket can itself
/// be satisfied by *another* blanket, so bound-checking re-enters the resolver.
const BLANKET_IMPL_BOUND_DEPTH: u32 = 16;

/// Whether `ty` is a concrete receiver — a value whose runtime type pins a
/// single static impl. A bare blanket `implement<T> I for T` applies ONLY to
/// these.
fn is_concrete_receiver(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Class(..)
            | Ty::Enum(..)
            | Ty::Int { .. }
            | Ty::Bigint { .. }
            | Ty::Float { .. }
            | Ty::String { .. }
            | Ty::Bool { .. }
            | Ty::Null { .. }
            | Ty::Uint8Array { .. }
            | Ty::Media(..)
            | Ty::List(..)
            | Ty::Map { .. }
            | Ty::Future(..)
            | Ty::Type { .. }
            | Ty::Resource { .. }
            | Ty::PromptAst { .. }
    )
}

/// Substitute an interface bound's own generics + associated types through
/// `bindings` — a bound may reference sibling impl params.
pub fn substitute_interface(
    iface: &baml_type::Interface,
    bindings: &TypeBindings,
) -> baml_type::Interface {
    baml_type::Interface {
        name: iface.name.clone(),
        generics: iface
            .generics
            .iter()
            .map(|g| substitute_ty(g, bindings))
            .collect(),
        associated_types: iface
            .associated_types
            .iter()
            .map(|(name, ty)| (name.clone(), substitute_ty(ty, bindings)))
            .collect(),
    }
}

/// The unique `implements` block by which a fully *realized* concrete type
/// implements a fully *realized* interface — the canonical
/// `(interface, concrete implementor)` → impl lookup.
pub fn get_implements_block<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_id: PackageId<'db>,
    concrete_ty: &Ty,
    requested_iface: &baml_type::Interface,
    aliases: &HashMap<QualifiedTypeName, Ty>,
) -> Option<ResolvedImpl<'db>> {
    get_implements_block_within_depth(
        db,
        pkg_id,
        concrete_ty,
        requested_iface,
        aliases,
        BLANKET_IMPL_BOUND_DEPTH,
    )
}

/// Collect the package of every qualified type name occurring in `ty`.
fn collect_ty_packages(ty: &Ty, out: &mut Vec<Name>) {
    let push = |qtn: &QualifiedTypeName, out: &mut Vec<Name>| {
        if !out.contains(qtn.package()) {
            out.push(qtn.package().clone());
        }
    };
    match ty {
        Ty::Class(qtn, args, _) => {
            push(qtn, out);
            for a in args {
                collect_ty_packages(a, out);
            }
        }
        Ty::Interface(qtn, args, assoc, _) => {
            push(qtn, out);
            for a in args {
                collect_ty_packages(a, out);
            }
            for (_, t) in assoc {
                collect_ty_packages(t, out);
            }
        }
        Ty::Enum(qtn, _) | Ty::EnumVariant(qtn, _, _) | Ty::TypeAlias(qtn, _) => push(qtn, out),
        Ty::List(inner, _) => {
            collect_ty_packages(inner, out);
        }
        Ty::Map { key, value, .. } | Ty::Future(key, value, _) => {
            collect_ty_packages(key, out);
            collect_ty_packages(value, out);
        }
        Ty::Union(members, _) => {
            for m in members {
                collect_ty_packages(m, out);
            }
        }
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            for p in params {
                collect_ty_packages(&p.ty, out);
            }
            collect_ty_packages(ret, out);
            collect_ty_packages(throws, out);
        }
        Ty::AssociatedTypeProjection {
            base, interface, ..
        } => {
            collect_ty_packages(base, out);
            collect_interface_packages(interface, out);
        }
        // No qualified name: primitives, literals, type variables, sentinels.
        Ty::Int { .. }
        | Ty::Bigint { .. }
        | Ty::Float { .. }
        | Ty::String { .. }
        | Ty::Bool { .. }
        | Ty::Null { .. }
        | Ty::Uint8Array { .. }
        | Ty::Media(..)
        | Ty::Literal(..)
        | Ty::TypeVar(..)
        | Ty::RustType { .. }
        | Ty::Type { .. }
        | Ty::Resource { .. }
        | Ty::PromptAst { .. }
        | Ty::Void { .. }
        | Ty::Unknown { .. }
        | Ty::Never { .. }
        | Ty::Error { .. } => {}
    }
}

/// [`collect_ty_packages`] for an interface constraint.
fn collect_interface_packages(iface: &baml_type::Interface, out: &mut Vec<Name>) {
    if !out.contains(iface.name.package()) {
        out.push(iface.name.package().clone());
    }
    for g in &iface.generics {
        collect_ty_packages(g, out);
    }
    for (_, t) in &iface.associated_types {
        collect_ty_packages(t, out);
    }
}

/// Universal (∀) interface membership — the single seam every membership
/// consumer calls. True iff EVERY realized instantiation of `concrete`
/// implements `interface`. FROZEN CONTRACT: callers depend only on this
/// signature.
pub fn implements_interface(
    db: &dyn baml_compiler2_ppir::Db,
    concrete: &Ty,
    interface: &baml_type::Interface,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    mut is_subtype: impl FnMut(&Ty, &Ty) -> bool,
) -> bool {
    // The blanket stdlib impl supplies AnyClass's default-method dispatch, but
    // membership is compiler-derived and narrower. Reuse the normalizer's
    // class-only rule so `requires AnyClass` cannot observe the blanket.
    if interface.name.is_reflect_root_type("AnyClass") {
        return is_subtype(concrete, &interface.to_ty());
    }

    // The orphan rule keeps a legal impl of `(concrete, interface)` in the
    // package of some type that *appears in the query*.
    let mut roots = Vec::new();
    collect_ty_packages(concrete, &mut roots);
    collect_interface_packages(interface, &mut roots);

    let realized = baml_type::RealizedTy::try_from(concrete).is_ok()
        && baml_type::RealizedTy::try_from(&interface.to_ty()).is_ok();
    for pkg_name in roots {
        let pkg_id = PackageId::new(db, pkg_name);
        let found = if realized {
            get_implements_block(db, pkg_id, concrete, interface, aliases).is_some()
        } else {
            type_implements_interface(db, pkg_id, concrete, interface, aliases, &mut is_subtype)
        };
        if found {
            return true;
        }
    }
    false
}

/// Symbolic universal membership — the type-var-bearing backend of
/// [`implements_interface`].
pub fn type_implements_interface<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_id: PackageId<'db>,
    concrete: &Ty,
    interface: &baml_type::Interface,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    mut is_subtype: impl FnMut(&Ty, &Ty) -> bool,
) -> bool {
    let mut packages = vec![pkg_id];
    packages.extend(baml_compiler2_hir::package::package_dependency_closure(
        db, pkg_id,
    ));
    for pkg in packages {
        for &impl_loc in package_impl_locs(db, pkg) {
            let Ok(data) = impl_data(db, impl_loc).as_ref() else {
                continue;
            };
            let Some(bindings) = match_impl_head(db, data, concrete, interface, aliases) else {
                continue;
            };
            if impl_bounds_hold_symbolic(data, &bindings, &mut is_subtype) {
                return true;
            }
        }
    }
    false
}

/// Symbolic counterpart of [`get_implements_block`]: unlike membership, the
/// caller reads the match's realized associated-type pins, so a UNIQUE
/// matching block is required: several distinct matches return `None`.
pub fn get_implements_block_symbolic<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_id: PackageId<'db>,
    concrete: &Ty,
    interface: &baml_type::Interface,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    mut is_subtype: impl FnMut(&Ty, &Ty) -> bool,
) -> Option<ResolvedImpl<'db>> {
    let mut packages = vec![pkg_id];
    packages.extend(baml_compiler2_hir::package::package_dependency_closure(
        db, pkg_id,
    ));
    let mut found: Option<ResolvedImpl<'db>> = None;
    for pkg in packages {
        for &impl_loc in package_impl_locs(db, pkg) {
            let Ok(data) = impl_data(db, impl_loc).as_ref() else {
                continue;
            };
            let Some(bindings) = match_impl_head(db, data, concrete, interface, aliases) else {
                continue;
            };
            if !impl_bounds_hold_symbolic(data, &bindings, &mut is_subtype) {
                continue;
            }
            if found.is_some() {
                // Coherence guarantees uniqueness only for realized inputs.
                return None;
            }
            found = Some(ResolvedImpl { impl_loc, bindings });
        }
    }
    found
}

/// [`get_implements_block`] with an explicit recursion budget.
fn get_implements_block_within_depth<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_id: PackageId<'db>,
    concrete_ty: &Ty,
    requested_iface: &baml_type::Interface,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    depth: u32,
) -> Option<ResolvedImpl<'db>> {
    debug_assert!(
        !contains_typevar(concrete_ty)
            && requested_iface
                .generics
                .iter()
                .all(|arg| !contains_typevar(arg))
            && requested_iface
                .associated_types
                .iter()
                .all(|(_, ty)| !contains_typevar(ty)),
        "get_implements_block requires a fully-realized (interface, type); an unrealized \
         receiver dispatches dynamically via a runtime virtual call, never here"
    );

    let mut packages = vec![pkg_id];
    packages.extend(baml_compiler2_hir::package::package_dependency_closure(
        db, pkg_id,
    ));

    for pkg in packages {
        for &impl_loc in package_impl_locs(db, pkg) {
            let Ok(data) = impl_data(db, impl_loc).as_ref() else {
                continue;
            };
            // Structural match: exact interface head, the bare-blanket guard,
            // joint for-type + interface-arg unification, and associated pins.
            let Some(bindings) = match_impl_head(db, data, concrete_ty, requested_iface, aliases)
            else {
                continue;
            };

            // Generic bounds must hold: a bound `T extends I` is satisfied iff
            // the bound type itself implements `I` — possibly via another
            // blanket impl, hence the bounded re-entry.
            let bounds_satisfied = data.generic_params.iter().all(|(name, bounds)| {
                let Some(actual) = bindings.get(name) else {
                    return true;
                };
                bounds.iter().all(|bound| {
                    let bound = substitute_interface(bound, &bindings);
                    // A bound still mentioning an unbound param can't be checked
                    // statically here; treat it as vacuously satisfied.
                    if bound.generics.iter().any(contains_typevar)
                        || bound
                            .associated_types
                            .iter()
                            .any(|(_, ty)| contains_typevar(ty))
                    {
                        return true;
                    }
                    depth > 0
                        && get_implements_block_within_depth(
                            db,
                            pkg_id,
                            actual,
                            &bound,
                            aliases,
                            depth - 1,
                        )
                        .is_some()
                })
            });
            if !bounds_satisfied {
                continue;
            }

            // Unique by coherence — the first full match is the only match.
            return Some(ResolvedImpl { impl_loc, bindings });
        }
    }
    None
}

/// Structurally match one impl against a requested `(concrete, interface)`.
/// Declared generic *bounds* are NOT checked here.
fn match_impl_head<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    data: &ImplData<'db>,
    concrete: &Ty,
    requested_iface: &baml_type::Interface,
    aliases: &HashMap<QualifiedTypeName, Ty>,
) -> Option<TypeBindings> {
    if interface_loc_qtn(db, data.interface).as_ref() != Some(&requested_iface.name)
        || data.interface_args.len() != requested_iface.generics.len()
    {
        return None;
    }
    let param_names: Vec<ParamTy> = data
        .generic_params
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    if let Ty::TypeVar(name, _) = &data.for_ty_pattern
        && param_names.contains(name)
        && !is_concrete_receiver(concrete)
    {
        return None;
    }
    let mut pairs: Vec<(&Ty, &Ty)> = Vec::with_capacity(1 + requested_iface.generics.len());
    pairs.push((&data.for_ty_pattern, concrete));
    pairs.extend(data.interface_args.iter().zip(&requested_iface.generics));
    let bindings = match_ty_patterns(&pairs, &param_names, aliases)?;

    let associated_types_agree =
        requested_iface
            .associated_types
            .iter()
            .all(|(name, requested_ty)| {
                match data.associated_types.iter().find(|(n, _)| n == name) {
                    Some((_, impl_ty)) => baml_type::unify::AliasEquivCtx(aliases)
                        .equivalent(&substitute_ty(impl_ty, &bindings), requested_ty),
                    None => true,
                }
            });
    associated_types_agree.then_some(bindings)
}

/// Whether every declared bound the match *pinned* holds, discharged through
/// `is_subtype`. A param the match left free, or a bound still mentioning a
/// free param, is deferred.
fn impl_bounds_hold_symbolic(
    data: &ImplData<'_>,
    bindings: &TypeBindings,
    is_subtype: &mut impl FnMut(&Ty, &Ty) -> bool,
) -> bool {
    data.generic_params.iter().all(|(name, bounds)| {
        let Some(actual) = bindings.get(name) else {
            return true;
        };
        bounds.iter().all(|bound| {
            let bound = substitute_interface(bound, bindings);
            if bound.generics.iter().any(contains_typevar)
                || bound
                    .associated_types
                    .iter()
                    .any(|(_, ty)| contains_typevar(ty))
            {
                return true;
            }
            is_subtype(actual, &bound.to_ty())
        })
    })
}

/// Every impl block in `pkg_id` + dependency closure that applies to `concrete`.
/// Symbolic-capable: `concrete` may carry free vars.
pub fn impls_for_type<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_id: PackageId<'db>,
    concrete: &Ty,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    mut is_subtype: impl FnMut(&Ty, &Ty) -> bool,
) -> Vec<ResolvedImpl<'db>> {
    let mut packages = vec![pkg_id];
    packages.extend(baml_compiler2_hir::package::package_dependency_closure(
        db, pkg_id,
    ));
    let mut out = Vec::new();
    for pkg in packages {
        for &impl_loc in package_impl_locs(db, pkg) {
            let Ok(data) = impl_data(db, impl_loc).as_ref() else {
                continue;
            };
            let param_names: Vec<ParamTy> = data
                .generic_params
                .iter()
                .map(|(name, _)| name.clone())
                .collect();
            if let Ty::TypeVar(name, _) = &data.for_ty_pattern
                && param_names.contains(name)
                && !is_concrete_receiver(concrete)
            {
                continue;
            }
            let Some(bindings) =
                match_ty_patterns(&[(&data.for_ty_pattern, concrete)], &param_names, aliases)
            else {
                continue;
            };
            if impl_bounds_hold_symbolic(data, &bindings, &mut is_subtype) {
                out.push(ResolvedImpl { impl_loc, bindings });
            }
        }
    }
    out
}

/// When `concrete` *almost* implements the interface `requested` via some impl
/// block — the implementor shape matches but a concrete generic bound fails —
/// return the first failing `(param, required_bound_as_ty, actual_arg)`.
/// Diagnostic-only.
pub fn first_failing_impl_bound<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_id: PackageId<'db>,
    concrete: &Ty,
    requested: &Ty,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    mut is_subtype: impl FnMut(&Ty, &Ty) -> bool,
) -> Option<(Name, Ty, Ty)> {
    let Ty::Interface(requested_qtn, requested_args, _, _) = requested else {
        return None;
    };
    let mut packages = vec![pkg_id];
    packages.extend(baml_compiler2_hir::package::package_dependency_closure(
        db, pkg_id,
    ));
    for pkg in packages {
        for &impl_loc in package_impl_locs(db, pkg) {
            let Ok(data) = impl_data(db, impl_loc).as_ref() else {
                continue;
            };
            // Only an impl of the requested interface can "almost" satisfy it.
            if interface_loc_qtn(db, data.interface).as_ref() != Some(requested_qtn) {
                continue;
            }
            let param_names: Vec<ParamTy> = data
                .generic_params
                .iter()
                .map(|(name, _)| name.clone())
                .collect();
            let mut pairs: Vec<(&Ty, &Ty)> = vec![(&data.for_ty_pattern, concrete)];
            if data.interface_args.len() == requested_args.len() {
                pairs.extend(data.interface_args.iter().zip(requested_args));
            }
            let Some(bindings) = match_ty_patterns(&pairs, &param_names, aliases) else {
                continue;
            };
            for (name, bounds) in &data.generic_params {
                let Some(actual) = bindings.get(name) else {
                    continue;
                };
                for bound in bounds {
                    let bound = substitute_interface(bound, &bindings);
                    if bound.generics.iter().any(contains_typevar)
                        || bound
                            .associated_types
                            .iter()
                            .any(|(_, ty)| contains_typevar(ty))
                    {
                        continue;
                    }
                    let bound_ty = bound.to_ty();
                    if !is_subtype(actual, &bound_ty) {
                        return Some((name.name().clone(), bound_ty, actual.clone()));
                    }
                }
            }
        }
    }
    None
}

impl<'db> ResolvedImpl<'db> {
    /// The interface this impl provides at its resolved instantiation:
    /// the declared interface with the impl's bindings substituted in.
    pub fn implemented_interface(
        &self,
        db: &'db dyn baml_compiler2_ppir::Db,
    ) -> baml_type::Interface {
        let data = impl_data(db, self.impl_loc)
            .as_ref()
            .unwrap_or_else(|_| unreachable!("a ResolvedImpl carries an impl_data-Ok impl"));
        let name = interface_loc_qtn(db, data.interface)
            .unwrap_or_else(|| unreachable!("an impl_data-Ok impl has a named interface target"));
        let generics = data
            .interface_args
            .iter()
            .map(|arg| substitute_ty(arg, &self.bindings))
            .collect();
        let associated_types = data
            .associated_types
            .iter()
            .map(|(name, ty)| (name.clone(), substitute_ty(ty, &self.bindings)))
            .collect();
        baml_type::Interface::new(name, generics, associated_types)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qtn(pkg: &str, name: &str) -> QualifiedTypeName {
        QualifiedTypeName::new(Name::new(pkg), Vec::new(), Name::new(name))
    }

    #[test]
    fn collect_ty_packages_covers_head_and_nested_covered_args() {
        let ty = Ty::Class(
            qtn("user", "Box"),
            Box::new([Ty::Enum(qtn("dep", "Meters"), TyAttr::default())]),
            TyAttr::default(),
        );
        let mut out = Vec::new();
        collect_ty_packages(&ty, &mut out);
        assert!(
            out.contains(&Name::new("user")),
            "for-type head package, got {out:?}"
        );
        assert!(
            out.contains(&Name::new("dep")),
            "nested covered-arg package, got {out:?}"
        );
    }

    #[test]
    fn collect_interface_packages_covers_head_args_and_pins() {
        let iface = baml_type::Interface::new(
            qtn("ifacepkg", "Conv"),
            Box::new([Ty::Class(
                qtn("argpkg", "Meters"),
                Box::new([]),
                TyAttr::default(),
            )]),
            Box::new([(
                Name::new("Out"),
                Ty::Enum(qtn("pinpkg", "Unit"), TyAttr::default()),
            )]),
        );
        let mut out = Vec::new();
        collect_interface_packages(&iface, &mut out);
        for pkg in ["ifacepkg", "argpkg", "pinpkg"] {
            assert!(out.contains(&Name::new(pkg)), "missing {pkg}, got {out:?}");
        }
    }
}
