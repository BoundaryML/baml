// Re-enable the `deprecated` lint the parent `interfaces` module silences for its own
// internal self-use so the transitional seam body (`type_implements_interface`) still
// flags its single remaining legacy `package_implements_registry` use — the point to
// re-base onto `impl_data`.
#![warn(deprecated)]

use std::collections::HashMap;

use baml_base::{Name, Span, TyAttr};
use baml_compiler2_hir::{contributions::Definition, package::PackageId};
use baml_type::{QualifiedTypeName, Ty};

use crate::{
    generics::{contains_typevar, substitute_ty},
    interfaces::{
        InterfaceImplOrigin, TypeBindings, lower_interface_associated_bindings, match_ty_patterns,
        resolve_path_to_interface,
    },
    lower_type_expr::qualify_def,
    normalize::is_same_normalized_type,
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
    pub generic_params: Vec<(Name, Vec<baml_type::Interface>)>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

/// Spans for an `implements` block, split out of [`ImplData`] for Salsa
/// early-cutoff (semantic resolution must not re-run on whitespace edits).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplDataSourceMap {
    /// The span coherence attributes a conflict to: the interface-target span
    /// for in-body impls, the whole-block span for out-of-body impls — matching
    /// the pre-`impl_data` `InterfaceImplRule::source_span`.
    pub impl_span: Span,
    /// Span of the interface-target type expr (`implement <here> for T`).
    pub interface_target_span: Span,
    /// Span of the for-target type expr; `None` for in-body impls (no written
    /// for-target — the for-type is the synthesized class).
    pub for_target_span: Option<Span>,
}

/// The qualified name of a resolved interface loc (head identity for building a
/// `Ty::Interface`). `None` only if the loc no longer points at an interface.
pub fn interface_loc_qtn<'db>(
    db: &'db dyn crate::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
) -> Option<QualifiedTypeName> {
    let tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
    let data = tree.interfaces.get(&iface_loc.id(db))?;
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
/// into `diags`. `generic_param_names` are the in-scope type-var names so a
/// bound naming a sibling param doesn't read as an unresolved type.
fn lower_generic_param_interface_bounds(
    db: &dyn crate::Db,
    bounds: &[&baml_compiler2_ast::TypeExpr],
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    ns: &[Name],
    generic_param_names: &[Name],
    diags: &mut Vec<crate::infer_context::TirTypeError>,
) -> Vec<baml_type::Interface> {
    let mut ifaces = Vec::new();
    for bound in bounds {
        let ty = crate::lower_type_expr::lower_type_expr(
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
            Ty::Interface(qtn, generics, assoc, _) => {
                // A generic interface used as a bare bound under-instantiates it —
                // a bound cannot infer the missing argument (mirrors the decl-env
                // check in `install_generic_param_bounds`).
                if generics.is_empty()
                    && let Some(arity) =
                        crate::inference::interface_declared_generic_arity(db, &qtn)
                    && arity > 0
                {
                    diags.push(crate::infer_context::TirTypeError::WrongNumberOfTypeArgs {
                        type_name: qtn.name().clone(),
                        expected: arity,
                        got: 0,
                    });
                }
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
    use baml_compiler2_hir::item_tree::ImplSubject;

    let file = impl_loc.file(db);
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    let block = item_tree
        .impls
        .get(&impl_loc.id(db))
        .ok_or(ImplDataError::Malformed)?;

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
        ImplSubject::InClass { class, .. } => {
            let class_data = item_tree
                .classes
                .get(class)
                .ok_or(ImplDataError::Malformed)?;
            let class_loc = baml_compiler2_hir::loc::ClassLoc::new(db, file, *class);
            let class_qtn = qualify_def(db, Definition::Class(class_loc), &class_data.name);
            let for_ty = Ty::Class(
                class_qtn.clone(),
                class_data
                    .generic_params
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
            let generic_params: Vec<(Name, Vec<baml_type::Interface>)> = class_data
                .generic_params
                .iter()
                .zip(class_data.generic_param_bounds.iter())
                .map(|(name, bound)| {
                    let bounds: Vec<&baml_compiler2_ast::TypeExpr> = bound.iter().collect();
                    let ifaces = lower_generic_param_interface_bounds(
                        db,
                        &bounds,
                        pkg_items,
                        ns,
                        &class_data.generic_params,
                        &mut class_bound_diags,
                    );
                    (name.clone(), ifaces)
                })
                .collect();
            (
                class_data.generic_params.clone(),
                for_ty,
                generic_params,
                // In-body impls have no written for-target, and the class owns
                // its bounds' diagnostics — so neither contributes here.
                Vec::new(),
                Vec::new(),
                InterfaceImplOrigin::InBodyClass { class_qtn },
            )
        }
        ImplSubject::Free {
            for_target,
            generics,
        } => {
            let names: Vec<Name> = generics.iter().map(|g| g.name.clone()).collect();
            let mut for_target_diags = Vec::new();
            // The impl generics' bounds are not threaded into this header lowering
            // (a `T.member` type-variable projection in an impl header is a separate
            // gap). A *concrete* projection here (`… for C.Item`) re-enters
            // `impl_data` via `impls_for_type` regardless of bounds; that cycle is
            // caught by `impl_data`'s `cycle_result` (→ `CyclicHeader`), so such
            // headers are illegal rather than a panic.
            let for_ty = crate::lower_type_expr::lower_type_expr(
                for_target,
                &crate::lower_type_expr::ScopeCtx {
                    db,
                    package_items: pkg_items,
                    ns_context: ns,
                    generic_params: &names,
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
                .map(|g| {
                    let bounds: Vec<&baml_compiler2_ast::TypeExpr> = g.bounds.iter().collect();
                    let ifaces = lower_generic_param_interface_bounds(
                        db,
                        &bounds,
                        pkg_items,
                        ns,
                        &names,
                        &mut bound_diags,
                    );
                    (g.name.clone(), ifaces)
                })
                .collect();
            (
                names,
                for_ty,
                generic_params,
                for_target_diags,
                bound_diags,
                InterfaceImplOrigin::OutOfBody,
            )
        }
    };

    let mut interface_target_diags = Vec::new();
    let lowered_interface = crate::lower_type_expr::lower_type_expr(
        &block.interface_target,
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
    let interface_args = if let Ty::Interface(_, args, _, _) = lowered_interface {
        args
    } else {
        Vec::new()
    };

    // Resolve the interface head to its loc *after* lowering, so a bad interface
    // target still surfaces its diagnostics (and the for-target / bound ones).
    // Associated bindings are skipped here — they can't be checked without the
    // interface declaration.
    let Some(iface_loc) = resolve_path_to_interface(db, &block.interface_target, pkg_items, ns)
    else {
        // The head didn't name an interface. The *head* diagnostic (unknown /
        // not-an-interface) belongs to the dedicated implements-target validator,
        // which has a specialized message — so `interface_target_diags` (the
        // generic "unresolved type" lowering error) is dropped here to avoid
        // double-reporting it. Only the for-target and bound diagnostics, which
        // nothing else reports, ride along.
        let diagnostics = for_target_diags
            .into_iter()
            .map(|e| (e, ImplDiagnosticLocation::ForTarget))
            .chain(
                bound_diags
                    .into_iter()
                    .map(|e| (e, ImplDiagnosticLocation::Bound)),
            )
            .collect();
        return Err(ImplDataError::InterfaceUnresolved { diagnostics });
    };
    let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
    let iface_data = iface_tree
        .interfaces
        .get(&iface_loc.id(db))
        .ok_or(ImplDataError::Malformed)?;

    let iface_pkg_info = baml_compiler2_hir::file_package::file_package(db, iface_loc.file(db));
    let iface_pkg_id = PackageId::new(db, iface_pkg_info.package.clone());
    let iface_pkg_items = baml_compiler2_ppir::package_items(db, iface_pkg_id);
    let mut assoc_diags = Vec::new();
    // The impl's own generic bounds, so a `T.member` projection in a binding value
    // (`type Item = T.Elem`) resolves through the impl generic's declared bound.
    let impl_bounds: crate::lower_type_expr::TypeVarBoundsMap =
        generic_params.iter().cloned().collect();
    let associated_types = lower_interface_associated_bindings(
        db,
        iface_data,
        &interface_args,
        &block.associated_type_bindings,
        iface_pkg_items,
        pkg_items,
        &iface_pkg_info.namespace_path,
        ns,
        &generic_param_names,
        &impl_bounds,
        &mut assoc_diags,
    );

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

    let methods = block
        .methods
        .iter()
        .map(|id| baml_compiler2_hir::loc::FunctionLoc::new(db, file, *id))
        .collect();
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
) -> Option<ImplDataSourceMap> {
    use baml_compiler2_hir::item_tree::ImplSubject;

    let file = impl_loc.file(db);
    let file_id = file.file_id(db);
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    let block = item_tree.impls.get(&impl_loc.id(db))?;
    // In-body impls attribute to the interface-target span; out-of-body to the
    // whole block span — matching the prior `InterfaceImplRule::source_span`.
    let (impl_range, for_target_span) = match &block.subject {
        ImplSubject::InClass { .. } => (block.interface_target.span, None),
        ImplSubject::Free { for_target, .. } => {
            (block.span, Some(Span::new(file_id, for_target.span)))
        }
    };
    Some(ImplDataSourceMap {
        impl_span: Span::new(file_id, impl_range),
        interface_target_span: Span::new(file_id, block.interface_target.span),
        for_target_span,
    })
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

/// Every `implements` block id declared in a package, as stable [`ImplLoc`]s.
/// Uniform over in-body and out-of-body impls (both live in
/// [`file_item_tree`](baml_compiler2_hir::file_item_tree)`.impls`).
pub(crate) fn package_impl_locs<'db>(
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
        let tree = baml_compiler2_hir::file_item_tree(db, file);
        // The `impls` map is unordered; iterate in source order so the resolver's
        // "first full match" is reproducible (coherence guarantees ≤1 match, but
        // a stable order keeps a coherence-violating program from resolving
        // arbitrarily).
        let mut file_impls: Vec<_> = tree.impls.iter().collect();
        file_impls.sort_by_key(|(_, block)| block.span.start());
        out.extend(
            file_impls
                .into_iter()
                .map(|(id, _)| baml_compiler2_hir::loc::ImplLoc::new(db, file, *id)),
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
fn substitute_interface(
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
        Ty::List(inner, _) | Ty::EvolvingList(inner, _) | Ty::WatchAccessor(inner, _) => {
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
            if let Some(iface) = interface {
                collect_interface_packages(iface, out);
            }
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

/// Symbolic universal membership — the type-var-bearing backend.
/// MIGRATION SEAM: today via the `InterfaceImplRule` registry; the single point to
/// re-base onto an `impl_data`-driven symbolic resolver. Callers depend only on this sig.
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
        for impl_loc in package_impl_locs(db, pkg) {
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
        for impl_loc in package_impl_locs(db, pkg) {
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
    let param_names: Vec<Name> = data
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
                    Some((_, impl_ty)) => is_same_normalized_type(
                        &substitute_ty(impl_ty, &bindings),
                        requested_ty,
                        aliases,
                    ),
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
pub(crate) fn impls_for_type<'db>(
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
        for impl_loc in package_impl_locs(db, pkg) {
            let Ok(data) = impl_data(db, impl_loc).as_ref() else {
                continue;
            };
            let param_names: Vec<Name> = data
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
        let data = impl_data(db, self.impl_loc).as_ref().ok()?;

        // The impl's own override — a free function, framed by the impl's
        // generic params bound to the realized type arguments. A param that the
        // for-type/interface match left unbound (used only in a bound, say)
        // falls back to the top type, matching the prior resolver.
        for &func_loc in &data.methods {
            let func_tree = baml_compiler2_hir::file_item_tree(db, func_loc.file(db));
            if func_tree[func_loc.id(db)].name == *method {
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
        let iface_tree = baml_compiler2_hir::file_item_tree(db, data.interface.file(db));
        let iface_data = iface_tree.interfaces.get(&data.interface.id(db))?;
        for &fn_id in &iface_data.default_methods {
            if iface_tree[fn_id].name == *method {
                let frame_type_args = data
                    .interface_args
                    .iter()
                    .map(|arg| substitute_ty(arg, &self.bindings))
                    .collect();
                return Some(ResolvedMethod {
                    method: baml_compiler2_hir::loc::FunctionLoc::new(
                        db,
                        data.interface.file(db),
                        fn_id,
                    ),
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
    /// realization, so this is an [`Interface`] *constraint*, deliberately not a
    /// `RealizedInterface`.
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
