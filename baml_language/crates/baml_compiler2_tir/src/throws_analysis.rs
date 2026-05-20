use std::collections::BTreeSet;

use baml_base::Name;
use baml_compiler2_ast::{Expr, ExprBody, ExprId, Stmt, StmtId};

use crate::{
    throw_inference::flatten_ty_to_facts,
    ty::{Ty, TyAttr},
};

pub(crate) trait ThrowsAnalysisContext {
    fn expression_type(&self, expr_id: ExprId) -> Option<Ty>;

    fn catch_residual_throws(&self, expr_id: ExprId) -> Option<BTreeSet<Ty>>;

    fn instantiated_callee_throws(
        &self,
        callee_expr_id: ExprId,
        args: &[ExprId],
        unwrap_optional_callee: bool,
    ) -> Option<Ty>;

    fn named_callee_summary(&self, callee_expr_id: ExprId, body: &ExprBody)
    -> Option<BTreeSet<Ty>>;
}

pub(crate) fn expr_to_path_segments(expr_id: ExprId, body: &ExprBody) -> Option<Vec<Name>> {
    match &body.exprs[expr_id] {
        Expr::Path(segments) if !segments.is_empty() => Some(segments.clone()),
        Expr::MemberAccess { base, member } => {
            let mut base_segments = expr_to_path_segments(*base, body)?;
            base_segments.push(member.clone());
            Some(base_segments)
        }
        _ => None,
    }
}

pub(crate) fn collect_escaping_throws<C: ThrowsAnalysisContext>(
    context: &C,
    body: &ExprBody,
) -> BTreeSet<Ty> {
    let mut out = BTreeSet::new();
    if let Some(root) = body.root_expr {
        collect_from_expr(context, root, body, &mut out);
    }
    out
}

fn collect_value_throw_facts<C: ThrowsAnalysisContext>(
    context: &C,
    value_expr_id: ExprId,
    out: &mut BTreeSet<Ty>,
) {
    let thrown_ty = context
        .expression_type(value_expr_id)
        .unwrap_or(Ty::Unknown {
            attr: TyAttr::default(),
        });
    out.extend(flatten_ty_to_facts(&thrown_ty));
}

pub(crate) fn collect_callee_escaping_throws<C: ThrowsAnalysisContext>(
    context: &C,
    callee_expr_id: ExprId,
    args: &[ExprId],
    body: &ExprBody,
    unwrap_optional_callee: bool,
    out: &mut BTreeSet<Ty>,
) {
    let mut accounted = false;

    if let Some(throws) =
        context.instantiated_callee_throws(callee_expr_id, args, unwrap_optional_callee)
    {
        out.extend(flatten_ty_to_facts(&throws));
        accounted = true;
    }

    if !accounted && let Some(summary) = context.named_callee_summary(callee_expr_id, body) {
        out.extend(summary);
        accounted = true;
    }

    if !accounted {
        out.insert(Ty::Unknown {
            attr: TyAttr::default(),
        });
    }
}

fn collect_from_stmt<C: ThrowsAnalysisContext>(
    context: &C,
    stmt_id: StmtId,
    body: &ExprBody,
    out: &mut BTreeSet<Ty>,
) {
    match &body.stmts[stmt_id] {
        Stmt::Expr(expr_id) => collect_from_expr(context, *expr_id, body, out),
        Stmt::Let { initializer, .. } => {
            if let Some(init) = initializer {
                collect_from_expr(context, *init, body, out);
            }
        }
        Stmt::While {
            condition,
            body: while_body,
            after,
            ..
        } => {
            collect_from_expr(context, *condition, body, out);
            collect_from_expr(context, *while_body, body, out);
            if let Some(after_stmt) = after {
                collect_from_stmt(context, *after_stmt, body, out);
            }
        }
        Stmt::For {
            collection,
            body: for_body,
            ..
        } => {
            collect_from_expr(context, *collection, body, out);
            collect_from_expr(context, *for_body, body, out);
        }
        Stmt::Return(expr) => {
            if let Some(expr) = expr {
                collect_from_expr(context, *expr, body, out);
            }
        }
        Stmt::Assign { target, value } | Stmt::AssignOp { target, value, .. } => {
            collect_from_expr(context, *target, body, out);
            collect_from_expr(context, *value, body, out);
        }
        Stmt::Throw { value } => {
            collect_from_expr(context, *value, body, out);
            collect_value_throw_facts(context, *value, out);
        }
        Stmt::Break | Stmt::Continue | Stmt::Missing | Stmt::HeaderComment { .. } => {}
    }
}

fn collect_from_expr<C: ThrowsAnalysisContext>(
    context: &C,
    expr_id: ExprId,
    body: &ExprBody,
    out: &mut BTreeSet<Ty>,
) {
    match &body.exprs[expr_id] {
        Expr::Throw { value } => {
            collect_from_expr(context, *value, body, out);
            collect_value_throw_facts(context, *value, out);
        }
        Expr::Call { callee, args, .. } => {
            collect_from_expr(context, *callee, body, out);
            let arg_exprs: Vec<_> = args.iter().map(|arg| arg.expr).collect();
            for arg in args {
                collect_from_expr(context, arg.expr, body, out);
            }
            // When the callee is an `OptionalMemberAccess` (`obj?.method`), the
            // inferred callee type is `Ty::Optional(Ty::Function { ... })`.
            // `instantiated_callee_throws` only handles `Ty::Function`, so we
            // must strip the optional wrapper to get the actual throws.  This
            // mirrors the type-inference fast-path in `builder.rs` that routes
            // `Call { callee: OptionalMemberAccess }` through
            // `finalize_optional_callee_call`.
            let unwrap_optional = matches!(&body.exprs[*callee], Expr::OptionalMemberAccess { .. });
            collect_callee_escaping_throws(
                context,
                *callee,
                &arg_exprs,
                body,
                unwrap_optional,
                out,
            );
        }
        Expr::OptionalCall { callee, args } => {
            collect_from_expr(context, *callee, body, out);
            let arg_exprs: Vec<_> = args.iter().map(|arg| arg.expr).collect();
            for arg in args {
                collect_from_expr(context, arg.expr, body, out);
            }
            collect_callee_escaping_throws(context, *callee, &arg_exprs, body, true, out);
        }
        Expr::Catch { base, clauses } => {
            if let Some(residual) = context.catch_residual_throws(expr_id) {
                out.extend(residual);
            } else {
                collect_from_expr(context, *base, body, out);
            }
            for clause in clauses {
                for arm_id in &clause.arms {
                    let arm = &body.catch_arms[*arm_id];
                    collect_from_expr(context, arm.body, body, out);
                }
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_from_expr(context, *condition, body, out);
            collect_from_expr(context, *then_branch, body, out);
            if let Some(else_expr) = else_branch {
                collect_from_expr(context, *else_expr, body, out);
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            collect_from_expr(context, *scrutinee, body, out);
            for arm_id in arms {
                let arm = &body.match_arms[*arm_id];
                if let Some(guard) = arm.guard {
                    collect_from_expr(context, guard, body, out);
                }
                collect_from_expr(context, arm.body, body, out);
            }
        }
        Expr::Is { scrutinee, .. } => {
            collect_from_expr(context, *scrutinee, body, out);
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_from_expr(context, *lhs, body, out);
            collect_from_expr(context, *rhs, body, out);
        }
        Expr::Unary { expr, .. } | Expr::OptionalChain { expr } => {
            collect_from_expr(context, *expr, body, out);
        }
        Expr::Object {
            fields, spreads, ..
        } => {
            for (_, value) in fields {
                collect_from_expr(context, *value, body, out);
            }
            for spread in spreads {
                collect_from_expr(context, spread.expr, body, out);
            }
        }
        Expr::Array { elements } => {
            for elem in elements {
                collect_from_expr(context, *elem, body, out);
            }
        }
        Expr::Map { entries } => {
            for (key, value) in entries {
                collect_from_expr(context, *key, body, out);
                collect_from_expr(context, *value, body, out);
            }
        }
        Expr::Block { stmts, tail_expr } => {
            for stmt_id in stmts {
                collect_from_stmt(context, *stmt_id, body, out);
            }
            if let Some(tail) = tail_expr {
                collect_from_expr(context, *tail, body, out);
            }
        }
        Expr::MemberAccess { base, .. } | Expr::OptionalMemberAccess { base, .. } => {
            collect_from_expr(context, *base, body, out);
        }
        Expr::Index { base, index } | Expr::OptionalIndex { base, index } => {
            collect_from_expr(context, *base, body, out);
            collect_from_expr(context, *index, body, out);
        }
        Expr::Spawn {
            name,
            body: spawn_body,
        } => {
            // Throws from a spawned body do NOT escape the spawning
            // function — they are captured into the resulting
            // `Future<T, E>`'s E parameter and only re-thrown at an
            // `await` site. The name expression itself can throw, so
            // walk it; do not walk spawn_body.
            if let Some(name_id) = name {
                collect_from_expr(context, *name_id, body, out);
            }
            let _ = spawn_body;
        }
        Expr::Await { future } => {
            collect_from_expr(context, *future, body, out);
        }
        Expr::Lambda(_)
        | Expr::Literal(_)
        | Expr::ByteStringLiteral(_)
        | Expr::Null
        | Expr::Path(_)
        | Expr::Missing => {}
    }
}
