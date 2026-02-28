//! Shared divergence analysis utilities.
//!
//! This module centralizes "definitely diverges" logic so both type inference
//! and narrowing use one implementation.

use std::{collections::HashSet, hash::Hash};

use baml_base::Name;
use baml_compiler_hir::{Expr, ExprBody, ExprId, Stmt, StmtId};

/// Build a canonical function/callee name from a path expression.
///
/// `foo` -> `foo`, `baml.llm.render_prompt` -> `baml.llm.render_prompt`.
pub(crate) fn call_target_from_callee_expr(
    callee_expr_id: ExprId,
    body: &ExprBody,
) -> Option<Name> {
    let Expr::Path(segments) = &body.exprs[callee_expr_id] else {
        return None;
    };
    if segments.is_empty() {
        return None;
    }
    let joined = segments
        .iter()
        .map(Name::as_str)
        .collect::<Vec<_>>()
        .join(".");
    Some(Name::new(joined))
}

/// Return true when any statement in the slice definitely diverges.
///
/// A divergent statement makes all following statements unreachable, so the
/// whole block diverges.
pub(crate) fn any_stmt_diverges<F>(stmts: &[StmtId], body: &ExprBody, call_diverges: &F) -> bool
where
    F: Fn(&Name) -> bool,
{
    stmts
        .iter()
        .any(|stmt_id| stmt_definitely_diverges(*stmt_id, body, call_diverges))
}

/// Return true when this statement definitely does not fall through.
///
/// This treats `return`, `break`, `continue`, `throw` all as diverging —
/// the statement prevents subsequent code in the block from executing.
/// Use `stmt_never_returns` for the stricter "never returns to caller" check.
pub(crate) fn stmt_definitely_diverges<F>(
    stmt_id: StmtId,
    body: &ExprBody,
    call_diverges: &F,
) -> bool
where
    F: Fn(&Name) -> bool,
{
    match &body.stmts[stmt_id] {
        Stmt::Return(_) | Stmt::Break | Stmt::Continue | Stmt::Throw { .. } => true,
        Stmt::Expr(expr_id) => expr_definitely_diverges(*expr_id, body, call_diverges),
        _ => false,
    }
}

/// Return true when this statement never returns to the caller.
///
/// Unlike `stmt_definitely_diverges`, `Stmt::Return` is NOT considered
/// "never returns" — it completes normally by returning a value.
/// Only `throw` and calls to always-diverging functions qualify.
fn stmt_never_returns<F>(stmt_id: StmtId, body: &ExprBody, call_diverges: &F) -> bool
where
    F: Fn(&Name) -> bool,
{
    match &body.stmts[stmt_id] {
        Stmt::Throw { .. } => true,
        Stmt::Expr(expr_id) => expr_never_returns(*expr_id, body, call_diverges),
        _ => false,
    }
}

/// Return true when this expression definitely does not produce a value.
pub(crate) fn expr_definitely_diverges<F>(
    expr_id: ExprId,
    body: &ExprBody,
    call_diverges: &F,
) -> bool
where
    F: Fn(&Name) -> bool,
{
    match &body.exprs[expr_id] {
        Expr::Throw { .. } => true,
        Expr::Call { callee, .. } => call_target_from_callee_expr(*callee, body)
            .as_ref()
            .is_some_and(call_diverges),
        Expr::Block { stmts, tail_expr } => {
            // Any diverging statement forces the whole block to diverge, even if
            // there is a syntactic tail expression (which is then unreachable).
            if any_stmt_diverges(stmts, body, call_diverges) {
                true
            } else if let Some(tail_expr) = tail_expr {
                expr_definitely_diverges(*tail_expr, body, call_diverges)
            } else {
                false
            }
        }
        Expr::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } => {
            expr_definitely_diverges(*then_branch, body, call_diverges)
                && expr_definitely_diverges(*else_branch, body, call_diverges)
        }
        Expr::Match { arms, .. } => {
            !arms.is_empty()
                && arms.iter().all(|arm_id| {
                    let arm = &body.match_arms[*arm_id];
                    expr_definitely_diverges(arm.body, body, call_diverges)
                })
        }
        _ => false,
    }
}

/// Return true when this expression never returns to the caller.
///
/// Unlike `expr_definitely_diverges`, `return` statements are NOT diverging
/// here — they complete normally. Only `throw` and always-diverging calls
/// cause a function to "never return." Used by `function_divergence_set`.
pub(crate) fn expr_never_returns<F>(expr_id: ExprId, body: &ExprBody, call_diverges: &F) -> bool
where
    F: Fn(&Name) -> bool,
{
    match &body.exprs[expr_id] {
        Expr::Throw { .. } => true,
        Expr::Call { callee, .. } => call_target_from_callee_expr(*callee, body)
            .as_ref()
            .is_some_and(call_diverges),
        Expr::Block { stmts, tail_expr } => {
            let any_stmt_never_returns = stmts
                .iter()
                .any(|stmt_id| stmt_never_returns(*stmt_id, body, call_diverges));
            if any_stmt_never_returns {
                true
            } else if let Some(tail_expr) = tail_expr {
                expr_never_returns(*tail_expr, body, call_diverges)
            } else {
                false
            }
        }
        Expr::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } => {
            expr_never_returns(*then_branch, body, call_diverges)
                && expr_never_returns(*else_branch, body, call_diverges)
        }
        Expr::Match { arms, .. } => {
            !arms.is_empty()
                && arms.iter().all(|arm_id| {
                    let arm = &body.match_arms[*arm_id];
                    expr_never_returns(arm.body, body, call_diverges)
                })
        }
        _ => false,
    }
}

/// Solve a monotonic divergence fixed point.
///
/// `is_divergent(name, known)` should return true when `name` is definitely
/// divergent given the currently known divergent set.
pub(crate) fn solve_divergence_fixed_point<N, F>(
    nodes: impl IntoIterator<Item = N>,
    mut is_divergent: F,
) -> HashSet<N>
where
    N: Eq + Hash + Clone,
    F: FnMut(&N, &HashSet<N>) -> bool,
{
    let nodes: Vec<N> = nodes.into_iter().collect();
    let mut divergent: HashSet<N> = HashSet::new();
    let mut changed = true;

    while changed {
        changed = false;
        for node in &nodes {
            if divergent.contains(node) {
                continue;
            }
            if is_divergent(node, &divergent) {
                divergent.insert(node.clone());
                changed = true;
            }
        }
    }

    divergent
}

#[cfg(test)]
mod tests {
    use super::solve_divergence_fixed_point;

    #[test]
    fn fixed_point_handles_non_divergent_cycle() {
        let nodes = vec!["A", "B"];
        let divergent = solve_divergence_fixed_point(nodes, |node, known| match *node {
            "A" => known.contains("B"),
            "B" => known.contains("A"),
            _ => false,
        });
        assert!(divergent.is_empty());
    }

    #[test]
    fn fixed_point_propagates_through_cycle_with_seed() {
        let nodes = vec!["A", "B"];
        let divergent = solve_divergence_fixed_point(nodes, |node, known| match *node {
            "A" => true, // direct divergence seed
            "B" => known.contains("A"),
            _ => false,
        });
        assert!(divergent.contains("A"));
        assert!(divergent.contains("B"));
    }

    #[test]
    fn fixed_point_handles_mutual_recursion_chain() {
        let nodes = vec!["A", "B", "C"];
        let divergent = solve_divergence_fixed_point(nodes, |node, known| match *node {
            "A" => known.contains("B"),
            "B" => known.contains("C"),
            "C" => true, // direct divergence seed
            _ => false,
        });
        assert!(divergent.contains("A"));
        assert!(divergent.contains("B"));
        assert!(divergent.contains("C"));
    }
}
