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
//! 2. It provides the concrete type arguments (e.g. `[Ty::Primitive(Int, TyAttr::default())]`).
//! 3. `bind_type_vars` zips them together: `{T → int}`.
//! 4. For each method parameter/return type, `lower_type_expr_with_generics`
//!    is called: if the `TypeExpr` is a `Path(["T"])` that matches a bound
//!    variable, it returns the bound concrete type directly; otherwise it
//!    falls through to normal `lower_type_expr` and then applies
//!    `substitute_ty` to replace any residual type-variable references.

use baml_base::Name;
use baml_compiler2_ast::TypeExpr;
use rustc_hash::FxHashMap;

use crate::{
    infer_context::TirTypeError,
    lower_type_expr::lower_type_expr_in_ns,
    ty::{FunctionParamMode, FunctionParamTy, Ty, TyAttr},
};

// ── Type variable binding ─────────────────────────────────────────────────────

/// Bind type variables from generic params to concrete type arguments.
///
/// Example: `bind_type_vars(&["T"], &[Ty::Primitive(Int, TyAttr::default())])` → `{"T" → Int}`
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
/// Recursively walks the type, replacing any `Ty::Unknown` that corresponds to
/// an unresolved type variable. In practice, type variables that were not
/// resolved by `lower_type_expr` appear as `Ty::Unknown`.
///
/// Note: we cannot distinguish "T was an unknown type variable" from "T was a
/// genuinely unresolvable name" at the `Ty` level. That ambiguity is resolved
/// by `lower_type_expr_with_generics`, which intercepts type-variable paths
/// at the `TypeExpr` level (before `lower_type_expr` produces `Ty::Unknown`).
pub fn substitute_ty(ty: &Ty, bindings: &FxHashMap<Name, Ty>) -> Ty {
    if bindings.is_empty() {
        return ty.clone();
    }
    match ty {
        Ty::TypeVar(name, _) => bindings.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Ty::List(inner, attr) => Ty::List(Box::new(substitute_ty(inner, bindings)), attr.clone()),
        Ty::Map(k, v, attr) => Ty::Map(
            Box::new(substitute_ty(k, bindings)),
            Box::new(substitute_ty(v, bindings)),
            attr.clone(),
        ),
        Ty::Optional(inner, attr) => {
            Ty::Optional(Box::new(substitute_ty(inner, bindings)), attr.clone())
        }
        Ty::Union(members, attr) => Ty::Union(
            members.iter().map(|m| substitute_ty(m, bindings)).collect(),
            attr.clone(),
        ),
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
                    ty: substitute_ty(&param.ty, bindings),
                    mode: param.mode,
                })
                .collect(),
            ret: Box::new(substitute_ty(ret, bindings)),
            throws: Box::new(substitute_ty(throws, bindings)),
            attr: attr.clone(),
        },
        Ty::Class(name, type_args, attr) => {
            let substituted_args: Vec<Ty> = type_args
                .iter()
                .map(|t| substitute_ty(t, bindings))
                .collect();
            Ty::Class(name.clone(), substituted_args, attr.clone())
        }
        // All other types are leaves (primitives, enums, etc.) — pass through.
        _ => ty.clone(),
    }
}

// ── TypeExpr-level substitution ───────────────────────────────────────────────

/// Check if a `TypeExpr` is a single-segment path that matches a bound type variable.
///
/// Returns `Some(bound_ty)` if the expression is `Path(["T"])` and `"T"` is in
/// `bindings`. Returns `None` if it's not a type variable reference.
///
/// This is called at the `TypeExpr` level, before `lower_type_expr`, so we can
/// intercept `T` references that would otherwise produce `Ty::Unknown`.
fn substitute_type_expr(expr: &TypeExpr, bindings: &FxHashMap<Name, Ty>) -> Option<Ty> {
    match expr {
        TypeExpr::Path { segments, .. } if segments.len() == 1 => {
            bindings.get(&segments[0]).cloned()
        }
        _ => None,
    }
}

// ── Combined lowering with generic substitution ───────────────────────────────

/// Lower a `TypeExpr` to `Ty` with type variable substitution applied.
///
/// For complex type expressions (e.g. `T[]`, `map<K, V>`, `V?`), first lowers
/// normally then substitutes type variables in the result. For single-segment
/// paths that directly name a type variable (e.g. `T`, `K`, `V`), intercepts
/// before lowering to avoid the "unresolved type" diagnostic that `lower_type_expr`
/// would otherwise emit.
///
/// Diagnostics from the lowering step (for non-variable paths that genuinely
/// don't exist) are collected into `diagnostics`.
///
/// `ns_context` is the defining file's namespace within its package (e.g. `["llm"]`
/// for `<builtin>/baml/llm/llm.baml`); unqualified type paths resolve there first.
pub fn lower_type_expr_with_generics(
    db: &dyn crate::Db,
    expr: &TypeExpr,
    package_items: &baml_compiler2_hir::package::PackageItems<'_>,
    ns_context: &[Name],
    bindings: &FxHashMap<Name, Ty>,
    diagnostics: &mut Vec<TirTypeError>,
) -> Ty {
    // Fast path: empty bindings — no substitution needed.
    if bindings.is_empty() {
        return lower_type_expr_in_ns(db, expr, package_items, ns_context, &[], diagnostics);
    }

    // Intercept single-segment paths that are type variables.
    if let Some(ty) = substitute_type_expr(expr, bindings) {
        return ty;
    }

    // For composite types (List, Map, Optional, Union), recurse with substitution
    // rather than lowering first then substituting, so that type-variable references
    // in nested positions are also intercepted before triggering "unresolved type".
    match expr {
        TypeExpr::Optional { inner, .. } => Ty::Optional(
            Box::new(lower_type_expr_with_generics(
                db,
                inner,
                package_items,
                ns_context,
                bindings,
                diagnostics,
            )),
            TyAttr::default(),
        ),
        TypeExpr::List { inner, .. } => Ty::List(
            Box::new(lower_type_expr_with_generics(
                db,
                inner,
                package_items,
                ns_context,
                bindings,
                diagnostics,
            )),
            TyAttr::default(),
        ),
        TypeExpr::Map { key, value, .. } => Ty::Map(
            Box::new(lower_type_expr_with_generics(
                db,
                key,
                package_items,
                ns_context,
                bindings,
                diagnostics,
            )),
            Box::new(lower_type_expr_with_generics(
                db,
                value,
                package_items,
                ns_context,
                bindings,
                diagnostics,
            )),
            TyAttr::default(),
        ),
        TypeExpr::Union {
            variants: members, ..
        } => Ty::Union(
            members
                .iter()
                .map(|m| {
                    lower_type_expr_with_generics(
                        db,
                        m,
                        package_items,
                        ns_context,
                        bindings,
                        diagnostics,
                    )
                })
                .collect(),
            TyAttr::default(),
        ),
        TypeExpr::Function {
            params,
            ret,
            throws,
            ..
        } => Ty::Function {
            params: params
                .iter()
                .map(|p| FunctionParamTy {
                    name: p.name.clone(),
                    ty: lower_type_expr_with_generics(
                        db,
                        &p.ty,
                        package_items,
                        ns_context,
                        bindings,
                        diagnostics,
                    ),
                    mode: if p.optional {
                        FunctionParamMode::Optional
                    } else {
                        FunctionParamMode::Required
                    },
                })
                .collect(),
            ret: Box::new(lower_type_expr_with_generics(
                db,
                ret,
                package_items,
                ns_context,
                bindings,
                diagnostics,
            )),
            throws: Box::new(
                throws
                    .as_deref()
                    .map(|throws| {
                        lower_type_expr_with_generics(
                            db,
                            throws,
                            package_items,
                            ns_context,
                            bindings,
                            diagnostics,
                        )
                    })
                    .unwrap_or(Ty::Never {
                        attr: TyAttr::default(),
                    }),
            ),
            attr: TyAttr::default(),
        },
        // For all other type expressions (primitives, multi-segment paths, etc.),
        // lower normally and then substitute in the result.
        //
        // We pass the binding keys as `generic_params` so that nested type variable
        // references inside path generic args (e.g. `StreamCache<T, S>`) are preserved
        // as `Ty::TypeVar` by `lower_type_expr_in_ns` rather than triggering "unresolved
        // type" diagnostics. `substitute_ty` then replaces those TypeVars with the
        // concrete bound types.
        other => {
            let binding_keys: Vec<Name> = bindings.keys().cloned().collect();
            let ty = lower_type_expr_in_ns(
                db,
                other,
                package_items,
                ns_context,
                &binding_keys,
                diagnostics,
            );
            substitute_ty(&ty, bindings)
        }
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

/// Check if a type contains any `Ty::TypeVar` anywhere in its structure.
pub fn contains_typevar(ty: &Ty) -> bool {
    match ty {
        Ty::TypeVar(_, _) => true,
        Ty::List(inner, _) | Ty::Optional(inner, _) | Ty::EvolvingList(inner, _) => {
            contains_typevar(inner)
        }
        Ty::Map(k, v, _) | Ty::EvolvingMap(k, v, _) => contains_typevar(k) || contains_typevar(v),
        Ty::Union(tys, _) => tys.iter().any(contains_typevar),
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params.iter().any(|param| contains_typevar(&param.ty))
                || contains_typevar(ret)
                || contains_typevar(throws)
        }
        Ty::Class(_, type_args, _) => type_args.iter().any(contains_typevar),
        _ => false,
    }
}

/// Infer type variable bindings by walking formal and actual types in parallel.
///
/// When `formal` is `Ty::TypeVar("T", TyAttr::default())` and `actual` is `Ty::Primitive(Int, TyAttr::default())`,
/// records `T → int` in `bindings`. For structural types, recurses into
/// matching structures. Conflicting inferences are merged via `union_ty`.
fn infer_bindings_inner(
    formal: &Ty,
    actual: &Ty,
    bindings: &mut FxHashMap<Name, Ty>,
    allow_typevar_actuals: bool,
) {
    match (formal, actual) {
        (Ty::TypeVar(name, _), actual_ty) => {
            // Skip TypeVar-to-TypeVar bindings by default — they usually provide
            // no information for ordinary call inference. Some higher-order
            // callable-summary paths opt into preserving them explicitly.
            if !allow_typevar_actuals && matches!(actual_ty, Ty::TypeVar(_, _)) {
                return;
            }
            bindings
                .entry(name.clone())
                .and_modify(|existing| *existing = union_ty(existing, actual_ty))
                .or_insert_with(|| actual_ty.clone());
        }
        (Ty::List(f, _), Ty::List(a, _)) => {
            infer_bindings_inner(f, a, bindings, allow_typevar_actuals);
        }
        (Ty::Map(fk, fv, _), Ty::Map(ak, av, _)) => {
            infer_bindings_inner(fk, ak, bindings, allow_typevar_actuals);
            infer_bindings_inner(fv, av, bindings, allow_typevar_actuals);
        }
        (Ty::Optional(f, _), Ty::Optional(a, _)) => {
            infer_bindings_inner(f, a, bindings, allow_typevar_actuals);
        }
        (
            Ty::Function {
                params: fp,
                ret: fr,
                throws: fth,
                ..
            },
            Ty::Function {
                params: ap,
                ret: ar,
                throws: ath,
                ..
            },
        ) => {
            for (fp, ap) in fp.iter().zip(ap.iter()) {
                infer_bindings_inner(&fp.ty, &ap.ty, bindings, allow_typevar_actuals);
            }
            infer_bindings_inner(fr, ar, bindings, allow_typevar_actuals);
            infer_bindings_inner(fth, ath, bindings, allow_typevar_actuals);
        }
        (Ty::Class(fn_name, f_args, _), Ty::Class(an_name, a_args, _)) if fn_name == an_name => {
            for (ft, at) in f_args.iter().zip(a_args.iter()) {
                infer_bindings_inner(ft, at, bindings, allow_typevar_actuals);
            }
        }
        // Builtin container bridging: Array<T> ↔ List(T), Map<K,V> ↔ Map(K,V)
        // This enables UFCS calls like `Array.length(arr)` where the formal self
        // type is Class(Array, [T]) and the actual is List(int).
        (Ty::Class(class_name, f_args, _), Ty::List(actual_inner, _))
            if class_name.is_builtin_root_type("Array") && f_args.len() == 1 =>
        {
            infer_bindings_inner(&f_args[0], actual_inner, bindings, allow_typevar_actuals);
        }
        (Ty::Class(class_name, f_args, _), Ty::Map(actual_key, actual_val, _))
            if class_name.is_builtin_root_type("Map") && f_args.len() == 2 =>
        {
            infer_bindings_inner(&f_args[0], actual_key, bindings, allow_typevar_actuals);
            infer_bindings_inner(&f_args[1], actual_val, bindings, allow_typevar_actuals);
        }
        _ => {} // Concrete types: nothing to infer
    }
}

pub fn infer_bindings(formal: &Ty, actual: &Ty, bindings: &mut FxHashMap<Name, Ty>) {
    infer_bindings_inner(formal, actual, bindings, false);
}

pub fn infer_bindings_allow_typevars(formal: &Ty, actual: &Ty, bindings: &mut FxHashMap<Name, Ty>) {
    infer_bindings_inner(formal, actual, bindings, true);
}

/// Combine two types into a union, deduplicating members.
///
/// Used when the same type variable is inferred from multiple arguments
/// (e.g., `deep_equals(myInt, myString)` → `T` gets `int` then `string`).
pub fn union_ty(a: &Ty, b: &Ty) -> Ty {
    if a == b {
        return a.clone();
    }
    let mut members = Vec::new();
    match a {
        Ty::Union(tys, _) => members.extend(tys.iter().cloned()),
        other => members.push(other.clone()),
    }
    match b {
        Ty::Union(tys, _) => {
            for t in tys {
                if !members.contains(t) {
                    members.push(t.clone());
                }
            }
        }
        other => {
            if !members.contains(other) {
                members.push(other.clone());
            }
        }
    }
    if members.len() == 1 {
        members.pop().unwrap()
    } else {
        // TODO(TyAttr): This union is synthesized from two input types — there's no single
        // "original attr" to preserve. If both inputs are unions with different attrs, which
        // one wins? May need a merge/lattice operation on TyAttr, or TyAttr::default() may
        // be correct if attrs describe declaration sites rather than computed types.
        Ty::Union(members, TyAttr::default())
    }
}

/// Replace any remaining `Ty::TypeVar` with `Ty::Unknown` and emit diagnostics.
///
/// Called after call-site inference to ensure no type variables escape to
/// VIR/runtime. Each erased `TypeVar` produces a `CannotInferTypeParameter`
/// diagnostic.
#[allow(clippy::only_used_in_recursion)] // diagnostics param kept for future use
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
        Ty::Map(k, v, attr) => Ty::Map(
            Box::new(erase_unresolved_typevars(k, diagnostics)),
            Box::new(erase_unresolved_typevars(v, diagnostics)),
            attr.clone(),
        ),
        Ty::Optional(inner, attr) => Ty::Optional(
            Box::new(erase_unresolved_typevars(inner, diagnostics)),
            attr.clone(),
        ),
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
        other => other.clone(),
    }
}
