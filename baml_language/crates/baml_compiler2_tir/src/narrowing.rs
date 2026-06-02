//! Type narrowing for control-flow-dependent type refinement.
//!
//! Analyzes condition expressions to produce narrowing information that refines
//! variable types inside then/else branches of `if` expressions, and after
//! diverging branches (early-return narrowing).
//!
//! ## Patterns recognized
//!
//! - `x != null` → then: remove null, else: null
//! - `x == null` → then: null, else: remove null
//! - `x` (truthiness) → then: remove null, else: original
//! - `!(x == null)` → same as `x != null` (negation flips then/else)
//! - `x is <pattern>` → then: pattern's matched type, else: scrutinee
//!   union minus the matched members (TypeScript-style for `typeof` /
//!   `instanceof`; falls back to the original scrutinee type when the
//!   subtraction doesn't simplify, e.g. literal patterns on primitives)
//!
//! ## Early-return narrowing
//!
//! When an `if` expression's then-branch always diverges (returns/breaks), the
//! else-narrowings are applied to the rest of the enclosing block. This makes
//! patterns like:
//!
//! ```baml
//! if (x == null) { return 0; }
//! // Here x: T (not T?)
//! ```
//!
//! type-check without errors.

use baml_base::Name;
use baml_compiler2_ast::{BinaryOp, Expr, ExprBody, ExprId, PatId, UnaryOp};
use rustc_hash::FxHashMap;

use crate::{
    builder::LocalBinding,
    ty::{PrimitiveType, Ty, TyAttr},
};

// ── Narrowing descriptor ──────────────────────────────────────────────────────

/// A single narrowing: how a named variable's type is refined in then/else branches.
#[derive(Debug, Clone)]
pub struct Narrowing {
    /// The local variable whose type is narrowed.
    pub name: Name,
    /// Refined type in the then-branch (condition is true).
    pub then_type: Ty,
    /// Refined type in the else-branch (condition is false).
    pub else_type: Ty,
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Extract narrowing information from a condition expression.
///
/// Walks the condition looking for patterns that constrain a local variable's
/// type. Returns a `Vec<Narrowing>` — one per narrowed variable. Most conditions
/// narrow zero or one variable; `&&` chains could narrow multiple, but we don't
/// handle that yet.
///
/// # Parameters
/// - `condition`: the `ExprId` of the condition expression (already inferred)
/// - `body`: the `ExprBody` arena for the current scope
/// - `expr_types`: the accumulated expression type map (from `self.expressions`
///   in the builder — expressions inferred *before* the condition are present)
/// - `pattern_types`: the per-`PatId` matched-type map (from `self.pattern_types`
///   in the builder — populated when `<expr> is <pattern>` is inferred). Read
///   only by the `Expr::Is` branch; null-only conditions ignore it.
pub fn extract_narrowings(
    condition: ExprId,
    body: &ExprBody,
    expr_types: &FxHashMap<ExprId, Ty>,
    pattern_types: &FxHashMap<PatId, Ty>,
) -> Vec<Narrowing> {
    let mut narrowings = Vec::new();
    collect_narrowings(
        condition,
        body,
        expr_types,
        pattern_types,
        false,
        &mut narrowings,
    );
    narrowings
}

/// Internal recursive collector. `negated` tracks whether we're inside a `!`.
fn collect_narrowings(
    expr_id: ExprId,
    body: &ExprBody,
    expr_types: &FxHashMap<ExprId, Ty>,
    pattern_types: &FxHashMap<PatId, Ty>,
    negated: bool,
    out: &mut Vec<Narrowing>,
) {
    let expr = &body.exprs[expr_id];
    match expr {
        // x == null / x != null  (or with null on the left). For `==` the
        // then-branch is null; for `!=` it's the non-null type. An enclosing
        // `!` flips which branch sees null.
        Expr::Binary {
            op: op @ (BinaryOp::Eq | BinaryOp::Ne),
            lhs,
            rhs,
        } => {
            if let Some((name, original_ty)) = null_check_name(*lhs, *rhs, body, expr_types) {
                let null_when_true = matches!(op, BinaryOp::Eq) ^ negated;
                let (then_type, else_type) = if null_when_true {
                    (null_ty(), remove_null(&original_ty))
                } else {
                    (remove_null(&original_ty), null_ty())
                };
                out.push(Narrowing {
                    name,
                    then_type,
                    else_type,
                });
            }
        }

        // !(inner) — flip then/else by recursing with toggled negation
        Expr::Unary {
            op: UnaryOp::Not,
            expr: inner,
        } => {
            collect_narrowings(*inner, body, expr_types, pattern_types, !negated, out);
        }

        // `name is <pattern>` — narrow `name` to the pattern's matched type
        // in the then-branch (or in the else-branch if negated). The
        // opposite branch tries to subtract the matched members from the
        // scrutinee union, TypeScript-style; when subtraction can't
        // simplify (literal pattern over a primitive, single non-union
        // scrutinee, no overlap, …) it falls back to the original.
        Expr::Is { scrutinee, pattern } => {
            let Some((name, original_ty)) = local_name_and_ty(*scrutinee, body, expr_types) else {
                return;
            };
            let Some(matched_ty) = pattern_types.get(pattern) else {
                return;
            };
            let complement = subtract_pattern_type(original_ty, matched_ty);
            let (then_type, else_type) = if negated {
                (complement, matched_ty.clone())
            } else {
                (matched_ty.clone(), complement)
            };
            out.push(Narrowing {
                name: name.clone(),
                then_type,
                else_type,
            });
        }

        // Truthiness: if (x) where x is optional — then-branch removes null
        Expr::Path(_) => {
            if let Some((name, ty)) = local_name_and_ty(expr_id, body, expr_types) {
                // Only narrow if the type is optional / nullable
                if is_nullable(ty) {
                    let (then_type, else_type) = if negated {
                        // !x — then: null/falsy, else: non-null
                        (ty.clone(), remove_null(ty))
                    } else {
                        // x — then: non-null, else: original (might still be null)
                        (remove_null(ty), ty.clone())
                    };
                    out.push(Narrowing {
                        name: name.clone(),
                        then_type,
                        else_type,
                    });
                }
            }
        }

        // For all other patterns, no narrowing is extracted.
        _ => {}
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// If `id` is a single-segment path referring to a local whose type is known,
/// return its name and recorded type.
fn local_name_and_ty<'a>(
    id: ExprId,
    body: &'a ExprBody,
    expr_types: &'a FxHashMap<ExprId, Ty>,
) -> Option<(&'a Name, &'a Ty)> {
    let Expr::Path(segments) = &body.exprs[id] else {
        return None;
    };
    if segments.len() != 1 {
        return None;
    }
    Some((&segments[0], expr_types.get(&id)?))
}

/// Check if a binary comparison is `name op null` or `null op name`.
///
/// Returns `(variable_name, original_type)` if one side is a single-segment
/// path name (referring to a local whose type is known) and the other side is
/// a `null` literal.
fn null_check_name(
    lhs: ExprId,
    rhs: ExprId,
    body: &ExprBody,
    expr_types: &FxHashMap<ExprId, Ty>,
) -> Option<(Name, Ty)> {
    // Whichever side is a known local, the other side must be `null`.
    for (name_side, null_side) in [(lhs, rhs), (rhs, lhs)] {
        if matches!(&body.exprs[null_side], Expr::Null) {
            if let Some((name, ty)) = local_name_and_ty(name_side, body, expr_types) {
                return Some((name.clone(), ty.clone()));
            }
        }
    }
    None
}

/// Returns `true` if the type contains a null component (is optional or union
/// with null) or is directly `Null`.
fn is_nullable(ty: &Ty) -> bool {
    match ty {
        Ty::Optional(_, _) => true,
        Ty::Primitive(PrimitiveType::Null, _) => true,
        Ty::Union(members, _) => members
            .iter()
            .any(|m| matches!(m, Ty::Primitive(PrimitiveType::Null, _))),
        _ => false,
    }
}

/// Compute the complement of `matched` within `scrutinee` for the
/// else-branch of a pattern test. Decomposes top-level unions on both
/// sides and keeps each scrutinee member whose "shape" (ignoring
/// `TyAttr` and `Freshness`) doesn't appear in `matched`.
///
/// Falls back to the original scrutinee when:
///   - the scrutinee isn't a union (no members to drop)
///   - no scrutinee member matched (preserve fidelity)
///   - subtraction would leave an empty union (pattern covered everything;
///     callers will handle the else side being effectively dead)
///
/// We deliberately avoid `Ty`'s derived `PartialEq` because that compares
/// `TyAttr` exactly — narrowing-derived types frequently carry
/// `TyAttr::default()` while source-derived members may not. Comparing by
/// shape only is sound for the membership test we need here.
pub(crate) fn subtract_pattern_type(scrutinee: &Ty, matched: &Ty) -> Ty {
    let matched_set: Vec<&Ty> = match matched {
        Ty::Union(members, _) => members.iter().collect(),
        _ => vec![matched],
    };

    let scrut_members: Vec<&Ty> = match scrutinee {
        Ty::Union(members, _) => members.iter().collect(),
        _ => return scrutinee.clone(),
    };

    let remaining: Vec<Ty> = scrut_members
        .iter()
        .filter(|m| !matched_set.iter().any(|w| ty_shape_eq(m, w)))
        .map(|m| (*m).clone())
        .collect();

    if remaining.len() == scrut_members.len() {
        // No member subtracted — return the original to avoid pretending we
        // refined when we didn't.
        return scrutinee.clone();
    }

    crate::ty::union_of(remaining, TyAttr::default())
}

/// Fixed `null` type constructor used by null-narrowing.
fn null_ty() -> Ty {
    Ty::Primitive(PrimitiveType::Null, TyAttr::default())
}

/// Fixed `never` type constructor used by union collapse.
fn never_ty() -> Ty {
    Ty::Never {
        attr: TyAttr::default(),
    }
}

/// Structural shape-equality for `Ty` that ignores `TyAttr` and literal
/// `Freshness`. Used only by [`subtract_pattern_type`] to decide whether a
/// scrutinee union member is "covered" by the pattern's matched set.
///
/// Returns `false` for compound shapes we don't bother decomposing
/// (function types, type variables, error/unknown). Returning `false`
/// keeps the member in the residual — that's the conservative direction
/// (less narrowing, never unsound).
fn ty_shape_eq(a: &Ty, b: &Ty) -> bool {
    match (a, b) {
        (Ty::Primitive(p1, _), Ty::Primitive(p2, _)) => p1 == p2,
        (Ty::Class(n1, args1, _), Ty::Class(n2, args2, _)) => {
            n1 == n2
                && args1.len() == args2.len()
                && args1.iter().zip(args2).all(|(a, b)| ty_shape_eq(a, b))
        }
        (Ty::Enum(n1, _), Ty::Enum(n2, _)) => n1 == n2,
        (Ty::EnumVariant(n1, v1, _), Ty::EnumVariant(n2, v2, _)) => n1 == n2 && v1 == v2,
        (Ty::TypeAlias(n1, _), Ty::TypeAlias(n2, _)) => n1 == n2,
        (Ty::List(t1, _), Ty::List(t2, _)) => ty_shape_eq(t1, t2),
        (Ty::Map(k1, v1, _), Ty::Map(k2, v2, _)) => ty_shape_eq(k1, k2) && ty_shape_eq(v1, v2),
        (Ty::Optional(t1, _), Ty::Optional(t2, _)) => ty_shape_eq(t1, t2),
        (Ty::Literal(l1, _, _), Ty::Literal(l2, _, _)) => l1 == l2,
        _ => false,
    }
}

/// Remove null from a type, producing the non-null inner type.
///
/// | Input               | Output                     |
/// |---------------------|----------------------------|
/// | `T?`                | `T`                        |
/// | `A \| null \| B`    | `A` (if single) or `A \| B`|
/// | `null`              | `never`                    |
/// | `T` (not nullable)  | `T` (unchanged)            |
pub fn remove_null(ty: &Ty) -> Ty {
    match ty {
        Ty::Optional(inner, _) => inner.as_ref().clone(),
        Ty::Union(members, _) => {
            let filtered: Vec<Ty> = members
                .iter()
                .filter(|m| !matches!(m, Ty::Primitive(PrimitiveType::Null, _)))
                .cloned()
                .collect();
            crate::ty::union_of(filtered, TyAttr::default())
        }
        Ty::Primitive(PrimitiveType::Null, _) => never_ty(),
        _ => ty.clone(),
    }
}

// ── Narrowing application helpers ─────────────────────────────────────────────

/// Apply then-branch narrowings to the locals map, saving original types.
///
/// Returns `Vec<(Name, Option<Ty>)>` — the saved originals for later restoration.
/// `None` means the name was not in `locals` before (e.g. a parameter that
/// wasn't shadowed as a local yet).
pub(crate) fn apply_then_narrowings(
    narrowings: &[Narrowing],
    locals: &mut FxHashMap<Name, LocalBinding>,
) -> Vec<(Name, Option<Ty>)> {
    let saved = narrowings
        .iter()
        .map(|n| {
            (
                n.name.clone(),
                locals
                    .get(&n.name)
                    .map(|binding| binding.current_ty.clone()),
            )
        })
        .collect();
    for n in narrowings {
        set_current_type(locals, n.name.clone(), n.then_type.clone());
    }
    saved
}

/// Restore each saved original type, removing names that weren't present before.
fn restore_saved(saved: &[(Name, Option<Ty>)], locals: &mut FxHashMap<Name, LocalBinding>) {
    for (name, original) in saved {
        match original {
            Some(ty) => set_current_type(locals, name.clone(), ty.clone()),
            None => {
                locals.remove(name);
            }
        }
    }
}

/// Restore original types and then apply else-branch narrowings.
pub(crate) fn restore_and_apply_else(
    narrowings: &[Narrowing],
    saved: &[(Name, Option<Ty>)],
    locals: &mut FxHashMap<Name, LocalBinding>,
) {
    restore_saved(saved, locals);
    for n in narrowings {
        set_current_type(locals, n.name.clone(), n.else_type.clone());
    }
}

/// Restore types to their state before narrowing was applied.
pub(crate) fn restore_narrowings(
    saved: &[(Name, Option<Ty>)],
    locals: &mut FxHashMap<Name, LocalBinding>,
) {
    restore_saved(saved, locals);
}

/// Apply else-branch narrowings after a then-branch that diverged.
///
/// Used for early-return narrowing: when the then-branch always diverges
/// (returns, breaks, etc.), the else-type is what holds for the remainder
/// of the enclosing block. This is called in `check_stmt` after detecting
/// that a `Stmt::Expr(Expr::If)` with a diverging then-branch was processed.
pub(crate) fn apply_post_diverge_narrowings(
    narrowings: &[Narrowing],
    locals: &mut FxHashMap<Name, LocalBinding>,
) {
    for n in narrowings {
        // Only narrow variables that are already in scope
        if let Some(binding) = locals.get_mut(&n.name) {
            binding.current_ty = n.else_type.clone();
        }
    }
}

fn set_current_type(locals: &mut FxHashMap<Name, LocalBinding>, name: Name, ty: Ty) {
    if let Some(binding) = locals.get_mut(&name) {
        binding.current_ty = ty;
    } else {
        locals.insert(
            name,
            LocalBinding {
                current_ty: ty,
                declared_ty: None,
                pattern: None,
            },
        );
    }
}
