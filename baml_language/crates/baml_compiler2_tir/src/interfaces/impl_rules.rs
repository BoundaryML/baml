use std::collections::HashMap;

use baml_base::{Name, Span, TyAttr};
use baml_compiler2_hir::{contributions::Definition, package::PackageId};
use baml_type::{ParamTy, QualifiedTypeName, Ty, normalize::TypeContext};

use crate::{
    generics::{contains_typevar, substitute_ty},
    interfaces::{
        InterfaceImplOrigin, lower_interface_associated_bindings, match_ty_patterns,
        normalized_arg_implements_bound, resolve_ref_to_interface,
    },
    lower_type_expr::qualify_def,
    type_context::AliasEquivCtx,
    unify::TypeBindings,
};

/// Fully-resolved data for one `implements` block, keyed by its stable
/// [`ImplLoc`](baml_compiler2_hir::loc::ImplLoc).
///
/// Every impl — in-body or out-of-body — normalizes to the same *free* shape
/// here: an in-body `implements I {…}` inside `class C<T>` resolves exactly as
/// `implement<T> I for C<T>` (`for_ty_pattern` is `C<T…>`, generics are the
/// class's). The in-body/out-of-body distinction survives only as `origin`,
/// which is diagnostic metadata and MUST NOT drive resolution/dispatch.
///
/// This is the single point where an impl's interface target, for-type, and
/// associated bindings are resolved; the registry, MIR, and LSP all read it
/// instead of re-lowering the raw `TypeExpr` paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplData<'db> {
    /// The implemented interface's resolved head identity. Impls whose target
    /// doesn't resolve to an interface are dropped (`impl_data` → `None`).
    pub interface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    /// The interface's generic input args (`<int>` in `Container<int>`).
    pub interface_args: Vec<Ty>,
    /// The resolved implementor pattern (may carry `Ty::TypeVar`s).
    pub for_ty_pattern: Ty,
    /// Generic params with their interface bounds (BEP-044). All bounds are
    /// interfaces; multiple per param (`T extends A & B`) are carried. Not yet
    /// consumed by the registry/emit (they keep the legacy single-bound path) —
    /// plumbed for the deferred bound-enforcement work.
    pub generic_params: Vec<(ParamTy, Vec<baml_type::Interface>)>,
    /// Diagnostics produced while resolving this impl — lowering errors plus
    /// non-interface generic bounds (the E0142 case). Each is paired with the
    /// span-free [`ImplDiagnosticLocation`] it originated from so check.rs can
    /// render it at a precise source range. Span-free so the query stays
    /// Salsa-cacheable. Never dropped.
    pub diagnostics: Vec<(crate::infer_context::TirTypeError, ImplDiagnosticLocation)>,
    /// The impl body's own method overrides, as stable function ids. Inherited
    /// interface defaults are merged by downstream consumers, not here.
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
/// `ImplData<'db>` holds Salsa interned locs (`InterfaceLoc`, `FunctionLoc`)
/// with a db-tied lifetime, so it can't auto-derive `salsa::Update`. Mirrors
/// `baml_compiler2_hir::namespace::NamespaceItems`'s impl: `maybe_update` uses
/// `PartialEq` for proper early-cutoff.
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
    /// impls (their for-type is the synthesized class); falls back to the block.
    ForTarget,
    /// A generic bound (`<T extends <here>>`). Bounds carry no source span, so
    /// this resolves to the whole-block span.
    Bound,
    /// An override method in the block, by name — resolved via
    /// [`ImplDataSourceMap::method_spans`] to the span of *every* override with that name: if
    /// several share the name (itself an E0093 duplicate), the diagnostic marks all of them.
    /// Falls back to the whole block.
    Method(Name),
    /// The interface-field side of a `field as class_field` link, by interface-field name —
    /// resolved via [`ImplDataSourceMap::interface_field_link_spans`] to *every* link with that
    /// interface-field name (a duplicate marks all sites). Falls back to the whole block.
    InterfaceFieldLink(Name),
    /// The class-field side of a `field as class_field` link, by class-field name — resolved via
    /// [`ImplDataSourceMap::class_field_link_spans`] to *every* link with that class-field name.
    /// Falls back to the whole block.
    ClassFieldLink(Name),
    /// A `type Name = …` associated-type binding in the block, by binding name — resolved via
    /// [`ImplDataSourceMap::associated_binding_spans`] to *every* binding with that name (a
    /// duplicate marks all sites). Falls back to the whole block.
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
    /// Span of the for-target type expr; `None` for in-body impls (no written
    /// for-target — the for-type is the synthesized class).
    pub for_target_span: Option<Span>,
    /// Override-method name → the span of *every* override with that name (source order), so a
    /// [`ImplDiagnosticLocation::Method`] diagnostic marks all same-named overrides.
    pub method_spans: HashMap<Name, Vec<Span>>,
    /// Interface-field name → the span of the interface-field side of *every* `field as
    /// class_field` link with that name, so an [`ImplDiagnosticLocation::InterfaceFieldLink`]
    /// diagnostic (E0128/E0130) marks all such links.
    pub interface_field_link_spans: HashMap<Name, Vec<Span>>,
    /// Class-field name → the span of the class-field side of *every* `field as class_field` link
    /// with that name, so an [`ImplDiagnosticLocation::ClassFieldLink`] diagnostic (E0129) marks
    /// all such links.
    pub class_field_link_spans: HashMap<Name, Vec<Span>>,
    /// Associated-binding name → the name span of *every* `type Name = …` binding in the block, so
    /// an [`ImplDiagnosticLocation::AssociatedBinding`] diagnostic marks all same-named bindings.
    pub associated_binding_spans: HashMap<Name, Vec<Span>>,
}

/// The qualified name of a resolved interface loc (head identity for building a
/// `Ty::Interface`). Always `Some` for a genuine loc (the firewall lookup is
/// total); the `Option` shape is kept for its many callers.
pub fn interface_loc_qtn<'db>(
    db: &'db dyn crate::Db,
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
    /// The implements target does not name an interface, so no resolved impl
    /// exists. The diagnostics lowered before the failure (the ill-formed
    /// interface target itself, plus the for-target and any bounds) ride along
    /// so check.rs still surfaces them — `impl_data` remains the single owner of
    /// an impl's diagnostics even when it can't be fully resolved. (Associated
    /// bindings are absent: they can't be checked without the interface.)
    InterfaceUnresolved {
        diagnostics: Vec<(crate::infer_context::TirTypeError, ImplDiagnosticLocation)>,
    },
    /// The impl block, its class, or the interface declaration was missing from
    /// the item tree (internal invariant).
    Malformed,
    /// The impl header references its own resolution: lowering it resolves a
    /// concrete associated-type projection (`… for C.Item`, or an interface
    /// argument `I<X = C.Item>`), which enumerates the package's impls via
    /// [`impls_for_type`] — including this one, mid-computation. Rather than
    /// panic on the salsa cycle, [`impl_data`]'s `cycle_result` converges here:
    /// **concrete projections in an impl header are illegal.** (An explicit
    /// `(C as I).Item` qualifier or a type-variable projection does not
    /// enumerate impls and is unaffected.)
    CyclicHeader,
}

/// Lower one generic param's bounds to its interface constraints, pushing both
/// the lowering diagnostics and the non-interface-bound (E0142) diagnostics
/// into `diags`. `store` is the arena the bound ids index (the declaring item's
/// `type_refs`); `generic_param_names` are the in-scope type-var names so a
/// bound naming a sibling param doesn't read as an unresolved type.
fn lower_generic_param_interface_bounds(
    db: &dyn crate::Db,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    bounds: &[baml_compiler2_hir::type_ref::TypeRefId],
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    ns: &[Name],
    generic_param_names: &[ParamTy],
    diags: &mut Vec<crate::infer_context::TirTypeError>,
) -> Vec<baml_type::Interface> {
    let mut ifaces = Vec::new();
    for &bound in bounds {
        let ty = crate::lower_type_expr::lower_constraint_head_type_ref(
            store,
            bound,
            &crate::lower_type_expr::ScopeCtx {
                db,
                package_items: pkg_items,
                ns_context: ns,
                generic_params: generic_param_names,
                bounds: &crate::lower_type_expr::TypeVarBoundsMap::default(),
                self_ty: None,
            },
            diags,
        );
        match ty {
            // BEP-062: `baml.AnyFunction` is legal only as a value type (an
            // existential); as a bound it is rejected and contributes no
            // constraint (recovery treats the param as unbounded).
            Ty::Interface(qtn, ..) if qtn.is_builtin_root_type("AnyFunction") => {
                diags.push(
                    crate::infer_context::TirTypeError::BuiltinInterfaceNotABound {
                        interface: qtn,
                    },
                );
            }
            Ty::Interface(qtn, generics, assoc, _) => {
                ifaces.push(baml_type::Interface {
                    name: qtn,
                    generics,
                    associated_types: assoc,
                });
            }
            // Already diagnosed by lowering the bound expression itself — a second
            // "not an interface" here would be redundant.
            Ty::Unknown { .. } | Ty::Error { .. } | Ty::BuiltinUnknown { .. } => {}
            // BEP-044 requires bounds to be interfaces (E0142). A sibling type
            // variable (`<T, U extends T>`) or an associated-type projection is
            // not an interface either — bounds constrain by interface contract,
            // never by subtyping against another parameter.
            other => diags.push(
                crate::infer_context::TirTypeError::GenericBoundNotInterface { bound: other },
            ),
        }
    }
    ifaces
}

/// The fallback for a self-referential [`impl_data`] computation: an impl header
/// whose lowering re-enters `impl_data` (via a concrete projection →
/// [`impls_for_type`]) is [`ImplDataError::CyclicHeader`]. Salsa uses this
/// `cycle_result` to converge the cycle immediately instead of panicking, so
/// such headers are a deterministic error rather than a crash.
fn impl_data_cycle_result<'db>(
    _db: &'db dyn crate::Db,
    _id: salsa::Id,
    _impl_loc: baml_compiler2_hir::loc::ImplLoc<'db>,
) -> Result<ImplData<'db>, ImplDataError> {
    Err(ImplDataError::CyclicHeader)
}

/// Resolve one `implements` block to its [`ImplData`].
///
/// `Err(ImplDataError::InterfaceUnresolved { diagnostics })` when the interface
/// target doesn't name an interface — the diagnostics gathered before that point
/// (the bad interface target, the for-target, the bounds) ride along so they're
/// still surfaced (callers needing the resolved data skip such impls).
/// `Err(Malformed)` is an internal invariant violation (a loc pointing at a
/// missing item-tree entry). `Err(CyclicHeader)` is a header that projects
/// through its own resolution (see [`impl_data_cycle_result`]).
///
/// All of this impl's diagnostics are owned here — in [`ImplData::diagnostics`]
/// on success, or the `InterfaceUnresolved` payload on failure — and surfaced at
/// the impl's span by check.rs. `impl_data` is the single owner; check.rs never
/// re-derives them.
#[salsa::tracked(returns(ref), cycle_result = impl_data_cycle_result)]
pub fn impl_data<'db>(
    db: &'db dyn crate::Db,
    impl_loc: baml_compiler2_hir::loc::ImplLoc<'db>,
) -> Result<ImplData<'db>, ImplDataError> {
    use baml_compiler2_ppir::item_data::{ImplSubjectData, function_data, impl_block_data};

    let file = impl_loc.file(db);
    let block = impl_block_data(db, impl_loc);

    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let ns = &pkg_info.namespace_path;

    // Diagnostics are collected per *origin* (for-target, interface-target, a
    // bound) so check.rs can render each at a precise span rather than the whole
    // block. Never dropped.
    //
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
            let generic_env = crate::generic_env::class_generic_env(db, *class);
            let generic_param_names = generic_env.params().to_vec();
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
            // owns its bounds' diagnostics (lowering errors + non-interface
            // bounds). Resolve them to interface constraints here, but emit no
            // diagnostics for them at the impl (a discarded sink) — they belong
            // to the class, and would otherwise misattribute and duplicate across
            // every in-body impl of that class.
            let mut class_bound_diags = Vec::new();
            let generic_params: Vec<(ParamTy, Vec<baml_type::Interface>)> = generic_param_names
                .iter()
                .zip(class_data.generic_param_bounds.iter())
                .map(|(name, bound)| {
                    let bounds: Vec<baml_compiler2_hir::type_ref::TypeRefId> =
                        bound.iter().copied().collect();
                    let ifaces = lower_generic_param_interface_bounds(
                        db,
                        &class_data.type_refs,
                        &bounds,
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
                // In-body impls have no written for-target, and the class owns
                // its bounds' diagnostics — so neither contributes here.
                Vec::new(),
                Vec::new(),
                // A simple `implement I for C` is merged onto `C` (`InClass` subject) but
                // written out-of-body — its origin stays `OutOfBody`.
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
            let generic_param_names = crate::generic_env::impl_generic_env(db, impl_loc)
                .params()
                .to_vec();
            let mut for_target_diags = Vec::new();
            // The impl generics' bounds are not threaded into this header lowering
            // (a `T.member` type-variable projection in an impl header is a separate
            // gap). A *concrete* projection here (`… for C.Item`) re-enters
            // `impl_data` via `impls_for_type` regardless of bounds; that cycle is
            // caught by `impl_data`'s `cycle_result` (→ `CyclicHeader`), so such
            // headers are illegal rather than a panic.
            let for_ty = crate::lower_type_expr::lower_type_ref(
                &block.type_refs,
                *for_target,
                &crate::lower_type_expr::ScopeCtx {
                    db,
                    package_items: pkg_items,
                    ns_context: ns,
                    generic_params: &generic_param_names,
                    bounds: &crate::lower_type_expr::TypeVarBoundsMap::default(),
                    self_ty: None,
                },
                &mut for_target_diags,
            );
            let mut bound_diags = Vec::new();
            // `implements<T, T> …` — a duplicate impl generic is a declaration error
            // (the in-body form's generics are the class's, whose declaration owns
            // that check). Reported at the block via the `Bound` location.
            for (idx, name) in names.iter().enumerate() {
                if names[..idx].contains(name) {
                    bound_diags.push(crate::infer_context::TirTypeError::DuplicateGenericParam {
                        name: name.clone(),
                    });
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
    // The target is a constraint head: it pins only its written inline
    // bindings (the block's `type X = …` bindings are folded in separately),
    // so neither defaults nor completeness apply here.
    let lowered_interface = crate::lower_type_expr::lower_constraint_head_type_ref(
        &block.type_refs,
        block.interface_target,
        &crate::lower_type_expr::ScopeCtx {
            db,
            package_items: pkg_items,
            ns_context: ns,
            generic_params: &generic_param_names,
            bounds: &crate::lower_type_expr::TypeVarBoundsMap::default(),
            self_ty: None,
        },
        &mut interface_target_diags,
    );
    let interface_args = if let Ty::Interface(_, args, _, _) = &lowered_interface {
        args.clone()
    } else {
        Vec::new()
    };

    // Resolve the interface head to its loc *after* lowering, so a bad interface
    // target still surfaces its diagnostics (and the for-target / bound ones).
    // Associated bindings are skipped here — they can't be checked without the
    // interface declaration.
    let Some(iface_loc) =
        resolve_ref_to_interface(db, &block.type_refs, block.interface_target, pkg_items, ns)
    else {
        // The head didn't name an interface. If it resolved to a named non-interface type, that
        // is a specialized "not an interface" (E0119); otherwise the head is unknown, so the
        // generic unresolved-type lowering error (E0112-equivalent) rides along. The for-target
        // and bound diagnostics ride along either way.
        let head_diags: Vec<_> = match &lowered_interface {
            Ty::Class(qtn, ..) | Ty::Enum(qtn, ..) => vec![(
                crate::infer_context::TirTypeError::ImplTargetNotInterface {
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
    let impl_bounds: crate::lower_type_expr::TypeVarBoundsMap =
        generic_params.iter().cloned().collect();
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

    // Conformance, computed alongside lowering (each diagnostic carries its own location): a
    // field-bearing interface can't be implemented out-of-body (E0126); every required method
    // must be provided by an override or an inherited default (E0113); and every override must
    // correspond to a required or default method of the interface (E0115). (Signature conformance,
    // field links, and the `requires` closure are separate slices.)
    let mut conformance_diags: Vec<(crate::infer_context::TirTypeError, ImplDiagnosticLocation)> =
        Vec::new();
    if let Some(iface_qtn) = interface_loc_qtn(db, iface_loc) {
        // BEP-062 (E0153): `baml.AnyFunction`'s conformance is derived by the
        // compiler (every function type implements it, in the subtype engine);
        // a written impl is rejected outright. The block's other diagnostics
        // still ride along so the user sees everything at once.
        if iface_qtn.is_builtin_root_type("AnyFunction") {
            conformance_diags.push((
                crate::infer_context::TirTypeError::BuiltinInterfaceNotImplementable {
                    interface: iface_qtn.clone(),
                },
                ImplDiagnosticLocation::InterfaceTarget,
            ));
        }
        if matches!(origin, InterfaceImplOrigin::OutOfBody) && !iface_data.fields.is_empty() {
            conformance_diags.push((
                crate::infer_context::TirTypeError::OutOfBodyImplementsFieldInterface {
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
        // E0113: a required method with no override and no inherited default.
        for required in &iface_data.required_methods {
            let provided = override_names.iter().any(|n| **n == required.name)
                || default_names.iter().any(|n| **n == required.name);
            if !provided {
                conformance_diags.push((
                    crate::infer_context::TirTypeError::MissingInterfaceMethod {
                        interface: iface_qtn.clone(),
                        method: required.name.clone(),
                    },
                    ImplDiagnosticLocation::InterfaceTarget,
                ));
            }
        }
        // E0115: an override matching no required or default method overrides nothing. Reported
        // once per name (a duplicate override is E0093; the `Method` location marks every
        // same-named override).
        for (idx, &name) in override_names.iter().enumerate() {
            if override_names[..idx].contains(&name) {
                continue;
            }
            let is_member = iface_data.required_methods.iter().any(|m| m.name == *name)
                || default_names.iter().any(|n| **n == *name);
            if !is_member {
                conformance_diags.push((
                    crate::infer_context::TirTypeError::UnknownInterfaceMember {
                        interface: iface_qtn.clone(),
                        member: name.clone(),
                    },
                    ImplDiagnosticLocation::Method(name.clone()),
                ));
            }
        }
        // Field-side conformance, in-body class impls only (out-of-body field impls are
        // already E0126, and `field as` links only appear in a class body). Field-link
        // well-formedness (E0128/E0129/E0130) runs whenever links are present; field coverage
        // (E0124) runs when the interface declares fields.
        if let InterfaceImplOrigin::InBodyClass { class_qtn } = &origin
            && let ImplSubjectData::InClass { class, .. } = &block.subject
            && (!block.field_links.is_empty() || !iface_data.fields.is_empty())
        {
            let class_fields = crate::inference::resolve_class_fields(db, *class);
            let is_iface_field =
                |name: &Name| iface_data.fields.iter().any(|fld| fld.name == *name);
            let is_class_field =
                |name: &Name| class_fields.fields.iter().any(|(n, _, _)| n == name);

            // Interface-field side of each link, deduped by interface-field name (the location
            // resolves to every link with that name). E0130: linked more than once. E0128: the
            // named interface field does not exist.
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
                        crate::infer_context::TirTypeError::DuplicateInterfaceFieldLink {
                            interface: iface_qtn.clone(),
                            field: iface_field.clone(),
                        },
                        ImplDiagnosticLocation::InterfaceFieldLink(iface_field.clone()),
                    ));
                }
                if !is_iface_field(iface_field) {
                    conformance_diags.push((
                        crate::infer_context::TirTypeError::UnknownInterfaceFieldLink {
                            interface: iface_qtn.clone(),
                            field: iface_field.clone(),
                        },
                        ImplDiagnosticLocation::InterfaceFieldLink(iface_field.clone()),
                    ));
                }
            }
            // Class-field side (E0129), only for links whose interface field is valid (an
            // unknown-interface-field link is already E0128 — mirrors the old stack's skip).
            // Deduped by class-field name among those eligible links.
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
                        crate::infer_context::TirTypeError::UnknownClassFieldInInterfaceLink {
                            class: class_qtn.name().clone(),
                            interface: iface_qtn.clone(),
                            field: class_field.clone(),
                        },
                        ImplDiagnosticLocation::ClassFieldLink(class_field.clone()),
                    ));
                }
            }
            // E0124: every interface field must be covered by a same-named class field or an
            // explicit `field as class_field` link.
            for iface_field in &iface_data.fields {
                let linked = block
                    .field_links
                    .iter()
                    .any(|fl| fl.interface_field == iface_field.name);
                if !linked && !is_class_field(&iface_field.name) {
                    conformance_diags.push((
                        crate::infer_context::TirTypeError::MissingInterfaceField {
                            interface: iface_qtn.clone(),
                            field: iface_field.name.clone(),
                        },
                        ImplDiagnosticLocation::InterfaceTarget,
                    ));
                }
            }
        }

        // Associated-type binding hygiene, name-based (applies to every impl, in-body or
        // out-of-body). Bound satisfaction needs the type algebra and is checked downstream in
        // `validate_impl_signatures`.
        let is_assoc = |name: &Name| iface_data.associated_types.iter().any(|a| a.name == *name);
        for (idx, binding) in block.associated_type_bindings.iter().enumerate() {
            // Dedup by binding name (the location marks every binding with that name).
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
                    crate::infer_context::TirTypeError::DuplicateAssociatedTypeBinding {
                        interface: iface_qtn.clone(),
                        name: binding.name.clone(),
                    },
                    ImplDiagnosticLocation::AssociatedBinding(binding.name.clone()),
                ));
            }
            // Unknown: names no associated type of the interface.
            if !is_assoc(&binding.name) {
                conformance_diags.push((
                    crate::infer_context::TirTypeError::UnknownAssociatedTypeBinding {
                        interface: iface_qtn.clone(),
                        name: binding.name.clone(),
                    },
                    ImplDiagnosticLocation::AssociatedBinding(binding.name.clone()),
                ));
            }
        }
        // Missing: an associated type the interface declares with no default is not bound, so it
        // is left undetermined.
        for assoc in &iface_data.associated_types {
            if assoc.default.is_none()
                && !block
                    .associated_type_bindings
                    .iter()
                    .any(|b| b.name == assoc.name)
            {
                conformance_diags.push((
                    crate::infer_context::TirTypeError::MissingImplAssociatedTypeBinding {
                        interface: iface_qtn.clone(),
                        name: assoc.name.clone(),
                    },
                    ImplDiagnosticLocation::InterfaceTarget,
                ));
            }
        }
        // Bindings written on the `implements` target (`implements I<Item = …>`) instead of in
        // the block are rejected — the block's `type Name = …` is the only binding site.
        if let baml_compiler2_hir::type_ref::TypeRefKind::Path {
            associated_type_bindings,
            ..
        } = &block.type_refs[block.interface_target].kind
            && !associated_type_bindings.is_empty()
        {
            conformance_diags.push((
                crate::infer_context::TirTypeError::AssociatedTypeBindingsOnImplementsTarget {
                    interface: iface_qtn,
                },
                ImplDiagnosticLocation::InterfaceTarget,
            ));
        }
    }

    // Tag each diagnostic with its origin (interface ref → InterfaceTarget,
    // associated bindings ride along the interface reference, for-target →
    // ForTarget, bounds → Bound). Deterministic order for stable output.
    let diagnostics: Vec<(crate::infer_context::TirTypeError, ImplDiagnosticLocation)> =
        interface_target_diags
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
    db: &'db dyn crate::Db,
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
    // Group each field link's endpoint spans by name, so a per-name field-link diagnostic
    // (E0128/E0129/E0130) marks every link that mentions that name.
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
    // Group each `type Name = …` binding's name span by name, so a per-name assoc-binding
    // diagnostic (unknown / duplicate / bound violation) marks every binding with that name.
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

/// Collect every `Ty::TypeVar` name in `ty` (at any depth) into `out` — used to decide which
/// impl generic params the for-type / interface args determine (E0135).
fn collect_type_var_names(ty: &Ty, out: &mut Vec<ParamTy>) {
    match ty {
        Ty::TypeVar(name, _) => out.push(name.clone()),
        Ty::List(inner, _) | Ty::EvolvingList(inner, _) => {
            collect_type_var_names(inner, out);
        }
        Ty::Map { key, value, .. } | Ty::EvolvingMap(key, value, _) => {
            collect_type_var_names(key, out);
            collect_type_var_names(value, out);
        }
        Ty::Future(value, error, _) => {
            collect_type_var_names(value, out);
            collect_type_var_names(error, out);
        }
        Ty::Union(tys, _) | Ty::Class(_, tys, _) => {
            for t in tys {
                collect_type_var_names(t, out);
            }
        }
        Ty::Interface(_, args, bindings, _) => {
            for t in args {
                collect_type_var_names(t, out);
            }
            for (_, t) in bindings {
                collect_type_var_names(t, out);
            }
        }
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            for p in params {
                collect_type_var_names(&p.ty, out);
            }
            collect_type_var_names(ret, out);
            collect_type_var_names(throws, out);
        }
        Ty::AssociatedTypeProjection { base, .. } => collect_type_var_names(base, out),
        _ => {}
    }
}

/// A method spec's generic parameters paired with their lowered interface-bound *conjunction*
/// (declaration order), for comparing an override's bounds against the interface method's. A
/// bound is a conjunction (`T extends A & B`), so each param maps to a list of interfaces; each
/// lowers in `scope_generics` (the enclosing + method type variables). A non-interface /
/// unresolved conjunct (already diagnosed at its own declaration) is dropped.
fn method_generic_bound_interfaces(
    db: &dyn crate::Db,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    ns: &[Name],
    scope_generics: &[ParamTy],
    spec: &crate::builder::interface_resolution::InterfaceMethodSpec,
) -> Vec<(Name, Vec<baml_type::Interface>)> {
    let empty = crate::lower_type_expr::TypeVarBoundsMap::default();
    spec.generic_bounds()
        .iter()
        .map(|(name, bound_ids)| {
            let conjunction = bound_ids
                .iter()
                .filter_map(|&id| {
                    let mut d = Vec::new();
                    crate::lower_type_expr::lower_type_ref(
                        spec.bound_store(),
                        id,
                        &crate::lower_type_expr::ScopeCtx {
                            db,
                            package_items: pkg_items,
                            ns_context: ns,
                            generic_params: scope_generics,
                            bounds: &empty,
                            self_ty: None,
                        },
                        &mut d,
                    )
                    .as_interface()
                })
                .collect();
            (name.clone(), conjunction)
        })
        .collect()
}

/// The RFC-2451 covered-rule outcome for an out-of-body impl.
enum OrphanOutcome {
    Ok,
    UncoveredParam(Name),
    NoLocalType,
}

/// RFC-2451 "covered" rule (BEP-044): an out-of-body `implement<..> I<args..> for T` of a foreign
/// interface is allowed only if — scanning `[T, args..]` left to right — a type local to
/// `current_package` appears before any *uncovered* type parameter (a bare `TypeVar` root).
/// Implementing your own interface is always allowed. Non-local constructors are opaque (their
/// args don't participate); associated bindings are outputs, excluded.
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

/// Realize an interface-scoped type (a method signature or a `requires`-clause interface)
/// through `lower`, treating `Self` as a *rigid type variable* bound to `self_bound` — the
/// interface being implemented — and substituting `Self -> receiver` (plus `bindings`) *last*.
///
/// Unqualified `Self.member` therefore lowers to a symbolic `(Self as I).member` projection —
/// its declaring interface fixed by `I`'s own `requires`-closure, not the receiver's whole
/// impl set — and only collapses to the receiver's realization *after* substitution, where
/// `normalize` reduces `(receiver as I).member` against `receiver`'s impl. Pinning
/// `Self = receiver` *before* lowering (the former shape) instead routed `Self.member` through
/// the concrete receiver's impls, so a receiver implementing several interfaces that each
/// declare `member` made the projection ambiguous — a spurious `Ty::Error` that failed
/// conformance even for a valid impl.
#[expect(clippy::too_many_arguments)]
fn realize_with_symbolic_self<'db>(
    db: &'db dyn crate::Db,
    package_items: &baml_compiler2_hir::package::PackageItems<'db>,
    ns_context: &[Name],
    base_generics: &[ParamTy],
    self_param: &ParamTy,
    base_bounds: &crate::lower_type_expr::TypeVarBoundsMap,
    self_bound: &baml_type::Interface,
    receiver: &Ty,
    bindings: &TypeBindings,
    lower: impl FnOnce(&crate::lower_type_expr::ScopeCtx<'_, 'db>) -> Ty,
) -> Ty {
    // `Self` joins the in-scope generics with the implemented interface as its bound, so a
    // `Self.member` projection resolves the declaring interface through that bound's closure.
    let mut generics = base_generics.to_vec();
    if !generics.contains(self_param) {
        generics.push(self_param.clone());
    }
    let mut bounds = base_bounds.clone();
    bounds.insert(self_param.clone(), vec![self_bound.clone()]);
    let lowered = lower(&crate::lower_type_expr::ScopeCtx {
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

/// Phase-5 signature/type conformance for one `implements` block. Runs strictly downstream of
/// [`impl_data`] + associated-type resolution, so the canonical type algebra is fully
/// determined here: `equivalent`/`is_subtype` may re-enter `impl_data` (via `impls_for_type`)
/// but never re-enter THIS query, so there is no cycle. It validates that field and method
/// *types* match the interface's declared types (E0116 / E0120); the name-based membership
/// checks (which need no algebra) live in [`impl_data`]. check.rs surfaces these alongside
/// `impl_data(loc).diagnostics`.
#[salsa::tracked(returns(ref))]
pub fn validate_impl_signatures<'db>(
    db: &'db dyn crate::Db,
    impl_loc: baml_compiler2_hir::loc::ImplLoc<'db>,
) -> Vec<(crate::infer_context::TirTypeError, ImplDiagnosticLocation)> {
    use baml_compiler2_ppir::item_data::{
        ImplSubjectData, function_data, impl_block_data, interface_data,
    };

    use crate::builder::interface_resolution::InterfaceMethodSpec;

    let mut diags = Vec::new();
    let data = match impl_data(db, impl_loc).as_ref() {
        Ok(data) => data,
        // A cyclic header (`impl_data`'s cycle fallback) can't carry its own diagnostic — re-detect
        // and surface it here so the user's impl doesn't silently vanish.
        Err(ImplDataError::CyclicHeader) => {
            return vec![(
                crate::infer_context::TirTypeError::CyclicImplHeader,
                ImplDiagnosticLocation::ForTarget,
            )];
        }
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

    // The canonical algebra context — fully usable in this phase (see the fn doc).
    let res_ctx = crate::package_interface::package_resolution_context(db, pkg_id);
    let aliases = crate::inference::package_resolved_aliases(db, pkg_id);
    let bounds: crate::lower_type_expr::TypeVarBoundsMap =
        data.generic_params.iter().cloned().collect();
    let ctx = crate::type_context::GlobalTypeContext {
        db,
        res_ctx,
        aliases,
        bounds: &bounds,
    };

    let iface_data = interface_data(db, data.interface);
    let iface_env = crate::generic_env::interface_generic_env(db, data.interface);
    let (iface_self_param, iface_generic_params) = iface_env.interface_param_parts();
    let iface_self_param = iface_self_param.clone();
    let iface_pkg_info =
        baml_compiler2_hir::file_package::file_package(db, data.interface.file(db));
    let iface_pkg_items =
        baml_compiler2_ppir::package_items(db, PackageId::new(db, iface_pkg_info.package.clone()));

    // ── E0116: field-type conformance (in-body impls; out-of-body field impls are E0126). ──
    if !iface_data.fields.is_empty()
        && matches!(data.origin, InterfaceImplOrigin::InBodyClass { .. })
        && let ImplSubjectData::InClass { class, .. } = &block.subject
    {
        let class_fields = crate::inference::resolve_class_fields(db, *class);
        let iface_field_bounds =
            crate::lower_type_expr::interface_generic_param_bounds(db, data.interface);
        // Realize the interface's declared field types at the impl's interface args.
        let iface_bindings =
            crate::generics::bind_type_vars(iface_generic_params, &data.interface_args);
        // A field type may name `Self.Item` (an associated-type field); realize it symbolically
        // and substitute `Self -> for-type` last, so `(for-type as I).Item` reduces to the impl's
        // binding — exactly as the method-signature conformance below does.
        let self_bound =
            baml_type::Interface::new(iface_qtn.clone(), data.interface_args.clone(), vec![]);
        for iface_field in &iface_data.fields {
            let Some(iface_field_ref) = iface_field.type_ref else {
                continue;
            };
            // The satisfying class field: explicit link, else same name. Absent → E0124 (impl_data).
            let class_field_name = block
                .field_links
                .iter()
                .find(|fl| fl.interface_field == iface_field.name)
                .map_or(&iface_field.name, |fl| &fl.class_field);
            let Some((_, class_field_ty, _)) = class_fields
                .fields
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
                iface_generic_params,
                &iface_self_param,
                iface_field_bounds,
                &self_bound,
                &data.for_ty_pattern,
                &iface_bindings,
                |scope| {
                    crate::lower_type_expr::lower_type_ref(
                        &iface_data.type_refs,
                        iface_field_ref,
                        scope,
                        &mut lower_diags,
                    )
                },
            );
            // Field types are invariant — the class field must be the same type.
            if !baml_type::normalize::equivalent(&declared, class_field_ty, &ctx) {
                diags.push((
                    crate::infer_context::TirTypeError::InterfaceFieldTypeMismatch {
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

    // ── Impl-header gates (out-of-body only; an in-body impl's for-type is the enclosing class,
    // always a valid local subject). ──
    if matches!(data.origin, InterfaceImplOrigin::OutOfBody) {
        // E0138: the for-target must be a single concrete impl subject (alias-expanded).
        if !baml_type::normalize::normalize(&data.for_ty_pattern, &ctx).is_valid_impl_subject() {
            diags.push((
                crate::infer_context::TirTypeError::ImplTargetNotConcrete {
                    target: data.for_ty_pattern.clone(),
                },
                ImplDiagnosticLocation::ForTarget,
            ));
        }
        // E0135: every declared generic param must be determined by the for-type or interface args.
        let mut determined = Vec::new();
        collect_type_var_names(&data.for_ty_pattern, &mut determined);
        for arg in &data.interface_args {
            collect_type_var_names(arg, &mut determined);
        }
        for (name, _) in &data.generic_params {
            if !determined.contains(name) {
                diags.push((
                    crate::infer_context::TirTypeError::UnconstrainedImplTypeParam {
                        name: name.name().clone(),
                    },
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
                crate::infer_context::TirTypeError::ImplViolatesOrphanRule {
                    interface: iface_qtn.clone(),
                    uncovered_param: Some(name),
                },
                ImplDiagnosticLocation::InterfaceTarget,
            )),
            OrphanOutcome::NoLocalType => diags.push((
                crate::infer_context::TirTypeError::ImplViolatesOrphanRule {
                    interface: iface_qtn.clone(),
                    uncovered_param: None,
                },
                ImplDiagnosticLocation::InterfaceTarget,
            )),
        }
    }

    // ── E0120: method-signature conformance. Function subtyping is standard (args/kwargs
    // contravariant, return/throws covariant), so an override conforms iff its lowered
    // `Ty::Function` is a subtype of the interface method's realized one. `Self` = for-type on
    // both sides; the interface's generics bind to `interface_args`. The override's *effective*
    // throws (declared or inferred from its body) comes from `callable_throws`. ──
    let for_ty = data.for_ty_pattern.clone();
    let impl_generic_names: Vec<ParamTy> =
        data.generic_params.iter().map(|(n, _)| n.clone()).collect();
    let iface_bindings =
        crate::generics::bind_type_vars(iface_generic_params, &data.interface_args);
    let iface_bounds = crate::lower_type_expr::interface_generic_param_bounds(db, data.interface);
    // In the interface's own declared signatures (and `requires` clauses), `Self` is a rigid
    // type variable bound to the interface being implemented, realized at the impl's args. Both
    // conformance sides realize `Self.member` symbolically through this bound, then substitute
    // `Self -> for_ty` last (see `realize_with_symbolic_self`).
    let self_bound =
        baml_type::Interface::new(iface_qtn.clone(), data.interface_args.clone(), vec![]);
    let no_bindings = TypeBindings::default();

    for &method_loc in &data.methods {
        let method_name = function_data(db, method_loc).name.clone();

        // A `$rust_io_function` (sys-op) override with method-level generics can't
        // be dispatched through interface (virtual) dispatch — the only way an
        // impl-block method is reached. Virtual dispatch does not reconstruct the
        // synthetic type-argument slots a generic sys-op's glue reads off the
        // stack, so such a call would fail at runtime; reject it at declaration.
        // (A generic sys-op declared directly on a class lowers to a direct
        // `SysOp` instruction, which supplies those slots, so it is fine.)
        if !function_data(db, method_loc).generic_params.is_empty()
            && matches!(
                baml_compiler2_ppir::function_body(db, method_loc).as_ref(),
                baml_compiler2_hir::body::FunctionBody::Builtin(
                    baml_compiler2_ast::BuiltinKind::Io
                )
            )
        {
            diags.push((
                crate::infer_context::TirTypeError::GenericSysOpMethodInInterfaceImpl {
                    interface: iface_qtn.clone(),
                    method: method_name.clone(),
                },
                ImplDiagnosticLocation::Method(method_name.clone()),
            ));
            continue;
        }

        // The interface method this override targets: a required sig or a default's function.
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
        // The override's function type: Self = for-type; impl + method generics in scope. Its
        // declared throws is replaced by the effective (inferred-or-declared) throws.
        let impl_spec = InterfaceMethodSpec::from_default(db, method_loc);
        let impl_scope_generics = crate::generic_env::append_params(
            &impl_generic_names,
            &impl_spec.generic_param_names(),
        );
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
            **throws = crate::callable::callable_throws(db, method_loc).clone();
        }

        // The interface method's function type, realized at `interface_args`.
        let iface_scope_generics = crate::generic_env::append_params(
            iface_generic_params,
            &iface_spec.generic_param_names(),
        );
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
            iface_bounds,
            &self_bound,
            &for_ty,
            &iface_method_bindings,
            |scope| iface_spec.to_function_ty(scope, &mut d),
        );

        if !method_generic_arity_matches
            || !baml_type::normalize::is_subtype(&impl_fn, &iface_fn, &ctx)
        {
            diags.push((
                crate::infer_context::TirTypeError::InterfaceMethodSignatureMismatch {
                    interface: iface_qtn.clone(),
                    method: method_name.clone(),
                    expected: iface_fn,
                    got: impl_fn,
                },
                ImplDiagnosticLocation::Method(method_name.clone()),
            ));
        }

        // An override may not add a generic bound the interface method does not declare — that
        // would reject callers the interface accepts (Rust's E0276). Method generic bounds are
        // not part of the `Ty::Function` compared above, so check them positionally: each
        // override bound must be entailed by the interface method's bound at the same position
        // (equal, or a super-interface that `requires` it).
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
                .map(|bound| {
                    bound.map_tys(|ty| crate::generics::substitute_ty(ty, &iface_method_bindings))
                })
                .collect();
            (name, bounds)
        })
        .collect::<Vec<_>>();
        for (i, (param, impl_conjunction)) in impl_bounds.iter().enumerate() {
            let iface_conjunction = iface_method_bounds
                .get(i)
                .map(|(_, b)| b.as_slice())
                .unwrap_or(&[]);
            // Every conjunct the override requires must be entailed by the interface method's
            // conjunction (equal, or a super-interface that `requires` it); an unentailed one is
            // a stricter requirement.
            for impl_bound in impl_conjunction {
                let entailed = iface_conjunction.iter().any(|iface_bound| {
                    super::carried_bound_satisfies(&ctx, iface_bound, impl_bound)
                        || ctx.interface_requires(iface_bound, impl_bound)
                });
                if !entailed {
                    diags.push((
                        crate::infer_context::TirTypeError::InterfaceMethodAddsGenericBound {
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

    // ── E0125: the for-type must implement each interface the implemented one `requires`.
    // Cycle-safe here — `implements_interface` re-enters `impl_data`, never this phase-5 query. ──
    {
        let mut d = Vec::new();
        for &required_ref in &iface_data.requires {
            // A `requires` clause may project `Self.member` (`requires I<Item = Self.Item>`), so
            // realize it with `Self` bound to the implemented interface and `Self -> for_ty` last.
            // It is a constraint head: the obligation pins only what the clause writes — an
            // unwritten member is the implementor's to choose.
            let required = realize_with_symbolic_self(
                db,
                iface_pkg_items,
                &iface_pkg_info.namespace_path,
                iface_generic_params,
                &iface_self_param,
                iface_bounds,
                &self_bound,
                &for_ty,
                &iface_bindings,
                |scope| {
                    crate::lower_type_expr::lower_constraint_head_type_ref(
                        &iface_data.type_refs,
                        required_ref,
                        scope,
                        &mut d,
                    )
                },
            );
            // Reduce any `Self.member` projection in the realized obligation
            // (`(for_ty as I).X` -> the for-type's binding) so the associated pins below are
            // concrete and `implements_interface` matches them structurally.
            let required = baml_type::normalize::normalize(&required, &ctx);
            let Ty::Interface(qtn, generics, assoc, _) = &required else {
                continue;
            };
            let required_iface = baml_type::Interface {
                name: qtn.clone(),
                generics: generics.clone(),
                associated_types: assoc.clone(),
            };
            if !implements_interface(db, &for_ty, &required_iface, aliases, |a, b| {
                baml_type::normalize::is_subtype(a, b, &ctx)
            }) {
                diags.push((
                    crate::infer_context::TirTypeError::MissingRequiredInterface {
                        interface: iface_qtn.clone(),
                        required: required_iface,
                    },
                    ImplDiagnosticLocation::InterfaceTarget,
                ));
            }
        }
    }

    // ── Associated-type binding bound satisfaction: an explicit `type Name = V` binding must
    // *implement* the interface's declared bound for `Name` (`type Name extends J`) — an implements
    // relation, like a generic bound. Cycle-safe here (the bound check re-enters `impl_data`, never
    // this query). Defaults are the interface's own obligation, checked at its declaration. ──
    {
        let target_iface = baml_type::Interface {
            name: iface_qtn.clone(),
            generics: data.interface_args.clone(),
            associated_types: data.associated_types.clone(),
        };
        for binding in &block.associated_type_bindings {
            // The explicit binding's resolved value; skip unknown bindings (impl_data reports those).
            let Some((_, binding_ty)) = data
                .associated_types
                .iter()
                .find(|(n, _)| *n == binding.name)
            else {
                continue;
            };
            let normalized = baml_type::normalize::normalize(binding_ty, &ctx);
            for bound in crate::builder::associated_projection::associated_type_declared_bound(
                db,
                &target_iface,
                &binding.name,
            ) {
                if !normalized_arg_implements_bound(&ctx, &normalized, &bound) {
                    diags.push((
                        crate::infer_context::TirTypeError::AssociatedTypeBindingViolatesBound {
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
    /// `(interface, type)`, so this is a single id, never a candidate set.
    pub impl_loc: baml_compiler2_hir::loc::ImplLoc<'db>,
    /// The impl's own generic params bound to the realized type arguments — e.g.
    /// `U := int` for `implement<U> Foo for Box<U>` resolved at `Box<int>`. The
    /// method/frame resolution reads these to instantiate the callee.
    pub bindings: TypeBindings,
}

/// Every `implements` block id declared in a package, as stable
/// [`ImplLoc`](baml_compiler2_hir::loc::ImplLoc)s.
/// Uniform over in-body and out-of-body impls (both enumerated by
/// [`file_impls`](baml_compiler2_ppir::item_data::file_impls)). Public so MIR can enumerate
/// a package's impls to rebuild the runtime interface-implementor tables on the L1 substrate.
///
/// Salsa-tracked: this walks *every file in the project* (to find the
/// package's files) and is called from every impl-resolution loop, coherence
/// checking, and type-expression lowering, so an untracked version re-scanned
/// the whole project on each call. The result is a pure function of the
/// project's files and their item trees, so memoizing per package is safe.
#[salsa::tracked(returns(ref))]
pub fn package_impl_locs<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
) -> Vec<baml_compiler2_hir::loc::ImplLoc<'db>> {
    let mut out = Vec::new();
    for file in baml_compiler2_hir::compiler2_all_files(db) {
        let file_pkg = PackageId::new(
            db,
            baml_compiler2_hir::file_package::file_package(db, file).package,
        );
        if file_pkg != pkg_id {
            continue;
        }
        // `file_impls` yields the blocks in source order, so the resolver's
        // "first full match" is reproducible (coherence guarantees ≤1 match, but
        // a stable order keeps a coherence-violating program from resolving
        // arbitrarily).
        out.extend(
            baml_compiler2_ppir::item_data::file_impls(db, file)
                .iter()
                .copied(),
        );
    }
    out
}

/// Recursion budget for verifying generic bounds: a bounded blanket
/// (`implement<T extends Printable> Loud for T`) can itself be satisfied by
/// *another* blanket (`implement<U> Printable for U`), so bound-checking
/// re-enters the resolver. Mirrors the MIR's `BLANKET_BOUND_DEPTH`.
const BLANKET_IMPL_BOUND_DEPTH: u32 = 16;

/// Whether `ty` is a concrete receiver — a value whose runtime type pins a
/// single static impl. A bare blanket `implement<T> I for T` applies ONLY to
/// these; a type-var / interface-existential / union receiver has no single
/// static impl and must dispatch dynamically. Containers count (their element
/// type rides along as a nested arg). Mirrors the MIR concrete-dispatch gate.
///
/// This is a broader question than [`Ty::is_valid_impl_subject`], which asks
/// whether a type may be a *written* impl's for-type: `Future` is excluded there
/// (a top-level `implement I for Future<T>` would bake an undispatchable rule)
/// but is a valid blanket *receiver* here — the blanket's for-template is a
/// wildcard that binds the concrete `Future<…>` at runtime.
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
/// `bindings` — a bound may reference sibling impl params (`T extends
/// Comparable<U>`).
pub(crate) fn substitute_interface(
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
///
/// Coherence (orphan + overlap rules) guarantees at most one impl per realized
/// `(interface, type)`, so this returns `Option`, never a candidate set. From
/// the result, [`ResolvedImpl::impl_loc`]'s [`impl_data`] yields the methods to
/// pick by name. Built only on `impl_data` + the stable `ImplId` substrate.
///
/// Both `requested_iface` and `concrete_ty` must be **fully realized**: no free
/// type variables from an enclosing scope, with all nested type args filled
/// recursively (the "realized types" of `TYPE_SYSTEM.md`). The impl's *own*
/// generic params are what the match binds — an `implement<U> Foo for Box<U>`
/// resolves against a realized `Box<int>`. An unrealized receiver or interface
/// (carrying in-scope type vars) has no single static impl; the caller must
/// emit a runtime virtual call / unresolved-associated-type that the VM
/// resolves once the call's type arguments are known, rather than calling here.
///
/// BAML interfaces are bounds, not inheritance: an impl of a sub-interface never
/// satisfies a requested super-interface, so the interface head must match
/// exactly (the `requires`/`extends` closure governs bound-checking, not
/// dispatch). The request's associated-type pins (`Iterator<Item = int>`) gate
/// the match too — an otherwise-matching impl that resolves an associate
/// differently is rejected.
///
/// A matched impl's generic bounds are then enforced: `implement<T extends
/// Printable> Loud for T` matches `Widget` only if `Widget` itself implements
/// `Printable` (which may come from another blanket impl). And a *bare* blanket
/// (`for T`, the for-type being the param itself) applies only to a concrete
/// receiver — never to an existential or another type-var.
pub fn get_implements_block<'db>(
    db: &'db dyn crate::Db,
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

/// Collect the package of every qualified type name occurring in `ty` — its head
/// and, recursively, every nested type. Used to derive the complete set of
/// packages a legal impl of `(ty, interface)` could live in (see
/// [`implements_interface`]).
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
        Ty::List(inner, _) | Ty::EvolvingList(inner, _) => {
            collect_ty_packages(inner, out);
        }
        Ty::Map { key, value, .. } | Ty::EvolvingMap(key, value, _) | Ty::Future(key, value, _) => {
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
        // No qualified name: primitives, literals, type variables, and sentinels.
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
        | Ty::BuiltinUnknown { .. }
        | Ty::Never { .. }
        | Ty::Unknown { .. }
        | Ty::Error { .. }
        | Ty::Infer { .. } => {}
    }
}

/// [`collect_ty_packages`] for an interface constraint — its head plus every
/// generic argument and associated-type binding.
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

/// Universal (∀) interface membership — the single seam every membership consumer calls.
/// True iff EVERY realized instantiation of `concrete` (rigid vars per their bounds)
/// implements `interface`. Realized→`get_implements_block` (unique by coherence);
/// symbolic→`type_implements_interface`. `concrete` must be non-interface.
/// FROZEN CONTRACT: callers depend only on this signature.
pub fn implements_interface(
    db: &dyn crate::Db,
    concrete: &Ty,
    interface: &baml_type::Interface,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    mut is_subtype: impl FnMut(&Ty, &Ty) -> bool,
) -> bool {
    // The orphan rule (RFC-2451 covered rule) keeps a legal impl of `(concrete,
    // interface)` in the package of some type that *appears in the query* — the
    // interface head, the for-type head, or a covered generic argument. So the
    // complete search is: the package of every qualified name in `concrete` +
    // `interface` (each search additionally expands its own dependency closure).
    // A single guessed root (e.g. only the class's or the interface's package)
    // misses orphan-legal placements like `implement dep.I for LocalEnum` or
    // `implement baml.ops.Add<Meters> for int`.
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
/// [`implements_interface`], walking `impl_data` for a structurally-matching block whose
/// pinned bounds hold. Callers depend only on this signature.
pub fn type_implements_interface<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
    concrete: &Ty,
    interface: &baml_type::Interface,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    mut is_subtype: impl FnMut(&Ty, &Ty) -> bool,
) -> bool {
    // Orphan rule: an impl lives in the package of either the interface or the
    // implementor, so this package + its dependency closure is the complete search.
    // Membership holds iff some impl structurally matches `(concrete, interface)` and its
    // pinned bounds hold. Coherence guarantees ≤1 match for realized inputs; for symbolic
    // inputs there may be several applicable blocks — any one establishes membership.
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

/// Symbolic counterpart of [`get_implements_block`]: the impl block matching a possibly
/// typevar-carrying `(concrete, interface)` — `(Map<T, R> as Iterator)` inside a generic
/// scope — with generic bounds discharged through `is_subtype`, which judges rigid vars
/// against the caller's scope bounds. Unlike membership ([`type_implements_interface`],
/// where any match suffices), the caller reads the match's realized associated-type pins,
/// so a UNIQUE matching block is required: several distinct matches return `None`
/// (fail-closed) rather than guessing.
pub(crate) fn get_implements_block_symbolic<'db>(
    db: &'db dyn crate::Db,
    pkg_id: PackageId<'db>,
    concrete: &Ty,
    interface: &baml_type::Interface,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    mut is_subtype: impl FnMut(&Ty, &Ty) -> bool,
) -> Option<ResolvedImpl<'db>> {
    // Orphan rule: an impl lives in the package of either the interface or the
    // implementor, so this package + its dependency closure is the complete search.
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
                // Coherence guarantees uniqueness only for realized inputs; two blocks
                // structurally matching a symbolic query cannot both realize the pins.
                return None;
            }
            found = Some(ResolvedImpl { impl_loc, bindings });
        }
    }
    found
}

/// [`get_implements_block`] with an explicit recursion budget. Bound
/// verification re-enters this function (a blanket's bound may be satisfied by
/// another blanket), so the budget bounds that recursion.
fn get_implements_block_within_depth<'db>(
    db: &'db dyn crate::Db,
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

    // The orphan rule keeps every impl in the package of either the interface or
    // the implementor type, so this package plus its dependency closure is the
    // complete search space.
    let mut packages = vec![pkg_id];
    packages.extend(baml_compiler2_hir::package::package_dependency_closure(
        db, pkg_id,
    ));

    for pkg in packages {
        for &impl_loc in package_impl_locs(db, pkg) {
            let Ok(data) = impl_data(db, impl_loc).as_ref() else {
                continue;
            };
            // Structural match: exact interface head, the bare-blanket guard, joint
            // for-type + interface-arg unification, and associated-type pins. Inputs are
            // realized here, so the resulting bindings are ground.
            let Some(bindings) = match_impl_head(db, data, concrete_ty, requested_iface, aliases)
            else {
                continue;
            };

            // Generic bounds must hold: a bound `T extends I` is satisfied iff the
            // bound type (`T`'s binding) itself implements `I` — possibly via
            // another blanket impl, hence the bounded re-entry. An *unbound* param
            // (not pinned by this match) has its bounds skipped, mirroring the
            // prior `validate_rule_bounds(require_all_bindings = false)`.
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

/// Structurally match one impl against a requested `(concrete, interface)`: exact
/// interface head + arity, the bare-blanket-only-for-a-concrete-receiver guard, joint
/// for-type + interface-arg unification (a param may appear in either), and the
/// requested associated-type pins. Returns the impl's generic bindings on a structural
/// match; declared generic *bounds* are NOT checked here — the realized resolver
/// discharges them by bounded re-entry, the symbolic ones through `is_subtype`.
///
/// `concrete`/`interface` may carry free type vars (the symbolic case): each binds an
/// impl param or stays free, so an unbound param leaves the match parametric.
fn match_impl_head<'db>(
    db: &'db dyn crate::Db,
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
                    Some((_, impl_ty)) => AliasEquivCtx(aliases)
                        .equivalent(&substitute_ty(impl_ty, &bindings), requested_ty),
                    None => true,
                }
            });
    associated_types_agree.then_some(bindings)
}

/// Whether every declared bound the match *pinned* holds, discharged through
/// `is_subtype` (so it works when a binding still carries free vars). A param the match
/// left free, or a bound still mentioning a free param, is deferred — it becomes an
/// obligation on the eventual call-site instantiation, mirroring `get_implements_block`'s
/// "skip unbound params" rule for the realized case.
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
///
/// Symbolic-capable: `concrete` may carry free vars, and a matched impl's interface (read
/// via `impl_data(impl_loc)`) stays parametric in the impl's still-free params — e.g.
/// `implement<T> Foo<T> for MyType` matches `MyType` with `T` free, i.e. `MyType: Foo<_>`
/// for every `T`. This enumerates the finite set of impl *blocks*, never the (possibly
/// infinite) set of interface instantiations; membership against a *named* interface is
/// [`type_implements_interface`], and the realized unique match is [`get_implements_block`].
///
/// `is_subtype` discharges the impls' generic bounds; new callers pass a
/// `baml_type::normalize`-backed closure rather than the deprecated local oracle.
/// Public so downstream consumers (MIR's open-world dispatch lowering) enumerate
/// impls through the same canonical substrate as the checker.
pub fn impls_for_type<'db>(
    db: &'db dyn crate::Db,
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

/// When `concrete` *almost* implements the interface `requested` via some impl block — the
/// implementor shape (and interface args) match, but a concrete generic bound fails — return
/// the first failing `(param, required_bound_as_ty, actual_arg)`. This turns a bare "type
/// mismatch" into a message naming the unsatisfied bound (BEP-044 wf3 #G18).
///
/// Diagnostic-only: `None` (falling back to a plain mismatch) whenever nothing almost-matches,
/// so precision here never affects soundness. `requested` must be an interface; anything else
/// returns `None`. A still-symbolic bound (typevars survived substitution) is skipped — only a
/// *definite* concrete failure is reported.
pub(crate) fn first_failing_impl_bound<'db>(
    db: &'db dyn crate::Db,
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
            // Bind the impl's params from both the implementor shape (against the value) and
            // the interface args (against the requested interface's args) in one unification
            // pass. A shape or arg mismatch is a genuine non-match, not a bound failure — skip.
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

/// An interface method resolved on a [`ResolvedImpl`] — the function backing it
/// plus the type arguments to instantiate its generic frame. Produced by
/// [`ResolvedImpl::get_method`].
pub struct ResolvedMethod<'db> {
    /// The function providing the implementation: the impl block's own override,
    /// or — when the impl does not override it — the interface's default method.
    pub method: baml_compiler2_hir::loc::FunctionLoc<'db>,
    /// `true` when `method` is the interface's default body rather than an impl
    /// override. The two are referenced differently downstream: a free function
    /// vs. a method dispatched through the interface on its implementor.
    pub from_interface_default: bool,
    /// Type arguments for the callee's generic frame, in frame order: the impl's
    /// own generic params resolved through the impl bindings for an override, or
    /// the realized interface input args for an inherited default.
    pub frame_type_args: Vec<Ty>,
}

impl<'db> ResolvedImpl<'db> {
    /// Resolve `method` to its backing function and frame on this impl. Returns
    /// the impl block's override if it defines one, otherwise *this interface's*
    /// own default method, otherwise `None` (this interface neither declares an
    /// overridable method nor a default body of that name).
    ///
    /// `method` MUST be declared on the resolved interface itself. This does NOT
    /// walk the `requires`/`extends` closure: because BAML interfaces are bounds
    /// (not inheritance), a method inherited from a super-interface `Base` is
    /// dispatched through a *separate* `impl Base for T`, so the caller resolves
    /// `method` to its declaring interface and looks that impl up via
    /// [`get_implements_block`] before calling here — never passing a
    /// sub-/super-interface of the one that declares the method.
    pub fn get_method(&self, db: &'db dyn crate::Db, method: &Name) -> Option<ResolvedMethod<'db>> {
        use baml_compiler2_ppir::item_data::{function_data, interface_data};

        let data = impl_data(db, self.impl_loc).as_ref().ok()?;

        // The impl's own override — a free function, framed by the impl's
        // generic params bound to the realized type arguments. A param that the
        // for-type/interface match left unbound (used only in a bound, say)
        // falls back to the top type, matching the prior resolver.
        for &func_loc in &data.methods {
            if function_data(db, func_loc).name == *method {
                let frame_type_args = data
                    .generic_params
                    .iter()
                    .map(|(name, _)| {
                        self.bindings
                            .get(name)
                            .cloned()
                            .unwrap_or(Ty::BuiltinUnknown {
                                attr: TyAttr::default(),
                            })
                    })
                    .collect();
                return Some(ResolvedMethod {
                    method: func_loc,
                    from_interface_default: false,
                    frame_type_args,
                });
            }
        }

        // The interface's default — framed by the realized interface input args.
        let iface_data = interface_data(db, data.interface);
        for &fn_loc in &iface_data.default_methods {
            if function_data(db, fn_loc).name == *method {
                let frame_type_args = data
                    .interface_args
                    .iter()
                    .map(|arg| substitute_ty(arg, &self.bindings))
                    .collect();
                return Some(ResolvedMethod {
                    method: fn_loc,
                    from_interface_default: true,
                    frame_type_args,
                });
            }
        }
        None
    }

    /// The interface this impl provides at its resolved instantiation: the declared interface
    /// with the impl's [`bindings`](Self::bindings) substituted in — `impl<U> I<U> for Box<U>`
    /// resolved at `Box<int>` yields `I<int>`, and at `Box<T>` (a generic caller) yields
    /// `I<T>`. NOT necessarily typevar-free: a generic caller's *rigid* params survive
    /// realization, so this is an [`Interface`](baml_type::Interface) *constraint*,
    /// deliberately not a `RealizedInterface`.
    pub fn implemented_interface(&self, db: &'db dyn crate::Db) -> baml_type::Interface {
        // A `ResolvedImpl` is only ever constructed for an `impl_data`-Ok impl — every producer
        // (`impls_for_type`, `get_implements_block`) filters with `let Ok(data) = ...` — and an
        // Ok impl resolved its interface target (`InterfaceUnresolved` would be the `Err`), so
        // its loc always has a qualified name. Both branches below are therefore unreachable.
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
        // The membership root set must include every package a covered type appears in:
        // the for-type head, and any nested generic arg / assoc pin. Mirrors the orphan
        // rule's own anchoring — a legal impl lives in the package of one of these.
        let ty = Ty::Class(
            qtn("user", "Box"),
            vec![Ty::Enum(qtn("dep", "Meters"), TyAttr::default())],
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
            vec![Ty::Class(
                qtn("argpkg", "Meters"),
                Vec::new(),
                TyAttr::default(),
            )],
            vec![(
                Name::new("Out"),
                Ty::Enum(qtn("pinpkg", "Unit"), TyAttr::default()),
            )],
        );
        let mut out = Vec::new();
        collect_interface_packages(&iface, &mut out);
        for pkg in ["ifacepkg", "argpkg", "pinpkg"] {
            assert!(out.contains(&Name::new(pkg)), "missing {pkg}, got {out:?}");
        }
    }
}
