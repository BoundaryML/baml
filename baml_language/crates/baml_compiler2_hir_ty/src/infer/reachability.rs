//! Reachability diagnostics over finalized expression types. This pass does
//! not propagate values through storage or change typing: in particular, a
//! previous read or write of a shared field cannot prove its next value.

use baml_compiler2_ast::{BinaryOp, Expr, ExprBody, ExprId, Stmt, StmtId, UnaryOp};
use baml_type::interned::{ClosedTy, InferTy, Ty};

use super::{
    WorkingResult,
    flow::LoopVerdicts,
    truthy::{Truthiness, truthiness},
};
use crate::diagnostics::{
    DiagnosticLocation, DiagnosticSeverity, RelatedLocation, RelatedNote, TirDiagnostic,
    TirTypeError,
};

pub(super) fn add_diagnostics<'db>(
    body: &ExprBody,
    result: &WorkingResult<'db>,
    loop_verdicts: &LoopVerdicts,
    diagnostics: &mut Vec<TirDiagnostic<'db>>,
) {
    let reachability = Reachability {
        body,
        result,
        loop_verdicts,
    };
    for (_, expr) in body.exprs.iter() {
        match expr {
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                if let Some(value) = reachability.condition(*condition) {
                    let unreachable = if value {
                        *else_branch
                    } else {
                        Some(*then_branch)
                    };
                    reachability.warn(
                        diagnostics,
                        *condition,
                        value,
                        unreachable.map(|branch| {
                            RelatedNote::new(
                                RelatedLocation::Expr(branch),
                                "this branch is unreachable",
                            )
                        }),
                    );
                }
            }
            Expr::Binary {
                op: BinaryOp::And | BinaryOp::Or,
                lhs,
                rhs,
            } => {
                let short_circuit = matches!(
                    expr,
                    Expr::Binary {
                        op: BinaryOp::Or,
                        ..
                    }
                );
                if reachability.condition(*lhs) == Some(short_circuit) {
                    reachability.warn(
                        diagnostics,
                        *lhs,
                        short_circuit,
                        Some(RelatedNote::new(
                            RelatedLocation::Expr(*rhs),
                            "this expression is unreachable",
                        )),
                    );
                }
            }
            Expr::Block { stmts, tail_expr } => {
                if let Some(index) = stmts.iter().position(|stmt| reachability.stmt_exits(*stmt)) {
                    let remaining = &stmts[index + 1..];
                    let primary = remaining
                        .first()
                        .copied()
                        .map(DiagnosticLocation::Stmt)
                        .or_else(|| tail_expr.map(DiagnosticLocation::Expr));
                    if let Some(primary) = primary {
                        diagnostics.push(TirDiagnostic {
                            error: TirTypeError::DeadCode {
                                unreachable_count: remaining.len()
                                    + usize::from(tail_expr.is_some()),
                            },
                            severity: DiagnosticSeverity::Warning,
                            primary,
                            related: reachability
                                .never_ending_loop_note(stmts[index])
                                .into_iter()
                                .collect(),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    // A `while` condition that is always falsy leaves the body dead. One
    // that is always truthy makes nothing dead by itself: the loop is left
    // through its body's `break`s, or not at all, and then the code after
    // it is reported above with the condition as the reason.
    for (_, stmt) in body.stmts.iter() {
        if let Stmt::While {
            condition,
            body: loop_body,
            ..
        } = stmt
            && reachability.condition(*condition) == Some(false)
        {
            reachability.warn(
                diagnostics,
                *condition,
                false,
                Some(RelatedNote::new(
                    RelatedLocation::Expr(*loop_body),
                    "this loop body is unreachable",
                )),
            );
        }
    }
    for (_, arm) in body.match_arms.iter() {
        if let Some(guard) = arm.guard
            && let Some(value) = reachability.condition(guard)
        {
            let note = (!value).then(|| {
                RelatedNote::new(
                    RelatedLocation::Expr(arm.body),
                    "this match arm is unreachable",
                )
            });
            reachability.warn(diagnostics, guard, value, note);
        }
    }
}

struct Reachability<'a, 'db> {
    body: &'a ExprBody,
    result: &'a WorkingResult<'db>,
    /// Inference's verdicts about loops without an exit and jumps without a
    /// loop.
    loop_verdicts: &'a LoopVerdicts,
}

impl<'db> Reachability<'_, 'db> {
    /// Why the code after `stmt` is dead, when `stmt` is a loop that never
    /// ends because its condition is always truthy and its body never
    /// `break`s: the condition is the note.
    fn never_ending_loop_note(&self, stmt: StmtId) -> Option<RelatedNote<'db>> {
        let Stmt::While { condition, .. } = &self.body.stmts[stmt] else {
            return None;
        };
        (self.loop_verdicts.diverging_loops.contains(&stmt)
            && self.condition(*condition) == Some(true))
        .then(|| {
            RelatedNote::new(
                RelatedLocation::Expr(*condition),
                "this loop never ends: its condition is always truthy and its body never breaks",
            )
        })
    }

    fn condition(&self, expr: ExprId) -> Option<bool> {
        static_condition(
            self.body,
            &mut |expr| self.result.type_of_expr.get(&expr).cloned(),
            expr,
        )
    }

    fn warn(
        &self,
        diagnostics: &mut Vec<TirDiagnostic<'db>>,
        condition: ExprId,
        value: bool,
        note: Option<RelatedNote<'db>>,
    ) {
        // Reuse a warning on the condition's cause, including through `!`,
        // `&&`, and `||`, instead of reporting the same cause twice.
        if let Some(index) = self.condition_warning(condition, diagnostics) {
            if let Some(note) = note {
                diagnostics[index].related.push(note);
            }
            return;
        }
        let Some(ty) = self.result.type_of_expr.get(&condition) else {
            return;
        };
        let ty = (static_truthiness(ty) == Some(value))
            .then(|| ClosedTy::try_from(ty).ok().map(|ty| ty.to_plain()))
            .flatten();
        diagnostics.push(TirDiagnostic {
            error: TirTypeError::ConditionAlwaysConstant {
                ty,
                always_true: value,
            },
            severity: DiagnosticSeverity::Warning,
            primary: DiagnosticLocation::Expr(condition),
            related: note.into_iter().collect(),
        });
    }

    fn condition_warning(
        &self,
        condition: ExprId,
        diagnostics: &[TirDiagnostic<'db>],
    ) -> Option<usize> {
        diagnostics
            .iter()
            .position(|diagnostic| {
                diagnostic.primary == DiagnosticLocation::Expr(condition)
                    && matches!(
                        diagnostic.error,
                        TirTypeError::ConditionAlwaysConstant { .. }
                            | TirTypeError::UncalledFunctionInCondition { .. }
                            | TirTypeError::ComparisonAlwaysDisjoint { .. }
                    )
            })
            .or_else(|| match self.body.exprs[condition] {
                Expr::Unary {
                    op: UnaryOp::Not,
                    expr,
                } => self.condition_warning(expr, diagnostics),
                Expr::Binary {
                    op: BinaryOp::And | BinaryOp::Or,
                    lhs,
                    rhs,
                } => self
                    .condition_warning(lhs, diagnostics)
                    .or_else(|| self.condition_warning(rhs, diagnostics)),
                _ => None,
            })
    }

    fn expr_exits(&self, expr: ExprId) -> bool {
        match &self.body.exprs[expr] {
            Expr::Return { .. } | Expr::Throw { .. } => true,
            Expr::Lambda(_) => false,
            Expr::Block { stmts, tail_expr } => {
                stmts.iter().any(|stmt| self.stmt_exits(*stmt))
                    || tail_expr.is_some_and(|tail| self.expr_exits(tail))
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expr_exits(*condition)
                    || match self.condition(*condition) {
                        Some(true) => self.expr_exits(*then_branch),
                        Some(false) => else_branch.is_some_and(|branch| self.expr_exits(branch)),
                        None => {
                            self.expr_exits(*then_branch)
                                && else_branch.is_some_and(|branch| self.expr_exits(branch))
                        }
                    }
            }
            Expr::Binary {
                op: BinaryOp::And | BinaryOp::Or,
                lhs,
                rhs,
            } => {
                let evaluate_rhs = matches!(
                    self.body.exprs[expr],
                    Expr::Binary {
                        op: BinaryOp::And,
                        ..
                    }
                );
                self.expr_exits(*lhs)
                    || (self.condition(*lhs) == Some(evaluate_rhs) && self.expr_exits(*rhs))
            }
            _ => self
                .result
                .type_of_expr
                .get(&expr)
                .is_some_and(|ty| matches!(ty.kind(), InferTy::Never)),
        }
    }

    fn stmt_exits(&self, stmt: StmtId) -> bool {
        match &self.body.stmts[stmt] {
            Stmt::Return(_) | Stmt::Throw { .. } => true,
            // A jump inference rejected (no loop takes it) is not a jump.
            Stmt::Break | Stmt::Continue => !self.loop_verdicts.rejected_jumps.contains(&stmt),
            Stmt::Expr(expr) => self.expr_exits(*expr),
            Stmt::Let { initializer, .. } => initializer.is_some_and(|expr| self.expr_exits(expr)),
            Stmt::Assign { value, .. } | Stmt::AssignOp { value, .. } => self.expr_exits(*value),
            // A loop with no exit edge never completes: a header that never
            // misses (a statically true condition, a header that always
            // returns) and no live `break` in the body. Inference records
            // that verdict from its walk of the loop, where every `break`
            // registers with the loop it leaves.
            Stmt::While { .. } | Stmt::WhileLet { .. } => {
                self.loop_verdicts.diverging_loops.contains(&stmt)
            }
            // A `for let` collection runs before the loop opens: its exits
            // leave the enclosing code.
            Stmt::For { collection, .. } => self.expr_exits(*collection),
            _ => false,
        }
    }
}

/// A condition's static value, when the types decide it: walk-time De
/// Morgan through `!`, `&&` and `||`, down to the static truthiness of each
/// operand's type as `type_of` reports it (`None` for an operand not typed
/// yet). Inference and reachability judge the SAME walk - inference on the
/// types it has while typing a loop, to decide whether a `while` condition
/// has a false edge; reachability on the final types.
pub(super) fn static_condition(
    body: &ExprBody,
    type_of: &mut dyn FnMut(ExprId) -> Option<Ty>,
    expr: ExprId,
) -> Option<bool> {
    match &body.exprs[expr] {
        Expr::Unary {
            op: UnaryOp::Not,
            expr,
        } => static_condition(body, type_of, *expr).map(|v| !v),
        Expr::Binary {
            op: BinaryOp::And,
            lhs,
            rhs,
        } => match (
            static_condition(body, type_of, *lhs),
            static_condition(body, type_of, *rhs),
        ) {
            (Some(false), _) | (_, Some(false)) => Some(false),
            (Some(true), Some(true)) => Some(true),
            _ => None,
        },
        Expr::Binary {
            op: BinaryOp::Or,
            lhs,
            rhs,
        } => match (
            static_condition(body, type_of, *lhs),
            static_condition(body, type_of, *rhs),
        ) {
            (Some(true), _) | (_, Some(true)) => Some(true),
            (Some(false), Some(false)) => Some(false),
            _ => None,
        },
        _ => type_of(expr).as_ref().and_then(static_truthiness),
    }
}

fn static_truthiness(ty: &Ty) -> Option<bool> {
    match truthiness(ty) {
        Truthiness::AlwaysTruthy => Some(true),
        Truthiness::AlwaysFalsy => Some(false),
        Truthiness::Runtime => None,
    }
}
