//! Type narrowing and divergence analysis for the expression type checker.
//!
//! This module handles:
//! - **Divergence detection**: Determining whether a block/statement always exits
//!   via `return`/`break`/`continue` (used to enable narrowing after early returns).
//! - **Condition narrowing**: Extracting type narrowing implications from conditions
//!   (`x == null`, `x instanceof Foo`, `x.type == "literal"`, `!cond`, truthiness).
//! - **Early-return narrowing**: Applying the negation of an if-condition to code
//!   after the if, when the if-body definitely diverges.

use baml_base::Name;
use baml_compiler_hir::{ExprBody, ExprId, StmtId};

use crate::{TypeContext, types::Ty};

// ── Divergence detection ────────────────────────────────────────────────

/// Check if an expression definitely diverges (return/break/continue).
///
/// An expression definitely diverges if:
/// - It is a Block whose last statement is Return/Break/Continue
/// - It is a Block whose last statement is an if where BOTH branches diverge
/// - It is a Block whose last statement is a match where ALL arms diverge
///
/// This is intentionally conservative — only checks the last statement.
fn definitely_diverges(expr_id: ExprId, body: &ExprBody) -> bool {
    use baml_compiler_hir::Expr;

    let expr = &body.exprs[expr_id];
    match expr {
        Expr::Block { stmts, tail_expr } => {
            // A block with a tail expression produces a value, doesn't diverge
            if tail_expr.is_some() {
                return false;
            }
            if let Some(&last_stmt_id) = stmts.last() {
                stmt_definitely_diverges(last_stmt_id, body)
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Check if a statement definitely diverges.
fn stmt_definitely_diverges(stmt_id: StmtId, body: &ExprBody) -> bool {
    use baml_compiler_hir::Stmt;

    let stmt = &body.stmts[stmt_id];
    match stmt {
        Stmt::Return(_) | Stmt::Break | Stmt::Continue => true,
        Stmt::Expr(expr_id) => {
            match &body.exprs[*expr_id] {
                // An if where both branches diverge
                baml_compiler_hir::Expr::If {
                    then_branch,
                    else_branch: Some(else_branch),
                    ..
                } => {
                    definitely_diverges(*then_branch, body)
                        && definitely_diverges(*else_branch, body)
                }
                // A match where all arms diverge
                baml_compiler_hir::Expr::Match { arms, .. } => {
                    !arms.is_empty()
                        && arms.iter().all(|arm_id| {
                            let arm = &body.match_arms[*arm_id];
                            definitely_diverges(arm.body, body)
                        })
                }
                _ => false,
            }
        }
        _ => false,
    }
}

// ── Type manipulation helpers ───────────────────────────────────────────

/// Check if a type is nullable (Optional or Union containing Null).
fn is_nullable(ty: &Ty) -> bool {
    match ty {
        Ty::Optional(_) | Ty::Null => true,
        Ty::Union(members) => members.iter().any(|m| matches!(m, Ty::Null)),
        _ => false,
    }
}

/// Remove null from a type, producing the non-null narrowing.
///
/// - `Optional(T)` → `T`
/// - `Union([A, Null, B])` → `Union([A, B])` (or just `A` if one remains)
/// - `Null` → `Unknown` (degenerate)
/// - Other → unchanged
fn remove_null(ty: &Ty) -> Ty {
    match ty {
        Ty::Optional(inner) => (**inner).clone(),
        Ty::Union(members) => {
            let non_null: Vec<Ty> = members
                .iter()
                .filter(|m| !matches!(m, Ty::Null))
                .cloned()
                .collect();
            match non_null.len() {
                0 => Ty::Unknown,
                1 => non_null.into_iter().next().unwrap(),
                _ => Ty::Union(non_null),
            }
        }
        Ty::Null => Ty::Unknown,
        other => other.clone(),
    }
}

/// Get the simple name of a named type (Class, Enum, or `TypeAlias`).
fn named_type_name(ty: &Ty) -> Option<&Name> {
    match ty {
        Ty::Class(qn) | Ty::Enum(qn) | Ty::TypeAlias(qn) => Some(&qn.name),
        _ => None,
    }
}

/// Check if two types refer to the same named type, even if one is `TypeAlias`
/// and the other is Class/Enum (since instanceof returns `TypeAlias`).
fn same_named_type(a: &Ty, b: &Ty) -> bool {
    if a == b {
        return true;
    }
    match (named_type_name(a), named_type_name(b)) {
        (Some(a_name), Some(b_name)) => a_name == b_name,
        _ => false,
    }
}

/// Remove a specific type from a union (for instanceof false-branch narrowing).
fn remove_type_from(ty: &Ty, to_remove: &Ty) -> Ty {
    match ty {
        Ty::Union(members) => {
            let remaining: Vec<Ty> = members
                .iter()
                .filter(|m| !same_named_type(m, to_remove))
                .cloned()
                .collect();
            match remaining.len() {
                0 => Ty::Unknown,
                1 => remaining.into_iter().next().unwrap(),
                _ => Ty::Union(remaining),
            }
        }
        Ty::Optional(inner) if same_named_type(inner.as_ref(), to_remove) => Ty::Null,
        _ => ty.clone(),
    }
}

/// Look up a field on a single union member type without emitting errors.
/// Returns `Some(field_type)` if the member has that field, None otherwise.
pub(crate) fn infer_union_member_field(
    ctx: &TypeContext<'_>,
    member: &Ty,
    field: &Name,
) -> Option<Ty> {
    match member {
        Ty::Class(fqn) => {
            let key = fqn.display_name();
            ctx.lookup_class_field(&key, field).cloned()
        }
        Ty::TypeAlias(fqn) => {
            let key = fqn.display_name();
            ctx.lookup_class_field(&key, field).cloned()
        }
        _ => None,
    }
}

// ── Narrowing extraction ────────────────────────────────────────────────

/// Extract instanceof narrowing info from a condition expression.
///
/// If the condition is `x instanceof Foo`, returns `Some((x, Foo_type))`.
/// Otherwise returns `None`.
fn extract_instanceof_narrowing(
    _ctx: &TypeContext<'_>,
    condition: ExprId,
    body: &ExprBody,
) -> Option<(Name, Ty)> {
    use baml_compiler_hir::Expr;

    let expr = &body.exprs[condition];

    // Check if this is an instanceof expression
    if let Expr::Binary { op, lhs, rhs } = expr {
        if *op == baml_compiler_hir::BinaryOp::Instanceof {
            // LHS should be a simple path (variable name)
            if let Expr::Path(segments) = &body.exprs[*lhs] {
                if segments.len() == 1 {
                    let var_name = segments[0].clone();

                    // RHS should be a simple path (type name)
                    if let Expr::Path(type_segments) = &body.exprs[*rhs] {
                        if type_segments.len() == 1 {
                            use baml_compiler_hir::QualifiedName;
                            let type_name = type_segments[0].clone();
                            return Some((
                                var_name,
                                Ty::TypeAlias(QualifiedName::local(type_name)),
                            ));
                        }
                    }
                }
            }
        }
    }

    None
}

/// Check if a binary comparison involves null on one side and a variable on the other.
/// Returns (`variable_name`, `variable_type`) if matched.
fn extract_null_comparison(
    ctx: &TypeContext<'_>,
    lhs: ExprId,
    rhs: ExprId,
    body: &ExprBody,
) -> Option<(Name, Ty)> {
    use baml_compiler_hir::{Expr, Literal};

    let lhs_expr = &body.exprs[lhs];
    let rhs_expr = &body.exprs[rhs];

    // `x == null` or `null == x`
    let var_segments = match (lhs_expr, rhs_expr) {
        (Expr::Path(segments), Expr::Literal(Literal::Null)) => segments,
        (Expr::Literal(Literal::Null), Expr::Path(segments)) => segments,
        _ => return None,
    };

    if var_segments.len() != 1 {
        return None;
    }

    let var_name = &var_segments[0];
    let var_ty = ctx.lookup(var_name)?.clone();
    Some((var_name.clone(), var_ty))
}

/// Extract discriminated union narrowing from `x.field == "literal"` patterns.
///
/// When `x: A | B` and `x.type == "a_type"`, and class A has field `type: "a_type"`,
/// narrow x to A in the true branch.
fn extract_discriminated_union_narrowing(
    ctx: &TypeContext<'_>,
    lhs: ExprId,
    rhs: ExprId,
    body: &ExprBody,
    when_true: bool,
) -> Option<Vec<(Name, Ty)>> {
    use baml_compiler_hir::{Expr, Literal};

    let lhs_expr = &body.exprs[lhs];
    let rhs_expr = &body.exprs[rhs];

    // Match `x.field == "literal"` or `"literal" == x.field`
    let (var_name, field_name, literal_str) = match (lhs_expr, rhs_expr) {
        // x.field == "literal" — Path form: Path(["x", "field"])
        (Expr::Path(segments), Expr::Literal(Literal::String(s))) if segments.len() == 2 => {
            (&segments[0], &segments[1], s.as_str())
        }
        // "literal" == x.field
        (Expr::Literal(Literal::String(s)), Expr::Path(segments)) if segments.len() == 2 => {
            (&segments[0], &segments[1], s.as_str())
        }
        // x.field == "literal" — FieldAccess form
        (Expr::FieldAccess { base, field }, Expr::Literal(Literal::String(s))) => {
            if let Expr::Path(segments) = &body.exprs[*base] {
                if segments.len() == 1 {
                    (&segments[0], field, s.as_str())
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
        // "literal" == x.field — FieldAccess form
        (Expr::Literal(Literal::String(s)), Expr::FieldAccess { base, field }) => {
            if let Expr::Path(segments) = &body.exprs[*base] {
                if segments.len() == 1 {
                    (&segments[0], field, s.as_str())
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
        _ => return None,
    };

    // Look up the variable's type — must be a union
    let var_ty = ctx.lookup(var_name)?;
    let Ty::Union(members) = var_ty else {
        return None;
    };

    // Find which union members have the field with a matching literal type
    let mut matching_members = Vec::new();
    for member in members {
        if let Some(field_ty) = infer_union_member_field(ctx, member, field_name) {
            match &field_ty {
                Ty::Literal(crate::types::LiteralValue::String(s)) if s.as_str() == literal_str => {
                    matching_members.push(member.clone());
                }
                _ => {}
            }
        } else {
            // Member doesn't have the field — can't narrow
            return None;
        }
    }

    if matching_members.is_empty() {
        return None;
    }

    if when_true {
        // Narrow to matching members
        let narrowed = match matching_members.len() {
            1 => matching_members.into_iter().next().unwrap(),
            _ => Ty::Union(matching_members),
        };
        Some(vec![(var_name.clone(), narrowed)])
    } else {
        // Narrow to non-matching members
        let non_matching: Vec<Ty> = members
            .iter()
            .filter(|m| !matching_members.contains(m))
            .cloned()
            .collect();
        if non_matching.is_empty() {
            None
        } else {
            let narrowed = match non_matching.len() {
                1 => non_matching.into_iter().next().unwrap(),
                _ => Ty::Union(non_matching),
            };
            Some(vec![(var_name.clone(), narrowed)])
        }
    }
}

/// Extract type narrowing implications from a condition expression.
///
/// Returns a list of (`variable_name`, `narrowed_type`) pairs that should be
/// applied when the condition evaluates to the given `when_true` value.
///
/// Supports:
/// - `x == null` / `x != null` → null narrowing
/// - `x.field == "literal"` → discriminated union narrowing
/// - `x instanceof Foo` → instanceof narrowing
/// - `!cond` → flips the branch
/// - Simple variable truthiness (`if (x)` where x is nullable)
pub(crate) fn extract_condition_narrowing(
    ctx: &TypeContext<'_>,
    condition: ExprId,
    body: &ExprBody,
    when_true: bool,
) -> Vec<(Name, Ty)> {
    use baml_compiler_hir::{BinaryOp, Expr, UnaryOp};

    let expr = &body.exprs[condition];

    match expr {
        // `!cond` → flip the branch
        Expr::Unary {
            op: UnaryOp::Not,
            expr: inner,
        } => extract_condition_narrowing(ctx, *inner, body, !when_true),

        Expr::Binary { op, lhs, rhs } => match op {
            // `x == null` / `null == x`, or `x.field == "literal"` for discriminated unions
            BinaryOp::Eq => {
                if let Some((var_name, var_ty)) = extract_null_comparison(ctx, *lhs, *rhs, body) {
                    if when_true {
                        vec![(var_name, Ty::Null)]
                    } else {
                        vec![(var_name, remove_null(&var_ty))]
                    }
                } else {
                    extract_discriminated_union_narrowing(ctx, *lhs, *rhs, body, when_true)
                        .unwrap_or_default()
                }
            }

            // `x != null` / `null != x`
            BinaryOp::Ne => {
                if let Some((var_name, var_ty)) = extract_null_comparison(ctx, *lhs, *rhs, body) {
                    if when_true {
                        vec![(var_name, remove_null(&var_ty))]
                    } else {
                        vec![(var_name, Ty::Null)]
                    }
                } else {
                    extract_discriminated_union_narrowing(ctx, *lhs, *rhs, body, !when_true)
                        .unwrap_or_default()
                }
            }

            // `x instanceof Foo`
            BinaryOp::Instanceof => {
                if when_true {
                    extract_instanceof_narrowing(ctx, condition, body)
                        .into_iter()
                        .collect()
                } else {
                    // When false: remove Foo from x's type
                    if let Some((var_name, instanceof_ty)) =
                        extract_instanceof_narrowing(ctx, condition, body)
                    {
                        if let Some(var_ty) = ctx.lookup(&var_name) {
                            let narrowed = remove_type_from(var_ty, &instanceof_ty);
                            vec![(var_name, narrowed)]
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    }
                }
            }

            _ => vec![],
        },

        // Simple variable truthiness: `if (x)` where x is nullable
        Expr::Path(segments) if segments.len() == 1 => {
            let var_name = &segments[0];
            if let Some(var_ty) = ctx.lookup(var_name) {
                let var_ty = var_ty.clone();
                if is_nullable(&var_ty) {
                    if when_true {
                        vec![(var_name.clone(), remove_null(&var_ty))]
                    } else {
                        // When falsy, we can't narrow precisely (other falsy values exist)
                        vec![]
                    }
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        }

        _ => vec![],
    }
}

/// Extract type narrowings that should apply after a statement,
/// if the statement is an if-with-early-return pattern.
///
/// For example, given `if (x == null) { return; }`, this returns
/// `[(x, non_null_type)]` because after this statement, x is guaranteed
/// to be non-null.
pub(crate) fn extract_early_return_narrowing(
    ctx: &TypeContext<'_>,
    stmt_id: StmtId,
    body: &ExprBody,
) -> Vec<(Name, Ty)> {
    use baml_compiler_hir::{Expr, Stmt};

    let stmt = &body.stmts[stmt_id];

    // Must be an expression statement containing an if
    let Stmt::Expr(expr_id) = stmt else {
        return vec![];
    };

    let Expr::If {
        condition,
        then_branch,
        else_branch,
    } = &body.exprs[*expr_id]
    else {
        return vec![];
    };

    // Case 1: if (cond) { <diverges> } — no else
    // After the if, cond was false.
    if else_branch.is_none() && definitely_diverges(*then_branch, body) {
        return extract_condition_narrowing(ctx, *condition, body, false);
    }

    // Case 2: if (cond) { ... } else { <diverges> }
    // After the if, cond was true.
    if let Some(else_br) = else_branch {
        if definitely_diverges(*else_br, body) && !definitely_diverges(*then_branch, body) {
            return extract_condition_narrowing(ctx, *condition, body, true);
        }
    }

    // Case 3: if (cont) { <diverges> } else { ... }
    // After the if, cond was false.
    if let Some(else_br) = else_branch {
        if definitely_diverges(*then_branch, body) && !definitely_diverges(*else_br, body) {
            return extract_condition_narrowing(ctx, *condition, body, false);
        }
    }

    vec![]
}
