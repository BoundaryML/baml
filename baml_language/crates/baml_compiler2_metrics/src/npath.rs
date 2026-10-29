//! `NPath` complexity (Nejmeh 1988): the number of acyclic execution paths.
//!
//! Follows PMD's `NpathBaseVisitor`: sequential code multiplies, branches
//! add, and each condition adds its count of `&&` / `||` operators. Rules
//! for constructs Java lacks:
//!
//! - `match`: the sum of its arms, like a `switch`.
//! - `catch`: the protected expression plus each handler, like `try`.
//! - `a ?? b`: either side, like a ternary.
//! - `?.` access and call: the full path plus the short-circuit.
//! - `let ... else`: the binding's paths times (match, or the else branch).
//! - Template `if` / `for`: as their statement counterparts.
//!
//! Lambda bodies are separate functions and contribute one path, as nested
//! methods do in PMD. Counts are unbounded, as in PMD and Checkstyle: a long
//! run of sequential branches passes `u64::MAX`.

use baml_compiler2_ast::{
    BinaryOp, BodyNode, Expr, ExprBody, ExprId, Stmt, StmtId, TemplateSegment, TemplateTag,
};
use num_bigint::BigUint;

pub(crate) fn npath_expr(body: &ExprBody, root: ExprId) -> BigUint {
    NPath { body }.expr(root)
}

pub(crate) fn npath_template(body: &ExprBody, segments: &[TemplateSegment]) -> BigUint {
    NPath { body }.segments(segments)
}

struct NPath<'a> {
    body: &'a ExprBody,
}

impl NPath<'_> {
    fn expr(&self, id: ExprId) -> BigUint {
        let body = self.body;
        match &body.exprs[id] {
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expr(*then_branch)
                    + else_branch.map_or_else(one, |e| self.expr(e))
                    + self.boolean_complexity(*condition)
            }
            Expr::IfLet {
                then_branch,
                else_branch,
                ..
            } => self.expr(*then_branch) + else_branch.map_or_else(one, |e| self.expr(e)),
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.boolean_complexity(*scrutinee)
                    + arms
                        .iter()
                        .map(|&arm| self.expr(body.match_arms[arm].body))
                        .sum::<BigUint>()
            }
            Expr::Catch { base, clause } => {
                self.expr(*base)
                    + clause
                        .arms
                        .iter()
                        .map(|&arm| self.expr(body.catch_arms[arm].body))
                        .sum::<BigUint>()
            }
            Expr::Return { value } => value.map_or_else(one, |value| {
                self.expr(value) + self.boolean_complexity(value)
            }),
            Expr::Binary {
                op: BinaryOp::NullCoalesce,
                lhs,
                rhs,
            } => self.expr(*lhs) + self.expr(*rhs),
            Expr::OptionalMemberAccess { .. }
            | Expr::OptionalIndex { .. }
            | Expr::OptionalCall { .. } => self.children(id) + 1u32,
            Expr::Lambda(_) => one(),
            Expr::Template { tag, segments } => {
                let tag = match tag {
                    TemplateTag::Custom { tag, .. } => self.expr(*tag),
                    TemplateTag::Default { .. } => one(),
                };
                tag * self.segments(segments)
            }
            _ => self.children(id),
        }
    }

    fn stmt(&self, id: StmtId) -> BigUint {
        match &self.body.stmts[id] {
            Stmt::While {
                condition, body, ..
            } => self.expr(*body) + self.boolean_complexity(*condition) + 1u32,
            Stmt::WhileLet { body, .. } | Stmt::For { body, .. } => self.expr(*body) + 1u32,
            Stmt::Let {
                initializer,
                else_branch: Some(else_branch),
                ..
            } => {
                initializer.map_or_else(one, |init| self.expr(init))
                    * (self.expr(*else_branch) + 1u32)
            }
            Stmt::TypeBinding {
                operand,
                else_branch: Some(else_branch),
                ..
            } => self.expr(*operand) * (self.expr(*else_branch) + 1u32),
            Stmt::Return(value) => value.map_or_else(one, |value| {
                self.expr(value) + self.boolean_complexity(value)
            }),
            _ => {
                let mut children = Vec::new();
                self.body.stmt_children(id, &mut children);
                self.product(&children)
            }
        }
    }

    fn segments(&self, segments: &[TemplateSegment]) -> BigUint {
        segments
            .iter()
            .map(|segment| match segment {
                TemplateSegment::Text(_) => one(),
                TemplateSegment::Interp(expr) => self.expr(*expr),
                TemplateSegment::Effect { stmts, .. } => {
                    stmts.iter().map(|&stmt| self.stmt(stmt)).product()
                }
                TemplateSegment::For { body, .. } => self.segments(body) + 1u32,
                TemplateSegment::CStyleFor { cond, body, .. } => {
                    self.segments(body) + self.boolean_complexity(*cond) + 1u32
                }
                TemplateSegment::If {
                    branches,
                    else_body,
                } => {
                    else_body
                        .as_ref()
                        .map_or_else(one, |body| self.segments(body))
                        + branches
                            .iter()
                            .map(|branch| {
                                self.segments(&branch.body)
                                    + self.boolean_complexity(branch.condition)
                            })
                            .sum::<BigUint>()
                }
            })
            .product()
    }

    fn children(&self, id: ExprId) -> BigUint {
        let mut children = Vec::new();
        self.body.expr_children(id, &mut children);
        self.product(&children)
    }

    fn product(&self, nodes: &[BodyNode]) -> BigUint {
        nodes
            .iter()
            .map(|node| match *node {
                BodyNode::Expr(id) => self.expr(id),
                BodyNode::Stmt(id) => self.stmt(id),
            })
            .product()
    }

    /// The number of `&&` and `||` operators in `id`, outside nested lambdas.
    fn boolean_complexity(&self, id: ExprId) -> BigUint {
        let operators = self
            .body
            .reachable_excluding_lambdas(id)
            .into_iter()
            .filter(|node| {
                matches!(
                    node,
                    BodyNode::Expr(expr) if matches!(
                        self.body.exprs[*expr],
                        Expr::Binary { op: BinaryOp::And | BinaryOp::Or, .. }
                    )
                )
            })
            .count();
        operators.into()
    }
}

fn one() -> BigUint {
    BigUint::from(1u32)
}
