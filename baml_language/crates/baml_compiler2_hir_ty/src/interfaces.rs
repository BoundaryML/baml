//! Interface resolution substrate (BEP-044): path→interface resolution and identity, the
//! transitive `requires` closure, associated-type binding lowering, and the generic
//! type-pattern matcher used by impl resolution. Relocated from TIR during the S17
//! retirement; the declaration lowering now rides `hir_ty`'s one `LowerCtx` road.
//!
//! Nominal subtyping is decided on the `impl_rules` substrate (`impl_data` /
//! `get_implements_block`), not here: `Class T <: Interface I` iff `T` has an `implements I`
//! block, and interface `A <: B` iff `B` is in `A`'s `requires` closure — there is no
//! shape-matching escape hatch.

mod coherence;
mod impl_rules;

use baml_base::{Literal, Name};
use baml_compiler2_hir::{contributions::Definition, package::PackageId};
use baml_type::{
    ParamTy, QualifiedTypeName, Ty, TyAttr,
    normalize::TypeContext as _,
    pattern_overlap::TypeVarBoundsMap,
    unify::{AliasEquivCtx, TypeBindings, contains_bound_typevar},
};
pub use coherence::*;
pub use impl_rules::*;
use rustc_hash::FxHashSet;

use crate::{diagnostics::TirTypeError, lower::qualify_def};

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
) -> rustc_hash::FxHashMap<ParamTy, Vec<baml_type::interned::InferInterface>> {
    bounds
        .iter()
        .map(|(param, ifaces)| {
            (
                param.clone(),
                ifaces
                    .iter()
                    .map(baml_type::interned::InferInterface::from_constraint)
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
}

pub(crate) fn lower_ref_in(
    scope: &LowerScope<'_, '_>,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    id: baml_compiler2_hir::type_ref::TypeRefId,
    diags: &mut Vec<TirTypeError>,
) -> Ty {
    lower_ref_in_at(
        scope,
        store,
        id,
        crate::lower::TypePosition::Existential,
        diags,
    )
}

/// [`lower_ref_in`] at an explicit [`crate::lower::TypePosition`] - for
/// constraint heads (bounds, `implements`/`requires` targets), which pin
/// only what they write.
pub(crate) fn lower_ref_in_at(
    scope: &LowerScope<'_, '_>,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    id: baml_compiler2_hir::type_ref::TypeRefId,
    position: crate::lower::TypePosition,
    diags: &mut Vec<TirTypeError>,
) -> Ty {
    let ctx = scope_ctx(scope);
    let (lowered, lowering_diagnostics) =
        ctx.lower_type_ref_at_with_diagnostics(store, id, position);
    diags.extend(
        lowering_diagnostics
            .into_iter()
            .map(|diag| crate::lower::lowering_diag_error(&diag.kind)),
    );
    crate::lower::reject_holes(&lowered)
}

/// [`lower_expr_in`] at an explicit [`crate::lower::TypePosition`].
pub(crate) fn lower_expr_in_at(
    scope: &LowerScope<'_, '_>,
    expr: &baml_compiler2_ast::TypeExpr,
    position: crate::lower::TypePosition,
    diags: &mut Vec<TirTypeError>,
) -> Ty {
    let mut builder = baml_compiler2_hir::type_ref::TypeRefBuilder::new();
    let id = builder.lower(expr);
    let (store, _spans) = builder.finish();
    lower_ref_in_at(scope, &store, id, position, diags)
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
pub fn interface_declared_param_bounds(
    db: &dyn baml_compiler2_ppir::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'_>,
) -> TypeVarBoundsMap {
    let data = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
    let frame = crate::lower::interface_frame(db, iface_loc);
    let declared = crate::lower::interface_declared_params(db, iface_loc);
    let ctx = crate::lower::lower_ctx_for_file(db, iface_loc.file(db)).with_frame(frame);
    let mut bounds = TypeVarBoundsMap::default();
    for (param, declared) in declared.iter().zip(data.generic_params.iter()) {
        let constraints: Vec<_> = declared
            .bounds
            .iter()
            .filter_map(|&id| {
                crate::lower::reject_holes(&ctx.lower_type_ref_at(
                    &data.type_refs,
                    id,
                    crate::lower::TypePosition::ConstraintHead,
                ))
                .as_interface()
            })
            .collect();
        if !constraints.is_empty() {
            bounds.insert(param.clone(), constraints);
        }
    }
    bounds
}

// ── Package-level alias environments (TIR's `package_resolved_aliases` /
// `normalized_alias_map`) ──────────────────────────────────────────────────

/// Every type alias visible to `pkg_id` (its own plus its dependency
/// closure's), resolved to its one-level value through the `hir_ty` road.
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
                        .or_insert_with(
                            #[expect(
                                deprecated,
                                reason = "pre-D3: materializes interned inference state; moves to the finalized facts layer with the cutover"
                            )]
                            || crate::lower::type_alias_value(db, *loc).to_plain(),
                        );
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
        Ty::Error { .. } => return true,
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
                            interface_args.into(),
                            resolved_pins.clone().into(),
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

pub(crate) fn complete_interface_associated_bindings_from_tys<'db>(
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
                interface_args.into(),
                self_pins.into(),
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

/// Owned ingredients for lowering a type written inside an interface's own
/// declaration: the interface's generic parameters as rigid type variables and
/// a symbolic `Self` — a rigid `Self` type variable bounded by this interface
/// at those parameters — so `Self.member` lowers to a projection over the
/// symbolic `Self` instead of erroring for want of a receiver.
///
/// The one scope recipe behind [`interface_associated_type_default`],
/// [`resolve_interface_fields`], and [`resolve_interface_required_methods`].
struct InterfaceDeclScope<'db> {
    db: &'db dyn baml_compiler2_ppir::Db,
    pkg_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    ns: Vec<Name>,
    /// `Self` followed by the interface's declared generic parameters.
    generics: Vec<ParamTy>,
    /// The interface's declared bounds plus the `Self` bound.
    bounds: TypeVarBoundsMap,
    self_param: ParamTy,
}

impl<'db> InterfaceDeclScope<'db> {
    fn new(
        db: &'db dyn baml_compiler2_ppir::Db,
        iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    ) -> Self {
        let iface = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, iface_loc.file(db));
        let pkg_items =
            baml_compiler2_ppir::package_items(db, PackageId::new(db, pkg_info.package.clone()));

        // `Self` plus the declared params: the frame's universal prefix,
        // without the associated-type slots that follow it.
        let generics = {
            let frame = crate::lower::interface_frame(db, iface_loc);
            let declared_len = crate::lower::interface_declared_params(db, iface_loc).len();
            frame[..=declared_len].to_vec()
        };
        let self_param = generics
            .first()
            .cloned()
            .expect("interface frame starts with Self");
        let self_constraint = baml_type::Interface::new(
            qualify_def(db, Definition::Interface(iface_loc), &iface.name),
            generics[1..]
                .iter()
                .map(|p| Ty::TypeVar(p.clone(), TyAttr::default()))
                .collect(),
            Box::new([]),
        );
        let mut bounds = interface_declared_param_bounds(db, iface_loc);
        bounds.insert(self_param.clone(), vec![self_constraint]);

        Self {
            db,
            pkg_items,
            ns: pkg_info.namespace_path,
            generics,
            bounds,
            self_param,
        }
    }

    /// A lowering scope over this interface's own generics and bounds.
    fn ctx(&self) -> LowerScope<'_, 'db> {
        self.ctx_with(&self.generics, &self.bounds)
    }

    /// A lowering scope with extended generics/bounds (a method's own
    /// parameters appended); both must contain this scope's entries.
    fn ctx_with<'a>(
        &'a self,
        generics: &'a [ParamTy],
        bounds: &'a TypeVarBoundsMap,
    ) -> LowerScope<'a, 'db> {
        LowerScope {
            db: self.db,
            package_items: self.pkg_items,
            ns_context: &self.ns,
            generic_params: generics,
            bounds,
            self_ty: Some(Ty::TypeVar(self.self_param.clone(), TyAttr::default())),
        }
    }
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

    let scope = InterfaceDeclScope::new(db, iface_loc);
    let mut diagnostics = Vec::new();
    let lowered = lower_ref_in(&scope.ctx(), &iface.type_refs, default, &mut diagnostics);
    Some((lowered, diagnostics))
}

/// An interface's declared fields with their types resolved against the
/// interface's own scope (symbolic `Self`, rigid generic parameters). The
/// interface analogue of [`crate::lower::resolve_class_fields`].
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedInterfaceFields {
    /// (field name, resolved type, field-level attributes)
    pub fields: Vec<(Name, Ty, Vec<baml_compiler2_hir::item_tree::Attribute>)>,
    /// Type lowering diagnostics: (error, span of the type annotation).
    pub diagnostics: Vec<(TirTypeError, text_size::TextRange)>,
}

// Safety: contains `Ty` (which has `Name`, a Salsa interned type). Manual
// `Update` impl uses `PartialEq` for early-cutoff.
#[allow(unsafe_code)]
unsafe impl salsa::Update for ResolvedInterfaceFields {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        #[allow(unsafe_code)]
        let old_ref = unsafe { &*old_pointer };
        if *old_ref == new_value {
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

/// Resolve an interface's declared field types in its own scope.
#[salsa::tracked(returns(ref))]
pub fn resolve_interface_fields<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
) -> ResolvedInterfaceFields {
    let iface = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
    let iface_spans = baml_compiler2_ppir::item_data::interface_source_map(db, iface_loc);
    let scope = InterfaceDeclScope::new(db, iface_loc);

    let mut fields = Vec::new();
    let mut diagnostics = Vec::new();
    for field in &iface.fields {
        let mut field_diags = Vec::new();
        let ty = lower_ref_in(
            &scope.ctx(),
            &iface.type_refs,
            field.type_ref,
            &mut field_diags,
        );
        let span = iface_spans.type_refs.span(field.type_ref);
        diagnostics.extend(field_diags.into_iter().map(|d| (d, span)));
        fields.push((field.name.clone(), ty, field.attributes.clone()));
    }

    ResolvedInterfaceFields {
        fields,
        diagnostics,
    }
}

/// A required interface method's declaration-site resolved signature.
///
/// `function_ty` keeps `Self` symbolic — the rigid `Self` type variable
/// bounded by the declaring interface — exactly as written; an impl-site
/// consumer realizes it by substituting a receiver (the conformance checker
/// does this via its own path). A required method with no written `throws`
/// clause carries `throws unknown` in `function_ty` — required signatures
/// have no body to infer from, and the conformance checker shares that
/// convention (see `signature::lower_signature`'s `Missing` slot handling).
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedInterfaceMethod {
    pub name: Name,
    /// The full `Ty::Function` (params with names/modes, return, declared throws).
    pub function_ty: Ty,
    /// The method's own generic parameters with their resolved interface
    /// bounds — the same shape as [`ImplData::generic_params`]. Excludes the
    /// interface's parameters and `Self`.
    pub generic_params: Vec<(ParamTy, Vec<baml_type::Interface>)>,
    /// Span-free lowering diagnostics; the declaration checker surfaces its
    /// own copies, these travel with the surface for completeness.
    pub diagnostics: Vec<TirTypeError>,
}

// Safety: contains `Ty` (which has `Name`, a Salsa interned type). Manual
// `Update` impl uses `PartialEq` for early-cutoff.
#[allow(unsafe_code)]
unsafe impl salsa::Update for ResolvedInterfaceMethod {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        #[allow(unsafe_code)]
        let old_ref = unsafe { &*old_pointer };
        if *old_ref == new_value {
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

/// Resolve every *required* method signature of an interface at its
/// declaration site, in declaration order (parallel to
/// `InterfaceData::required_methods`, whose entries carry the docstrings and
/// whose source map carries the name spans).
///
/// Default methods are ordinary `FunctionLoc`s — their resolved signatures
/// come from [`crate::lower::function_signature`] instead.
#[salsa::tracked(returns(ref))]
pub fn resolve_interface_required_methods<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
) -> Vec<ResolvedInterfaceMethod> {
    let iface = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
    let scope = InterfaceDeclScope::new(db, iface_loc);

    iface
        .required_methods
        .iter()
        .map(|sig| {
            let mut diagnostics = Vec::new();
            let spec = InterfaceMethodSpec::from_required(iface, sig);

            // The method's own generics join the interface's for lowering;
            // its bounds resolve in the *interface* scope (a method-level
            // bound may reference the interface's parameters).
            let method_generics = append_params(&scope.generics, &spec.generic_param_names());
            let own_params = &method_generics[scope.generics.len()..];
            let mut bounds = scope.bounds.clone();
            let mut generic_params = Vec::new();
            for (param, data) in own_params.iter().zip(spec.generic_bounds()) {
                let ifaces = lower_generic_param_interface_bounds(
                    db,
                    spec.bound_store(),
                    &data.bounds,
                    scope.pkg_items,
                    &scope.ns,
                    &method_generics,
                    &mut diagnostics,
                );
                bounds.insert(param.clone(), ifaces.clone());
                generic_params.push((param.clone(), ifaces));
            }

            let function_ty =
                spec.to_function_ty(&scope.ctx_with(&method_generics, &bounds), &mut diagnostics);

            ResolvedInterfaceMethod {
                name: sig.name.clone(),
                function_ty,
                generic_params,
                diagnostics,
            }
        })
        .collect()
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
                ctx.interface_args.into(),
                self_pins.into(),
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
        (Ty::List(p, _), Ty::List(c, _)) => {
            match_ty_pattern_into(p, c, generic_params, aliases, bindings)
        }
        (
            Ty::Map {
                key: pk, value: pv, ..
            },
            Ty::Map {
                key: ck, value: cv, ..
            },
        ) => {
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
    let ty = lower_expr_in_at(
        &LowerScope {
            db,
            package_items: pkg_items,
            ns_context: current_ns,
            generic_params: &[],
            bounds: &TypeVarBoundsMap::default(),
            self_ty: None,
        },
        target,
        crate::lower::TypePosition::ConstraintHead,
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
    let ty = lower_ref_in_at(
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
        crate::lower::TypePosition::ConstraintHead,
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

/// Every interface reachable from `roots` (each root plus its transitive
/// `requires` closure) that declares an associated type named `member`.
///
/// `roots` is a *conjunction* — the bounds of one type variable (`T extends A &
/// B`). Deduplication is shared across all of them, so a declarer two conjuncts
/// both reach through `requires` is counted **once**: that is one declarer, not
/// an ambiguity. Deduplicating per root and pooling afterwards is the bug this
/// exists to prevent.
///
/// Order is stable — roots in the order given, each root's closure in BFS order
/// — so a caller rendering these into a diagnostic gets a deterministic list.
pub fn interfaces_declaring_associated_type<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    roots: impl IntoIterator<Item = baml_compiler2_hir::loc::InterfaceLoc<'db>>,
    member: &Name,
) -> Vec<baml_compiler2_hir::loc::InterfaceLoc<'db>> {
    let mut out = Vec::new();
    let mut seen: FxHashSet<baml_compiler2_hir::loc::InterfaceLoc<'db>> = FxHashSet::default();
    for root in roots {
        for loc in interface_closure_locs(db, root) {
            if !seen.insert(loc) {
                continue;
            }
            if baml_compiler2_ppir::item_data::interface_data(db, loc)
                .associated_types
                .iter()
                .any(|assoc| assoc.name == *member)
            {
                out.push(loc);
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
        let self_bound = interface_loc_qtn(db, loc).map(|qtn| {
            baml_type::Interface::new(qtn, args.clone().into(), associated_bindings.clone().into())
        });
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

// ── Written-type well-formedness: generic-argument bounds ──────────────────
//
// rustc's wfcheck for ADT instantiations in signatures, adapted to the
// declaration walks: every `Class`/`Interface` head inside a WRITTEN type
// must supply arguments satisfying the head's declared bounds. Judged with
// the *implements* relation (never subset `is_subtype`) via
// [`normalized_arg_implements_bound`]; a bounded type variable in the
// enclosing scope discharges through its own carried bounds.

/// Every generic-argument bound violation inside the written type `ty`.
/// `scope_bounds` is the enclosing scope's param env (a `Box<T>` argument
/// that is itself a bounded var judges through it). Aliases expand
/// cycle-guarded; a cyclic alias is its own diagnostic elsewhere.
pub fn type_generic_bound_errors(
    db: &dyn baml_compiler2_ppir::Db,
    scope_bounds: &rustc_hash::FxHashMap<ParamTy, Vec<baml_type::Interface>>,
    ty: &baml_type::LoweringTy,
) -> Vec<TirTypeError> {
    let facts = crate::facts::Facts::with_bounds(db, scope_bounds.clone());
    let mut errors = Vec::new();
    let mut seen_aliases = FxHashSet::default();
    collect_type_generic_bound_errors(db, &facts, ty, &mut seen_aliases, &mut errors);
    errors
}

fn collect_type_generic_bound_errors<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &crate::facts::Facts<'db>,
    ty: &baml_type::LoweringTy,
    seen_aliases: &mut FxHashSet<QualifiedTypeName>,
    errors: &mut Vec<TirTypeError>,
) {
    use baml_type::normalize::TypeContext as _;
    match ty {
        baml_type::LoweringTy::Class(qtn, args, _) => {
            for arg in args {
                collect_type_generic_bound_errors(db, facts, arg, seen_aliases, errors);
            }
            if let Some(Definition::Class(class)) = facts.definition_of(qtn) {
                let params = crate::lower::class_generic_frame(db, class);
                let declared = crate::lower::class_generic_bounds(db, class);
                check_head_args(facts, &params, &declared, args, errors);
            }
        }
        baml_type::LoweringTy::Interface(qtn, args, pins, _) => {
            for arg in args {
                collect_type_generic_bound_errors(db, facts, arg, seen_aliases, errors);
            }
            for (_, pin) in pins {
                collect_type_generic_bound_errors(db, facts, pin, seen_aliases, errors);
            }
            if let Some(Definition::Interface(iface)) = facts.definition_of(qtn) {
                let params = crate::lower::interface_declared_params(db, iface);
                let plain = interface_declared_param_bounds(db, iface);
                let declared: rustc_hash::FxHashMap<
                    ParamTy,
                    Vec<baml_type::interned::InferInterface>,
                > = plain
                    .iter()
                    .map(|(param, bounds)| {
                        (
                            param.clone(),
                            bounds
                                .iter()
                                .map(baml_type::interned::InferInterface::from_constraint)
                                .collect(),
                        )
                    })
                    .collect();
                check_head_args(facts, &params, &declared, args, errors);
            }
        }
        baml_type::LoweringTy::List(inner, _) => {
            collect_type_generic_bound_errors(db, facts, inner, seen_aliases, errors);
        }
        baml_type::LoweringTy::Map { key, value, .. } => {
            collect_type_generic_bound_errors(db, facts, key, seen_aliases, errors);
            collect_type_generic_bound_errors(db, facts, value, seen_aliases, errors);
        }
        baml_type::LoweringTy::Union(members, _) => {
            for member in members {
                collect_type_generic_bound_errors(db, facts, member, seen_aliases, errors);
            }
        }
        baml_type::LoweringTy::Function {
            params,
            ret,
            throws,
            ..
        } => {
            for param in params {
                collect_type_generic_bound_errors(db, facts, &param.ty, seen_aliases, errors);
            }
            collect_type_generic_bound_errors(db, facts, ret, seen_aliases, errors);
            collect_type_generic_bound_errors(db, facts, throws, seen_aliases, errors);
        }
        baml_type::LoweringTy::Future(value, error, _) => {
            collect_type_generic_bound_errors(db, facts, value, seen_aliases, errors);
            collect_type_generic_bound_errors(db, facts, error, seen_aliases, errors);
        }
        baml_type::LoweringTy::TypeAlias(qtn, _) => {
            if !seen_aliases.insert(qtn.clone()) {
                return;
            }
            if let Some(expanded) = facts.alias_def(qtn) {
                // Alias definitions are finalized facts; the walk's lowering
                // vocabulary is the wider member, so the upcast is zero-cost.
                collect_type_generic_bound_errors(
                    db,
                    facts,
                    expanded.as_lowering_ty(),
                    seen_aliases,
                    errors,
                );
            }
            seen_aliases.remove(qtn);
        }
        _ => {}
    }
}

/// One head's arguments against its declared bounds: each conjunct is a
/// separate requirement, reported independently. Bounds may reference
/// sibling params (`class Pair<A, B extends Container<A>>`), so the
/// head's own bindings substitute through them first.
#[expect(
    deprecated,
    reason = "pre-D3: materializes interned inference state; moves to the finalized facts layer with the cutover"
)]
fn check_head_args(
    facts: &crate::facts::Facts<'_>,
    params: &[ParamTy],
    declared: &rustc_hash::FxHashMap<ParamTy, Vec<baml_type::interned::InferInterface>>,
    args: &[baml_type::LoweringTy],
    errors: &mut Vec<TirTypeError>,
) {
    use baml_type::normalize::TypeContext as _;
    if params.is_empty() || args.is_empty() {
        return;
    }
    // A hole-carrying argument is judged after instantiation, not here: only
    // closed arguments are bound-checked, and only they contribute to the
    // sibling-bound substitution environment. (The vacuous pass the retired
    // lossy materialization gave holes, now decided by the narrowing instead
    // of by walking an inert sentinel.)
    let closed: Vec<Option<Ty>> = args.iter().map(|arg| Ty::try_from(arg).ok()).collect();
    let (closed_params, closed_args): (Vec<ParamTy>, Vec<Ty>) = params
        .iter()
        .zip(&closed)
        .filter_map(|(param, arg)| Some((param.clone(), arg.clone()?)))
        .unzip();
    let bindings = baml_type::unify::bind_type_vars(&closed_params, &closed_args);
    for (index, param) in params.iter().enumerate() {
        let Some(actual) = closed.get(index).and_then(Option::as_ref) else {
            continue;
        };
        for bound in declared.get(param).into_iter().flatten() {
            let bound_ty = Ty::Interface(
                bound.name.clone(),
                bound
                    .generics
                    .iter()
                    .map(baml_type::interned::Ty::to_plain)
                    .collect(),
                bound
                    .associated_types
                    .iter()
                    .map(|(name, ty)| (name.clone(), ty.to_plain()))
                    .collect(),
                TyAttr::default(),
            );
            let Some(bound) = baml_type::unify::substitute_ty(&bound_ty, &bindings).as_interface()
            else {
                continue;
            };
            let arg = facts.normalize(actual);
            let admissible = arg.is_concrete()
                || matches!(
                    arg,
                    Ty::TypeVar(..) | Ty::AssociatedTypeProjection { .. } | Ty::Error { .. }
                );
            if !admissible {
                errors.push(TirTypeError::BoundedTypeArgNotConcrete {
                    arg: actual.clone(),
                    bound: Box::new([bound.clone()]),
                });
            } else if !normalized_arg_implements_bound(facts, &arg, &bound) {
                errors.push(TirTypeError::TypeMismatch {
                    expected: bound.to_ty(),
                    got: actual.clone(),
                });
            }
        }
    }
}

// ── Item projection determination (lowering) ───────────────────────────────
//
// TIR's `builder::associated_projection`, on hir_ty's substrate: lowering a
// written `base.member` / `(base as I).member` determines the declaring
// interface so the canonical triple is self-describing. The mechanism is
// fixed by the base's KIND - a type variable searches its bound
// conjunction's `requires`-closures, an interface existential its own, a
// concrete type its visible impls, a chained projection the inner member's
// declared bound. An explicit qualifier narrows the search to its QTN and
// must be proven; when the determined interface already pins `member`, the
// projection collapses to the pin (opportunistic - realization is the
// oracle's job).
//
// Determination is NAMESPACE-BLIND: which interface declares a member is
// fixed by the base, never by what the member is. [`MemberNamespace`]
// therefore parameterizes only the declaration oracle and the result the
// caller builds - rustc's shape, where `(Self type, trait ref, item)` is one
// concept and the item's namespace picks between a `ProjectionTy` and an
// associated-fn `DefId`.

/// Which namespace an item projection resolves its member in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberNamespace {
    /// Associated types - `(T as I).Assoc`, written in type position.
    Type,
    /// Fields and methods, `self`-less interface statics included -
    /// `(T as I).item`, written in value position.
    Value,
}

/// What an interface declares under a name in [`MemberNamespace::Value`].
/// The two kinds share the namespace but not the access shape (a field read
/// dispatches virtually on a receiver; a method may be called with `Self`
/// written instead), so the kind rides along with the determination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueMemberKind {
    Field,
    Method,
}

/// What an interface declares under a name, across both namespaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterfaceMemberKind {
    AssociatedType,
    Value(ValueMemberKind),
}

/// The result of lowering a projection: the resolved plain [`Ty`] - the
/// canonical triple when the interface is determined, otherwise `Ty::Error`
/// - plus the diagnostics the caller surfaces.
pub struct ProjectionLowering {
    pub ty: Ty,
    pub diagnostics: Vec<TirTypeError>,
}

/// The outcome of resolving which interface declares a projection's member.
///
/// Public because both namespaces consume it: the type namespace maps it to
/// a `Ty` in [`lower_projection`], the value namespace to a resolved item.
/// The variants carry no namespace-specific wording - each caller phrases
/// its own diagnostics, since "no associated type `X`" and "no method `x`"
/// are the same determination reported two ways.
pub enum Determination {
    Determined(baml_type::Interface),
    Undeclared {
        container: crate::diagnostics::AssocContainer,
    },
    Ambiguous(Vec<baml_type::Interface>),
    SubjectDoesNotImplementQualifier {
        subject: Ty,
        qualifier: baml_type::Interface,
    },
    InvalidBase,
    Poisoned,
}

/// Whether `ty` already carries an upstream error, so a projection over it
/// must not emit a fresh diagnostic.
fn projection_poisoned(ty: &Ty) -> bool {
    matches!(ty, Ty::Error { .. } | Ty::Unknown { .. })
}

/// Determine which interface declares `member` for `base`, in `ns` - the
/// namespace-blind core both item-projection roads share. `scope_bounds` is
/// the enclosing scope's PLAIN param env (a type variable base resolves
/// through it).
///
/// A written `explicit_interface` that is not an interface at all is rejected
/// here, so callers never have to re-check the qualifier: the returned
/// [`Determination::Poisoned`] carries the diagnostic (or none, when the
/// qualifier was already an error type and must not double-report).
pub fn determine_member_interface(
    db: &dyn baml_compiler2_ppir::Db,
    scope_bounds: &rustc_hash::FxHashMap<ParamTy, Vec<baml_type::Interface>>,
    base: &Ty,
    explicit_interface: Option<Ty>,
    member: &Name,
    ns: MemberNamespace,
) -> (Determination, Vec<TirTypeError>) {
    let facts = crate::facts::Facts::with_bounds(db, scope_bounds.clone());
    determine_member_interface_with_facts(db, &facts, base, explicit_interface, member, ns)
}

/// [`determine_member_interface`] against an ALREADY-BUILT fact oracle. A
/// caller that holds one (inference does) reuses it rather than rebuilding
/// the param env per projection.
pub fn determine_member_interface_with_facts(
    db: &dyn baml_compiler2_ppir::Db,
    facts: &crate::facts::Facts<'_>,
    base: &Ty,
    explicit_interface: Option<Ty>,
    member: &Name,
    ns: MemberNamespace,
) -> (Determination, Vec<TirTypeError>) {
    let explicit = match explicit_interface {
        None => None,
        Some(iface_ty) => {
            let iface_ty = projection_expand_aliases(facts, iface_ty);
            match iface_ty.as_interface() {
                Some(interface) => Some(interface),
                None if projection_poisoned(&iface_ty) => {
                    return (Determination::Poisoned, Vec::new());
                }
                None => {
                    return (
                        Determination::Poisoned,
                        vec![TirTypeError::NonInterfaceProjectionQualifier],
                    );
                }
            }
        }
    };
    (
        determine_interface(db, facts, base, explicit, member, ns),
        Vec::new(),
    )
}

/// Lower `base.member` or `(base as explicit).member` to its canonical
/// projection, determining the declaring interface. `scope_bounds` is the
/// enclosing scope's PLAIN param env (a type variable base resolves through
/// it); `pkg` scopes equivalence and alias expansion.
///
/// The [`MemberNamespace::Type`] half of [`determine_member_interface`]: the
/// determination is shared, and only the product - a canonical projection
/// triple, or the interface's own pin for `member` - is namespace-specific.
pub fn lower_projection(
    db: &dyn baml_compiler2_ppir::Db,
    scope_bounds: &rustc_hash::FxHashMap<ParamTy, Vec<baml_type::Interface>>,
    base: Ty,
    explicit_interface: Option<Ty>,
    member: Name,
) -> ProjectionLowering {
    let (determination, mut diagnostics) = determine_member_interface(
        db,
        scope_bounds,
        &base,
        explicit_interface,
        &member,
        MemberNamespace::Type,
    );
    let ty = match determination {
        Determination::Determined(interface) => {
            if let Some((_, pinned)) = interface
                .associated_types
                .iter()
                .find(|(name, _)| *name == member)
            {
                pinned.clone()
            } else {
                Ty::AssociatedTypeProjection {
                    base: Box::new(base),
                    interface: Box::new(interface),
                    member,
                    attr: TyAttr::default(),
                }
            }
        }
        Determination::Undeclared { container } => {
            diagnostics.push(TirTypeError::UnknownAssociatedType { member, container });
            Ty::Error {
                attr: TyAttr::default(),
            }
        }
        Determination::Ambiguous(candidates) => {
            diagnostics.push(TirTypeError::AmbiguousAssociatedTypeProjection {
                member,
                candidates: candidates.into_iter().map(|iface| iface.name).collect(),
            });
            Ty::Error {
                attr: TyAttr::default(),
            }
        }
        Determination::SubjectDoesNotImplementQualifier { subject, qualifier } => {
            diagnostics.push(TirTypeError::TypeDoesNotImplementInterface {
                value_type: subject,
                interface: qualifier.to_ty(),
            });
            Ty::Error {
                attr: TyAttr::default(),
            }
        }
        Determination::InvalidBase | Determination::Poisoned => Ty::Error {
            attr: TyAttr::default(),
        },
    };
    ProjectionLowering { ty, diagnostics }
}

#[expect(
    deprecated,
    reason = "pre-D3: materializes interned inference state; moves to the finalized facts layer with the cutover"
)]
fn determine_interface<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &crate::facts::Facts<'db>,
    base: &Ty,
    explicit: Option<baml_type::Interface>,
    member: &Name,
    ns: MemberNamespace,
) -> Determination {
    use crate::diagnostics::AssocContainer;
    // An explicit qualifier must declare `member` DIRECTLY: `requires` is a
    // bound, not inheritance.
    if let Some(qualifier) = &explicit
        && !interface_declares_member(db, &qualifier.name, member, ns)
    {
        return Determination::Undeclared {
            container: AssocContainer::Interface(qualifier.name.clone()),
        };
    }
    let base = projection_expand_aliases(facts, base.clone());
    match &base {
        Ty::Interface(qtn, args, assoc, _) => {
            let root = baml_type::Interface::new(qtn.clone(), args.clone(), assoc.clone());
            let undeclared = AssocContainer::Interface(root.name.clone());
            resolve_via_roots(
                db,
                facts,
                vec![root],
                explicit,
                member,
                undeclared,
                &base,
                true,
                ns,
            )
        }
        Ty::TypeVar(param, _) => {
            use baml_type::normalize::TypeContext as _;
            let bounds = facts.type_var_bound(param);
            if bounds.is_empty() {
                match explicit {
                    Some(qualifier) => Determination::SubjectDoesNotImplementQualifier {
                        subject: base.clone(),
                        qualifier,
                    },
                    None => Determination::Undeclared {
                        container: AssocContainer::TypeVar(param.name().clone()),
                    },
                }
            } else {
                let undeclared = AssocContainer::Interface(bounds[0].name.clone());
                resolve_via_roots(
                    db, facts, bounds, explicit, member, undeclared, &base, false, ns,
                )
            }
        }
        Ty::Class(..)
        | Ty::Enum(..)
        | Ty::List(..)
        | Ty::Map { .. }
        | Ty::Int { .. }
        | Ty::Bigint { .. }
        | Ty::Float { .. }
        | Ty::String { .. }
        | Ty::Bool { .. }
        | Ty::Null { .. }
        | Ty::Uint8Array { .. }
        | Ty::Media(..)
        | Ty::Literal(..)
        | Ty::EnumVariant(..) => match explicit {
            None => determine_concrete(db, facts, &base, member, ns),
            Some(qualifier) => match concrete_realized_interface(db, &base, &qualifier) {
                Some(realized) => Determination::Determined(realized),
                None => Determination::SubjectDoesNotImplementQualifier {
                    subject: base.clone(),
                    qualifier,
                },
            },
        },
        Ty::AssociatedTypeProjection {
            base: inner_base,
            interface: inner_interface,
            member: inner_member,
            ..
        } => {
            let inner_ref = baml_type::interned::InferInterface::from_constraint(inner_interface);
            let inner_base_interned = baml_type::interned::Ty::from_plain(inner_base);
            let root = crate::impls::realized_assoc_bound(
                db,
                &inner_ref,
                &inner_base_interned,
                inner_member,
            )
            .and_then(|bound| bound.to_plain().as_interface());
            match root {
                Some(root) => {
                    let container = AssocContainer::Interface(root.name.clone());
                    resolve_via_roots(
                        db,
                        facts,
                        vec![root],
                        explicit,
                        member,
                        container,
                        &base,
                        false,
                        ns,
                    )
                }
                None => match explicit {
                    Some(qualifier) => Determination::SubjectDoesNotImplementQualifier {
                        subject: base.clone(),
                        qualifier,
                    },
                    None => Determination::Undeclared {
                        container: AssocContainer::Ty(base.clone()),
                    },
                },
            }
        }
        Ty::Error { .. } | Ty::Unknown { .. } => Determination::Poisoned,
        Ty::TypeAlias(..) => Determination::Poisoned,
        _ => match explicit {
            Some(qualifier) => Determination::SubjectDoesNotImplementQualifier {
                subject: base.clone(),
                qualifier,
            },
            None => Determination::InvalidBase,
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_via_roots<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &crate::facts::Facts<'db>,
    roots: Vec<baml_type::Interface>,
    explicit: Option<baml_type::Interface>,
    member: &Name,
    undeclared: crate::diagnostics::AssocContainer,
    subject: &Ty,
    // An existential root denotes one complete instantiation, so its
    // closure view fills defaulted members; a RIGID root (a bound, a
    // chained projection's declared bound) leaves them unpinned - the
    // implementor may override.
    fill_defaults: bool,
    ns: MemberNamespace,
) -> Determination {
    match explicit {
        None => resolve_through_roots(db, roots, member, undeclared, fill_defaults, ns),
        Some(qualifier) => {
            match realize_qualifier_through_roots(db, facts, &roots, &qualifier, fill_defaults) {
                Some(realized) => Determination::Determined(realized),
                None => Determination::SubjectDoesNotImplementQualifier {
                    subject: subject.clone(),
                    qualifier,
                },
            }
        }
    }
}

fn realize_qualifier_through_roots<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &crate::facts::Facts<'db>,
    roots: &[baml_type::Interface],
    qualifier: &baml_type::Interface,
    fill_defaults: bool,
) -> Option<baml_type::Interface> {
    for root in roots {
        let Some(root_loc) = projection_interface_loc(db, &root.name) else {
            continue;
        };
        for entry in interface_closure_locs_with_args_and_assoc(
            db,
            root_loc,
            &root.generics,
            &root.associated_types,
            fill_defaults,
        ) {
            let (loc, args, assoc) = entry;
            if let Some(qtn) = interface_loc_qtn(db, loc)
                && qtn == qualifier.name
            {
                let candidate = baml_type::Interface::new(qtn, args.into(), assoc.into());
                if written_qualifier_proven_by(facts, qualifier, &candidate) {
                    // The WRITTEN generics ride (they are equivalent to the
                    // candidate's, rigid vars included); the candidate
                    // supplies what the qualifier left unwritten.
                    return Some(baml_type::Interface {
                        name: candidate.name,
                        generics: qualifier.generics.clone(),
                        associated_types: candidate.associated_types,
                    });
                }
            }
        }
    }
    None
}

/// Every written qualifier constraint must be consistent with the
/// realization the roots prove; symbolic positions fail open.
/// Whether a bound-closure `candidate` PROVES the written qualifier: every
/// written position must be equivalent — a rigid type variable is equivalent
/// to itself and to nothing else — and written associated pins must match.
/// Nothing fails open: a symbolic position either proves rigidly or the
/// qualifier is unproven and the caller reports it.
fn written_qualifier_proven_by(
    facts: &crate::facts::Facts<'_>,
    written: &baml_type::Interface,
    candidate: &baml_type::Interface,
) -> bool {
    let equivalent = |a: &Ty, b: &Ty| baml_type::normalize::equivalent(a, b, facts);
    if written.generics.len() != candidate.generics.len() {
        return false;
    }
    if !written
        .generics
        .iter()
        .zip(&candidate.generics)
        .all(|(written, real)| equivalent(written, real))
    {
        return false;
    }
    written.associated_types.iter().all(|(name, written_ty)| {
        candidate
            .associated_types
            .iter()
            .any(|(real_name, real_ty)| real_name == name && equivalent(written_ty, real_ty))
    })
}

/// Unqualified: search every root - a root that declares `member` directly
/// shadows its own closure (the stdlib's `Iterator requires
/// Iterable<Item = Self.Item>` pinning idiom depends on it); declarers
/// dedupe by realized identity across the pool.
#[expect(
    deprecated,
    reason = "pre-D3: materializes interned inference state; moves to the finalized facts layer with the cutover"
)]
fn resolve_through_roots(
    db: &dyn baml_compiler2_ppir::Db,
    roots: Vec<baml_type::Interface>,
    member: &Name,
    undeclared: crate::diagnostics::AssocContainer,
    fill_defaults: bool,
    ns: MemberNamespace,
) -> Determination {
    let mut declarers: Vec<baml_type::Interface> = Vec::new();
    let push = |declarers: &mut Vec<baml_type::Interface>, interface: baml_type::Interface| {
        if !declarers.contains(&interface) {
            declarers.push(interface);
        }
    };
    for root in roots {
        if interface_declares_member(db, &root.name, member, ns) {
            push(&mut declarers, root);
            continue;
        }
        let root_ref = baml_type::interned::InferInterface::from_constraint(&root);
        let subject = root_ref.existential();
        if crate::package_interface::mounted_type_row(db, &root.name).is_some() {
            for inherited in crate::impls::direct_requires_closure(db, &root_ref, &subject, 64) {
                if interface_declares_member(db, &inherited.name, member, ns) {
                    push(
                        &mut declarers,
                        baml_type::Interface {
                            name: inherited.name,
                            generics: inherited
                                .generics
                                .iter()
                                .map(baml_type::interned::Ty::to_plain)
                                .collect(),
                            associated_types: inherited
                                .associated_types
                                .iter()
                                .map(|(name, ty)| (name.clone(), ty.to_plain()))
                                .collect(),
                        },
                    );
                }
            }
            continue;
        }
        let Some(root_loc) = projection_interface_loc(db, &root.name) else {
            continue;
        };
        for (loc, args, assoc) in interface_closure_locs_with_args_and_assoc(
            db,
            root_loc,
            &root.generics,
            &root.associated_types,
            fill_defaults,
        ) {
            if !interface_declares_member_at(db, loc, member, ns) {
                continue;
            }
            let Some(qtn) = interface_loc_qtn(db, loc) else {
                continue;
            };
            push(
                &mut declarers,
                baml_type::Interface::new(qtn, args.into(), assoc.into()),
            );
        }
    }
    match declarers.len() {
        0 => Determination::Undeclared {
            container: undeclared,
        },
        1 => Determination::Determined(declarers.pop().expect("length checked")),
        _ => Determination::Ambiguous(declarers),
    }
}

/// A concrete base's projection through its visible impls, requires-aware
/// root-wins across declarers (the most-derived interface shadows one it
/// transitively requires, mirroring the symbolic road).
#[expect(
    deprecated,
    reason = "pre-D3: materializes interned inference state; moves to the finalized facts layer with the cutover"
)]
fn determine_concrete<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: &crate::facts::Facts<'db>,
    base: &Ty,
    member: &Name,
    ns: MemberNamespace,
) -> Determination {
    use crate::diagnostics::AssocContainer;
    let Some(interned) = crate::impls::try_interned_ty(base) else {
        return Determination::Poisoned;
    };
    let mut declarers: Vec<baml_type::Interface> = Vec::new();
    for resolved in crate::impls::impls_for_type(db, &interned) {
        let view = resolved.implemented_view(db, &interned);
        let interface = baml_type::Interface {
            name: view.name.clone(),
            generics: view
                .generics
                .iter()
                .map(baml_type::interned::Ty::to_plain)
                .collect(),
            associated_types: view
                .associated_types
                .iter()
                .map(|(name, ty)| (name.clone(), ty.to_plain()))
                .collect(),
        };
        if interface_declares_member(db, &interface.name, member, ns)
            && !declarers.contains(&interface)
        {
            declarers.push(interface);
        }
    }
    if declarers.len() > 1 {
        let heads: Vec<baml_type::interned::InferInterface> = declarers
            .iter()
            .map(|iface| {
                baml_type::interned::InferInterface::new(
                    iface.name.clone(),
                    iface
                        .generics
                        .iter()
                        .map(baml_type::interned::Ty::from_plain)
                        .collect(),
                    Box::new([]),
                )
            })
            .collect();
        let keep: Vec<bool> = heads
            .iter()
            .map(|head| {
                !heads.iter().any(|other| {
                    other.name != head.name
                        && crate::impls::interface_requires(db, other, head, &interned, 8)
                })
            })
            .collect();
        declarers = declarers
            .into_iter()
            .zip(keep)
            .filter_map(|(declarer, keep)| keep.then_some(declarer))
            .collect();
    }
    let _ = facts;
    match declarers.len() {
        0 => Determination::Undeclared {
            container: match base {
                Ty::Class(qtn, ..) => AssocContainer::Class(qtn.clone()),
                Ty::Enum(qtn, ..) => AssocContainer::Enum(qtn.clone()),
                _ => AssocContainer::Ty(base.clone()),
            },
        },
        1 => Determination::Determined(declarers.pop().expect("length checked")),
        _ => Determination::Ambiguous(declarers),
    }
}

/// A concrete base's view of the WRITTEN qualifier, proven — not selected.
///
/// The qualifier is a GOAL for the impl oracle: `resolve_impl` matches it
/// with rustc's placeholder discipline, so a rigid type variable in a written
/// argument unifies only with an impl's own parameter (a blanket
/// `implements<U> Conv<U> for Multi` proves `(Multi as Conv<T>)`), never with
/// ground structure (`implements Conv<int> for Multi` does NOT prove it — the
/// claim must hold for every possible `T`). Written associated-type pins are
/// enforced by the same match, fail-closed.
///
/// The returned interface carries the WRITTEN generics — symbolic arguments
/// stay symbolic, ride the instantiation frame as their `TypeArgRef` slots,
/// and resolve per realized argument at runtime, exactly as a typevar `Self`
/// does — plus the impl's realization of the associated types the qualifier
/// left unwritten.
#[expect(
    deprecated,
    reason = "pre-D3: materializes interned inference state; moves to the finalized facts layer with the cutover"
)]
fn concrete_realized_interface(
    db: &dyn baml_compiler2_ppir::Db,
    base: &Ty,
    qualifier: &baml_type::Interface,
) -> Option<baml_type::Interface> {
    let interned = crate::impls::try_interned_ty(base)?;
    let goal = baml_type::interned::InferInterface::from_constraint(qualifier);
    let resolved = crate::impls::resolve_impl(db, &interned, &goal)?;
    let view = resolved.implemented_view(db, &interned);
    Some(baml_type::Interface {
        name: view.name.clone(),
        generics: qualifier.generics.clone(),
        associated_types: view
            .associated_types
            .iter()
            .map(|(name, ty)| (name.clone(), ty.to_plain()))
            .collect(),
    })
}

/// Expand a top-level alias chain, bounded against cycles.
fn projection_expand_aliases(facts: &crate::facts::Facts<'_>, mut ty: Ty) -> Ty {
    use baml_type::normalize::TypeContext as _;
    for _ in 0..64 {
        let Ty::TypeAlias(qtn, _) = &ty else {
            return ty;
        };
        match facts.alias_def(qtn) {
            Some(expanded) => ty = expanded,
            None => return ty,
        }
    }
    ty
}

/// Whether `qtn` declares `member` in `ns` — [`interface_declared_kind`]
/// collapsed to a `bool`, for callers that only need existence.
pub fn interface_declares_member(
    db: &dyn baml_compiler2_ppir::Db,
    qtn: &QualifiedTypeName,
    member: &Name,
    ns: MemberNamespace,
) -> bool {
    if let Some(row @ crate::package_interface::ExportedType::Interface { .. }) =
        crate::package_interface::mounted_type_row(db, qtn)
    {
        return mounted_declares_member(row, member, ns);
    }
    projection_interface_loc(db, qtn)
        .is_some_and(|loc| interface_declares_member_at(db, loc, member, ns))
}

fn interface_declares_member_at(
    db: &dyn baml_compiler2_ppir::Db,
    loc: baml_compiler2_hir::loc::InterfaceLoc<'_>,
    member: &Name,
    ns: MemberNamespace,
) -> bool {
    let data = baml_compiler2_ppir::item_data::interface_data(db, loc);
    match ns {
        MemberNamespace::Type => data
            .associated_types
            .iter()
            .any(|assoc| &assoc.name == member),
        MemberNamespace::Value => source_declared_value_kind(db, data, member).is_some(),
    }
}

/// The value-namespace kind an interface declares `name` as, reading a
/// SOURCE interface's span-free data. Fields win the tie because a field and
/// a method cannot share a name (the HIR rejects that at declaration), so the
/// order is a formality that keeps the scan single-pass.
fn source_declared_value_kind(
    db: &dyn baml_compiler2_ppir::Db,
    data: &baml_compiler2_ppir::item_data::InterfaceData<'_>,
    name: &Name,
) -> Option<ValueMemberKind> {
    if data.fields.iter().any(|field| field.name == *name) {
        return Some(ValueMemberKind::Field);
    }
    data.methods
        .iter()
        .any(|&method| baml_compiler2_ppir::item_data::function_data(db, method).name == *name)
        .then_some(ValueMemberKind::Method)
}

/// The same question against a MOUNTED package row, which splits what source
/// keeps in one `methods` list into required and defaulted halves.
fn mounted_declares_member(
    row: &crate::package_interface::ExportedType,
    member: &Name,
    ns: MemberNamespace,
) -> bool {
    mounted_declared_kind(row, member, ns).is_some()
}

/// The mounted-row twin of [`source_declared_value_kind`], generalized over
/// both namespaces so [`interface_declared_kind`] can answer either from one
/// row lookup. Returns `None` for a non-interface row.
fn mounted_declared_kind(
    row: &crate::package_interface::ExportedType,
    member: &Name,
    ns: MemberNamespace,
) -> Option<InterfaceMemberKind> {
    let crate::package_interface::ExportedType::Interface {
        associated_types,
        fields,
        required_methods,
        default_methods,
        ..
    } = row
    else {
        return None;
    };
    match ns {
        MemberNamespace::Type => associated_types
            .iter()
            .any(|assoc| assoc.name == *member)
            .then_some(InterfaceMemberKind::AssociatedType),
        MemberNamespace::Value => {
            if fields.iter().any(|(field, ..)| field == member) {
                return Some(InterfaceMemberKind::Value(ValueMemberKind::Field));
            }
            required_methods
                .iter()
                .chain(default_methods)
                .any(|method| method.name == *member)
                .then_some(InterfaceMemberKind::Value(ValueMemberKind::Method))
        }
    }
}

/// What `qtn` declares under `name` in `ns`, mounted rows and source alike -
/// the one declaration oracle both namespaces read. [`interface_declares_member`]
/// is this collapsed to a `bool`; callers that must tell a field from a method
/// (a virtual field read dispatches differently from a call, and the ambiguity
/// diagnostics word themselves differently) ask here instead.
pub fn interface_declared_kind(
    db: &dyn baml_compiler2_ppir::Db,
    qtn: &QualifiedTypeName,
    name: &Name,
    ns: MemberNamespace,
) -> Option<InterfaceMemberKind> {
    if let Some(row @ crate::package_interface::ExportedType::Interface { .. }) =
        crate::package_interface::mounted_type_row(db, qtn)
    {
        return mounted_declared_kind(row, name, ns);
    }
    let loc = projection_interface_loc(db, qtn)?;
    let data = baml_compiler2_ppir::item_data::interface_data(db, loc);
    match ns {
        MemberNamespace::Type => data
            .associated_types
            .iter()
            .any(|assoc| &assoc.name == name)
            .then_some(InterfaceMemberKind::AssociatedType),
        MemberNamespace::Value => {
            source_declared_value_kind(db, data, name).map(InterfaceMemberKind::Value)
        }
    }
}

fn projection_interface_loc<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    qtn: &QualifiedTypeName,
) -> Option<baml_compiler2_hir::loc::InterfaceLoc<'db>> {
    let pkg_id = PackageId::new(db, qtn.package().clone());
    let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
    match pkg_items.lookup_type(qtn.namespace(), qtn.name())? {
        Definition::Interface(loc) => Some(loc),
        _ => None,
    }
}

/// The interface a WRITTEN projection-base names without determining
/// `member` - the invalid interface-as-base spelling (Rust's E0223). The
/// one interface-headed base that CAN resolve is an alias of a fully-pinned
/// existential whose spelling pins `member`; that shape passes (`None`).
pub fn interface_base_without_member_pin(
    db: &dyn baml_compiler2_ppir::Db,
    base_ty: &Ty,
    member: &Name,
) -> Option<QualifiedTypeName> {
    use baml_type::normalize::TypeContext as _;
    let facts = crate::facts::Facts::new(db);
    let mut seen: FxHashSet<QualifiedTypeName> = FxHashSet::default();
    let mut current = base_ty.clone();
    loop {
        match current {
            Ty::Interface(qtn, _, pins, _) => {
                return (!pins.iter().any(|(name, _)| name == member)).then_some(qtn);
            }
            Ty::TypeAlias(qtn, _) => {
                if !seen.insert(qtn.clone()) {
                    return None;
                }
                current = facts.alias_def(&qtn)?;
            }
            _ => return None,
        }
    }
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
        Ty::Class(qtn(namespace, name), args.into(), TyAttr::default())
    }

    fn interface(name: &str, args: Vec<Ty>) -> Ty {
        Ty::Interface(qtn(&[], name), args.into(), Box::new([]), TyAttr::default())
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
            Box::new([]),
            Box::new([(
                Name::new("Item"),
                Ty::List(Box::new(type_var("T")), TyAttr::default()),
            )]),
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
        let pattern = Ty::Union(Box::new([type_var("T"), string()]), TyAttr::default());
        let actual = Ty::Union(Box::new([string(), int()]), TyAttr::default());
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
