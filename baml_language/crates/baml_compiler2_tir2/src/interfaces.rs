//! Interface resolution substrate (BEP-044): path→interface resolution and identity, the
//! transitive `requires` closure, associated-type binding lowering, and the generic
//! type-pattern matcher used by impl resolution.
//!
//! Nominal subtyping is decided on the [`impl_rules`] substrate (`impl_data` /
//! `get_implements_block`), not here: `Class T <: Interface I` iff `T` has an `implements I`
//! block, and interface `A <: B` iff `B` is in `A`'s `requires` closure — there is no
//! shape-matching escape hatch.

mod coherence;
mod impl_rules;

use baml_base::{Literal, Name};
use baml_compiler2_hir::{contributions::Definition, package::PackageId};
pub use coherence::*;
pub use impl_rules::*;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    generics,
    lower_type_expr::qualify_def,
    normalize,
    ty::{FunctionParamTy, QualifiedTypeName, Ty, TyAttr},
};

pub type TypeBindings = FxHashMap<Name, Ty>;
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
    iface: &'a baml_compiler2_hir::item_tree::Interface,
    interface_args: &'a [Ty],
    explicit_associated_bindings: &'a [baml_compiler2_ast::AssociatedTypeBinding],
    iface_pkg_items: &'a baml_compiler2_hir::package::PackageItems<'db>,
    binding_pkg_items: &'a baml_compiler2_hir::package::PackageItems<'db>,
    iface_namespace_path: &'a [Name],
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
            interface: Some(iface),
            member,
            ..
        } => ctx.associated_type_bound(iface, member.clone()),
        // Never determined — errored upstream; don't cascade.
        Ty::AssociatedTypeProjection {
            interface: None, ..
        } => return true,
        // A concrete argument implements the bound directly through its impls.
        _ => return ctx.implements_interface(arg, bound),
    };
    carried_bounds.iter().any(|have| {
        ctx.equivalent(&have.to_ty(), &bound.to_ty()) || ctx.interface_requires(have, bound)
    })
}

#[allow(clippy::too_many_arguments)]
fn lower_interface_associated_bindings(
    db: &dyn crate::Db,
    iface: &baml_compiler2_hir::item_tree::Interface,
    interface_args: &[Ty],
    block_associated_bindings: &[baml_compiler2_ast::AssociatedTypeBindingDef],
    iface_pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    binding_pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    iface_namespace_path: &[Name],
    binding_namespace_path: &[Name],
    generic_params: &[Name],
    caller_bounds: &crate::lower_type_expr::TypeVarBoundsMap,
    diagnostics: &mut Vec<crate::infer_context::TirTypeError>,
) -> Vec<(Name, Ty)> {
    // Each expression resolves names in the scope it was *written* in — never the
    // other declaration's. A binding value (impl-block source) sees the impl's own
    // generics; an associated-type default (interface source) sees the interface's.
    // Resolved associated types join both as they resolve (a later binding may
    // reference an earlier sibling). Each scope carries its own substitution map,
    // so a name collision between an impl generic and an interface parameter
    // resolves each expression to its own scope's meaning.
    let mut value_scope: Vec<Name> = generic_params.to_vec();
    let mut value_bindings: rustc_hash::FxHashMap<Name, Ty> = generic_params
        .iter()
        .map(|param| (param.clone(), Ty::TypeVar(param.clone(), TyAttr::default())))
        .collect();
    let mut default_scope: Vec<Name> = iface.generic_params.clone();
    let mut default_bindings = generics::bind_type_vars(&iface.generic_params, interface_args);
    // The interface's declared parameter bounds, so a `T.member` projection in an
    // associated-type *default* resolves `T`'s declaring interface; a binding
    // *value*'s type variables project through the impl's own `caller_bounds`.
    let iface_bounds = crate::lower_type_expr::lower_decl_generic_param_bounds(
        db,
        iface_pkg_items,
        iface_namespace_path,
        &iface.generic_params,
        &iface.generic_param_bounds,
    );

    iface
        .associated_types
        .iter()
        .filter_map(|assoc| {
            let ty = if let Some(binding) = block_associated_bindings
                .iter()
                .find(|binding| binding.name == assoc.name)
                && let Some(type_expr) = &binding.type_expr
            {
                crate::generics::substitute_ty(
                    &crate::lower_type_expr::lower_type_expr(
                        type_expr,
                        &crate::lower_type_expr::ScopeCtx {
                            db,
                            package_items: binding_pkg_items,
                            ns_context: binding_namespace_path,
                            generic_params: &value_scope,
                            bounds: caller_bounds,
                            self_ty: None,
                        },
                        diagnostics,
                    ),
                    &value_bindings,
                )
            } else {
                let default = assoc.default.as_ref()?;
                crate::generics::substitute_ty(
                    &crate::lower_type_expr::lower_type_expr(
                        default,
                        &crate::lower_type_expr::ScopeCtx {
                            db,
                            package_items: iface_pkg_items,
                            ns_context: iface_namespace_path,
                            generic_params: &default_scope,
                            bounds: &iface_bounds,
                            self_ty: None,
                        },
                        diagnostics,
                    ),
                    &default_bindings,
                )
            };
            value_scope.push(assoc.name.clone());
            value_bindings.insert(assoc.name.clone(), ty.clone());
            default_scope.push(assoc.name.clone());
            default_bindings.insert(assoc.name.clone(), ty.clone());
            Some((assoc.name.clone(), ty))
        })
        .collect()
}

fn complete_interface_associated_bindings_from_tys(
    db: &dyn crate::Db,
    iface: &baml_compiler2_hir::item_tree::Interface,
    interface_args: &[Ty],
    associated_bindings: &[(Name, Ty)],
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    iface_namespace_path: &[Name],
    diagnostics: &mut Vec<crate::infer_context::TirTypeError>,
) -> Vec<(Name, Ty)> {
    let mut bindings = generics::bind_type_vars(&iface.generic_params, interface_args);
    for (name, ty) in associated_bindings {
        bindings.insert(name.clone(), ty.clone());
    }
    // The interface's declared parameter bounds, so a `T.member` projection in
    // an associated-type default resolves `T`'s declaring interface.
    let iface_bounds = crate::lower_type_expr::lower_decl_generic_param_bounds(
        db,
        pkg_items,
        iface_namespace_path,
        &iface.generic_params,
        &iface.generic_param_bounds,
    );

    iface
        .associated_types
        .iter()
        .filter_map(|assoc| {
            if let Some((_, ty)) = associated_bindings
                .iter()
                .find(|(name, _)| name == &assoc.name)
            {
                let ty = generics::substitute_ty(ty, &bindings);
                bindings.insert(assoc.name.clone(), ty.clone());
                return Some((assoc.name.clone(), ty));
            }
            assoc.default.as_ref().map(|default| {
                let ty = {
                    let generic_params: Vec<_> = bindings.keys().cloned().collect();
                    crate::generics::substitute_ty(
                        &crate::lower_type_expr::lower_type_expr(
                            default,
                            &crate::lower_type_expr::ScopeCtx {
                                db,
                                package_items: pkg_items,
                                ns_context: iface_namespace_path,
                                generic_params: &generic_params,
                                bounds: &iface_bounds,
                                self_ty: None,
                            },
                            diagnostics,
                        ),
                        &bindings,
                    )
                };
                bindings.insert(assoc.name.clone(), ty.clone());
                (assoc.name.clone(), ty)
            })
        })
        .collect()
}

fn lower_interface_type_associated_bindings(
    ctx: &InterfaceTypeAssocLowering<'_, '_>,
    diagnostics: &mut Vec<crate::infer_context::TirTypeError>,
) -> Vec<(Name, Ty)> {
    let mut bindings = generics::bind_type_vars(&ctx.iface.generic_params, ctx.interface_args);
    for (name, ty) in ctx.outer_bindings {
        bindings.entry(name.clone()).or_insert_with(|| ty.clone());
    }
    // The interface's declared parameter bounds, so a `T.member` projection in a
    // binding value or default resolves `T`'s declaring interface.
    let iface_bounds = crate::lower_type_expr::lower_decl_generic_param_bounds(
        ctx.db,
        ctx.iface_pkg_items,
        ctx.iface_namespace_path,
        &ctx.iface.generic_params,
        &ctx.iface.generic_param_bounds,
    );

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
                    bounds.insert(Name::new("Self"), vec![self_bound.clone()]);
                    let generic_params: Vec<Name> = bindings
                        .keys()
                        .cloned()
                        .chain(std::iter::once(Name::new("Self")))
                        .collect();
                    let scope = crate::lower_type_expr::ScopeCtx {
                        db: ctx.db,
                        package_items: ctx.binding_pkg_items,
                        ns_context: ctx.binding_namespace_path,
                        generic_params: &generic_params,
                        bounds: &bounds,
                        self_ty: Some(Ty::TypeVar(Name::new("Self"), TyAttr::default())),
                    };
                    generics::substitute_ty(
                        &crate::lower_type_expr::lower_type_expr(&binding.ty, &scope, diagnostics),
                        &bindings,
                    )
                } else {
                    let generic_params: Vec<_> = bindings.keys().cloned().collect();
                    crate::generics::substitute_ty(
                        &crate::lower_type_expr::lower_type_expr(
                            &binding.ty,
                            &crate::lower_type_expr::ScopeCtx {
                                db: ctx.db,
                                package_items: ctx.binding_pkg_items,
                                ns_context: ctx.binding_namespace_path,
                                generic_params: &generic_params,
                                bounds: &iface_bounds,
                                self_ty: None,
                            },
                            diagnostics,
                        ),
                        &bindings,
                    )
                };
                bindings.insert(assoc.name.clone(), ty.clone());
                return Some((assoc.name.clone(), ty));
            }
            assoc.default.as_ref().map(|default| {
                let ty = {
                    let generic_params: Vec<_> = bindings.keys().cloned().collect();
                    crate::generics::substitute_ty(
                        &crate::lower_type_expr::lower_type_expr(
                            default,
                            &crate::lower_type_expr::ScopeCtx {
                                db: ctx.db,
                                package_items: ctx.iface_pkg_items,
                                ns_context: ctx.iface_namespace_path,
                                generic_params: &generic_params,
                                bounds: &iface_bounds,
                                self_ty: None,
                            },
                            diagnostics,
                        ),
                        &bindings,
                    )
                };
                bindings.insert(assoc.name.clone(), ty.clone());
                (assoc.name.clone(), ty)
            })
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
    generic_params: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> Option<TypeBindings> {
    let mut bindings = TypeBindings::default();
    for (pattern, concrete) in pairs {
        match_ty_pattern_into(pattern, concrete, generic_params, aliases, &mut bindings)?;
    }
    Some(bindings)
}

fn match_ty_pattern_into(
    pattern: &Ty,
    concrete: &Ty,
    generic_params: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    bindings: &mut TypeBindings,
) -> Option<()> {
    if let Ty::TypeVar(name, _) = pattern
        && generic_params.contains(name)
    {
        return bind_type_var(name, concrete, bindings, aliases);
    }

    if !contains_bound_typevar(pattern, generic_params)
        && normalize::is_same_normalized_type(pattern, concrete, aliases)
    {
        return Some(());
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
        _ if normalize::is_same_normalized_type(pattern, concrete, aliases) => Some(()),
        _ => None,
    }
}

fn match_union_members(
    pattern_members: &[Ty],
    concrete_members: &[Ty],
    generic_params: &[Name],
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
    name: &Name,
    concrete: &Ty,
    bindings: &mut TypeBindings,
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> Option<()> {
    match bindings.get(name) {
        Some(existing) if normalize::is_same_normalized_type(existing, concrete, aliases) => {
            Some(())
        }
        Some(_) => None,
        None => {
            bindings.insert(name.clone(), concrete.clone());
            Some(())
        }
    }
}

fn contains_bound_typevar(ty: &Ty, generic_params: &[Name]) -> bool {
    match ty {
        Ty::TypeVar(name, _) => generic_params.contains(name),
        Ty::Class(_, args, _) | Ty::Union(args, _) => args
            .iter()
            .any(|arg| contains_bound_typevar(arg, generic_params)),
        Ty::Interface(_, args, associated_bindings, _) => {
            args.iter()
                .any(|arg| contains_bound_typevar(arg, generic_params))
                || associated_bindings
                    .iter()
                    .any(|(_, ty)| contains_bound_typevar(ty, generic_params))
        }
        Ty::List(inner, _) | Ty::EvolvingList(inner, _) => {
            contains_bound_typevar(inner, generic_params)
        }
        Ty::Map {
            key: k, value: v, ..
        }
        | Ty::EvolvingMap(k, v, _)
        | Ty::Future(k, v, _) => {
            contains_bound_typevar(k, generic_params) || contains_bound_typevar(v, generic_params)
        }
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params
                .iter()
                .any(|FunctionParamTy { ty, .. }| contains_bound_typevar(ty, generic_params))
                || contains_bound_typevar(ret, generic_params)
                || contains_bound_typevar(throws, generic_params)
        }
        _ => false,
    }
}

/// Resolve a `TypeExprKind::Path` to an interface declaration and its fully
/// qualified identity. Returns `None` when the path doesn't resolve to an
/// interface.
pub fn resolve_path_to_interface_identity<'db>(
    db: &'db dyn crate::Db,
    target: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    current_ns: &[Name],
) -> Option<ResolvedInterface<'db>> {
    let mut diagnostics = Vec::new();
    let Ty::Interface(qtn, _, _, _) = crate::lower_type_expr::lower_type_expr(
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
    ) else {
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

/// Resolve a `TypeExprKind::Path` to an interface declaration. Returns `None`
/// when the path doesn't resolve to an interface.
pub fn resolve_path_to_interface<'db>(
    db: &'db dyn crate::Db,
    target: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    current_ns: &[Name],
) -> Option<baml_compiler2_hir::loc::InterfaceLoc<'db>> {
    resolve_path_to_interface_identity(db, target, pkg_items, current_ns)
        .map(|resolved| resolved.loc)
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
        let tree = baml_compiler2_hir::file_item_tree(db, loc.file(db));
        let Some(iface) = tree.interfaces.get(&loc.id(db)) else {
            continue;
        };
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, loc.file(db));
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let parent_pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
        for parent in &iface.requires {
            if let Some(parent_loc) =
                resolve_path_to_interface(db, parent, parent_pkg_items, &pkg_info.namespace_path)
            {
                queue.push_back(parent_loc);
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
        let tree = baml_compiler2_hir::file_item_tree(db, loc.file(db));
        let Some(iface) = tree.interfaces.get(&loc.id(db)) else {
            continue;
        };
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, loc.file(db));
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let parent_pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);
        let mut diags = Vec::new();
        let associated_bindings = complete_interface_associated_bindings_from_tys(
            db,
            iface,
            &args,
            &associated_bindings,
            parent_pkg_items,
            &pkg_info.namespace_path,
            &mut diags,
        );
        if !seen.insert((loc, args.clone(), associated_bindings.clone())) {
            continue;
        }
        out.push((loc, args.clone(), associated_bindings.clone()));
        let mut child_ancestors = ancestors.clone();
        child_ancestors.insert(loc);

        let mut bindings = generics::bind_type_vars(&iface.generic_params, &args);
        for (name, ty) in &associated_bindings {
            bindings.insert(name.clone(), ty.clone());
        }

        // This interface as a constraint (its associated types pinned to the realized
        // bindings) — so a required interface's `Item = Self.Item` resolves `Self.Item` here.
        let self_bound = interface_loc_qtn(db, loc)
            .map(|qtn| baml_type::Interface::new(qtn, args.clone(), associated_bindings.clone()));
        // The requiring interface's declared parameter bounds, so a `T.member`
        // projection in a parent's generic arguments resolves `T`'s declaring
        // interface.
        let iface_bounds = crate::lower_type_expr::interface_generic_param_bounds(db, loc);

        for parent in &iface.requires {
            let Some(parent_loc) =
                resolve_path_to_interface(db, parent, parent_pkg_items, &pkg_info.namespace_path)
            else {
                continue;
            };
            let parent_args = match &parent.kind {
                baml_compiler2_ast::TypeExprKind::Path { generic_args, .. } => {
                    let mut diags = Vec::new();
                    generic_args
                        .iter()
                        .map(|arg| {
                            let generic_params: Vec<_> = bindings.keys().cloned().collect();
                            crate::generics::substitute_ty(
                                &crate::lower_type_expr::lower_type_expr(
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
            let parent_tree = baml_compiler2_hir::file_item_tree(db, parent_loc.file(db));
            let Some(parent_iface) = parent_tree.interfaces.get(&parent_loc.id(db)) else {
                continue;
            };
            let parent_pkg =
                baml_compiler2_hir::file_package::file_package(db, parent_loc.file(db));
            let parent_iface_pkg_id = PackageId::new(db, parent_pkg.package.clone());
            let parent_iface_pkg_items =
                baml_compiler2_ppir::package_items(db, parent_iface_pkg_id);
            let (parent_explicit_assoc, parent_binding_ns): (
                &[baml_compiler2_ast::AssociatedTypeBinding],
                &[Name],
            ) = match &parent.kind {
                baml_compiler2_ast::TypeExprKind::Path {
                    associated_type_bindings,
                    ..
                } => (
                    associated_type_bindings.as_slice(),
                    &pkg_info.namespace_path,
                ),
                _ => (&[][..], &pkg_info.namespace_path),
            };
            let parent_assoc = lower_interface_type_associated_bindings(
                &InterfaceTypeAssocLowering {
                    db,
                    iface: parent_iface,
                    interface_args: &parent_args,
                    explicit_associated_bindings: parent_explicit_assoc,
                    iface_pkg_items: parent_iface_pkg_items,
                    binding_pkg_items: parent_pkg_items,
                    iface_namespace_path: &parent_pkg.namespace_path,
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
    ) {
        let iface_tree = baml_compiler2_hir::file_item_tree(db, iface_loc.file(db));
        let Some(iface_data) = iface_tree.interfaces.get(&iface_loc.id(db)) else {
            continue;
        };
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
        Ty::TypeVar(Name::new(name), TyAttr::default())
    }

    #[test]
    fn match_ty_pattern_rejects_repeated_type_var_conflict() {
        let pattern = class(&[], "Pair", vec![type_var("T"), type_var("T")]);
        let good = class(&[], "Pair", vec![int(), int()]);
        let bad = class(&[], "Pair", vec![int(), string()]);
        let params = vec![Name::new("T")];

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
        let params = vec![Name::new("T")];

        let bindings = match_ty_patterns(
            &[(&pattern, &actual)],
            &params,
            &std::collections::HashMap::default(),
        )
        .expect("nested list arg should bind T");
        assert_eq!(bindings.get(&Name::new("T")), Some(&int()));
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

        assert!(contains_bound_typevar(&ty, &[Name::new("T")]));
        assert!(!contains_bound_typevar(&ty, &[Name::new("U")]));
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
        let params = vec![Name::new("T")];

        let bindings = match_ty_patterns(
            &[(&pattern, &actual)],
            &params,
            &std::collections::HashMap::default(),
        )
        .expect("union members should be matched by type, not position");
        assert_eq!(bindings.get(&Name::new("T")), Some(&int()));
    }
}
