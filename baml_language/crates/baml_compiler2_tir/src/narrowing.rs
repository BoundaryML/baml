//! Type narrowing for control-flow-dependent type refinement.
//!
//! Analyzes condition expressions to produce narrowing information that refines
//! variable types inside then/else branches of `if` expressions, and after
//! diverging branches (early-return narrowing).
//!
//! ## Patterns recognized
//!
//! - `x != null` → then: remove null, else: null (or original)
//! - `x == null` → then: null, else: remove null
//! - `x` (truthiness) → then: remove null, else: original
//! - `!(x == null)` → same as `x != null` (negation flips then/else)
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
use baml_compiler2_ast::{BinaryOp, Expr, ExprBody, ExprId, UnaryOp};
use rustc_hash::FxHashMap;

use crate::ty::{PrimitiveType, Ty};

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
pub fn extract_narrowings(
    condition: ExprId,
    body: &ExprBody,
    expr_types: &FxHashMap<ExprId, Ty>,
) -> Vec<Narrowing> {
    let mut narrowings = Vec::new();
    collect_narrowings(condition, body, expr_types, false, &mut narrowings);
    narrowings
}

/// Internal recursive collector. `negated` tracks whether we're inside a `!`.
fn collect_narrowings(
    expr_id: ExprId,
    body: &ExprBody,
    expr_types: &FxHashMap<ExprId, Ty>,
    negated: bool,
    out: &mut Vec<Narrowing>,
) {
    let expr = &body.exprs[expr_id];
    match expr {
        // x != null  (or null != x)
        Expr::Binary {
            op: BinaryOp::Ne,
            lhs,
            rhs,
        } => {
            if let Some((name, original_ty)) = null_check_name(*lhs, *rhs, body, expr_types) {
                let (then_ty, else_ty) = if negated {
                    // !(x != null) == x == null
                    (
                        Ty::Primitive(PrimitiveType::Null),
                        remove_null(&original_ty),
                    )
                } else {
                    (
                        remove_null(&original_ty),
                        Ty::Primitive(PrimitiveType::Null),
                    )
                };
                out.push(Narrowing {
                    name,
                    then_type: then_ty,
                    else_type: else_ty,
                });
            }
        }

        // x == null  (or null == x)
        Expr::Binary {
            op: BinaryOp::Eq,
            lhs,
            rhs,
        } => {
            if let Some((name, original_ty)) = null_check_name(*lhs, *rhs, body, expr_types) {
                let (then_ty, else_ty) = if negated {
                    // !(x == null) == x != null
                    (
                        remove_null(&original_ty),
                        Ty::Primitive(PrimitiveType::Null),
                    )
                } else {
                    (
                        Ty::Primitive(PrimitiveType::Null),
                        remove_null(&original_ty),
                    )
                };
                out.push(Narrowing {
                    name,
                    then_type: then_ty,
                    else_type: else_ty,
                });
            }
        }

        // !(inner) — flip then/else by recursing with toggled negation
        Expr::Unary {
            op: UnaryOp::Not,
            expr: inner,
        } => {
            collect_narrowings(*inner, body, expr_types, !negated, out);
        }

        // Truthiness: if (x) where x is optional — then-branch removes null
        Expr::Path(segments) if segments.len() == 1 => {
            let name = &segments[0];
            if let Some(ty) = expr_types.get(&expr_id) {
                // Only narrow if the type is optional / nullable
                if is_nullable(ty) {
                    let (then_ty, else_ty) = if negated {
                        // !x — then: null/falsy, else: non-null
                        (ty.clone(), remove_null(ty))
                    } else {
                        // x — then: non-null, else: original (might still be null)
                        (remove_null(ty), ty.clone())
                    };
                    out.push(Narrowing {
                        name: name.clone(),
                        then_type: then_ty,
                        else_type: else_ty,
                    });
                }
            }
        }

        // For all other patterns, no narrowing is extracted.
        _ => {}
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

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
    // lhs is name, rhs is null
    if let Expr::Path(segments) = &body.exprs[lhs] {
        if segments.len() == 1 {
            if let Expr::Null = &body.exprs[rhs] {
                if let Some(ty) = expr_types.get(&lhs) {
                    return Some((segments[0].clone(), ty.clone()));
                }
            }
        }
    }
    // rhs is name, lhs is null
    if let Expr::Path(segments) = &body.exprs[rhs] {
        if segments.len() == 1 {
            if let Expr::Null = &body.exprs[lhs] {
                if let Some(ty) = expr_types.get(&rhs) {
                    return Some((segments[0].clone(), ty.clone()));
                }
            }
        }
    }
    None
}

/// Returns `true` if the type contains a null component (is optional or union
/// with null) or is directly `Null`.
fn is_nullable(ty: &Ty) -> bool {
    match ty {
        Ty::Optional(_) => true,
        Ty::Primitive(PrimitiveType::Null) => true,
        Ty::Union(members) => members
            .iter()
            .any(|m| matches!(m, Ty::Primitive(PrimitiveType::Null))),
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
        Ty::Optional(inner) => inner.as_ref().clone(),
        Ty::Union(members) => {
            let filtered: Vec<Ty> = members
                .iter()
                .filter(|m| !matches!(m, Ty::Primitive(PrimitiveType::Null)))
                .cloned()
                .collect();
            match filtered.len() {
                0 => Ty::Never,
                1 => filtered.into_iter().next().unwrap(),
                _ => Ty::Union(filtered),
            }
        }
        Ty::Primitive(PrimitiveType::Null) => Ty::Never,
        _ => ty.clone(),
    }
}

// ── Narrowing application helpers ─────────────────────────────────────────────

/// Apply then-branch narrowings to the locals map, saving original types.
///
/// Returns `Vec<(Name, Option<Ty>)>` — the saved originals for later restoration.
/// `None` means the name was not in `locals` before (e.g. a parameter that
/// wasn't shadowed as a local yet).
pub fn apply_then_narrowings(
    narrowings: &[Narrowing],
    locals: &mut FxHashMap<Name, Ty>,
) -> Vec<(Name, Option<Ty>)> {
    let saved = narrowings
        .iter()
        .map(|n| (n.name.clone(), locals.get(&n.name).cloned()))
        .collect();
    for n in narrowings {
        locals.insert(n.name.clone(), n.then_type.clone());
    }
    saved
}

/// Restore original types and then apply else-branch narrowings.
pub fn restore_and_apply_else(
    narrowings: &[Narrowing],
    saved: &[(Name, Option<Ty>)],
    locals: &mut FxHashMap<Name, Ty>,
) {
    // Restore originals
    for (name, original) in saved {
        match original {
            Some(ty) => {
                locals.insert(name.clone(), ty.clone());
            }
            None => {
                locals.remove(name);
            }
        }
    }
    // Apply else narrowings
    for n in narrowings {
        locals.insert(n.name.clone(), n.else_type.clone());
    }
}

/// Restore types to their state before narrowing was applied.
pub fn restore_narrowings(saved: Vec<(Name, Option<Ty>)>, locals: &mut FxHashMap<Name, Ty>) {
    for (name, original) in saved {
        match original {
            Some(ty) => {
                locals.insert(name, ty);
            }
            None => {
                locals.remove(&name);
            }
        }
    }
}

/// Apply else-branch narrowings after a then-branch that diverged.
///
/// Used for early-return narrowing: when the then-branch always diverges
/// (returns, breaks, etc.), the else-type is what holds for the remainder
/// of the enclosing block. This is called in `check_stmt` after detecting
/// that a `Stmt::Expr(Expr::If)` with a diverging then-branch was processed.
pub fn apply_post_diverge_narrowings(narrowings: &[Narrowing], locals: &mut FxHashMap<Name, Ty>) {
    for n in narrowings {
        // Only narrow variables that are already in scope
        if locals.contains_key(&n.name) {
            locals.insert(n.name.clone(), n.else_type.clone());
        }
    }
}
