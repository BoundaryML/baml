//! Single-pass AST counts: cyclomatic, cognitive, nesting, and exits.

use baml_compiler2_ast::{
    BinaryOp, BodyNode, Expr, ExprBody, ExprId, Stmt, StmtId, TemplateSegment, TemplateTag,
};

use crate::count;

/// What one walk of a body produces.
#[derive(Debug, Default)]
pub(crate) struct Structure {
    /// Decision points; the cyclomatic number is this plus one.
    pub decisions: u32,
    pub cognitive: u32,
    pub max_nesting: u32,
    /// Explicit `return` / `throw` sites outside nested lambdas.
    pub explicit_exits: u32,
}

pub(crate) fn walk_expr(body: &ExprBody, root: ExprId) -> Structure {
    let mut walker = Walker {
        body,
        lambda_depth: 0,
        out: Structure::default(),
    };
    walker.expr(root, 0);
    walker.out
}

pub(crate) fn walk_template(body: &ExprBody, segments: &[TemplateSegment]) -> Structure {
    let mut walker = Walker {
        body,
        lambda_depth: 0,
        out: Structure::default(),
    };
    walker.segments(segments, 0);
    walker.out
}

struct Walker<'a> {
    body: &'a ExprBody,
    lambda_depth: u32,
    out: Structure,
}

impl Walker<'_> {
    /// A nesting structure: `+1 + nesting` cognitive, and its contents sit one
    /// level deeper.
    fn structure(&mut self, nesting: u32) -> u32 {
        self.out.cognitive += 1 + nesting;
        self.out.max_nesting = self.out.max_nesting.max(nesting + 1);
        nesting + 1
    }

    fn exit(&mut self) {
        if self.lambda_depth == 0 {
            self.out.explicit_exits += 1;
        }
    }

    fn expr(&mut self, id: ExprId, nesting: u32) {
        let body = self.body;
        match &body.exprs[id] {
            Expr::If { .. } | Expr::IfLet { .. } => {
                let inner = self.structure(nesting);
                self.if_chain(id, nesting, inner);
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.out.decisions += count(arms.len().saturating_sub(1));
                self.expr(*scrutinee, nesting);
                let inner = self.structure(nesting);
                for &arm in arms {
                    let arm = &body.match_arms[arm];
                    if let Some(guard) = arm.guard {
                        self.out.decisions += 1;
                        self.expr(guard, inner);
                    }
                    self.expr(arm.body, inner);
                }
            }
            Expr::Catch { base, clause } => {
                let arms = &clause.arms;
                self.out.decisions += count(arms.len());
                self.expr(*base, nesting);
                let inner = self.structure(nesting);
                for &arm in arms {
                    let arm_body = body.catch_arms[arm].body;
                    self.expr(arm_body, inner);
                }
            }
            Expr::Binary {
                op: BinaryOp::And | BinaryOp::Or,
                ..
            } => {
                let mut ops = Vec::new();
                let mut leaves = Vec::new();
                self.flatten_logical(id, &mut ops, &mut leaves);
                self.out.decisions += count(ops.len());
                let runs = 1 + ops.windows(2).filter(|pair| pair[0] != pair[1]).count();
                self.out.cognitive += count(runs);
                for leaf in leaves {
                    self.expr(leaf, nesting);
                }
            }
            Expr::Binary {
                op: BinaryOp::NullCoalesce,
                ..
            }
            | Expr::OptionalMemberAccess { .. }
            | Expr::OptionalIndex { .. }
            | Expr::OptionalCall { .. } => {
                self.out.decisions += 1;
                self.children(id, nesting);
            }
            Expr::Return { .. } | Expr::Throw { .. } => {
                self.exit();
                self.children(id, nesting);
            }
            Expr::Lambda(lambda) => {
                if let Some(lambda_body) = lambda.body {
                    self.out.max_nesting = self.out.max_nesting.max(nesting + 1);
                    self.lambda_depth += 1;
                    self.expr(lambda_body, nesting + 1);
                    self.lambda_depth -= 1;
                }
            }
            Expr::Template { tag, segments } => {
                // `segments` is the template as written; the tag payload is
                // its desugaring, built from the same `ExprId`s, so walking
                // both would double-count.
                if let TemplateTag::Custom { tag, .. } = tag {
                    self.expr(*tag, nesting);
                }
                self.segments(segments, nesting);
            }
            _ => self.children(id, nesting),
        }
    }

    /// Walk an `if` / `if let` whose own cognitive increment the caller has
    /// already counted, then each `else if` (`+1` flat) and the final `else`
    /// (`+1`).
    fn if_chain(&mut self, mut id: ExprId, nesting: u32, inner: u32) {
        let body = self.body;
        loop {
            self.out.decisions += 1;
            let (condition, then_branch, else_branch) = match &body.exprs[id] {
                Expr::If {
                    condition,
                    then_branch,
                    else_branch,
                } => (*condition, *then_branch, *else_branch),
                Expr::IfLet {
                    scrutinee,
                    then_branch,
                    else_branch,
                    ..
                } => (*scrutinee, *then_branch, *else_branch),
                _ => unreachable!("if_chain called on a non-if expression"),
            };
            self.expr(condition, nesting);
            self.expr(then_branch, inner);
            let Some(else_branch) = else_branch else {
                return;
            };
            id = else_branch;
            self.out.cognitive += 1;
            if !matches!(body.exprs[id], Expr::If { .. } | Expr::IfLet { .. }) {
                self.expr(id, inner);
                return;
            }
        }
    }

    fn flatten_logical(&self, id: ExprId, ops: &mut Vec<BinaryOp>, leaves: &mut Vec<ExprId>) {
        match &self.body.exprs[id] {
            Expr::Binary {
                op: op @ (BinaryOp::And | BinaryOp::Or),
                lhs,
                rhs,
            } => {
                self.flatten_logical(*lhs, ops, leaves);
                ops.push(*op);
                self.flatten_logical(*rhs, ops, leaves);
            }
            _ => leaves.push(id),
        }
    }

    fn stmt(&mut self, id: StmtId, nesting: u32) {
        let body = self.body;
        match &body.stmts[id] {
            Stmt::While {
                condition,
                body,
                after,
                ..
            } => {
                self.out.decisions += 1;
                self.expr(*condition, nesting);
                let inner = self.structure(nesting);
                self.expr(*body, inner);
                if let Some(after) = after {
                    self.stmt(*after, inner);
                }
            }
            Stmt::WhileLet {
                scrutinee, body, ..
            } => {
                self.out.decisions += 1;
                self.expr(*scrutinee, nesting);
                let inner = self.structure(nesting);
                self.expr(*body, inner);
            }
            Stmt::For {
                collection, body, ..
            } => {
                self.out.decisions += 1;
                self.expr(*collection, nesting);
                let inner = self.structure(nesting);
                self.expr(*body, inner);
            }
            Stmt::Let {
                initializer,
                else_branch: Some(else_branch),
                ..
            } => self.diverging_else(*initializer, *else_branch, nesting),
            Stmt::TypeBinding {
                operand,
                else_branch: Some(else_branch),
                ..
            } => self.diverging_else(Some(*operand), *else_branch, nesting),
            Stmt::Return(_) | Stmt::Throw { .. } => {
                self.exit();
                self.stmt_children(id, nesting);
            }
            _ => self.stmt_children(id, nesting),
        }
    }

    /// `let ... else` and `let type ... else`: a refutable binding whose else
    /// branch must diverge.
    fn diverging_else(&mut self, initializer: Option<ExprId>, else_branch: ExprId, nesting: u32) {
        self.out.decisions += 1;
        if let Some(initializer) = initializer {
            self.expr(initializer, nesting);
        }
        let inner = self.structure(nesting);
        self.expr(else_branch, inner);
    }

    fn segments(&mut self, segments: &[TemplateSegment], nesting: u32) {
        for segment in segments {
            match segment {
                TemplateSegment::Text(_) => {}
                TemplateSegment::Interp(expr) => self.expr(*expr, nesting),
                TemplateSegment::Effect { stmts, .. } => {
                    for stmt in stmts {
                        self.stmt(*stmt, nesting);
                    }
                }
                TemplateSegment::For {
                    collection, body, ..
                } => {
                    self.out.decisions += 1;
                    self.expr(*collection, nesting);
                    let inner = self.structure(nesting);
                    self.segments(body, inner);
                }
                TemplateSegment::CStyleFor {
                    init,
                    cond,
                    step,
                    body,
                } => {
                    self.out.decisions += 1;
                    self.stmt(*init, nesting);
                    self.expr(*cond, nesting);
                    let inner = self.structure(nesting);
                    if let Some(step) = step {
                        self.stmt(*step, inner);
                    }
                    self.segments(body, inner);
                }
                TemplateSegment::If {
                    branches,
                    else_body,
                } => {
                    let inner = self.structure(nesting);
                    for (index, branch) in branches.iter().enumerate() {
                        self.out.decisions += 1;
                        if index > 0 {
                            self.out.cognitive += 1;
                        }
                        self.expr(branch.condition, nesting);
                        self.segments(&branch.body, inner);
                    }
                    if let Some(else_body) = else_body {
                        self.out.cognitive += 1;
                        self.segments(else_body, inner);
                    }
                }
            }
        }
    }

    fn children(&mut self, id: ExprId, nesting: u32) {
        let mut children = Vec::new();
        self.body.expr_children(id, &mut children);
        self.visit_all(&children, nesting);
    }

    fn stmt_children(&mut self, id: StmtId, nesting: u32) {
        let mut children = Vec::new();
        self.body.stmt_children(id, &mut children);
        self.visit_all(&children, nesting);
    }

    fn visit_all(&mut self, nodes: &[BodyNode], nesting: u32) {
        for node in nodes {
            match *node {
                BodyNode::Expr(id) => self.expr(id, nesting),
                BodyNode::Stmt(id) => self.stmt(id, nesting),
            }
        }
    }
}
