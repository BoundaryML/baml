//! Interface resolution substrate (BEP-044): path→interface resolution and identity, the
//! transitive `requires` closure, associated-type binding lowering, and the generic
//! type-pattern matcher used by impl resolution. Relocated from TIR during the S17
//! retirement; the declaration lowering now rides hir_ty's one `LowerCtx` road.
//!
//! Nominal subtyping is decided on the `impl_rules` substrate (`impl_data` /
//! `get_implements_block`), not here: `Class T <: Interface I` iff `T` has an `implements I`
//! block, and interface `A <: B` iff `B` is in `A`'s `requires` closure — there is no
//! shape-matching escape hatch.

mod coherence;
mod impl_rules;

use baml_base::{Literal, Name};
use baml_compiler2_hir::{contributions::Definition, package::PackageId};
use baml_type::normalize::TypeContext as _;
use baml_type::pattern_overlap::TypeVarBoundsMap;
use baml_type::unify::{AliasEquivCtx, TypeBindings, contains_bound_typevar};
use baml_type::{ParamTy, QualifiedTypeName, Ty, TyAttr};
pub use coherence::*;
pub use impl_rules::*;
use rustc_hash::FxHashSet;

use crate::diagnostics::TirTypeError;
use crate::lower::qualify_def;

pub type AssociatedBindings = Vec<(Name, Ty)>;
pub type InterfaceClosureEntry<'db> = (
    baml_compiler2_hir::loc::InterfaceLoc<'db>,
    Vec<Ty>,
    AssociatedBindings,
);
type InterfaceClosureQueueEntry<'db> = (
    baml_compiler2_hir::loc::InterfaceLoc<'db>,
    Vec<Ty>,
    AssociatedBindings,
    FxHashSet<baml_compiler2_hir::loc::InterfaceLoc<'db>>,
);

// ── The one lowering seam ──────────────────────────────────────────────────
//
// TIR's `ScopeCtx` + `lower_type_ref` pair, re-expressed over hir_ty's
// `LowerCtx`: same inputs (items view, namespace, rigid params, plain bounds,
// optional plain `Self`), lowering through the ONE declaration road and
// converting at the boundary. Diagnostics come off the ctx sink in the shared
// vocabulary.

pub(crate) struct LowerScope<'a, 'db> {
    pub db: &'db dyn baml_compiler2_ppir::Db,
    pub package_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    pub ns_context: &'a [Name],
    pub generic_params: &'a [ParamTy],
    pub bounds: &'a TypeVarBoundsMap,
    pub self_ty: Option<Ty>,
}

fn interned_bounds(
    bounds: &TypeVarBoundsMap,
) -> rustc_hash::FxHashMap<ParamTy, Vec<baml_type::interned::InterfaceRef>> {
    bounds
        .iter()
        .map(|(param, ifaces)| {
            (
                param.clone(),
                ifaces
                    .iter()
                    .map(baml_type::interned::InterfaceRef::from_constraint)
                    .collect(),
            )
        })
        .collect()
}

fn scope_ctx<'db>(scope: &LowerScope<'_, 'db>) -> crate::lower::LowerCtx<'db> {
    crate::lower::lower_ctx_for_package(scope.db, scope.package_items, scope.ns_context.to_vec())
        .with_frame(scope.generic_params.to_vec())
        .with_bounds(interned_bounds(scope.bounds))
        .with_self_ty(
            scope
                .self_ty
                .as_ref()
                .map(baml_type::interned::Ty::from_plain),
        )
        .with_diagnostics()
}

pub(crate) fn lower_ref_in(
    scope: &LowerScope<'_, '_>,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    id: baml_compiler2_hir::type_ref::TypeRefId,
    diags: &mut Vec<TirTypeError>,
) -> Ty {
    let ctx = scope_ctx(scope);
    let lowered = ctx.lower_type_ref(store, id).to_plain();
    diags.extend(
        ctx.take_diagnostics()
            .into_iter()
            .map(|diag| crate::lower::lowering_diag_error(&diag.kind)),
    );
    lowered
}

/// The AST twin of [`lower_ref_in`]: the `TypeExpr` lowers through hir's
/// firewall into a scratch store, then the same road.
pub(crate) fn lower_expr_in(
    scope: &LowerScope<'_, '_>,
    expr: &baml_compiler2_ast::TypeExpr,
    diags: &mut Vec<TirTypeError>,
) -> Ty {
    let mut builder = baml_compiler2_hir::type_ref::TypeRefBuilder::new();
    let id = builder.lower(expr);
    let (store, _spans) = builder.finish();
    lower_ref_in(scope, &store, id, diags)
}

// ── Interface generic-frame accessors (TIR's `interface_generic_env`) ──────

/// `Self` — the interface frame's universal slot 0.
pub(crate) fn interface_self_param(
    db: &dyn baml_compiler2_ppir::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'_>,
) -> ParamTy {
    crate::lower::interface_frame(db, iface_loc)
        .first()
        .cloned()
        .expect("interface frame starts with Self")
}

/// Resolve a name against the FULL interface frame (declared params and
/// associated slots), innermost-last-wins.
fn resolve_frame_param(frame: &[ParamTy], name: &Name) -> Option<ParamTy> {
    frame
        .iter()
        .rev()
        .find(|param| param.name() == name)
        .cloned()
}

/// One binding per param mapping it to itself as a rigid var — the identity
/// substitution seed.
fn identity_bindings(generic_params: &[ParamTy]) -> TypeBindings {
    generic_params
        .iter()
        .map(|param| (param.clone(), Ty::TypeVar(param.clone(), TyAttr::default())))
        .collect()
}

pub(crate) fn append_params(parent: &[ParamTy], names: &[Name]) -> Vec<ParamTy> {
    let mut params = parent.to_vec();
    ParamTy::extend_frame(&mut params, names);
    params
}

/// The interface's declared-parameter bounds (`interface I<T extends B>`),
/// keyed by the declared `ParamTy`s — TIR's `interface_generic_param_bounds`.
/// Only interface-shaped bounds contribute; lowering errors are the
/// declaration's own diagnostics, dropped here.
pub(crate) fn interface_declared_param_bounds(
    db: &dyn baml_compiler2_ppir::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'_>,
) -> TypeVarBoundsMap {
    let data = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
    let frame = crate::lower::interface_frame(db, iface_loc);
    let declared = crate::lower::interface_declared_params(db, iface_loc);
    let ctx = crate::lower::lower_ctx_for_file(db, iface_loc.file(db)).with_frame(frame);
    let mut bounds = TypeVarBoundsMap::default();
    for (param, bound) in declared.iter().zip(data.generic_param_bounds.iter()) {
        let Some(id) = bound else { continue };
        if let Some(constraint) = ctx
            .lower_type_ref(&data.type_refs, *id)
            .to_plain()
            .as_interface()
        {
            bounds.insert(param.clone(), vec![constraint]);
        }
    }
    bounds
}

// ── Package-level alias environments (TIR's `package_resolved_aliases` /
// `normalized_alias_map`) ──────────────────────────────────────────────────

/// Every type alias visible to `pkg_id` (its own plus its dependency
/// closure's), resolved to its one-level value through the hir_ty road.
#[salsa::tracked(returns(ref))]
pub fn package_resolved_aliases<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_id: PackageId<'db>,
) -> std::collections::HashMap<QualifiedTypeName, Ty> {
    let mut aliases = std::collections::HashMap::new();
    let mut packages = vec![pkg_id];
    packages.extend(baml_compiler2_hir::package::package_dependency_closure(
        db, pkg_id,
    ));
    for pkg in packages {
        let items = baml_compiler2_ppir::package_items(db, pkg);
        for ns in items.namespaces.values() {
            for (name, def) in &ns.types {
                if let Definition::TypeAlias(loc) = def {
                    aliases
                        .entry(qualify_def(db, Definition::TypeAlias(*loc), name))
                        .or_insert_with(|| crate::lower::type_alias_value(db, *loc).to_plain());
                }
            }
        }
    }
    aliases
}

/// Resolve an enum's full variant-name set (for `nf`'s complete-variant
/// folding), or `None` if `qtn` is not an enum.
pub fn enum_variant_names(
    db: &dyn baml_compiler2_ppir::Db,
    enum_qtn: &QualifiedTypeName,
) -> Option<Vec<Name>> {
    let package_id = PackageId::new(db, enum_qtn.package().clone());
    let items = baml_compiler2_ppir::package_items(db, package_id);
    let Definition::Enum(enum_loc) = items.lookup_type(enum_qtn.namespace(), enum_qtn.name())?
    else {
        return None;
    };
    Some(
        baml_compiler2_ppir::item_data::enum_data(db, enum_loc)
            .variants
            .iter()
            .map(|variant| variant.name.clone())
            .collect(),
    )
}

/// [`package_resolved_aliases`] with every body folded toward the union
/// canonical form the overlap machinery assumes (see `baml_type::unify`).
#[salsa::tracked(returns(ref))]
pub fn normalized_alias_map<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_id: PackageId<'db>,
) -> std::collections::HashMap<QualifiedTypeName, Ty> {
    let mut aliases = package_resolved_aliases(db, pkg_id).clone();
    let enum_variants = |qtn: &QualifiedTypeName| enum_variant_names(db, qtn);
    for body in aliases.values_mut() {
        *body = baml_type::unify::nf(body, &enum_variants);
    }
    aliases
}

// ── Relocated module body ──────────────────────────────────────────────────

struct InterfaceTypeAssocLowering<'a, 'db> {
    db: &'db dyn baml_compiler2_ppir::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    iface: &'a baml_compiler2_ppir::item_data::InterfaceData<'db>,
    interface_args: &'a [Ty],
    explicit_associated_bindings: &'a [baml_compiler2_hir::type_ref::AssociatedTypeBindingRef],
    /// The arena the explicit bindings' `ty` ids index — the *requiring* item's
    /// `type_refs` (the bindings are written at the `requires` site).
    binding_type_refs: &'a baml_compiler2_hir::type_ref::TypeRefStore,
    binding_pkg_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    binding_namespace_path: &'a [Name],
    outer_bindings: &'a TypeBindings,
    /// The requiring interface as a constraint (its associated types pinned to the realized
    /// bindings), so an explicit binding `Item = Self.Item` resolves `Self.Item` onto it —
    /// collapsing to the realized value (or a symbolic projection when unpinned). `None` only if
    /// that interface's qtn can't be resolved.
    self_bound: Option<baml_type::Interface>,
}

/// Where an interface implementation rule was written: in a class body, or out-of-body.
/// Diagnostic metadata ONLY — it MUST NOT drive resolution/dispatch/coherence. A simple
/// `implement I for C` on a concrete class is merged onto `C` for resolution, but is written
/// out-of-body, so its origin is `OutOfBody` (letting out-of-body-only rules like E0126 fire).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterfaceImplOrigin {
    /// `implements I { … }` written in the class body.
    InBodyClass { class_qtn: QualifiedTypeName },
    /// `implement<…> I for <for_target>` — any out-of-body impl (concrete class, generic, or
    /// non-class target).
    OutOfBody,
}

/// An interface declaration resolved from a path: its `InterfaceLoc` plus its
/// fully qualified identity. Produced by [`resolve_path_to_interface_identity`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedInterface<'db> {
    pub loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    pub qtn: QualifiedTypeName,
}

/// Whether `arg` (already [normalized](baml_type::normalize::TypeContext::normalize)) implements
/// the interface `bound`. A bound is an *implements* relation, never the subset `is_subtype`: only
/// concrete types implement interfaces, so a union/existential that passes a subtype check is not
/// an implementor. A concrete type implements `bound` through its impls; a bounded type variable
/// or associated-type projection is filled by a concrete type satisfying its own carried bound, so
/// it satisfies `bound` iff one of those bounds is, or transitively requires, `bound`. An error
/// sentinel is treated as satisfying it (its own diagnostic covers it — no cascade).
pub fn normalized_arg_implements_bound(
    ctx: &impl baml_type::normalize::TypeContext,
    arg: &Ty,
    bound: &baml_type::Interface,
) -> bool {
    let carried_bounds = match arg {
        Ty::Unknown { .. } | Ty::Error { .. } => return true,
        Ty::TypeVar(name, _) => ctx.type_var_bound(name),
        Ty::AssociatedTypeProjection {
            interface, member, ..
        } => ctx.associated_type_bound(interface, member.clone()),
        // A concrete argument implements the bound directly through its impls.
        _ => return ctx.implements_interface(arg, bound),
    };
    carried_bounds.iter().any(|have| {
        carried_bound_satisfies(ctx, have, bound) || ctx.interface_requires(have, bound)
    })
}

/// Whether a bound `have` carried by a type variable (or projection) discharges a
/// required `bound`: same interface, equivalent generics, and `have` pins every
/// associated type `bound` pins (it may pin more, never conflict).
pub(crate) fn carried_bound_satisfies(
    ctx: &impl baml_type::normalize::TypeContext,
    have: &baml_type::Interface,
    bound: &baml_type::Interface,
) -> bool {
    have.name == bound.name
        && have.generics.len() == bound.generics.len()
        && have
            .generics
            .iter()
            .zip(&bound.generics)
            .all(|(h, b)| ctx.equivalent(h, b))
        && bound.associated_types.iter().all(|(bound_name, bound_ty)| {
            have.associated_types.iter().any(|(have_name, have_ty)| {
                have_name == bound_name && ctx.equivalent(have_ty, bound_ty)
            })
        })
}

#[allow(clippy::too_many_arguments)]
fn lower_interface_associated_bindings<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    iface: &baml_compiler2_ppir::item_data::InterfaceData<'db>,
    interface_args: &[Ty],
    self_ty: &Ty,
    // The arena the block bindings' `type_ref` ids index — the impl block's own
    // `type_refs` (the bindings are written in the block's source).
    binding_type_refs: &baml_compiler2_hir::type_ref::TypeRefStore,
    block_associated_bindings: &[baml_compiler2_ppir::item_data::AssociatedTypeBindingData],
    binding_pkg_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    binding_namespace_path: &[Name],
    generic_params: &[ParamTy],
    caller_bounds: &TypeVarBoundsMap,
    diagnostics: &mut Vec<TirTypeError>,
) -> Vec<(Name, Ty)> {
    // A binding value (impl-block source) resolves names in the impl's own scope; an
    // associated-type default (interface source) is lowered once — with a symbolic `Self` —
    // by `interface_associated_type_default` and realized here at the impl's `self_ty`.
    //
    // A later binding references an earlier sibling as `Self.Item` (bare names are banned):
    // `Self` lowers as a rigid type variable whose bound is the interface carrying the
    // *already-resolved* pins, so the projection collapses to the earlier witness at
    // lowering time. A residual symbolic `Self` substitutes to the for-type afterwards.
    let frame = crate::lower::interface_frame(db, iface_loc);
    let self_param = frame
        .first()
        .cloned()
        .expect("interface frame starts with Self");
    let iface_params = crate::lower::interface_declared_params(db, iface_loc);
    let iface_qtn = interface_loc_qtn(db, iface_loc);
    let mut value_scope = generic_params.to_vec();
    value_scope.push(self_param.clone());
    let mut value_bindings: TypeBindings = identity_bindings(generic_params);
    value_bindings.insert(self_param.clone(), self_ty.clone());
    let mut resolved_pins: Vec<(Name, Ty)> = Vec::new();
    let mut default_bindings = baml_type::unify::bind_type_vars(&iface_params, interface_args);

    iface
        .associated_types
        .iter()
        .filter_map(|assoc| {
            let ty = if let Some(binding) = block_associated_bindings
                .iter()
                .find(|binding| binding.name == assoc.name)
                && let Some(type_ref) = binding.type_ref
            {
                let mut bounds = caller_bounds.clone();
                if let Some(qtn) = &iface_qtn {
                    bounds.insert(
                        self_param.clone(),
                        vec![baml_type::Interface::new(
                            qtn.clone(),
                            interface_args.to_vec(),
                            resolved_pins.clone(),
                        )],
                    );
                }
                baml_type::unify::substitute_ty(
                    &lower_ref_in(
                        &LowerScope {
                            db,
                            package_items: binding_pkg_items,
                            ns_context: binding_namespace_path,
                            generic_params: &value_scope,
                            bounds: &bounds,
                            self_ty: Some(Ty::TypeVar(self_param.clone(), TyAttr::default())),
                        },
                        binding_type_refs,
                        type_ref,
                        diagnostics,
                    ),
                    &value_bindings,
                )
            } else {
                // Fill the omitted default at the impl's receiver: `Self` is the for-type, so a
                // Self-referencing default (`type Items = Self.Item[]`) reduces through the impl.
                let (default, _diags) =
                    interface_associated_type_default(db, iface_loc, assoc.name.clone())?;
                let realized = realize_associated_default(
                    &default,
                    &iface_params,
                    interface_args,
                    &self_param,
                    self_ty,
                );
                baml_type::unify::substitute_ty(&realized, &default_bindings)
            };
            resolved_pins.push((assoc.name.clone(), ty.clone()));
            let assoc_param = resolve_frame_param(&frame, &assoc.name)
                .expect("associated type parameter is in its interface frame");
            default_bindings.insert(assoc_param, ty.clone());
            Some((assoc.name.clone(), ty))
        })
        .collect()
}

fn complete_interface_associated_bindings_from_tys<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    iface: &baml_compiler2_ppir::item_data::InterfaceData<'db>,
    interface_args: &[Ty],
    associated_bindings: &[(Name, Ty)],
    // When false, an unbound associated type is left absent rather than filled
    // with its declared default. Callers resolving through a *rigid* `Self` pass
    // false: the eventual implementor may override the default, so `Self.X` must
    // stay a symbolic projection instead of collapsing to the interface's default.
    fill_defaults: bool,
) -> Vec<(Name, Ty)> {
    let frame = crate::lower::interface_frame(db, iface_loc);
    let self_param = frame
        .first()
        .cloned()
        .expect("interface frame starts with Self");
    let iface_params = crate::lower::interface_declared_params(db, iface_loc);
    let mut bindings = baml_type::unify::bind_type_vars(&iface_params, interface_args);
    for (name, ty) in associated_bindings {
        let param = resolve_frame_param(&frame, name)
            .expect("associated type parameter is in its interface frame");
        bindings.insert(param, ty.clone());
    }

    iface
        .associated_types
        .iter()
        .filter_map(|assoc| {
            if let Some((_, ty)) = associated_bindings
                .iter()
                .find(|(name, _)| name == &assoc.name)
            {
                let ty = baml_type::unify::substitute_ty(ty, &bindings);
                let assoc_param = resolve_frame_param(&frame, &assoc.name)
                    .expect("associated type parameter is in its interface frame");
                bindings.insert(assoc_param, ty.clone());
                return Some((assoc.name.clone(), ty));
            }
            if !fill_defaults {
                return None;
            }
            // Fill the default eagerly at this interface realized on the receiver: `Self` is
            // that existential (its generic args plus the bindings resolved so far), so a
            // Self-referencing default (`type Items = Self.Item[]`) reduces against them. The
            // default is lowered once (symbolic `Self`) by the shared query.
            let (default, _diags) =
                interface_associated_type_default(db, iface_loc, assoc.name.clone())?;
            let self_pins: Vec<(Name, Ty)> = iface
                .associated_types
                .iter()
                .filter_map(|assoc| {
                    let param = resolve_frame_param(&frame, &assoc.name)?;
                    bindings
                        .get(&param)
                        .map(|ty| (assoc.name.clone(), ty.clone()))
                })
                .collect();
            let self_ty = Ty::Interface(
                interface_loc_qtn(db, iface_loc)?,
                interface_args.to_vec(),
                self_pins,
                TyAttr::default(),
            );
            let realized = realize_associated_default(
                &default,
                &iface_params,
                interface_args,
                &self_param,
                &self_ty,
            );
            let ty = baml_type::unify::substitute_ty(&realized, &bindings);
            let assoc_param = resolve_frame_param(&frame, &assoc.name)
                .expect("associated type parameter is in its interface frame");
            bindings.insert(assoc_param, ty.clone());
            Some((assoc.name.clone(), ty))
        })
        .collect()
}

/// An associated type's `default` type, lowered ONCE against the interface's own scope.
///
/// The default is lowered with the interface's generic parameters as rigid type variables
/// and a symbolic `Self` — a rigid `Self` type variable bounded by this interface at those
/// parameters — so a Self-referencing default (`type Items = Self.Item[]`) lowers to a
/// projection over that symbolic `Self` instead of erroring for want of a receiver. A
/// referencing site realizes the default by substituting its own `Self` and the interface's
/// actual generic arguments into the returned type (see [`realize_associated_default`]).
///
/// The lowering diagnostics travel with the type so the interface-declaration checker
/// surfaces them exactly once; every referencing site reuses the type and drops the
/// diagnostics. `None` when the associated type has no default.
#[allow(clippy::needless_pass_by_value)]
#[salsa::tracked]
pub fn interface_associated_type_default<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    name: Name,
) -> Option<(Ty, Vec<TirTypeError>)> {
    let iface = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
    let assoc = iface.associated_types.iter().find(|a| a.name == name)?;
    let default = assoc.default?;

    let pkg_info = baml_compiler2_hir::file_package::file_package(db, iface_loc.file(db));
    let pkg_items =
        baml_compiler2_ppir::package_items(db, PackageId::new(db, pkg_info.package.clone()));

    // A symbolic `Self`: a rigid type variable bounded by this interface at its own generic
    // parameters, so a `Self.Other` projection in the default resolves through the bound.
    let source_params = {
        let frame = crate::lower::interface_frame(db, iface_loc);
        let declared_len = crate::lower::interface_declared_params(db, iface_loc).len();
        frame[..1 + declared_len].to_vec()
    };
    let self_param = source_params
        .first()
        .cloned()
        .expect("interface frame starts with Self");
    let generic_params = &source_params[1..];
    let self_constraint = baml_type::Interface::new(
        qualify_def(db, Definition::Interface(iface_loc), &iface.name),
        generic_params
            .iter()
            .map(|p| Ty::TypeVar(p.clone(), TyAttr::default()))
            .collect(),
        Vec::new(),
    );
    let mut bounds = interface_declared_param_bounds(db, iface_loc);
    bounds.insert(self_param.clone(), vec![self_constraint]);

    let mut diagnostics = Vec::new();
    let lowered = lower_ref_in(
        &LowerScope {
            db,
            package_items: pkg_items,
            ns_context: &pkg_info.namespace_path,
            generic_params: &source_params,
            bounds: &bounds,
            self_ty: Some(Ty::TypeVar(self_param, TyAttr::default())),
        },
        &iface.type_refs,
        default,
        &mut diagnostics,
    );
    Some((lowered, diagnostics))
}

/// Realize an interface associated type's default (from [`interface_associated_type_default`])
/// at a concrete receiver: substitute `self_ty` for the symbolic `Self` and `interface_args`
/// for the interface's `generic_params`.
pub fn realize_associated_default(
    default: &Ty,
    generic_params: &[ParamTy],
    interface_args: &[Ty],
    self_param: &ParamTy,
    self_ty: &Ty,
) -> Ty {
    let mut bindings = baml_type::unify::bind_type_vars(generic_params, interface_args);
    bindings.insert(self_param.clone(), self_ty.clone());
    baml_type::unify::substitute_ty(default, &bindings)
}

/// The realized type of a *defaulted* associated `member` for an interface existential
/// `Ty::Interface(qtn, args, …)`, or `None` when `member` is not a defaulted associated
/// type of that interface. A *bound* never fills a default this way — its implementor may
/// override it — which is why this is keyed on the interface-existential base.
pub fn existential_associated_default(
    db: &dyn baml_compiler2_ppir::Db,
    res_ctx: &crate::package_interface::PackageResolutionContext<'_>,
    qtn: &QualifiedTypeName,
    args: &[Ty],
    self_ty: &Ty,
    member: &Name,
) -> Option<Ty> {
    let items = res_ctx.items_for_package(db, qtn.package())?;
    let Definition::Interface(iface_loc) = items.lookup_type(qtn.namespace(), qtn.name())? else {
        return None;
    };
    let (default, _diagnostics) = interface_associated_type_default(db, iface_loc, member.clone())?;
    let self_param = interface_self_param(db, iface_loc);
    let iface_params = crate::lower::interface_declared_params(db, iface_loc);
    Some(realize_associated_default(
        &default,
        &iface_params,
        args,
        &self_param,
        self_ty,
    ))
}

fn lower_interface_type_associated_bindings(
    ctx: &InterfaceTypeAssocLowering<'_, '_>,
    diagnostics: &mut Vec<TirTypeError>,
) -> Vec<(Name, Ty)> {
    let frame = crate::lower::interface_frame(ctx.db, ctx.iface_loc);
    let self_param = frame
        .first()
        .cloned()
        .expect("interface frame starts with Self");
    let iface_params = crate::lower::interface_declared_params(ctx.db, ctx.iface_loc);
    let mut bindings = baml_type::unify::bind_type_vars(&iface_params, ctx.interface_args);
    for (param, ty) in ctx.outer_bindings {
        bindings.entry(param.clone()).or_insert_with(|| ty.clone());
    }
    // The interface's declared parameter bounds, so a `T.member` projection in a
    // binding value or default resolves `T`'s declaring interface.
    let iface_bounds = interface_declared_param_bounds(ctx.db, ctx.iface_loc);

    ctx.iface
        .associated_types
        .iter()
        .filter_map(|assoc| {
            if let Some(binding) = ctx
                .explicit_associated_bindings
                .iter()
                .find(|binding| binding.name == assoc.name)
            {
                // The binding value may project `Self.Assoc` onto the requiring interface (a
                // `requires I<Item = Self.Item>` clause), so lower it through a context that
                // resolves `Self`, then substitute the realized generics / associated types.
                let ty = if let Some(self_bound) = &ctx.self_bound {
                    let mut bounds = iface_bounds.clone();
                    bounds.insert(self_param.clone(), vec![self_bound.clone()]);
                    let mut generic_params: Vec<ParamTy> = bindings.keys().cloned().collect();
                    if !generic_params.contains(&self_param) {
                        generic_params.push(self_param.clone());
                    }
                    let scope = LowerScope {
                        db: ctx.db,
                        package_items: ctx.binding_pkg_items,
                        ns_context: ctx.binding_namespace_path,
                        generic_params: &generic_params,
                        bounds: &bounds,
                        self_ty: Some(Ty::TypeVar(self_param.clone(), TyAttr::default())),
                    };
                    baml_type::unify::substitute_ty(
                        &lower_ref_in(&scope, ctx.binding_type_refs, binding.ty, diagnostics),
                        &bindings,
                    )
                } else {
                    let generic_params: Vec<_> = bindings.keys().cloned().collect();
                    baml_type::unify::substitute_ty(
                        &lower_ref_in(
                            &LowerScope {
                                db: ctx.db,
                                package_items: ctx.binding_pkg_items,
                                ns_context: ctx.binding_namespace_path,
                                generic_params: &generic_params,
                                bounds: &iface_bounds,
                                self_ty: None,
                            },
                            ctx.binding_type_refs,
                            binding.ty,
                            diagnostics,
                        ),
                        &bindings,
                    )
                };
                let assoc_param = resolve_frame_param(&frame, &assoc.name)
                    .expect("associated type parameter is in its interface frame");
                bindings.insert(assoc_param, ty.clone());
                return Some((assoc.name.clone(), ty));
            }
            // Fill the omitted default eagerly at this interface realized on the receiver.
            let (default, _diags) =
                interface_associated_type_default(ctx.db, ctx.iface_loc, assoc.name.clone())?;
            let self_pins: Vec<(Name, Ty)> = ctx
                .iface
                .associated_types
                .iter()
                .filter_map(|assoc| {
                    let param = resolve_frame_param(&frame, &assoc.name)?;
                    bindings
                        .get(&param)
                        .map(|ty| (assoc.name.clone(), ty.clone()))
                })
                .collect();
            let self_ty = Ty::Interface(
                interface_loc_qtn(ctx.db, ctx.iface_loc)?,
                ctx.interface_args.to_vec(),
                self_pins,
                TyAttr::default(),
            );
            let realized = realize_associated_default(
                &default,
                &iface_params,
                ctx.interface_args,
                &self_param,
                &self_ty,
            );
            let ty = baml_type::unify::substitute_ty(&realized, &bindings);
            let assoc_param = resolve_frame_param(&frame, &assoc.name)
                .expect("associated type parameter is in its interface frame");
            bindings.insert(assoc_param, ty.clone());
            Some((assoc.name.clone(), ty))
        })
        .collect()
}

/// Match several `(pattern, concrete)` pairs into one consistent set of
/// bindings, threading them across every pair — a `generic_param` that occurs
/// in more than one pattern must unify to the same type in all of them.
pub fn match_ty_patterns(
    pairs: &[(&Ty, &Ty)],
    generic_params: &[ParamTy],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> Option<TypeBindings> {
    let mut bindings = TypeBindings::default();
    for (pattern, concrete) in pairs {
        match_ty_pattern_into(pattern, concrete, generic_params, aliases, &mut bindings)?;
    }
    Some(bindings)
}

/// Incrementally match `pattern` against `concrete`, recording type-variable bindings into
/// `bindings` (params already bound must match consistently).
pub fn match_ty_pattern_into(
    pattern: &Ty,
    concrete: &Ty,
    generic_params: &[ParamTy],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    bindings: &mut TypeBindings,
) -> Option<()> {
    if let Ty::TypeVar(name, _) = pattern
        && generic_params.contains(name)
    {
        return bind_type_var(name, concrete, bindings, aliases);
    }

    if !contains_bound_typevar(pattern, generic_params)
        && AliasEquivCtx(aliases).equivalent(pattern, concrete)
    {
        return Some(());
    }

    // A pattern position whose type variables are all *already bound* is no longer a
    // pattern — substitute the bindings and compare normalized. On mismatch fall
    // through: structural matching may still succeed by re-binding positions to
    // equal values.
    if contains_bound_typevar(pattern, generic_params) {
        let unbound =
            |param: &ParamTy| generic_params.contains(param) && !bindings.contains_key(param);
        if !baml_type_runtime::contains_typevar_where(pattern, &unbound) {
            let substituted = baml_type::unify::substitute_ty(pattern, bindings);
            if AliasEquivCtx(aliases).equivalent(&substituted, concrete) {
                return Some(());
            }
        }
    }

    match (pattern, concrete) {
        (Ty::Class(p_qtn, p_args, _), Ty::Class(c_qtn, c_args, _))
            if p_qtn == c_qtn && p_args.len() == c_args.len() =>
        {
            for (p, c) in p_args.iter().zip(c_args.iter()) {
                match_ty_pattern_into(p, c, generic_params, aliases, bindings)?;
            }
            Some(())
        }
        (Ty::Interface(p_qtn, p_args, p_assoc, _), Ty::Interface(c_qtn, c_args, c_assoc, _))
            if p_qtn == c_qtn && p_args.len() == c_args.len() =>
        {
            for (p, c) in p_args.iter().zip(c_args.iter()) {
                match_ty_pattern_into(p, c, generic_params, aliases, bindings)?;
            }
            for (name, concrete_ty) in c_assoc {
                let (_, pattern_ty) = p_assoc.iter().find(|(p_name, _)| p_name == name)?;
                match_ty_pattern_into(pattern_ty, concrete_ty, generic_params, aliases, bindings)?;
            }
            Some(())
        }
        (Ty::List(p, _), Ty::List(c, _)) | (Ty::EvolvingList(p, _), Ty::EvolvingList(c, _)) => {
            match_ty_pattern_into(p, c, generic_params, aliases, bindings)
        }
        (
            Ty::Map {
                key: pk, value: pv, ..
            },
            Ty::Map {
                key: ck, value: cv, ..
            },
        )
        | (Ty::EvolvingMap(pk, pv, _), Ty::EvolvingMap(ck, cv, _)) => {
            match_ty_pattern_into(pk, ck, generic_params, aliases, bindings)?;
            match_ty_pattern_into(pv, cv, generic_params, aliases, bindings)
        }
        (Ty::Future(pv, pe, _), Ty::Future(cv, ce, _)) => {
            match_ty_pattern_into(pv, cv, generic_params, aliases, bindings)?;
            match_ty_pattern_into(pe, ce, generic_params, aliases, bindings)
        }
        (Ty::Union(p_members, _), Ty::Union(c_members, _))
            if p_members.len() == c_members.len() =>
        {
            match_union_members(p_members, c_members, generic_params, aliases, bindings)
        }
        (Ty::Int { .. }, Ty::Literal(Literal::Int(_), _, _))
        | (Ty::Bigint { .. }, Ty::Literal(Literal::Bigint(_), _, _))
        | (Ty::Float { .. }, Ty::Literal(Literal::Float(_), _, _))
        | (Ty::String { .. }, Ty::Literal(Literal::String(_), _, _))
        | (Ty::Bool { .. }, Ty::Literal(Literal::Bool(_), _, _)) => Some(()),
        // An enum variant is a member of its enum's set, so a `for Side` impl (or a
        // blanket `for T`) applies to a `Side.Left` receiver.
        (Ty::Enum(p_qtn, _), Ty::EnumVariant(c_qtn, _, _)) if p_qtn == c_qtn => Some(()),
        (
            Ty::Function {
                params: p_params,
                ret: p_ret,
                throws: p_throws,
                ..
            },
            Ty::Function {
                params: c_params,
                ret: c_ret,
                throws: c_throws,
                ..
            },
        ) if p_params.len() == c_params.len()
            && p_params
                .iter()
                .zip(c_params.iter())
                .all(|(p, c)| p.mode == c.mode) =>
        {
            // Function values are realized: neither type carries generic binders,
            // so match the param/ret/throws components directly.
            for (p, c) in p_params.iter().zip(c_params.iter()) {
                match_ty_pattern_into(&p.ty, &c.ty, generic_params, aliases, bindings)?;
            }
            match_ty_pattern_into(p_ret, c_ret, generic_params, aliases, bindings)?;
            match_ty_pattern_into(p_throws, c_throws, generic_params, aliases, bindings)
        }
        _ if AliasEquivCtx(aliases).equivalent(pattern, concrete) => Some(()),
        _ => None,
    }
}

fn match_union_members(
    pattern_members: &[Ty],
    concrete_members: &[Ty],
    generic_params: &[ParamTy],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    bindings: &mut TypeBindings,
) -> Option<()> {
    let Some((pattern_head, pattern_tail)) = pattern_members.split_first() else {
        return concrete_members.is_empty().then_some(());
    };

    for idx in 0..concrete_members.len() {
        let mut trial_bindings = bindings.clone();
        if match_ty_pattern_into(
            pattern_head,
            &concrete_members[idx],
            generic_params,
            aliases,
            &mut trial_bindings,
        )
        .is_none()
        {
            continue;
        }

        let remaining = concrete_members
            .iter()
            .enumerate()
            .filter(|(member_idx, _)| *member_idx != idx)
            .map(|(_, member)| member.clone())
            .collect::<Vec<_>>();
        if match_union_members(
            pattern_tail,
            &remaining,
            generic_params,
            aliases,
            &mut trial_bindings,
        )
        .is_some()
        {
            *bindings = trial_bindings;
            return Some(());
        }
    }

    None
}

fn bind_type_var(
    param: &ParamTy,
    concrete: &Ty,
    bindings: &mut TypeBindings,
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> Option<()> {
    match bindings.get(param) {
        Some(existing) if AliasEquivCtx(aliases).equivalent(existing, concrete) => Some(()),
        Some(_) => None,
        None => {
            bindings.insert(param.clone(), concrete.clone());
            Some(())
        }
    }
}

/// Resolve a `TypeExprKind::Path` to an interface declaration and its fully
/// qualified identity. Returns `None` when the path doesn't resolve to an
/// interface. The paths resolved here are `requires` / `implements` targets —
/// constraint heads — and only the identity is consumed.
pub fn resolve_path_to_interface_identity<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    target: &baml_compiler2_ast::TypeExpr,
    pkg_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    current_ns: &[Name],
) -> Option<ResolvedInterface<'db>> {
    let mut diagnostics = Vec::new();
    let ty = lower_expr_in(
        &LowerScope {
            db,
            package_items: pkg_items,
            ns_context: current_ns,
            generic_params: &[],
            bounds: &TypeVarBoundsMap::default(),
            self_ty: None,
        },
        target,
        &mut diagnostics,
    );
    resolved_interface_from_ty(db, ty)
}

/// The `TypeRef`-arena twin of [`resolve_path_to_interface_identity`], for
/// callers holding firewall data rather than an AST node.
pub fn resolve_ref_to_interface_identity<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    target: baml_compiler2_hir::type_ref::TypeRefId,
    pkg_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    current_ns: &[Name],
) -> Option<ResolvedInterface<'db>> {
    let mut diagnostics = Vec::new();
    let ty = lower_ref_in(
        &LowerScope {
            db,
            package_items: pkg_items,
            ns_context: current_ns,
            generic_params: &[],
            bounds: &TypeVarBoundsMap::default(),
            self_ty: None,
        },
        store,
        target,
        &mut diagnostics,
    );
    resolved_interface_from_ty(db, ty)
}

/// Shared tail of the two `resolve_*_to_interface_identity` functions.
fn resolved_interface_from_ty(
    db: &dyn baml_compiler2_ppir::Db,
    ty: Ty,
) -> Option<ResolvedInterface<'_>> {
    let Ty::Interface(qtn, _, _, _) = ty else {
        return None;
    };
    let pkg_id = PackageId::new(db, qtn.package().clone());
    let resolved_pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let Definition::Interface(loc) = resolved_pkg_items.lookup_type(qtn.namespace(), qtn.name())?
    else {
        return None;
    };
    Some(ResolvedInterface { loc, qtn })
}

/// Resolve a `TypeRef`-arena entry to an interface declaration.
pub fn resolve_ref_to_interface<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    target: baml_compiler2_hir::type_ref::TypeRefId,
    pkg_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    current_ns: &[Name],
) -> Option<baml_compiler2_hir::loc::InterfaceLoc<'db>> {
    resolve_ref_to_interface_identity(db, store, target, pkg_items, current_ns)
        .map(|resolved| resolved.loc)
}

/// If `root`'s transitive `requires` graph cycles back to `root`, return the name chain
/// `[root, …, root]` witnessing it; else `None`. The user-facing detector (E0118) for the
/// cycle that [`interface_closure_locs`] skips silently.
pub fn interface_requires_cycle<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    root: baml_compiler2_hir::loc::InterfaceLoc<'db>,
) -> Option<Vec<Name>> {
    let iface_name = |loc: baml_compiler2_hir::loc::InterfaceLoc<'db>| -> Name {
        baml_compiler2_ppir::item_data::interface_data(db, loc)
            .name
            .clone()
    };
    let required_locs =
        |loc: baml_compiler2_hir::loc::InterfaceLoc<'db>| -> Vec<baml_compiler2_hir::loc::InterfaceLoc<'db>> {
            let iface = baml_compiler2_ppir::item_data::interface_data(db, loc);
            let pkg = baml_compiler2_hir::file_package::file_package(db, loc.file(db));
            let pkg_items =
                baml_compiler2_ppir::package_items(db, PackageId::new(db, pkg.package.clone()));
            iface
                .requires
                .iter()
                .filter_map(|&p| {
                    resolve_ref_to_interface(db, &iface.type_refs, p, pkg_items, &pkg.namespace_path)
                })
                .collect()
        };
    let root_name = iface_name(root);
    // Frontier item: (interface to expand, the name chain from `root` to it).
    let mut frontier: Vec<(baml_compiler2_hir::loc::InterfaceLoc<'db>, Vec<Name>)> =
        required_locs(root)
            .into_iter()
            .map(|p| (p, vec![root_name.clone(), iface_name(p)]))
            .collect();
    let mut visited: FxHashSet<baml_compiler2_hir::loc::InterfaceLoc<'db>> = FxHashSet::default();
    while let Some((loc, chain)) = frontier.pop() {
        if loc == root {
            return Some(chain);
        }
        if !visited.insert(loc) {
            continue;
        }
        for p in required_locs(loc) {
            let mut next = chain.clone();
            next.push(iface_name(p));
            frontier.push((p, next));
        }
    }
    None
}

/// Walk the transitive `extends` closure of `root_iface` and return every
/// interface in it (including `root_iface` itself), in BFS order. Cycles are
/// skipped silently — they are reported elsewhere (E0118).
pub fn interface_closure_locs<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    root_iface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
) -> Vec<baml_compiler2_hir::loc::InterfaceLoc<'db>> {
    let mut out: Vec<baml_compiler2_hir::loc::InterfaceLoc<'db>> = Vec::new();
    let mut seen: FxHashSet<baml_compiler2_hir::loc::InterfaceLoc<'db>> = FxHashSet::default();
    let mut queue: std::collections::VecDeque<baml_compiler2_hir::loc::InterfaceLoc<'db>> =
        std::collections::VecDeque::new();
    queue.push_back(root_iface);
    while let Some(loc) = queue.pop_front() {
        if !seen.insert(loc) {
            continue;
        }
        out.push(loc);
        let iface = baml_compiler2_ppir::item_data::interface_data(db, loc);
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, loc.file(db));
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let parent_pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
        for &parent in &iface.requires {
            if let Some(parent_loc) = resolve_ref_to_interface(
                db,
                &iface.type_refs,
                parent,
                parent_pkg_items,
                &pkg_info.namespace_path,
            ) {
                queue.push_back(parent_loc);
            }
        }
    }
    out
}

/// Walk the transitive `requires` closure of `root_iface`, carrying generic
/// arguments and associated type bindings for each interface in the closure.
pub fn interface_closure_locs_with_args_and_assoc<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    root_iface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    root_args: &[Ty],
    root_associated_bindings: &[(Name, Ty)],
    // Whether to fill an unbound associated type with its declared default. Callers
    // resolving through a *rigid* `Self` pass false so an overridable associated type
    // stays a symbolic `(Self as I).X` projection.
    fill_associated_defaults: bool,
) -> Vec<InterfaceClosureEntry<'db>> {
    let mut out: Vec<InterfaceClosureEntry<'db>> = Vec::new();
    let mut seen: FxHashSet<InterfaceClosureEntry<'db>> = FxHashSet::default();
    let mut queue: std::collections::VecDeque<InterfaceClosureQueueEntry<'db>> =
        std::collections::VecDeque::new();
    queue.push_back((
        root_iface,
        root_args.to_vec(),
        root_associated_bindings.to_vec(),
        FxHashSet::default(),
    ));

    while let Some((loc, args, associated_bindings, ancestors)) = queue.pop_front() {
        if ancestors.contains(&loc) {
            continue;
        }
        let iface = baml_compiler2_ppir::item_data::interface_data(db, loc);
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, loc.file(db));
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let parent_pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
        let mut diags = Vec::new();
        let associated_bindings = complete_interface_associated_bindings_from_tys(
            db,
            loc,
            iface,
            &args,
            &associated_bindings,
            fill_associated_defaults,
        );
        if !seen.insert((loc, args.clone(), associated_bindings.clone())) {
            continue;
        }
        out.push((loc, args.clone(), associated_bindings.clone()));
        let mut child_ancestors = ancestors.clone();
        child_ancestors.insert(loc);

        let frame = crate::lower::interface_frame(db, loc);
        let iface_params = crate::lower::interface_declared_params(db, loc);
        let mut bindings = baml_type::unify::bind_type_vars(&iface_params, &args);
        for (name, ty) in &associated_bindings {
            let param = resolve_frame_param(&frame, name)
                .expect("associated type parameter is in its interface frame");
            bindings.insert(param, ty.clone());
        }

        // This interface as a constraint (its associated types pinned to the realized
        // bindings) — so a required interface's `Item = Self.Item` resolves `Self.Item` here.
        let self_bound = interface_loc_qtn(db, loc)
            .map(|qtn| baml_type::Interface::new(qtn, args.clone(), associated_bindings.clone()));
        // The requiring interface's declared parameter bounds, so a `T.member`
        // projection in a parent's generic arguments resolves `T`'s declaring
        // interface.
        let iface_bounds = interface_declared_param_bounds(db, loc);

        for &parent in &iface.requires {
            let Some(parent_loc) = resolve_ref_to_interface(
                db,
                &iface.type_refs,
                parent,
                parent_pkg_items,
                &pkg_info.namespace_path,
            ) else {
                continue;
            };
            let parent_args = match &iface.type_refs[parent].kind {
                baml_compiler2_hir::type_ref::TypeRefKind::Path { generic_args, .. } => {
                    let mut arg_diags = Vec::new();
                    generic_args
                        .iter()
                        .map(|&arg| {
                            let generic_params: Vec<_> = bindings.keys().cloned().collect();
                            baml_type::unify::substitute_ty(
                                &lower_ref_in(
                                    &LowerScope {
                                        db,
                                        package_items: parent_pkg_items,
                                        ns_context: &pkg_info.namespace_path,
                                        generic_params: &generic_params,
                                        bounds: &iface_bounds,
                                        self_ty: None,
                                    },
                                    &iface.type_refs,
                                    arg,
                                    &mut arg_diags,
                                ),
                                &bindings,
                            )
                        })
                        .collect()
                }
                _ => Vec::new(),
            };
            let parent_iface = baml_compiler2_ppir::item_data::interface_data(db, parent_loc);
            let (parent_explicit_assoc, parent_binding_ns): (
                &[baml_compiler2_hir::type_ref::AssociatedTypeBindingRef],
                &[Name],
            ) = match &iface.type_refs[parent].kind {
                baml_compiler2_hir::type_ref::TypeRefKind::Path {
                    associated_type_bindings,
                    ..
                } => (associated_type_bindings, &pkg_info.namespace_path),
                _ => (&[][..], &pkg_info.namespace_path),
            };
            let parent_assoc = lower_interface_type_associated_bindings(
                &InterfaceTypeAssocLowering {
                    db,
                    iface_loc: parent_loc,
                    iface: parent_iface,
                    interface_args: &parent_args,
                    explicit_associated_bindings: parent_explicit_assoc,
                    binding_type_refs: &iface.type_refs,
                    binding_pkg_items: parent_pkg_items,
                    binding_namespace_path: parent_binding_ns,
                    outer_bindings: &bindings,
                    self_bound: self_bound.clone(),
                },
                &mut diags,
            );
            queue.push_back((
                parent_loc,
                parent_args,
                parent_assoc,
                child_ancestors.clone(),
            ));
        }
    }

    out
}

/// Does interface constraint `sub` transitively (and *properly*) require `sup`?
///
/// Walks `sub`'s `requires` closure instantiated at `sub`'s generic arguments and
/// associated-type pins, and looks for an entry matching `sup` by qualified name,
/// argument list, and every associated-type pin `sup` specifies.
pub fn interface_requires<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    res_ctx: &'db crate::package_interface::PackageResolutionContext<'db>,
    sub: &baml_type::Interface,
    sup: &baml_type::Interface,
    mut equivalent: impl FnMut(&Ty, &Ty) -> bool,
) -> bool {
    if sub.name == sup.name {
        return false;
    }
    let Some(pkg_items) = res_ctx.items_for_package(db, sub.name.package()) else {
        return false;
    };
    let Some(Definition::Interface(sub_loc)) =
        pkg_items.lookup_type(sub.name.namespace(), sub.name.name())
    else {
        return false;
    };
    for (iface_loc, iface_args, iface_assoc) in interface_closure_locs_with_args_and_assoc(
        db,
        sub_loc,
        &sub.generics,
        &sub.associated_types,
        true,
    ) {
        let iface_data = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
        let iface_qtn = qualify_def(db, Definition::Interface(iface_loc), &iface_data.name);
        if iface_qtn == sup.name
            && iface_args.len() == sup.generics.len()
            && iface_args
                .iter()
                .zip(sup.generics.iter())
                .all(|(a, b)| equivalent(a, b))
            && sup.associated_types.iter().all(|(sup_name, sup_ty)| {
                iface_assoc
                    .iter()
                    .find(|(iface_name, _)| iface_name == sup_name)
                    .is_some_and(|(_, iface_ty)| equivalent(iface_ty, sup_ty))
            })
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qtn(namespace: &[&str], name: &str) -> QualifiedTypeName {
        QualifiedTypeName::new(
            Name::new("user"),
            namespace.iter().map(|part| Name::new(*part)).collect(),
            Name::new(name),
        )
    }

    fn class(namespace: &[&str], name: &str, args: Vec<Ty>) -> Ty {
        Ty::Class(qtn(namespace, name), args, TyAttr::default())
    }

    fn interface(name: &str, args: Vec<Ty>) -> Ty {
        Ty::Interface(qtn(&[], name), args, vec![], TyAttr::default())
    }

    fn int() -> Ty {
        Ty::Int {
            attr: TyAttr::default(),
        }
    }

    fn string() -> Ty {
        Ty::String {
            attr: TyAttr::default(),
        }
    }

    fn type_var(name: &str) -> Ty {
        Ty::TypeVar(param(name), TyAttr::default())
    }

    fn param(name: &str) -> ParamTy {
        ParamTy::new(0, Name::new(name))
    }

    #[test]
    fn match_ty_pattern_rejects_repeated_type_var_conflict() {
        let pattern = class(&[], "Pair", vec![type_var("T"), type_var("T")]);
        let good = class(&[], "Pair", vec![int(), int()]);
        let bad = class(&[], "Pair", vec![int(), string()]);
        let params = vec![param("T")];

        assert!(
            match_ty_patterns(
                &[(&pattern, &good)],
                &params,
                &std::collections::HashMap::default()
            )
            .is_some()
        );
        assert!(
            match_ty_patterns(
                &[(&pattern, &bad)],
                &params,
                &std::collections::HashMap::default()
            )
            .is_none()
        );
    }

    #[test]
    fn match_ty_pattern_matches_enum_variant_against_enum() {
        let side = Ty::Enum(qtn(&[], "Side"), TyAttr::default());
        let side_left = Ty::EnumVariant(qtn(&[], "Side"), Name::new("Left"), TyAttr::default());
        let other = Ty::EnumVariant(qtn(&[], "Coin"), Name::new("Heads"), TyAttr::default());
        let aliases = std::collections::HashMap::default();

        assert!(
            match_ty_patterns(&[(&side, &side_left)], &[], &aliases).is_some(),
            "`Side.Left` should match a `for Side` pattern",
        );
        assert!(
            match_ty_patterns(&[(&side, &other)], &[], &aliases).is_none(),
            "a variant of a *different* enum must not match",
        );
    }

    #[test]
    fn match_ty_pattern_handles_nested_interface_args() {
        let pattern = interface(
            "Container",
            vec![Ty::List(Box::new(type_var("T")), TyAttr::default())],
        );
        let actual = interface(
            "Container",
            vec![Ty::List(Box::new(int()), TyAttr::default())],
        );
        let params = vec![param("T")];

        let bindings = match_ty_patterns(
            &[(&pattern, &actual)],
            &params,
            &std::collections::HashMap::default(),
        )
        .expect("nested list arg should bind T");
        assert_eq!(bindings.get(&param("T")), Some(&int()));
    }

    #[test]
    fn contains_bound_typevar_checks_interface_associated_bindings() {
        let ty = Ty::Interface(
            qtn(&[], "Source"),
            vec![],
            vec![(
                Name::new("Item"),
                Ty::List(Box::new(type_var("T")), TyAttr::default()),
            )],
            TyAttr::default(),
        );

        assert!(contains_bound_typevar(&ty, &[param("T")]));
        assert!(!contains_bound_typevar(&ty, &[param("U")]));
    }

    #[test]
    fn match_ty_pattern_uses_full_qualified_type_names() {
        let pattern = class(&["alpha"], "Thing", vec![]);
        let same_short_name = class(&["beta"], "Thing", vec![]);

        assert!(
            match_ty_patterns(
                &[(&pattern, &same_short_name)],
                &[],
                &std::collections::HashMap::default()
            )
            .is_none(),
            "same short name in different namespaces must not match"
        );
    }

    #[test]
    fn match_ty_pattern_unions_are_order_insensitive_with_bindings() {
        let pattern = Ty::Union(vec![type_var("T"), string()], TyAttr::default());
        let actual = Ty::Union(vec![string(), int()], TyAttr::default());
        let params = vec![param("T")];

        let bindings = match_ty_patterns(
            &[(&pattern, &actual)],
            &params,
            &std::collections::HashMap::default(),
        )
        .expect("union members should be matched by type, not position");
        assert_eq!(bindings.get(&param("T")), Some(&int()));
    }
}
