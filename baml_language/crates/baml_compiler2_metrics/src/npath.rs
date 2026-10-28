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
//! methods do in PMD. Counts saturate at `u64::MAX`.

use baml_compiler2_ast::{
    BinaryOp, BodyNode, Expr, ExprBody, ExprId, Stmt, StmtId, TemplateSegment, TemplateTag,
};

pub(crate) fn npath_expr(body: &ExprBody, root: ExprId) -> u64 {
    NPath { body }.expr(root)
}

pub(crate) fn npath_template(body: &ExprBody, segments: &[TemplateSegment]) -> u64 {
    NPath { body }.segments(segments)
}

struct NPath<'a> {
    body: &'a ExprBody,
}

impl NPath<'_> {
    fn expr(&self, id: ExprId) -> u64 {
        let body = self.body;
        match &body.exprs[id] {
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => self
                .expr(*then_branch)
                .saturating_add(else_branch.map_or(1, |e| self.expr(e)))
                .saturating_add(self.boolean_complexity(*condition)),
            Expr::IfLet {
                then_branch,
                else_branch,
                ..
            } => self
                .expr(*then_branch)
                .saturating_add(else_branch.map_or(1, |e| self.expr(e))),
            Expr::Match {
                scrutinee, arms, ..
            } => arms
                .iter()
                .map(|&arm| self.expr(body.match_arms[arm].body))
                .fold(self.boolean_complexity(*scrutinee), u64::saturating_add),
            Expr::Catch { base, clause } => clause
                .arms
                .iter()
                .map(|&arm| self.expr(body.catch_arms[arm].body))
                .fold(self.expr(*base), u64::saturating_add),
            Expr::Return { value } => value.map_or(1, |value| {
                self.expr(value)
                    .saturating_add(self.boolean_complexity(value))
            }),
            Expr::Binary {
                op: BinaryOp::NullCoalesce,
                lhs,
                rhs,
            } => self.expr(*lhs).saturating_add(self.expr(*rhs)),
            Expr::OptionalMemberAccess { .. }
            | Expr::OptionalIndex { .. }
            | Expr::OptionalCall { .. } => self.children(id).saturating_add(1),
            Expr::Lambda(_) => 1,
            Expr::Template { tag, segments } => {
                let tag = match tag {
                    TemplateTag::Custom { tag, .. } => self.expr(*tag),
                    TemplateTag::Default { .. } => 1,
                };
                tag.saturating_mul(self.segments(segments))
            }
            _ => self.children(id),
        }
    }

    fn stmt(&self, id: StmtId) -> u64 {
        match &self.body.stmts[id] {
            Stmt::While {
                condition, body, ..
            } => self
                .expr(*body)
                .saturating_add(self.boolean_complexity(*condition))
                .saturating_add(1),
            Stmt::WhileLet { body, .. } | Stmt::For { body, .. } => {
                self.expr(*body).saturating_add(1)
            }
            Stmt::Let {
                initializer,
                else_branch: Some(else_branch),
                ..
            } => initializer
                .map_or(1, |init| self.expr(init))
                .saturating_mul(self.expr(*else_branch).saturating_add(1)),
            Stmt::TypeBinding {
                operand,
                else_branch: Some(else_branch),
                ..
            } => self
                .expr(*operand)
                .saturating_mul(self.expr(*else_branch).saturating_add(1)),
            Stmt::Return(value) => value.map_or(1, |value| {
                self.expr(value)
                    .saturating_add(self.boolean_complexity(value))
            }),
            _ => {
                let mut children = Vec::new();
                self.body.stmt_children(id, &mut children);
                self.product(&children)
            }
        }
    }

    fn segments(&self, segments: &[TemplateSegment]) -> u64 {
        segments
            .iter()
            .map(|segment| match segment {
                TemplateSegment::Text(_) => 1,
                TemplateSegment::Interp(expr) => self.expr(*expr),
                TemplateSegment::Effect { stmts, .. } => stmts
                    .iter()
                    .map(|&stmt| self.stmt(stmt))
                    .fold(1, u64::saturating_mul),
                TemplateSegment::For { body, .. } => self.segments(body).saturating_add(1),
                TemplateSegment::CStyleFor { cond, body, .. } => self
                    .segments(body)
                    .saturating_add(self.boolean_complexity(*cond))
                    .saturating_add(1),
                TemplateSegment::If {
                    branches,
                    else_body,
                } => branches
                    .iter()
                    .map(|branch| {
                        self.segments(&branch.body)
                            .saturating_add(self.boolean_complexity(branch.condition))
                    })
                    .fold(
                        else_body.as_ref().map_or(1, |body| self.segments(body)),
                        u64::saturating_add,
                    ),
            })
            .fold(1, u64::saturating_mul)
    }

    fn children(&self, id: ExprId) -> u64 {
        let mut children = Vec::new();
        self.body.expr_children(id, &mut children);
        self.product(&children)
    }

    fn product(&self, nodes: &[BodyNode]) -> u64 {
        nodes
            .iter()
            .map(|node| match *node {
                BodyNode::Expr(id) => self.expr(id),
                BodyNode::Stmt(id) => self.stmt(id),
            })
            .fold(1, u64::saturating_mul)
    }

    /// The number of `&&` and `||` operators in `id`, outside nested lambdas.
    fn boolean_complexity(&self, id: ExprId) -> u64 {
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
        u64::try_from(operators).unwrap_or(u64::MAX)
    }
}
