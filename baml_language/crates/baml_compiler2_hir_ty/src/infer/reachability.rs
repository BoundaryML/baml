//! Reachability diagnostics over finalized expression types. This pass does
//! not propagate values through storage or change typing: in particular, a
//! previous read or write of a shared field cannot prove its next value.

use baml_compiler2_ast::{BinaryOp, Expr, ExprBody, ExprId, Stmt, StmtId, UnaryOp};
use baml_type::interned::{ClosedTy, InferTy, Ty};

use super::{
    InferenceContext, WorkingResult,
    truthy::{Truthiness, truthiness},
};
use crate::diagnostics::{
    DiagnosticLocation, DiagnosticSeverity, RelatedLocation, RelatedNote, TirDiagnostic,
    TirTypeError,
};

pub(super) fn add_diagnostics<'db>(
    body: &ExprBody,
    result: &WorkingResult<'db>,
    diagnostics: &mut Vec<TirDiagnostic<'db>>,
) {
    let reachability = Reachability { body, result };
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
                            related: Vec::new(),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    for (_, stmt) in body.stmts.iter() {
        if let Stmt::While {
            condition,
            body: loop_body,
            ..
        } = stmt
            && let Some(value) = reachability.condition(*condition)
        {
            // Keep the conventional spelling of an unconditional loop quiet.
            if matches!(
                body.exprs[*condition],
                Expr::Literal(baml_type::Literal::Bool(true))
            ) {
                continue;
            }
            reachability.warn(
                diagnostics,
                *condition,
                value,
                (!value).then(|| {
                    RelatedNote::new(
                        RelatedLocation::Expr(*loop_body),
                        "this loop body is unreachable",
                    )
                }),
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
}

impl<'db> Reachability<'_, 'db> {
    fn condition(&self, expr: ExprId) -> Option<bool> {
        match &self.body.exprs[expr] {
            Expr::Unary {
                op: UnaryOp::Not,
                expr,
            } => self.condition(*expr).map(|v| !v),
            Expr::Binary {
                op: BinaryOp::And,
                lhs,
                rhs,
            } => match (self.condition(*lhs), self.condition(*rhs)) {
                (Some(false), _) | (_, Some(false)) => Some(false),
                (Some(true), Some(true)) => Some(true),
                _ => None,
            },
            Expr::Binary {
                op: BinaryOp::Or,
                lhs,
                rhs,
            } => match (self.condition(*lhs), self.condition(*rhs)) {
                (Some(true), _) | (_, Some(true)) => Some(true),
                (Some(false), Some(false)) => Some(false),
                _ => None,
            },
            _ => self
                .result
                .type_of_expr
                .get(&expr)
                .and_then(static_truthiness),
        }
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
            Expr::Lambda(_) | Expr::Spawn { .. } => false,
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
            Stmt::Return(_) | Stmt::Throw { .. } | Stmt::Break | Stmt::Continue => true,
            Stmt::Expr(expr) => self.expr_exits(*expr),
            Stmt::Let { initializer, .. } => initializer.is_some_and(|expr| self.expr_exits(expr)),
            Stmt::Assign { value, .. } | Stmt::AssignOp { value, .. } => self.expr_exits(*value),
            Stmt::While {
                condition,
                body,
                after,
                ..
            } => {
                self.expr_exits(*condition)
                    || (self.condition(*condition) == Some(true)
                        && !InferenceContext::loop_body_breaks(self.body, *body, *after))
            }
            Stmt::WhileLet { scrutinee, .. } => self.expr_exits(*scrutinee),
            Stmt::For { collection, .. } => self.expr_exits(*collection),
            _ => false,
        }
    }
}

fn static_truthiness(ty: &Ty) -> Option<bool> {
    match truthiness(ty) {
        Truthiness::AlwaysTruthy => Some(true),
        Truthiness::AlwaysFalsy => Some(false),
        Truthiness::Runtime => None,
    }
}
