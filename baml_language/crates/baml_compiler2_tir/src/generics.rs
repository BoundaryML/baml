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
//! 2. It provides the concrete type arguments (e.g. `[Ty::Primitive(Int)]`).
//! 3. `bind_type_vars` zips them together: `{T → int}`.
//! 4. For each method parameter/return type, `lower_type_expr_with_generics`
//!    is called: if the `TypeExpr` is a `Path(["T"])` that matches a bound
//!    variable, it returns the bound concrete type directly; otherwise it
//!    falls through to normal `lower_type_expr` and then applies
//!    `substitute_ty` to replace any residual type-variable references.

use baml_base::Name;
use baml_compiler2_ast::TypeExpr;
use rustc_hash::FxHashMap;

use crate::{infer_context::TirTypeError, lower_type_expr::lower_type_expr, ty::Ty};

// ── Type variable binding ─────────────────────────────────────────────────────

/// Bind type variables from generic params to concrete type arguments.
///
/// Example: `bind_type_vars(&["T"], &[Ty::Primitive(Int)])` → `{"T" → Int}`
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
        Ty::List(inner) => Ty::List(Box::new(substitute_ty(inner, bindings))),
        Ty::Map(k, v) => Ty::Map(
            Box::new(substitute_ty(k, bindings)),
            Box::new(substitute_ty(v, bindings)),
        ),
        Ty::Optional(inner) => Ty::Optional(Box::new(substitute_ty(inner, bindings))),
        Ty::Union(members) => {
            Ty::Union(members.iter().map(|m| substitute_ty(m, bindings)).collect())
        }
        Ty::Function { params, ret } => Ty::Function {
            params: params
                .iter()
                .map(|(n, t)| (n.clone(), substitute_ty(t, bindings)))
                .collect(),
            ret: Box::new(substitute_ty(ret, bindings)),
        },
        // All other types are leaves (primitives, class refs, enums, etc.) — pass through.
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
        TypeExpr::Path(segments) if segments.len() == 1 => bindings.get(&segments[0]).cloned(),
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
pub fn lower_type_expr_with_generics(
    db: &dyn crate::Db,
    expr: &TypeExpr,
    package_items: &baml_compiler2_hir::package::PackageItems<'_>,
    bindings: &FxHashMap<Name, Ty>,
    diagnostics: &mut Vec<TirTypeError>,
) -> Ty {
    // Fast path: empty bindings — no substitution needed.
    if bindings.is_empty() {
        return lower_type_expr(db, expr, package_items, diagnostics);
    }

    // Intercept single-segment paths that are type variables.
    if let Some(ty) = substitute_type_expr(expr, bindings) {
        return ty;
    }

    // For composite types (List, Map, Optional, Union), recurse with substitution
    // rather than lowering first then substituting, so that type-variable references
    // in nested positions are also intercepted before triggering "unresolved type".
    match expr {
        TypeExpr::Optional(inner) => Ty::Optional(Box::new(lower_type_expr_with_generics(
            db,
            inner,
            package_items,
            bindings,
            diagnostics,
        ))),
        TypeExpr::List(inner) => Ty::List(Box::new(lower_type_expr_with_generics(
            db,
            inner,
            package_items,
            bindings,
            diagnostics,
        ))),
        TypeExpr::Map { key, value } => Ty::Map(
            Box::new(lower_type_expr_with_generics(
                db,
                key,
                package_items,
                bindings,
                diagnostics,
            )),
            Box::new(lower_type_expr_with_generics(
                db,
                value,
                package_items,
                bindings,
                diagnostics,
            )),
        ),
        TypeExpr::Union(members) => Ty::Union(
            members
                .iter()
                .map(|m| lower_type_expr_with_generics(db, m, package_items, bindings, diagnostics))
                .collect(),
        ),
        TypeExpr::Function { params, ret } => Ty::Function {
            params: params
                .iter()
                .map(|p| {
                    (
                        p.name.clone(),
                        lower_type_expr_with_generics(
                            db,
                            &p.ty,
                            package_items,
                            bindings,
                            diagnostics,
                        ),
                    )
                })
                .collect(),
            ret: Box::new(lower_type_expr_with_generics(
                db,
                ret,
                package_items,
                bindings,
                diagnostics,
            )),
        },
        // For all other type expressions (primitives, multi-segment paths, etc.),
        // lower normally and then substitute in the result.
        other => {
            let ty = lower_type_expr(db, other, package_items, diagnostics);
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
pub fn skip_self_param(params: &[(Option<Name>, Ty)]) -> &[(Option<Name>, Ty)] {
    match params.first() {
        Some((Some(name), _)) if name.as_str() == "self" => &params[1..],
        _ => params,
    }
}
