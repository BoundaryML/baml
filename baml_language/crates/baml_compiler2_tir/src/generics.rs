//! Generic type variable binding and substitution.
//!
//! When the type checker encounters `arr.at(0)` where `arr: int[]`, it needs
//! to know that `at` returns `int` (not `T`). This module provides the
//! binding and substitution machinery.
//!
//! ## How it works
//!
//! 1. The caller looks up the builtin class (e.g. `Array`) from the `"baml"`
//!    package and extracts its `generic_params` (e.g. `["T"]`).
//! 2. It provides the concrete type arguments (e.g. `[Ty::Int { attr: TyAttr::default() }]`).
//! 3. `bind_type_vars` zips them together: `{T → int}`.
//! 4. For each method parameter/return type, the caller lowers the `TypeExpr`
//!    through a [`ScopeCtx`](crate::lower_type_expr::ScopeCtx) whose in-scope
//!    type variables are the binding keys (so `T` stays `Ty::TypeVar` instead
//!    of erroring as "unresolved type"), then applies [`substitute_ty`] to
//!    replace those type-variable references with their bound concrete types.

use baml_base::Name;
use rustc_hash::FxHashMap;

use crate::ty::{FunctionParamTy, Ty};

// ── Type variable binding ─────────────────────────────────────────────────────

/// Bind type variables from generic params to concrete type arguments.
///
/// Example: `bind_type_vars(&["T"], &[Ty::Int { attr: TyAttr::default() }])` → `{"T" → Int}`
///
/// If there are more params than args (or vice versa), the extra entries are
/// silently ignored — callers are responsible for providing matching lengths.
pub fn bind_type_vars(generic_params: &[Name], concrete_args: &[Ty]) -> FxHashMap<Name, Ty> {
    let mut bindings = FxHashMap::default();
    for (param, arg) in generic_params.iter().zip(concrete_args.iter()) {
        bindings.insert(param.clone(), arg.clone());
    }
    bindings
}

// ── Type substitution ─────────────────────────────────────────────────────────

/// Substitute type variables in a `Ty` using the provided bindings.
///
/// Recursively walks the type, replacing any `Ty::TypeVar` present in
/// `bindings`. This is used both for callable generic instantiation and for
/// interface implementation rule instantiation, so it must preserve the full
/// TIR shape rather than only class/member-signature types.
pub fn substitute_ty(ty: &Ty, bindings: &FxHashMap<Name, Ty>) -> Ty {
    if bindings.is_empty() {
        return ty.clone();
    }
    match ty {
        Ty::TypeVar(name, _) => bindings.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Ty::List(inner, attr) => Ty::List(Box::new(substitute_ty(inner, bindings)), attr.clone()),
        Ty::EvolvingList(inner, attr) => {
            Ty::EvolvingList(Box::new(substitute_ty(inner, bindings)), attr.clone())
        }
        Ty::Map {
            key: k,
            value: v,
            attr,
        } => Ty::Map {
            key: Box::new(substitute_ty(k, bindings)),
            value: Box::new(substitute_ty(v, bindings)),
            attr: attr.clone(),
        },
        Ty::EvolvingMap(k, v, attr) => Ty::EvolvingMap(
            Box::new(substitute_ty(k, bindings)),
            Box::new(substitute_ty(v, bindings)),
            attr.clone(),
        ),
        Ty::Future(value, error, attr) => Ty::Future(
            Box::new(substitute_ty(value, bindings)),
            Box::new(substitute_ty(error, bindings)),
            attr.clone(),
        ),
        Ty::AssociatedTypeProjection {
            base,
            interface,
            member,
            attr,
        } => Ty::AssociatedTypeProjection {
            base: Box::new(substitute_ty(base, bindings)),
            interface: Box::new(interface.map_tys(|t| substitute_ty(t, bindings))),
            member: member.clone(),
            attr: attr.clone(),
        },
        Ty::Union(members, attr) => normalize_union_members(
            members.iter().map(|m| substitute_ty(m, bindings)),
            attr.clone(),
        ),
        Ty::Function {
            params,
            ret,
            throws,
            attr,
        } => {
            // Function values are realized: a function type carries no generics of
            // its own, only free typevars from the enclosing context — so there is
            // nothing to shadow and substitution recurses with the same bindings.
            Ty::Function {
                params: params
                    .iter()
                    .map(|param| FunctionParamTy {
                        name: param.name.clone(),
                        ty: substitute_ty(&param.ty, bindings),
                        mode: param.mode,
                    })
                    .collect(),
                ret: Box::new(substitute_ty(ret, bindings)),
                throws: Box::new(substitute_ty(throws, bindings)),
                attr: attr.clone(),
            }
        }
        Ty::Class(name, type_args, attr) => {
            let substituted_args: Vec<Ty> = type_args
                .iter()
                .map(|t| substitute_ty(t, bindings))
                .collect();
            Ty::Class(name.clone(), substituted_args, attr.clone())
        }
        Ty::Interface(name, type_args, associated_bindings, attr) => {
            let substituted_args: Vec<Ty> = type_args
                .iter()
                .map(|t| substitute_ty(t, bindings))
                .collect();
            let substituted_bindings = associated_bindings
                .iter()
                .map(|(name, ty)| (name.clone(), substitute_ty(ty, bindings)))
                .collect();
            Ty::Interface(
                name.clone(),
                substituted_args,
                substituted_bindings,
                attr.clone(),
            )
        }
        // All other types are leaves (primitives, enums, etc.) — pass through.
        _ => ty.clone(),
    }
}

// ── Method parameter adjustment ───────────────────────────────────────────────

/// Skip the `self` parameter in a method's parameter list.
///
/// When `arr.length()` is a method call, the resolved `Ty::Function` includes
/// `self` as the first parameter (from the `.baml` stub declaration). The call
/// site already bound `arr` as the receiver — it should not count as an
/// explicit argument.
///
/// Returns the slice of params after `self`, or the full slice if `self` is
/// not the first parameter name.
pub fn skip_self_param(params: &[FunctionParamTy]) -> &[FunctionParamTy] {
    match params.first() {
        Some(param)
            if param
                .name
                .as_ref()
                .is_some_and(|name| name.as_str() == "self") =>
        {
            &params[1..]
        }
        _ => params,
    }
}

// ── Type variable utilities ────────────────────────────────────────────────

/// Deep any-node predicate over a type tree: does `pred` hold for `ty` itself
/// or for any type nested inside it? The single traversal behind the
/// `contains_*` family; the arms cover every child position a `Ty` can carry.
/// A function type carries no generic binders of its own (function values are
/// realized), so recursion enters its params/ret/throws with `pred` unchanged;
/// a projection is entered through both its base (`T::Item`) and its
/// qualifying interface's types.
pub fn contains_ty_where(ty: &Ty, pred: &dyn Fn(&Ty) -> bool) -> bool {
    if pred(ty) {
        return true;
    }
    match ty {
        Ty::AssociatedTypeProjection {
            base, interface, ..
        } => contains_ty_where(base, pred) || interface.tys().any(|t| contains_ty_where(t, pred)),
        Ty::List(inner, _) | Ty::EvolvingList(inner, _) => contains_ty_where(inner, pred),
        Ty::Map {
            key: k, value: v, ..
        }
        | Ty::EvolvingMap(k, v, _) => contains_ty_where(k, pred) || contains_ty_where(v, pred),
        Ty::Union(tys, _) => tys.iter().any(|t| contains_ty_where(t, pred)),
        Ty::Future(value, error, _) => {
            contains_ty_where(value, pred) || contains_ty_where(error, pred)
        }
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params
                .iter()
                .any(|param| contains_ty_where(&param.ty, pred))
                || contains_ty_where(ret, pred)
                || contains_ty_where(throws, pred)
        }
        Ty::Class(_, type_args, _) => type_args.iter().any(|t| contains_ty_where(t, pred)),
        Ty::Interface(_, type_args, associated_bindings, _) => {
            type_args.iter().any(|t| contains_ty_where(t, pred))
                || associated_bindings
                    .iter()
                    .any(|(_, ty)| contains_ty_where(ty, pred))
        }
        _ => false,
    }
}

/// Check if a type contains any `Ty::TypeVar` anywhere in its structure.
pub fn contains_typevar(ty: &Ty) -> bool {
    contains_ty_where(ty, &|t| matches!(t, Ty::TypeVar(_, _)))
}

/// Does `ty` carry an error-recovery sentinel (`Ty::Error` or `Ty::Unknown`)
/// anywhere in its structure? An expression recorded with such a type already
/// failed to compile at its own site; downstream consumers (e.g. call-site
/// generic inference) use this to recognize an already-failed input and avoid
/// cascading a second diagnostic off it.
pub fn contains_error_recovery(ty: &Ty) -> bool {
    contains_ty_where(ty, &|t| matches!(t, Ty::Error { .. } | Ty::Unknown { .. }))
}

/// Whether `ty` contains an associated-type projection whose base carries no
/// type variables — one that substitution has made concrete, so the canonical
/// algebra can reduce it to its realization (`(Risky as HasErr).E` → `Kaboom`).
/// Used to gate a post-substitution `normalize`: types without such a
/// projection are left exactly as written.
pub fn contains_concrete_base_projection(ty: &Ty) -> bool {
    match ty {
        Ty::AssociatedTypeProjection { base, .. } => {
            !contains_typevar(base) || contains_concrete_base_projection(base)
        }
        Ty::List(inner, _) | Ty::EvolvingList(inner, _) => contains_concrete_base_projection(inner),
        Ty::Map {
            key: k, value: v, ..
        }
        | Ty::EvolvingMap(k, v, _) => {
            contains_concrete_base_projection(k) || contains_concrete_base_projection(v)
        }
        Ty::Union(tys, _) => tys.iter().any(contains_concrete_base_projection),
        Ty::Future(value, error, _) => {
            contains_concrete_base_projection(value) || contains_concrete_base_projection(error)
        }
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params
                .iter()
                .any(|param| contains_concrete_base_projection(&param.ty))
                || contains_concrete_base_projection(ret)
                || contains_concrete_base_projection(throws)
        }
        Ty::Class(_, type_args, _) => type_args.iter().any(contains_concrete_base_projection),
        Ty::Interface(_, type_args, associated_bindings, _) => {
            type_args.iter().any(contains_concrete_base_projection)
                || associated_bindings
                    .iter()
                    .any(|(_, ty)| contains_concrete_base_projection(ty))
        }
        _ => false,
    }
}

/// Whether `name` is an *inferable* type var for a value call: either one of the
/// callee's declared `generic_params`, or a synthetic effect-polymorphism param
/// (`__effect_param_N`). Anything else is a *rigid ambient* var (the caller's
/// `T` in an instantiation value `foo<T>`), which must be matched structurally
/// rather than inferred. Used by call-site inference's binding-retention filter.
pub fn is_value_call_inferable(name: &Name, generic_params: &[Name]) -> bool {
    generic_params.contains(name) || crate::ty::is_synthetic_effect_param(name)
}

/// Like [`contains_typevar`], but ignores type variables whose name appears in
/// `rigid`. Used at a generic call site to tell the callee's still-unbound
/// inference variables apart from the *enclosing* function's generic params:
/// the latter are rigid (fixed, concrete-enough) types within the current body,
/// so a callback parameter like `Future<T, E>` — where `T`/`E` are the caller's
/// own generics — can drive bidirectional checking of an unannotated lambda
/// parameter instead of falling back to synthesis (which would reject the bare
/// param). A typevar that is NOT rigid is one the callee must still infer, so a
/// param mentioning it genuinely cannot give the lambda a concrete shape.
pub fn contains_non_rigid_typevar(ty: &Ty, rigid: &[Name]) -> bool {
    contains_typevar_where(ty, &|name| !rigid.iter().any(|r| r == name))
}

/// Returns `true` if `ty` contains any type variable for which `pred` returns
/// `true`. A general form of [`contains_typevar`] used by call validation to
/// distinguish *rigid* type variables (the pinned `Self`, caller-scope generic
/// params) — which must be checked — from genuinely-uninferred ones (callee
/// generics, free inference/effect vars) — which are deferred.
pub fn contains_typevar_where(ty: &Ty, pred: &dyn Fn(&Name) -> bool) -> bool {
    contains_ty_where(ty, &|t| matches!(t, Ty::TypeVar(name, _) if pred(name)))
}

// ── Type variable inference & union normalization ──────────────────────────────
//
// The pure `Ty`-walking inference/union primitives now live in `baml_type` so the
// runtime engine can share the single algorithm (it widens `RuntimeTy` → `Ty`,
// runs the unifier, narrows back) without a runtime → compiler dependency. They
// are re-exported here so every existing `crate::generics::…` caller is
// unchanged. See `01c-inbound-inference-reuse.md`.
pub use baml_type_runtime::{
    infer_bindings, infer_bindings_allow_typevars, infer_bindings_rigid_self,
    normalize_union_members, union_ty,
};

#[allow(clippy::only_used_in_recursion)]
pub fn erase_unresolved_typevars(
    ty: &Ty,
    diagnostics: &mut Vec<crate::infer_context::TirTypeError>,
) -> Ty {
    match ty {
        Ty::TypeVar(_, _) => {
            // Preserve TypeVars — they represent the enclosing function's generic
            // parameter and will be resolved at the outer call site.
            ty.clone()
        }
        Ty::List(inner, attr) => Ty::List(
            Box::new(erase_unresolved_typevars(inner, diagnostics)),
            attr.clone(),
        ),
        Ty::Map {
            key: k,
            value: v,
            attr,
        } => Ty::Map {
            key: Box::new(erase_unresolved_typevars(k, diagnostics)),
            value: Box::new(erase_unresolved_typevars(v, diagnostics)),
            attr: attr.clone(),
        },
        Ty::AssociatedTypeProjection {
            base,
            interface,
            member,
            attr,
        } => Ty::AssociatedTypeProjection {
            base: Box::new(erase_unresolved_typevars(base, diagnostics)),
            interface: Box::new(interface.map_tys(|t| erase_unresolved_typevars(t, diagnostics))),
            member: member.clone(),
            attr: attr.clone(),
        },
        Ty::Function {
            params,
            ret,
            throws,
            attr,
        } => Ty::Function {
            params: params
                .iter()
                .map(|param| FunctionParamTy {
                    name: param.name.clone(),
                    ty: erase_unresolved_typevars(&param.ty, diagnostics),
                    mode: param.mode,
                })
                .collect(),
            ret: Box::new(erase_unresolved_typevars(ret, diagnostics)),
            throws: Box::new(erase_unresolved_typevars(throws, diagnostics)),
            attr: attr.clone(),
        },
        Ty::Union(tys, attr) => Ty::Union(
            tys.iter()
                .map(|t| erase_unresolved_typevars(t, diagnostics))
                .collect(),
            attr.clone(),
        ),
        Ty::Class(name, type_args, attr) => Ty::Class(
            name.clone(),
            type_args
                .iter()
                .map(|t| erase_unresolved_typevars(t, diagnostics))
                .collect(),
            attr.clone(),
        ),
        Ty::Future(value, error, attr) => Ty::Future(
            Box::new(erase_unresolved_typevars(value, diagnostics)),
            Box::new(erase_unresolved_typevars(error, diagnostics)),
            attr.clone(),
        ),
        other => other.clone(),
    }
}

/// Replace selected type variables with `unknown` for runtime-facing metadata.
///
/// Bounded generic parameters are compile-time evidence, not concrete runtime
/// type tags. MIR and bytecode metadata both need the same erasure behavior, so
/// keep the recursive shape walk here beside the other generic utilities.
pub fn erase_typevars_matching(ty: &Ty, should_erase: &impl Fn(&Name) -> bool) -> Ty {
    if !contains_typevar(ty) {
        return ty.clone();
    }

    match ty {
        Ty::TypeVar(name, attr) if should_erase(name) => Ty::BuiltinUnknown { attr: attr.clone() },
        Ty::Class(qtn, args, attr) => Ty::Class(
            qtn.clone(),
            args.iter()
                .map(|arg| erase_typevars_matching(arg, should_erase))
                .collect(),
            attr.clone(),
        ),
        Ty::Interface(qtn, args, associated_bindings, attr) => Ty::Interface(
            qtn.clone(),
            args.iter()
                .map(|arg| erase_typevars_matching(arg, should_erase))
                .collect(),
            associated_bindings
                .iter()
                .map(|(name, ty)| (name.clone(), erase_typevars_matching(ty, should_erase)))
                .collect(),
            attr.clone(),
        ),
        Ty::List(inner, attr) => Ty::List(
            Box::new(erase_typevars_matching(inner, should_erase)),
            attr.clone(),
        ),
        Ty::EvolvingList(inner, attr) => Ty::EvolvingList(
            Box::new(erase_typevars_matching(inner, should_erase)),
            attr.clone(),
        ),
        Ty::Map { key, value, attr } => Ty::Map {
            key: Box::new(erase_typevars_matching(key, should_erase)),
            value: Box::new(erase_typevars_matching(value, should_erase)),
            attr: attr.clone(),
        },
        Ty::EvolvingMap(key, value, attr) => Ty::EvolvingMap(
            Box::new(erase_typevars_matching(key, should_erase)),
            Box::new(erase_typevars_matching(value, should_erase)),
            attr.clone(),
        ),
        Ty::Union(members, attr) => Ty::Union(
            members
                .iter()
                .map(|member| erase_typevars_matching(member, should_erase))
                .collect(),
            attr.clone(),
        ),
        Ty::Future(value, error, attr) => Ty::Future(
            Box::new(erase_typevars_matching(value, should_erase)),
            Box::new(erase_typevars_matching(error, should_erase)),
            attr.clone(),
        ),
        Ty::AssociatedTypeProjection {
            base,
            interface,
            member,
            attr,
        } => Ty::AssociatedTypeProjection {
            base: Box::new(erase_typevars_matching(base, should_erase)),
            interface: Box::new(interface.map_tys(|t| erase_typevars_matching(t, should_erase))),
            member: member.clone(),
            attr: attr.clone(),
        },
        Ty::Function {
            params,
            ret,
            throws,
            attr,
        } => Ty::Function {
            params: params
                .iter()
                .map(|param| FunctionParamTy {
                    name: param.name.clone(),
                    ty: erase_typevars_matching(&param.ty, should_erase),
                    mode: param.mode,
                })
                .collect(),
            ret: Box::new(erase_typevars_matching(ret, should_erase)),
            throws: Box::new(erase_typevars_matching(throws, should_erase)),
            attr: attr.clone(),
        },
        _ => ty.clone(),
    }
}

#[cfg(test)]
mod tests {
    use baml_base::TyAttr;
    use baml_type::{Interface, QualifiedTypeName};

    use super::*;

    fn attr() -> TyAttr {
        TyAttr::default()
    }

    fn err() -> Ty {
        Ty::Error { attr: attr() }
    }

    fn unknown() -> Ty {
        Ty::Unknown { attr: attr() }
    }

    fn int() -> Ty {
        Ty::Int { attr: attr() }
    }

    fn qn(name: &str) -> QualifiedTypeName {
        QualifiedTypeName::new(Name::new("test"), vec![], Name::new(name))
    }

    #[test]
    fn sentinels_match_directly() {
        assert!(contains_error_recovery(&err()));
        assert!(contains_error_recovery(&unknown()));
        assert!(!contains_error_recovery(&int()));
    }

    #[test]
    fn descends_into_lists() {
        assert!(contains_error_recovery(&Ty::List(Box::new(err()), attr())));
        assert!(contains_error_recovery(&Ty::EvolvingList(
            Box::new(unknown()),
            attr()
        )));
        assert!(!contains_error_recovery(&Ty::List(Box::new(int()), attr())));
    }

    #[test]
    fn descends_into_map_key_and_value() {
        let map = |key: Ty, value: Ty| Ty::Map {
            key: Box::new(key),
            value: Box::new(value),
            attr: attr(),
        };
        assert!(contains_error_recovery(&map(err(), int())));
        assert!(contains_error_recovery(&map(int(), err())));
        assert!(!contains_error_recovery(&map(int(), int())));
        assert!(contains_error_recovery(&Ty::EvolvingMap(
            Box::new(int()),
            Box::new(err()),
            attr()
        )));
    }

    #[test]
    fn descends_into_unions_and_futures() {
        assert!(contains_error_recovery(&Ty::Union(
            vec![int(), err()],
            attr()
        )));
        assert!(!contains_error_recovery(&Ty::Union(
            vec![int(), int()],
            attr()
        )));
        assert!(contains_error_recovery(&Ty::Future(
            Box::new(err()),
            Box::new(int()),
            attr()
        )));
        assert!(contains_error_recovery(&Ty::Future(
            Box::new(int()),
            Box::new(err()),
            attr()
        )));
    }

    #[test]
    fn descends_into_function_params_ret_and_throws() {
        let func = |param: Ty, ret: Ty, throws: Ty| Ty::Function {
            params: vec![FunctionParamTy::required(None, param)],
            ret: Box::new(ret),
            throws: Box::new(throws),
            attr: attr(),
        };
        assert!(contains_error_recovery(&func(err(), int(), int())));
        assert!(contains_error_recovery(&func(int(), err(), int())));
        assert!(contains_error_recovery(&func(int(), int(), err())));
        assert!(!contains_error_recovery(&func(int(), int(), int())));
    }

    #[test]
    fn descends_into_class_and_interface_type_args() {
        assert!(contains_error_recovery(&Ty::Class(
            qn("Box"),
            vec![err()],
            attr()
        )));
        assert!(!contains_error_recovery(&Ty::Class(
            qn("Box"),
            vec![int()],
            attr()
        )));
        assert!(contains_error_recovery(&Ty::Interface(
            qn("BoxLike"),
            vec![err()],
            vec![],
            attr()
        )));
        assert!(contains_error_recovery(&Ty::Interface(
            qn("Iter"),
            vec![],
            vec![(Name::new("Item"), err())],
            attr()
        )));
    }

    #[test]
    fn descends_into_projection_base_and_interface() {
        let projection = |base: Ty, iface_generic: Ty| Ty::AssociatedTypeProjection {
            base: Box::new(base),
            interface: Box::new(Interface::new(qn("Iter"), vec![iface_generic], vec![])),
            member: Name::new("Item"),
            attr: attr(),
        };
        assert!(contains_error_recovery(&projection(err(), int())));
        assert!(contains_error_recovery(&projection(int(), err())));
        assert!(!contains_error_recovery(&projection(int(), int())));
    }

    #[test]
    fn contains_typevar_where_filters_by_name() {
        let t = Ty::List(Box::new(Ty::TypeVar(Name::new("T"), attr())), attr());
        assert!(contains_typevar(&t));
        assert!(contains_typevar_where(&t, &|n| n.as_str() == "T"));
        assert!(!contains_typevar_where(&t, &|n| n.as_str() == "U"));
    }
}
