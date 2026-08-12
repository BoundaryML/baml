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

use rustc_hash::FxHashMap;

use crate::ty::{FunctionParamTy, ParamTy, Ty, TyAttr};

// ── Type variable binding ─────────────────────────────────────────────────────

pub(crate) fn identity_bindings(generic_params: &[ParamTy]) -> FxHashMap<ParamTy, Ty> {
    generic_params
        .iter()
        .map(|param| (param.clone(), Ty::TypeVar(param.clone(), TyAttr::default())))
        .collect()
}

// ── Type substitution ─────────────────────────────────────────────────────────

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
pub fn is_value_call_inferable(param: &ParamTy, generic_params: &[ParamTy]) -> bool {
    generic_params.contains(param) || crate::ty::is_synthetic_effect_param(param.name())
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
pub fn contains_non_rigid_typevar(ty: &Ty, rigid: &[ParamTy]) -> bool {
    contains_typevar_where(ty, &|param| !rigid.iter().any(|rigid| rigid == param))
}

// ── Type variable inference & union normalization ──────────────────────────────
//
// The pure `Ty`-walking inference/union primitives now live in `baml_type` so the
// runtime engine can share the single algorithm (it widens `RuntimeTy` → `Ty`,
// runs the unifier, narrows back) without a runtime → compiler dependency. They
// are re-exported here so every existing `crate::generics::…` caller is
// unchanged. See `01c-inbound-inference-reuse.md`.
pub use baml_type_runtime::{
    bind_type_vars, contains_error_recovery, contains_ty_where, contains_typevar,
    contains_typevar_where, erase_typevars_matching, infer_bindings, infer_bindings_allow_typevars,
    infer_bindings_rigid_self, normalize_union_members, substitute_ty, union_ty,
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

#[cfg(test)]
mod tests {
    use baml_base::{Name, TyAttr};
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
        let t = Ty::List(Box::new(Ty::type_var("T")), attr());
        assert!(contains_typevar(&t));
        assert!(contains_typevar_where(&t, &|n| n.as_str() == "T"));
        assert!(!contains_typevar_where(&t, &|n| n.as_str() == "U"));
    }

    #[test]
    fn substitution_does_not_conflate_same_named_parameters() {
        let outer = baml_type::ParamTy::new(0, Name::new("E"));
        let inner = baml_type::ParamTy::new(1, Name::new("E"));
        let mut bindings = rustc_hash::FxHashMap::default();
        bindings.insert(outer.clone(), Ty::int());

        assert_eq!(
            substitute_ty(&Ty::TypeVar(outer, attr()), &bindings),
            Ty::int()
        );
        assert_eq!(
            substitute_ty(&Ty::TypeVar(inner.clone(), attr()), &bindings),
            Ty::TypeVar(inner, attr())
        );
    }
}
