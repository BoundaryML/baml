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

/// Walks an expression body and accumulates the set of throw facts that escape
/// it. `context` and `body` are constant for the duration of a walk; `out` is
/// the accumulator the recursive methods feed into.
struct ThrowsWalker<'a, C: ThrowsAnalysisContext> {
    context: &'a C,
    body: &'a ExprBody,
    out: BTreeSet<Ty>,
}

impl<'a, C: ThrowsAnalysisContext> ThrowsWalker<'a, C> {
    fn new(context: &'a C, body: &'a ExprBody) -> Self {
        Self {
            context,
            body,
            out: BTreeSet::new(),
        }
    }

    /// Collect throws for a `throw <value>` statement or expression: walk the
    /// thrown value sub-expression and add the value's own type to the throws
    /// set.
    fn collect_throw_value(&mut self, value: ExprId) {
        self.collect_from_expr(value);
        let thrown_ty = self.context.expression_type(value).unwrap_or(Ty::Unknown);
        self.out.extend(flatten_ty_to_facts(&thrown_ty));
    }

    fn collect_callee_escaping_throws(
        &mut self,
        callee_expr_id: ExprId,
        args: &[ExprId],
        unwrap_optional_callee: bool,
    ) {
        if let Some(throws) =
            self.context
                .instantiated_callee_throws(callee_expr_id, args, unwrap_optional_callee)
        {
            self.out.extend(flatten_ty_to_facts(&throws));
        } else if let Some(summary) = self.context.named_callee_summary(callee_expr_id, self.body) {
            self.out.extend(summary);
        } else {
            self.out.insert(Ty::Unknown);
        }
    }

    fn collect_from_stmt(&mut self, stmt_id: StmtId) {
        match &self.body.stmts[stmt_id] {
            Stmt::Expr(expr_id) => self.collect_from_expr(*expr_id),
            Stmt::Let {
                initializer,
                else_branch,
                ..
            } => {
                if let Some(init) = initializer {
                    self.collect_from_expr(*init);
                }
                if let Some(else_expr) = else_branch {
                    // Throws from a `let … else` else block escape the
                    // enclosing function unless caught — they're part of the
                    // function's effect set just like throws from anywhere else
                    // in the body.
                    self.collect_from_expr(*else_expr);
                }
            }
            Stmt::While {
                condition,
                body: while_body,
                after,
                ..
            } => {
                self.collect_from_expr(*condition);
                self.collect_from_expr(*while_body);
                if let Some(after_stmt) = after {
                    self.collect_from_stmt(*after_stmt);
                }
            }
            Stmt::For {
                collection,
                body: for_body,
                ..
            } => {
                self.collect_from_expr(*collection);
                self.collect_from_expr(*for_body);
            }
            Stmt::Return(expr) => {
                if let Some(expr) = expr {
                    self.collect_from_expr(*expr);
                }
            }
            Stmt::Assign { target, value } | Stmt::AssignOp { target, value, .. } => {
                self.collect_from_expr(*target);
                self.collect_from_expr(*value);
            }
            Stmt::Throw { value } => self.collect_throw_value(*value),
            Stmt::Break | Stmt::Continue | Stmt::Missing | Stmt::HeaderComment { .. } => {}
        }
    }

    fn collect_call_throws(&mut self, callee: ExprId, args: &[CallArg], unwrap_optional: bool) {
        self.collect_from_expr(callee);
        let arg_exprs: Vec<_> = args.iter().map(|arg| arg.expr).collect();
        for &arg in &arg_exprs {
            self.collect_from_expr(arg);
        }
        self.collect_callee_escaping_throws(callee, &arg_exprs, unwrap_optional);
    }

    /// Collect the throws of a single expression (and everything it transitively
    /// evaluates) into `out`.
    fn collect_from_expr(&mut self, expr_id: ExprId) {
        match &self.body.exprs[expr_id] {
            Expr::Throw { value } => self.collect_throw_value(*value),
            Expr::Call { callee, args, .. } => {
                // When the callee is an `OptionalMemberAccess` (`obj?.method`), the
                // inferred callee type is `Ty::Optional(Ty::Function { ... })`.
                // `instantiated_callee_throws` only handles `Ty::Function`, so we
                // must strip the optional wrapper to get the actual throws.  This
                // mirrors the type-inference fast-path in `builder.rs` that routes
                // `Call { callee: OptionalMemberAccess }` through
                // `finalize_optional_callee_call`.
                let unwrap_optional = self.context.call_unwraps_optional_member_callee()
                    && matches!(&self.body.exprs[*callee], Expr::OptionalMemberAccess { .. });
                let (callee, args) = (*callee, args.clone());
                self.collect_call_throws(callee, &args, unwrap_optional);
            }
            Expr::OptionalCall { callee, args } => {
                let (callee, args) = (*callee, args.clone());
                self.collect_call_throws(callee, &args, true);
            }
            Expr::Catch { base, clauses } => {
                let (base, clauses) = (*base, clauses.clone());
                if let Some(residual) = self.context.catch_residual_throws(expr_id) {
                    self.out.extend(residual);
                } else {
                    self.collect_from_expr(base);
                }
                if self.context.walk_catch_clauses() {
                    for clause in &clauses {
                        for arm_id in &clause.arms {
                            let arm_body = self.body.catch_arms[*arm_id].body;
                            self.collect_from_expr(arm_body);
                        }
                    }
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let (condition, then_branch, else_branch) =
                    (*condition, *then_branch, *else_branch);
                self.collect_from_expr(condition);
                self.collect_from_expr(then_branch);
                if let Some(else_expr) = else_branch {
                    self.collect_from_expr(else_expr);
                }
            }
            Expr::IfLet {
                scrutinee,
                then_branch,
                else_branch,
                ..
            } => {
                let (scrutinee, then_branch, else_branch) =
                    (*scrutinee, *then_branch, *else_branch);
                self.collect_from_expr(scrutinee);
                self.collect_from_expr(then_branch);
                if let Some(else_expr) = else_branch {
                    self.collect_from_expr(else_expr);
                }
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                let (scrutinee, arms) = (*scrutinee, arms.clone());
                self.collect_from_expr(scrutinee);
                for arm_id in &arms {
                    let arm = &self.body.match_arms[*arm_id];
                    let (guard, arm_body) = (arm.guard, arm.body);
                    if let Some(guard) = guard {
                        self.collect_from_expr(guard);
                    }
                    self.collect_from_expr(arm_body);
                }
            }
            Expr::Is { scrutinee, .. } => {
                self.collect_from_expr(*scrutinee);
            }
            Expr::Binary { lhs, rhs, .. } => {
                let (lhs, rhs) = (*lhs, *rhs);
                self.collect_from_expr(lhs);
                self.collect_from_expr(rhs);
            }
            Expr::Unary { expr, .. } | Expr::OptionalChain { expr } => {
                self.collect_from_expr(*expr);
            }
            Expr::Object {
                fields, spreads, ..
            } => {
                let field_values: Vec<_> = fields.iter().map(|(_, value)| *value).collect();
                let spread_exprs: Vec<_> = spreads.iter().map(|spread| spread.expr).collect();
                for value in field_values {
                    self.collect_from_expr(value);
                }
                for spread in spread_exprs {
                    self.collect_from_expr(spread);
                }
            }
            Expr::Array { elements } => {
                let elements = elements.clone();
                for elem in elements {
                    self.collect_from_expr(elem);
                }
            }
            Expr::Map { entries } => {
                let entries = entries.clone();
                for (key, value) in entries {
                    self.collect_from_expr(key);
                    self.collect_from_expr(value);
                }
            }
            Expr::Block { stmts, tail_expr } => {
                let (stmts, tail_expr) = (stmts.clone(), *tail_expr);
                for stmt_id in &stmts {
                    self.collect_from_stmt(*stmt_id);
                }
                if let Some(tail) = tail_expr {
                    self.collect_from_expr(tail);
                }
            }
            Expr::MemberAccess { base, .. }
            | Expr::Upcast { base, .. }
            | Expr::OptionalMemberAccess { base, .. } => {
                self.collect_from_expr(*base);
            }
            Expr::Index { base, index } | Expr::OptionalIndex { base, index } => {
                let (base, index) = (*base, *index);
                self.collect_from_expr(base);
                self.collect_from_expr(index);
            }
            Expr::Spawn { name, body: _ } => {
                // Throws from a spawned body do NOT escape the spawning
                // function — they are captured into the resulting
                // `Future<T, E>`'s E parameter and only re-thrown at an
                // `await` site. The name expression itself can throw, so
                // walk it; do not walk the spawn body.
                if let Some(name_id) = name {
                    self.collect_from_expr(*name_id);
                }
            }
            Expr::Await { future } => {
                // `await` re-throws the future's error. Walk the future
                // expression (its construction can throw); when configured,
                // also add the future's `E` parameter to the throws set so the
                // surrounding scope's throws include it.
                let future = *future;
                self.collect_from_expr(future);
                if self.context.await_adds_future_error() {
                    if let Some(Ty::Future(_value, error)) = self.context.expression_type(future) {
                        self.out.extend(flatten_ty_to_facts(&error));
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
}

pub(crate) fn collect_escaping_throws<C: ThrowsAnalysisContext>(
    context: &C,
    body: &ExprBody,
) -> BTreeSet<Ty> {
    let mut walker = ThrowsWalker::new(context, body);
    if let Some(root) = body.root_expr {
        walker.collect_from_expr(root);
    }
    walker.out
}

/// Collect the throws escaping a single expression (and everything it
/// transitively evaluates). Used to compute the residual throw set flowing into
/// a `catch` from its base expression.
pub(crate) fn collect_escaping_throws_from<C: ThrowsAnalysisContext>(
    context: &C,
    body: &ExprBody,
    expr_id: ExprId,
) -> BTreeSet<Ty> {
    let mut walker = ThrowsWalker::new(context, body);
    walker.collect_from_expr(expr_id);
    walker.out
}
