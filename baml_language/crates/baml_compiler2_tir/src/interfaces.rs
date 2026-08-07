//! Interface resolution substrate (BEP-044): path→interface resolution and identity, the
//! transitive `requires` closure, associated-type binding lowering, and the generic
//! type-pattern matcher used by impl resolution.
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
pub use coherence::*;
pub use impl_rules::*;
use rustc_hash::FxHashSet;

use crate::{
    generics,
    lower_type_expr::qualify_def,
    ty::{ParamTy, QualifiedTypeName, Ty, TyAttr},
    type_context::AliasEquivCtx,
    unify::{TypeBindings, contains_bound_typevar},
};

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

struct InterfaceTypeAssocLowering<'a, 'db> {
    db: &'db dyn crate::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    iface: &'a baml_compiler2_ppir::item_data::InterfaceData<'db>,
    interface_args: &'a [Ty],
    explicit_associated_bindings: &'a [baml_compiler2_hir::type_ref::AssociatedTypeBindingRef],
    /// The arena the explicit bindings' `ty` ids index — the *requiring* item's
    /// `type_refs` (the bindings are written at the `requires` site).
    binding_type_refs: &'a baml_compiler2_hir::type_ref::TypeRefStore,
    binding_pkg_items: &'a baml_compiler2_hir::package::PackageItems<'db>,
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
///
/// Shared by the builder's generic-argument bound gate and the impl-side associated-type-binding
/// bound check, so every bound-check site reads a bound identically.
pub(crate) fn normalized_arg_implements_bound(
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
/// required `bound`.
///
/// The two are the same interface — same name and generic args — and `have` pins
/// every associated type `bound` pins, to the same type. `have` may pin *more*: a
/// more-specific carried bound like `Parser<Output = int>` still discharges the
/// bare `Parser` requirement, so a `<P extends Parser<Output = int>>` argument
/// satisfies a `<P extends Parser>` parameter. It may not *conflict* — a differing
/// pin fails — and a bare `have` does not satisfy a pinned requirement.
fn carried_bound_satisfies(
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
    db: &'db dyn crate::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    iface: &baml_compiler2_ppir::item_data::InterfaceData<'db>,
    interface_args: &[Ty],
    self_ty: &Ty,
    // The arena the block bindings' `type_ref` ids index — the impl block's own
    // `type_refs` (the bindings are written in the block's source).
    binding_type_refs: &baml_compiler2_hir::type_ref::TypeRefStore,
    block_associated_bindings: &[baml_compiler2_ppir::item_data::AssociatedTypeBindingData],
    binding_pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    binding_namespace_path: &[Name],
    generic_params: &[ParamTy],
    caller_bounds: &crate::lower_type_expr::TypeVarBoundsMap,
    diagnostics: &mut Vec<crate::infer_context::TirTypeError>,
) -> Vec<(Name, Ty)> {
    // A binding value (impl-block source) resolves names in the impl's own scope; an
    // associated-type default (interface source) is lowered once — with a symbolic `Self` —
    // by `interface_associated_type_default` and realized here at the impl's `self_ty`.
    //
    // A later binding references an earlier sibling as `Self.Item` (bare names are banned):
    // `Self` lowers as a rigid type variable whose bound is the interface carrying the
    // *already-resolved* pins, so the projection collapses to the earlier witness at
    // lowering time — never consulting the impl set (this runs inside `impl_data`; a
    // concrete-base projection here would re-enter it). A residual symbolic `Self`
    // substitutes to the for-type afterwards.
    let iface_env = crate::generic_env::interface_generic_env(db, iface_loc);
    let (self_param, iface_params) = iface_env.interface_param_parts();
    let self_param = self_param.clone();
    let iface_qtn = interface_loc_qtn(db, iface_loc);
    let mut value_scope = generic_params.to_vec();
    value_scope.push(self_param.clone());
    let mut value_bindings: TypeBindings = generics::identity_bindings(generic_params);
    value_bindings.insert(self_param.clone(), self_ty.clone());
    let mut resolved_pins: Vec<(Name, Ty)> = Vec::new();
    let mut default_bindings = generics::bind_type_vars(iface_params, interface_args);

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
                crate::generics::substitute_ty(
                    &crate::lower_type_expr::lower_type_ref(
                        binding_type_refs,
                        type_ref,
                        &crate::lower_type_expr::ScopeCtx {
                            db,
                            package_items: binding_pkg_items,
                            ns_context: binding_namespace_path,
                            generic_params: &value_scope,
                            bounds: &bounds,
                            self_ty: Some(Ty::TypeVar(self_param.clone(), TyAttr::default())),
                        },
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
                    iface_params,
                    interface_args,
                    &self_param,
                    self_ty,
                );
                crate::generics::substitute_ty(&realized, &default_bindings)
            };
            resolved_pins.push((assoc.name.clone(), ty.clone()));
            let assoc_param = iface_env
                .resolve_any_param(&assoc.name)
                .expect("associated type parameter is in its interface environment")
                .clone();
            default_bindings.insert(assoc_param, ty.clone());
            Some((assoc.name.clone(), ty))
        })
        .collect()
}

fn complete_interface_associated_bindings_from_tys<'db>(
    db: &'db dyn crate::Db,
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
    let iface_env = crate::generic_env::interface_generic_env(db, iface_loc);
    let (self_param, iface_params) = iface_env.interface_param_parts();
    let self_param = self_param.clone();
    let mut bindings = generics::bind_type_vars(iface_params, interface_args);
    for (name, ty) in associated_bindings {
        let param = iface_env
            .resolve_any_param(name)
            .expect("associated type parameter is in its interface environment")
            .clone();
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
                let ty = generics::substitute_ty(ty, &bindings);
                let assoc_param = iface_env
                    .resolve_any_param(&assoc.name)
                    .expect("associated type parameter is in its interface environment")
                    .clone();
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
                    let param = iface_env.resolve_any_param(&assoc.name)?;
                    bindings
                        .get(param)
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
                iface_params,
                interface_args,
                &self_param,
                &self_ty,
            );
            let ty = crate::generics::substitute_ty(&realized, &bindings);
            let assoc_param = iface_env
                .resolve_any_param(&assoc.name)
                .expect("associated type parameter is in its interface environment")
                .clone();
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
    db: &'db dyn crate::Db,
    pkg_items: &'db baml_compiler2_hir::package::PackageItems<'db>,
    ns: Vec<Name>,
    /// The interface's in-scope generic parameters (its declared ones).
    generics: Vec<ParamTy>,
    /// The interface's declared bounds plus the `Self` bound.
    bounds: crate::lower_type_expr::TypeVarBoundsMap,
    self_param: ParamTy,
}

impl<'db> InterfaceDeclScope<'db> {
    fn new(db: &'db dyn crate::Db, iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>) -> Self {
        let iface = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, iface_loc.file(db));
        let pkg_items =
            baml_compiler2_ppir::package_items(db, PackageId::new(db, pkg_info.package.clone()));

        let generic_env = crate::generic_env::interface_generic_env(db, iface_loc);
        let (self_param, generic_params) = generic_env.interface_param_parts();
        let self_param = self_param.clone();
        let self_constraint = baml_type::Interface::new(
            qualify_def(db, Definition::Interface(iface_loc), &iface.name),
            generic_params
                .iter()
                .map(|p| Ty::TypeVar(p.clone(), TyAttr::default()))
                .collect(),
            Vec::new(),
        );
        let mut bounds =
            crate::lower_type_expr::interface_generic_param_bounds(db, iface_loc).clone();
        bounds.insert(self_param.clone(), vec![self_constraint]);

        Self {
            db,
            pkg_items,
            ns: pkg_info.namespace_path,
            generics: generic_env.source_params().to_vec(),
            bounds,
            self_param,
        }
    }

    /// A lowering context over this scope's own generics and bounds.
    fn ctx(&self) -> crate::lower_type_expr::ScopeCtx<'_, 'db> {
        self.ctx_with(&self.generics, &self.bounds)
    }

    /// A lowering context with extended generics/bounds (a method's own
    /// parameters appended); both must contain this scope's entries.
    fn ctx_with<'a>(
        &'a self,
        generics: &'a [ParamTy],
        bounds: &'a crate::lower_type_expr::TypeVarBoundsMap,
    ) -> crate::lower_type_expr::ScopeCtx<'a, 'db> {
        crate::lower_type_expr::ScopeCtx {
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
/// projection over that symbolic `Self` (`(Self as I).Item[]`) instead of erroring for want
/// of a receiver. A referencing site realizes the default by substituting its own `Self` and
/// the interface's actual generic arguments into the returned type (see
/// [`realize_associated_default`]) — that substitution is the single path that fills a
/// defaulted associated type everywhere, replacing the former per-site re-lowering.
///
/// The lowering diagnostics travel with the type so the interface-declaration checker
/// surfaces them exactly once (`infer_scope_types`' interface arm); every referencing site
/// reuses the type and drops the diagnostics. `None` when the associated type has no default
/// (or the interface / associated type is absent).
// `name` is passed by value because a salsa tracked-query key must be owned; `#[allow]`
// (not `#[expect]`) because the salsa macro emits the argument in two places and only one
// trips `needless_pass_by_value`, so an expectation would read as unfulfilled.
#[allow(clippy::needless_pass_by_value)]
#[salsa::tracked]
pub fn interface_associated_type_default<'db>(
    db: &'db dyn crate::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    name: Name,
) -> Option<(Ty, Vec<crate::infer_context::TirTypeError>)> {
    let iface = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
    let assoc = iface.associated_types.iter().find(|a| a.name == name)?;
    let default = assoc.default?;

    let scope = InterfaceDeclScope::new(db, iface_loc);
    let mut diagnostics = Vec::new();
    let lowered = crate::lower_type_expr::lower_type_ref(
        &iface.type_refs,
        default,
        &scope.ctx(),
        &mut diagnostics,
    );
    Some((lowered, diagnostics))
}

/// An interface's declared fields with their types resolved against the
/// interface's own scope (symbolic `Self`, rigid generic parameters). The
/// interface analogue of [`crate::inference::resolve_class_fields`].
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedInterfaceFields {
    /// (field name, resolved type, field-level attributes)
    pub fields: Vec<(Name, Ty, Vec<baml_compiler2_hir::item_tree::Attribute>)>,
    /// Type lowering diagnostics: (error, span of the type annotation).
    pub diagnostics: Vec<(crate::infer_context::TirTypeError, text_size::TextRange)>,
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
    db: &'db dyn crate::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
) -> ResolvedInterfaceFields {
    let iface = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
    let iface_spans = baml_compiler2_ppir::item_data::interface_source_map(db, iface_loc);
    let scope = InterfaceDeclScope::new(db, iface_loc);

    let mut fields = Vec::new();
    let mut diagnostics = Vec::new();
    for field in &iface.fields {
        let mut field_diags = Vec::new();
        let ty = crate::lower_type_expr::lower_type_ref(
            &iface.type_refs,
            field.type_ref,
            &scope.ctx(),
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
    pub diagnostics: Vec<crate::infer_context::TirTypeError>,
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
/// come from [`crate::callable::function_signature_ty`] instead.
#[salsa::tracked(returns(ref))]
pub fn resolve_interface_required_methods<'db>(
    db: &'db dyn crate::Db,
    iface_loc: baml_compiler2_hir::loc::InterfaceLoc<'db>,
) -> Vec<ResolvedInterfaceMethod> {
    use crate::builder::interface_resolution::InterfaceMethodSpec;

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
            let method_generics =
                crate::generic_env::append_params(&scope.generics, &spec.generic_param_names());
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
/// for the interface's `generic_params`. This is how a defaulted associated type is *filled*
/// eagerly wherever the interface is used.
pub(crate) fn realize_associated_default(
    default: &Ty,
    generic_params: &[ParamTy],
    interface_args: &[Ty],
    self_param: &ParamTy,
    self_ty: &Ty,
) -> Ty {
    let mut bindings = generics::bind_type_vars(generic_params, interface_args);
    bindings.insert(self_param.clone(), self_ty.clone());
    generics::substitute_ty(default, &bindings)
}

/// The realized type of a *defaulted* associated `member` for an interface existential
/// `Ty::Interface(qtn, args, …)`, or `None` when `member` is not a defaulted associated
/// type of that interface.
///
/// An existential fixes an omitted defaulted associated type to its default: `Boxed<string>`
/// (with `type Item = T`) has `Item = string`. The default (lowered once by
/// [`interface_associated_type_default`]) is realized with `self_ty` — the existential
/// itself — as `Self`, so a Self-referencing default (`type Items = Self.Item[]`) resolves
/// against the existential's own pins. A *bound* never fills a default this way — its
/// implementor may override it — which is why this is keyed on the interface-existential
/// base, not the type-variable one.
pub(crate) fn existential_associated_default(
    db: &dyn crate::Db,
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
    let iface_env = crate::generic_env::interface_generic_env(db, iface_loc);
    let (self_param, iface_params) = iface_env.interface_param_parts();
    Some(realize_associated_default(
        &default,
        iface_params,
        args,
        self_param,
        self_ty,
    ))
}

fn lower_interface_type_associated_bindings(
    ctx: &InterfaceTypeAssocLowering<'_, '_>,
    diagnostics: &mut Vec<crate::infer_context::TirTypeError>,
) -> Vec<(Name, Ty)> {
    let iface_env = crate::generic_env::interface_generic_env(ctx.db, ctx.iface_loc);
    let (self_param, iface_params) = iface_env.interface_param_parts();
    let self_param = self_param.clone();
    let mut bindings = generics::bind_type_vars(iface_params, ctx.interface_args);
    for (param, ty) in ctx.outer_bindings {
        bindings.entry(param.clone()).or_insert_with(|| ty.clone());
    }
    // The interface's declared parameter bounds, so a `T.member` projection in a
    // binding value or default resolves `T`'s declaring interface.
    let iface_bounds =
        crate::lower_type_expr::interface_generic_param_bounds(ctx.db, ctx.iface_loc);

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
                    let scope = crate::lower_type_expr::ScopeCtx {
                        db: ctx.db,
                        package_items: ctx.binding_pkg_items,
                        ns_context: ctx.binding_namespace_path,
                        generic_params: &generic_params,
                        bounds: &bounds,
                        self_ty: Some(Ty::TypeVar(self_param.clone(), TyAttr::default())),
                    };
                    generics::substitute_ty(
                        &crate::lower_type_expr::lower_type_ref(
                            ctx.binding_type_refs,
                            binding.ty,
                            &scope,
                            diagnostics,
                        ),
                        &bindings,
                    )
                } else {
                    let generic_params: Vec<_> = bindings.keys().cloned().collect();
                    crate::generics::substitute_ty(
                        &crate::lower_type_expr::lower_type_ref(
                            ctx.binding_type_refs,
                            binding.ty,
                            &crate::lower_type_expr::ScopeCtx {
                                db: ctx.db,
                                package_items: ctx.binding_pkg_items,
                                ns_context: ctx.binding_namespace_path,
                                generic_params: &generic_params,
                                bounds: iface_bounds,
                                self_ty: None,
                            },
                            diagnostics,
                        ),
                        &bindings,
                    )
                };
                let assoc_param = iface_env
                    .resolve_any_param(&assoc.name)
                    .expect("associated type parameter is in its interface environment")
                    .clone();
                bindings.insert(assoc_param, ty.clone());
                return Some((assoc.name.clone(), ty));
            }
            // Fill the omitted default eagerly at this interface realized on the receiver:
            // `Self` is that existential (its generic args plus the bindings resolved so far),
            // so a Self-referencing default (`type Items = Self.Item[]`) reduces against them.
            // The default is lowered once (symbolic `Self`) by the shared query.
            let (default, _diags) =
                interface_associated_type_default(ctx.db, ctx.iface_loc, assoc.name.clone())?;
            let self_pins: Vec<(Name, Ty)> = ctx
                .iface
                .associated_types
                .iter()
                .filter_map(|assoc| {
                    let param = iface_env.resolve_any_param(&assoc.name)?;
                    bindings
                        .get(param)
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
                iface_params,
                ctx.interface_args,
                &self_param,
                &self_ty,
            );
            let ty = crate::generics::substitute_ty(&realized, &bindings);
            let assoc_param = iface_env
                .resolve_any_param(&assoc.name)
                .expect("associated type parameter is in its interface environment")
                .clone();
            bindings.insert(assoc_param, ty.clone());
            Some((assoc.name.clone(), ty))
        })
        .collect()
}

/// Match several `(pattern, concrete)` pairs into one consistent set of
/// bindings, threading them across every pair — a `generic_param` that occurs
/// in more than one pattern must unify to the same type in all of them. Returns
/// `None` if any pair fails or the bindings conflict. Used by the canonical
/// resolver to bind an impl's generics from its for-type pattern *and* its
/// interface input args simultaneously (a param may appear in either).
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
/// `bindings` (params already bound must match consistently). Exposed for MIR's blanket-impl
/// instantiation, which threads one binding set through a for-target then an interface match.
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

    // A pattern position whose type variables are all *already bound* (by the
    // for-type or an earlier position) is no longer a pattern — substitute the
    // bindings and compare normalized, so `Iterator<R, E | E2>` at
    // `{E: never, E2: never}` matches a requested `Iterator<int, never>`
    // (`never | never` normalizes to `never`, which no structural descent can
    // see). On mismatch fall through: structural matching may still succeed by
    // re-binding positions to equal values.
    if contains_bound_typevar(pattern, generic_params) {
        let unbound =
            |param: &ParamTy| generic_params.contains(param) && !bindings.contains_key(param);
        if !crate::generics::contains_typevar_where(pattern, &unbound) {
            let substituted = crate::generics::substitute_ty(pattern, bindings);
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
        // blanket `for T`) applies to a `Side.Left` receiver — the enum analogue of the
        // literal→primitive arms above (L45/L75 set semantics).
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
/// constraint heads — and only the identity is consumed, so the lowering
/// neither fills nor demands associated members.
pub fn resolve_path_to_interface_identity<'db>(
    db: &'db dyn crate::Db,
    target: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    current_ns: &[Name],
) -> Option<ResolvedInterface<'db>> {
    let mut diagnostics = Vec::new();
    let ty = crate::lower_type_expr::lower_constraint_head_type_expr(
        target,
        &crate::lower_type_expr::ScopeCtx {
            db,
            package_items: pkg_items,
            ns_context: current_ns,
            generic_params: &[],
            bounds: &crate::lower_type_expr::TypeVarBoundsMap::default(),
            self_ty: None,
        },
        &mut diagnostics,
    );
    resolved_interface_from_ty(db, ty)
}

/// The `TypeRef`-arena twin of [`resolve_path_to_interface_identity`], for
/// callers holding firewall data (`class_data(…).type_refs` + a `TypeRefId`)
/// rather than an AST node. Identical resolution through the arena form of
/// the same constraint-head lowering.
pub fn resolve_ref_to_interface_identity<'db>(
    db: &'db dyn crate::Db,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    target: baml_compiler2_hir::type_ref::TypeRefId,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    current_ns: &[Name],
) -> Option<ResolvedInterface<'db>> {
    let mut diagnostics = Vec::new();
    let ty = crate::lower_type_expr::lower_constraint_head_type_ref(
        store,
        target,
        &crate::lower_type_expr::ScopeCtx {
            db,
            package_items: pkg_items,
            ns_context: current_ns,
            generic_params: &[],
            bounds: &crate::lower_type_expr::TypeVarBoundsMap::default(),
            self_ty: None,
        },
        &mut diagnostics,
    );
    resolved_interface_from_ty(db, ty)
}

/// Shared tail of the two `resolve_*_to_interface_identity` functions: a lowered
/// type is an interface reference iff it lowered to `Ty::Interface`, whose `qtn`
/// then resolves to a declaration.
fn resolved_interface_from_ty(db: &dyn crate::Db, ty: Ty) -> Option<ResolvedInterface<'_>> {
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
    db: &'db dyn crate::Db,
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    target: baml_compiler2_hir::type_ref::TypeRefId,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    current_ns: &[Name],
) -> Option<baml_compiler2_hir::loc::InterfaceLoc<'db>> {
    resolve_ref_to_interface_identity(db, store, target, pkg_items, current_ns)
        .map(|resolved| resolved.loc)
}

/// If `root`'s transitive `requires` graph cycles back to `root`, return the name chain
/// `[root, …, root]` witnessing it; else `None`. A plain worklist walk (resolving each
/// `requires` target to an [`InterfaceLoc`](baml_compiler2_hir::loc::InterfaceLoc)) with a
/// visited set, so it terminates on any cycle. This is the user-facing detector (E0118) for the
/// cycle that [`interface_closure_locs`] skips silently. A cycle among *ancestors* that does not
/// pass back through `root` is that ancestor's own violation, reported when it is validated.
pub(crate) fn interface_requires_cycle<'db>(
    db: &'db dyn crate::Db,
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
/// interface in it (including `root_iface` itself), in BFS order so the
/// receiver appears before its parents. Cycles are skipped silently — they
/// are reported elsewhere (E0118).
pub fn interface_closure_locs<'db>(
    db: &'db dyn crate::Db,
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
    db: &'db dyn crate::Db,
    roots: impl IntoIterator<Item = baml_compiler2_hir::loc::InterfaceLoc<'db>>,
    member: &baml_base::Name,
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

/// Walk the transitive `requires` closure of `root_iface`, carrying the concrete
/// generic arguments for each interface in the closure. For example,
/// `Child<int> requires Parent<T>` yields `(Child, [int])` and `(Parent, [int])`.
/// Walk the transitive `requires` closure of `root_iface`, carrying generic
/// arguments and associated type bindings for each interface in the closure.
/// For example, `Child requires Parent<Item = int>` yields `(Child, [], [])`
/// and `(Parent, [], [(Item, int)])`.
pub fn interface_closure_locs_with_args_and_assoc<'db>(
    db: &'db dyn crate::Db,
    root_iface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
    root_args: &[Ty],
    root_associated_bindings: &[(Name, Ty)],
    // Whether to fill an unbound associated type with its declared default. Callers
    // resolving through a *rigid* `Self` (an interface's own default body) pass false
    // so an overridable associated type stays a symbolic `(Self as I).X` projection
    // rather than collapsing to the interface's default (see
    // `complete_interface_associated_bindings_from_tys`).
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

        let iface_env = crate::generic_env::interface_generic_env(db, loc);
        let (_, iface_params) = iface_env.interface_param_parts();
        let mut bindings = generics::bind_type_vars(iface_params, &args);
        for (name, ty) in &associated_bindings {
            let param = iface_env
                .resolve_any_param(name)
                .expect("associated type parameter is in its interface environment")
                .clone();
            bindings.insert(param, ty.clone());
        }

        // This interface as a constraint (its associated types pinned to the realized
        // bindings) — so a required interface's `Item = Self.Item` resolves `Self.Item` here.
        let self_bound = interface_loc_qtn(db, loc)
            .map(|qtn| baml_type::Interface::new(qtn, args.clone(), associated_bindings.clone()));
        // The requiring interface's declared parameter bounds, so a `T.member`
        // projection in a parent's generic arguments resolves `T`'s declaring
        // interface.
        let iface_bounds = crate::lower_type_expr::interface_generic_param_bounds(db, loc);

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
                    let mut diags = Vec::new();
                    generic_args
                        .iter()
                        .map(|&arg| {
                            let generic_params: Vec<_> = bindings.keys().cloned().collect();
                            crate::generics::substitute_ty(
                                &crate::lower_type_expr::lower_type_ref(
                                    &iface.type_refs,
                                    arg,
                                    &crate::lower_type_expr::ScopeCtx {
                                        db,
                                        package_items: parent_pkg_items,
                                        ns_context: &pkg_info.namespace_path,
                                        generic_params: &generic_params,
                                        bounds: iface_bounds,
                                        self_ty: None,
                                    },
                                    &mut diags,
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
/// argument list, and every associated-type pin `sup` specifies. Argument and pin
/// equality is delegated to `equivalent` (the caller's type-equality oracle) so
/// the walk stays independent of the normalization backend.
///
/// *Proper* requirement: an interface requiring itself is not a requirement, so
/// an identical qualified name short-circuits to `false` — structural reflexivity
/// is the normalizer's job, and the closure walk includes `sub` itself. Returns
/// `false` when `sub` does not resolve to an interface in an accessible package.
///
/// The global counterpart to the per-scope requirement check: a pure function of
/// the program's declarations, the resolution context that bounds package
/// visibility, and the supplied equality oracle.
pub fn interface_requires<'db>(
    db: &'db dyn crate::Db,
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
        // A `for Side` impl (pattern `Side`) applies to a `Side.Left` receiver — an enum
        // variant is a member of its enum's set, mirroring literal→primitive matching.
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
