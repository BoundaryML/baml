//! Structural traversal of an [`ExprBody`] arena.
//!
//! An `ExprBody` is a flat arena, but the nodes in it form a tree rooted at
//! [`ExprBody::root_expr`]. Iterating the arena directly and iterating the tree
//! are *not* the same thing, and the difference is load-bearing: a lambda's
//! body hangs off its `Expr::Lambda` node, so a flat scan sees a `throw` written
//! inside a lambda as though the enclosing function wrote it.
//!
//! These helpers enumerate a node's *direct children* and leave descent to the
//! caller, because whether to enter a lambda body is a per-analysis decision —
//! scope building enters it, effect analysis must not. A caller that stops at
//! lambdas checks the node itself before recursing:
//!
//! ```ignore
//! if matches!(body.exprs[id], Expr::Lambda(_)) {
//!     return; // a lambda's effects belong to the lambda, not to us
//! }
//! ```

use std::collections::HashSet;

use crate::ast::{
    Expr, ExprBody, ExprId, PatId, Pattern, Stmt, StmtId, TemplateSegment, TemplateTag,
};

/// A direct child of an expression or statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BodyNode {
    Expr(ExprId),
    Stmt(StmtId),
}

fn append_type_operands(ty: &crate::ast::TypeExpr, out: &mut Vec<BodyNode>) {
    let mut operands = Vec::new();
    ty.unreflect_operands(&mut operands);
    out.extend(operands.into_iter().map(BodyNode::Expr));
}

impl ExprBody {
    /// Append the direct children of `id` to `out`, in source order.
    ///
    /// A lambda's body is deliberately *not* a child. It is an `ExprId` in this
    /// same arena, reachable through [`crate::ast::LambdaDef::body`] — but a
    /// lambda is a value, so what its body does belongs to the lambda. Callers
    /// that want it ask for it explicitly.
    pub fn expr_children(&self, id: ExprId, out: &mut Vec<BodyNode>) {
        match &self.exprs[id] {
            Expr::Literal(_)
            | Expr::ByteStringLiteral(_)
            | Expr::Null
            | Expr::Path(_)
            | Expr::Missing => {}
            Expr::QualifiedPath {
                qself, interface, ..
            } => {
                append_type_operands(qself, out);
                append_type_operands(interface, out);
            }
            Expr::Lambda(def) => {
                for param in &def.params {
                    if let Some(ty) = &param.type_expr {
                        append_type_operands(ty, out);
                    }
                }
                if let Some(ty) = &def.return_type {
                    append_type_operands(ty, out);
                }
                if let Some(ty) = &def.throws {
                    append_type_operands(ty, out);
                }
            }
            Expr::GenericApply { base, type_args } => {
                out.push(BodyNode::Expr(*base));
                for ty in type_args {
                    append_type_operands(ty, out);
                }
            }
            Expr::MemberAccess { base, .. } | Expr::OptionalMemberAccess { base, .. } => {
                out.push(BodyNode::Expr(*base));
            }
            Expr::Upcast { base, target } => {
                out.push(BodyNode::Expr(*base));
                append_type_operands(target, out);
            }
            Expr::Unary { expr, .. } | Expr::OptionalChain { expr } => {
                out.push(BodyNode::Expr(*expr));
            }
            Expr::Throw { value } => out.push(BodyNode::Expr(*value)),
            Expr::Await { future } => out.push(BodyNode::Expr(*future)),
            Expr::Return { value } => out.extend(value.map(BodyNode::Expr)),
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                out.push(BodyNode::Expr(*condition));
                out.push(BodyNode::Expr(*then_branch));
                out.extend(else_branch.map(BodyNode::Expr));
            }
            Expr::IfLet {
                pattern,
                scrutinee,
                then_branch,
                else_branch,
            } => {
                out.push(BodyNode::Expr(*scrutinee));
                self.pattern_expr_children(*pattern, out);
                out.push(BodyNode::Expr(*then_branch));
                out.extend(else_branch.map(BodyNode::Expr));
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                out.push(BodyNode::Expr(*scrutinee));
                for arm in arms {
                    let arm = &self.match_arms[*arm];
                    self.pattern_expr_children(arm.pattern, out);
                    out.extend(arm.guard.map(BodyNode::Expr));
                    out.push(BodyNode::Expr(arm.body));
                }
            }
            Expr::Is { scrutinee, pattern } => {
                out.push(BodyNode::Expr(*scrutinee));
                self.pattern_expr_children(*pattern, out);
            }
            Expr::Catch { base, clauses } => {
                out.push(BodyNode::Expr(*base));
                for clause in clauses {
                    self.pattern_expr_children(clause.binding, out);
                    if let Some(binding) = clause.stack_trace_binding {
                        self.pattern_expr_children(binding, out);
                    }
                    for arm in &clause.arms {
                        let arm = &self.catch_arms[*arm];
                        self.pattern_expr_children(arm.pattern, out);
                        out.push(BodyNode::Expr(arm.body));
                    }
                }
            }
            Expr::Spawn {
                name,
                with_exprs,
                body,
            } => {
                out.extend(name.map(BodyNode::Expr));
                out.extend(with_exprs.iter().copied().map(BodyNode::Expr));
                out.push(BodyNode::Expr(*body));
            }
            Expr::Binary { lhs, rhs, .. } => {
                out.push(BodyNode::Expr(*lhs));
                out.push(BodyNode::Expr(*rhs));
            }
            Expr::Index { base, index } | Expr::OptionalIndex { base, index } => {
                out.push(BodyNode::Expr(*base));
                out.push(BodyNode::Expr(*index));
            }
            Expr::Call {
                callee,
                type_args,
                args,
            } => {
                out.push(BodyNode::Expr(*callee));
                for ty in type_args {
                    append_type_operands(ty, out);
                }
                out.extend(args.iter().map(|arg| BodyNode::Expr(arg.expr)));
            }
            Expr::OptionalCall { callee, args } => {
                out.push(BodyNode::Expr(*callee));
                out.extend(args.iter().map(|arg| BodyNode::Expr(arg.expr)));
            }
            Expr::Object {
                type_args,
                fields,
                spreads,
                ..
            } => {
                for ty in type_args {
                    append_type_operands(ty, out);
                }
                out.extend(fields.iter().map(|field| BodyNode::Expr(field.value)));
                out.extend(spreads.iter().map(|s| BodyNode::Expr(s.expr)));
            }
            Expr::Array { elements } => {
                out.extend(elements.iter().copied().map(BodyNode::Expr));
            }
            Expr::Map { entries } => {
                for entry in entries {
                    out.push(BodyNode::Expr(entry.key));
                    out.push(BodyNode::Expr(entry.value));
                }
            }
            Expr::Block { stmts, tail_expr } => {
                out.extend(stmts.iter().copied().map(BodyNode::Stmt));
                out.extend(tail_expr.map(BodyNode::Expr));
            }
            Expr::Template { tag, segments } => {
                // Both representations, because they carry different things and
                // HIR/TIR each read only one: `segments` holds the user's
                // `${…}` expressions (what diagnostics and name resolution
                // point at), while the tag payload holds the desugared
                // realization — the concat chain, its `.to_string()` calls, the
                // `${for}` accumulators. Effect and call-graph analysis need the
                // latter. The two share `ExprId`s by construction, which is why
                // `reachable_excluding_lambdas` must de-duplicate.
                match tag {
                    TemplateTag::Default { elaborated } => out.push(BodyNode::Expr(*elaborated)),
                    TemplateTag::Custom { tag, body } => {
                        out.push(BodyNode::Expr(*tag));
                        out.push(BodyNode::Expr(*body));
                    }
                }
                for segment in segments {
                    template_segment_children(self, segment, out);
                }
            }
        }
    }

    /// Append expression operands nested in a pattern, in source order.
    /// Pattern shapes are not body nodes themselves; only runtime
    /// `unreflect(expr)` atoms contribute expression children.
    pub fn pattern_expr_children(&self, id: PatId, out: &mut Vec<BodyNode>) {
        match &self.patterns[id] {
            Pattern::Wildcard => {}
            Pattern::Type(ty) => append_type_operands(ty, out),
            Pattern::Unreflect(operand) => out.push(BodyNode::Expr(*operand)),
            Pattern::Bind { subpat, .. } => {
                if let Some(subpat) = subpat {
                    self.pattern_expr_children(*subpat, out);
                }
            }
            Pattern::Class {
                generic_args,
                associated_type_bindings,
                fields,
                ..
            } => {
                for ty in generic_args {
                    append_type_operands(ty, out);
                }
                for binding in associated_type_bindings {
                    append_type_operands(&binding.ty, out);
                }
                for field in fields {
                    self.pattern_expr_children(field.pat, out);
                }
            }
            Pattern::Array {
                prefix,
                rest,
                suffix,
                ascription,
            } => {
                if let Some(ty) = ascription {
                    append_type_operands(ty, out);
                }
                for pattern in prefix {
                    self.pattern_expr_children(*pattern, out);
                }
                if let Some(pattern) = rest.as_ref().and_then(|rest| rest.pat) {
                    self.pattern_expr_children(pattern, out);
                }
                for pattern in suffix {
                    self.pattern_expr_children(*pattern, out);
                }
            }
            Pattern::Or(patterns) => {
                for pattern in patterns {
                    self.pattern_expr_children(*pattern, out);
                }
            }
        }
    }

    /// Append the direct children of `id` to `out`, in source order.
    pub fn stmt_children(&self, id: StmtId, out: &mut Vec<BodyNode>) {
        match &self.stmts[id] {
            Stmt::Break | Stmt::Continue | Stmt::Missing | Stmt::HeaderComment { .. } => {}
            Stmt::Expr(expr) | Stmt::Throw { value: expr } | Stmt::Defer { body: expr } => {
                out.push(BodyNode::Expr(*expr));
            }
            Stmt::TypeBinding { value, .. } => append_type_operands(value, out),
            Stmt::Return(expr) => out.extend(expr.map(BodyNode::Expr)),
            Stmt::Let {
                pattern,
                initializer,
                else_branch,
                ..
            } => {
                out.extend(initializer.map(BodyNode::Expr));
                self.pattern_expr_children(*pattern, out);
                out.extend(else_branch.map(BodyNode::Expr));
            }
            Stmt::While {
                condition,
                body,
                after,
                ..
            } => {
                out.push(BodyNode::Expr(*condition));
                out.push(BodyNode::Expr(*body));
                out.extend(after.map(BodyNode::Stmt));
            }
            Stmt::WhileLet {
                pattern,
                scrutinee,
                body,
            } => {
                out.push(BodyNode::Expr(*scrutinee));
                self.pattern_expr_children(*pattern, out);
                out.push(BodyNode::Expr(*body));
            }
            Stmt::For {
                binding,
                collection,
                body,
            } => {
                out.push(BodyNode::Expr(*collection));
                self.pattern_expr_children(*binding, out);
                out.push(BodyNode::Expr(*body));
            }
            Stmt::Assign { target, value } | Stmt::AssignOp { target, value, .. } => {
                out.push(BodyNode::Expr(*target));
                out.push(BodyNode::Expr(*value));
            }
        }
    }

    /// Every node reachable from `root`, in pre-order, without entering nested
    /// lambda bodies.
    ///
    /// This is the traversal an effect analysis wants: a lambda is a *value*, so
    /// what its body throws or calls belongs to the lambda, not to the code that
    /// defines it. Only invoking it transfers those effects.
    pub fn reachable_excluding_lambdas(&self, root: ExprId) -> Vec<BodyNode> {
        // The arena is a DAG, not a tree: a template's `segments` and its
        // desugared tag payload are built from the *same* `ExprId`s, so a
        // subtree is reachable by more than one path. Without this set the walk
        // re-visits shared nodes once per path — exponential in template
        // nesting — and callers that push per visit (find-references,
        // call-site collection) report duplicates.
        let mut seen: HashSet<BodyNode> = HashSet::new();
        let mut stack = vec![BodyNode::Expr(root)];
        let mut visited = Vec::new();
        let mut children = Vec::new();
        while let Some(node) = stack.pop() {
            if !seen.insert(node) {
                continue;
            }
            visited.push(node);
            children.clear();
            // `expr_children` yields nothing for a lambda, so a lambda node is
            // visited but its body is not descended into.
            match node {
                BodyNode::Expr(id) => self.expr_children(id, &mut children),
                BodyNode::Stmt(id) => self.stmt_children(id, &mut children),
            }
            stack.extend(children.iter().rev().copied());
        }
        visited
    }
}

fn template_segment_children(
    expr_body: &ExprBody,
    segment: &TemplateSegment,
    out: &mut Vec<BodyNode>,
) {
    match segment {
        TemplateSegment::Text(_) => {}
        TemplateSegment::Interp(expr) => out.push(BodyNode::Expr(*expr)),
        TemplateSegment::For {
            binding,
            collection,
            body,
        } => {
            out.push(BodyNode::Expr(*collection));
            expr_body.pattern_expr_children(*binding, out);
            for inner in body {
                template_segment_children(expr_body, inner, out);
            }
        }
        TemplateSegment::CStyleFor {
            init,
            cond,
            step,
            body,
        } => {
            out.push(BodyNode::Stmt(*init));
            out.push(BodyNode::Expr(*cond));
            out.extend(step.map(BodyNode::Stmt));
            for inner in body {
                template_segment_children(expr_body, inner, out);
            }
        }
        TemplateSegment::If {
            branches,
            else_body,
        } => {
            for branch in branches {
                out.push(BodyNode::Expr(branch.condition));
                for inner in &branch.body {
                    template_segment_children(expr_body, inner, out);
                }
            }
            for inner in else_body.iter().flatten() {
                template_segment_children(expr_body, inner, out);
            }
        }
    }
}
