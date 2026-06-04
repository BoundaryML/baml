use std::collections::BTreeSet;

use baml_base::Name;
use baml_compiler2_ast::{CallArg, Expr, ExprBody, ExprId, Stmt, StmtId};

use crate::{throw_inference::flatten_ty_to_facts, ty::Ty};

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

    /// Whether a `catch` expression's clause arms should be walked and have
    /// their throws collected. The default (`true`) treats a `catch` as a
    /// normal sub-expression whose handler bodies can themselves throw. The
    /// "catch base" walk used to compute the residual throws *flowing into* a
    /// catch overrides this to `false` so a nested catch is opaque (only its
    /// `base` is walked, never its clauses).
    fn walk_catch_clauses(&self) -> bool {
        true
    }

    /// Whether an `await` expression should add the awaited future's `E`
    /// (error) parameter to the throws set. The default (`false`) only walks
    /// the future sub-expression. The "catch base" walk overrides this to
    /// `true` so an `await` inside a `catch` base re-throws the future's error.
    fn await_adds_future_error(&self) -> bool {
        false
    }

    /// Whether a plain `Call` whose callee is an `OptionalMemberAccess`
    /// (`obj?.method()`) should unwrap the optional wrapper before reading the
    /// callee's throws. The default (`true`) mirrors the type-inference
    /// fast-path. The "catch base" walk overrides this to `false` to preserve
    /// its original (non-unwrapping) behavior.
    fn call_unwraps_optional_member_callee(&self) -> bool {
        true
    }
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

/// Collect throws for a `throw <value>` statement or expression: walk the
/// thrown value sub-expression and add the value's own type to the throws set.
fn collect_throw_value<C: ThrowsAnalysisContext>(
    context: &C,
    value: ExprId,
    body: &ExprBody,
    out: &mut BTreeSet<Ty>,
) {
    collect_from_expr(context, value, body, out);
    let thrown_ty = context.expression_type(value).unwrap_or(Ty::Unknown);
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
    if let Some(throws) =
        context.instantiated_callee_throws(callee_expr_id, args, unwrap_optional_callee)
    {
        out.extend(flatten_ty_to_facts(&throws));
    } else if let Some(summary) = context.named_callee_summary(callee_expr_id, body) {
        out.extend(summary);
    } else {
        out.insert(Ty::Unknown);
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
        Stmt::Let {
            initializer,
            else_branch,
            ..
        } => {
            if let Some(init) = initializer {
                collect_from_expr(context, *init, body, out);
            }
            if let Some(else_expr) = else_branch {
                // Throws from a `let … else` else block escape the
                // enclosing function unless caught — they're part of the
                // function's effect set just like throws from anywhere else
                // in the body.
                collect_from_expr(context, *else_expr, body, out);
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
        Stmt::Throw { value } => collect_throw_value(context, *value, body, out),
        Stmt::Break | Stmt::Continue | Stmt::Missing | Stmt::HeaderComment { .. } => {}
    }
}

fn collect_call_throws<C: ThrowsAnalysisContext>(
    context: &C,
    callee: ExprId,
    args: &[CallArg],
    body: &ExprBody,
    unwrap_optional: bool,
    out: &mut BTreeSet<Ty>,
) {
    collect_from_expr(context, callee, body, out);
    let arg_exprs: Vec<_> = args.iter().map(|arg| arg.expr).collect();
    for &arg in &arg_exprs {
        collect_from_expr(context, arg, body, out);
    }
    collect_callee_escaping_throws(context, callee, &arg_exprs, body, unwrap_optional, out);
}

/// Collect the throws of a single expression (and everything it transitively
/// evaluates) into `out`. Used to compute the residual throw set flowing into
/// a `catch` from its base expression.
pub(crate) fn collect_from_expr<C: ThrowsAnalysisContext>(
    context: &C,
    expr_id: ExprId,
    body: &ExprBody,
    out: &mut BTreeSet<Ty>,
) {
    match &body.exprs[expr_id] {
        Expr::Throw { value } => collect_throw_value(context, *value, body, out),
        Expr::Call { callee, args, .. } => {
            // When the callee is an `OptionalMemberAccess` (`obj?.method`), the
            // inferred callee type is `Ty::Optional(Ty::Function { ... })`.
            // `instantiated_callee_throws` only handles `Ty::Function`, so we
            // must strip the optional wrapper to get the actual throws.  This
            // mirrors the type-inference fast-path in `builder.rs` that routes
            // `Call { callee: OptionalMemberAccess }` through
            // `finalize_optional_callee_call`.
            let unwrap_optional = context.call_unwraps_optional_member_callee()
                && matches!(&body.exprs[*callee], Expr::OptionalMemberAccess { .. });
            collect_call_throws(context, *callee, args, body, unwrap_optional, out);
        }
        Expr::OptionalCall { callee, args } => {
            collect_call_throws(context, *callee, args, body, true, out);
        }
        Expr::Catch { base, clauses } => {
            if let Some(residual) = context.catch_residual_throws(expr_id) {
                out.extend(residual);
            } else {
                collect_from_expr(context, *base, body, out);
            }
            if context.walk_catch_clauses() {
                for clause in clauses {
                    for arm_id in &clause.arms {
                        let arm = &body.catch_arms[*arm_id];
                        collect_from_expr(context, arm.body, body, out);
                    }
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
        Expr::IfLet {
            scrutinee,
            then_branch,
            else_branch,
            ..
        } => {
            collect_from_expr(context, *scrutinee, body, out);
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
        Expr::MemberAccess { base, .. }
        | Expr::Upcast { base, .. }
        | Expr::OptionalMemberAccess { base, .. } => {
            collect_from_expr(context, *base, body, out);
        }
        Expr::Index { base, index } | Expr::OptionalIndex { base, index } => {
            collect_from_expr(context, *base, body, out);
            collect_from_expr(context, *index, body, out);
        }
        Expr::Spawn { name, body: _ } => {
            // Throws from a spawned body do NOT escape the spawning
            // function — they are captured into the resulting
            // `Future<T, E>`'s E parameter and only re-thrown at an
            // `await` site. The name expression itself can throw, so
            // walk it; do not walk the spawn body.
            if let Some(name_id) = name {
                collect_from_expr(context, *name_id, body, out);
            }
        }
        Expr::Await { future } => {
            // `await` re-throws the future's error. Walk the future
            // expression (its construction can throw); when configured,
            // also add the future's `E` parameter to the throws set so the
            // surrounding scope's throws include it.
            collect_from_expr(context, *future, body, out);
            if context.await_adds_future_error() {
                if let Some(Ty::Future(_value, error)) = context.expression_type(*future) {
                    out.extend(flatten_ty_to_facts(&error));
                }
            }
        }
        Expr::Lambda(_)
        | Expr::Literal(_)
        | Expr::ByteStringLiteral(_)
        | Expr::Null
        | Expr::Path(_)
        | Expr::Missing => {}
    }
}
