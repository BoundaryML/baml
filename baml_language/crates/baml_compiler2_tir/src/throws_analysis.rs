use std::collections::BTreeSet;

use baml_base::Name;
use baml_compiler2_ast::{self as ast, Expr, ExprBody, ExprId, Stmt, StmtId};

use crate::{
    throw_inference::flatten_ty_to_facts,
    ty::{Ty, TyAttr},
};

pub(crate) trait ThrowsAnalysisContext {
    fn expression_type(&self, expr_id: ExprId) -> Option<Ty>;

    fn catch_residual_throws(&self, expr_id: ExprId) -> Option<BTreeSet<Ty>>;

    fn instantiated_callee_throws(
        &self,
        call_expr_id: ExprId,
        callee_expr_id: ExprId,
        args: &[ExprId],
        unwrap_optional_callee: bool,
    ) -> Option<Ty>;

    fn named_callee_summary(&self, callee_expr_id: ExprId, body: &ExprBody)
    -> Option<BTreeSet<Ty>>;

    /// Throw summary of the implicit `baml.id.set` call that the `$id = e`
    /// special form lowers to (MIR `lower_set_runtime_id`). Without this,
    /// `$id = e` would silently bypass the throws contract that a direct
    /// `baml.id.set(e)` call is held to.
    fn runtime_id_set_throws(&self) -> Option<BTreeSet<Ty>>;

    /// Throw summary of `baml.json.from` — what a `recv.to_json()` sugar
    /// fallback lowers to. Unlike `string.from` (`throws never`), `baml.json.from`
    /// throws `JsonSerializationError`, so the fallback must charge that precise
    /// type rather than be skipped.
    fn to_json_fallback_throws(&self) -> Option<BTreeSet<Ty>>;

    /// Throw summary of `baml.json.to` — what a `Type.from_json(j)` sugar fallback
    /// lowers to. Decoding throws `JsonDecodeError`, so charge that precise type
    /// rather than the unaccounted-callee `unknown` default. (Named for the
    /// `from_json` sugar it serves, parallel to `to_json_fallback_throws`; the
    /// `from_` prefix is not a constructor.)
    #[allow(clippy::wrong_self_convention)]
    fn from_json_fallback_throws(&self) -> Option<BTreeSet<Ty>>;
}

/// Whether `expr` has the shape of a `recv.to_string()` call callee: a
/// `to_string` member access, or a dotted path whose last segment is `to_string`.
/// This is the syntactic trigger shared by the `obj.to_string()` sugar across TIR
/// (type inference + this throws pass) and MIR lowering; each site layers its own
/// guards on top (in-scope-local root, callee left untyped, etc.).
pub fn is_to_string_call_callee(expr: &Expr) -> bool {
    match expr {
        Expr::MemberAccess { member, .. } => member.as_str() == "to_string",
        Expr::Path(segs) => {
            segs.len() >= 2 && segs.last().is_some_and(|s| s.as_str() == "to_string")
        }
        _ => false,
    }
}

/// Whether `callee` is the callee of a `recv.to_string()` sugar fallback — a
/// `to_string` call shape the type checker left untyped (`Unknown`/`Error`)
/// because the receiver has no real `to_string` method. Such a call lowers to
/// `string.from(recv)`, which is `throws never`.
fn is_to_string_fallback_callee<C: ThrowsAnalysisContext>(
    context: &C,
    callee: ExprId,
    body: &ExprBody,
) -> bool {
    is_to_string_call_callee(&body.exprs[callee])
        && context
            .expression_type(callee)
            .is_none_or(|t| matches!(t, Ty::Unknown { .. } | Ty::Error { .. }))
}

/// Whether `expr` has the shape of a `recv.to_json()` call callee: a `to_json`
/// member access, or a dotted path whose last segment is `to_json`. The
/// syntactic trigger shared by the `obj.to_json()` sugar across TIR (inference +
/// this throws pass) and MIR lowering; the json analog of
/// [`is_to_string_call_callee`].
pub fn is_to_json_call_callee(expr: &Expr) -> bool {
    match expr {
        Expr::MemberAccess { member, .. } => member.as_str() == "to_json",
        Expr::Path(segs) => segs.len() >= 2 && segs.last().is_some_and(|s| s.as_str() == "to_json"),
        _ => false,
    }
}

/// Whether `callee` is the callee of a `recv.to_json()` sugar fallback — a
/// `to_json` call shape the type checker left untyped because the receiver has no
/// real `to_json` method. Such a call lowers to `baml.json.from(recv)`, which
/// throws `JsonSerializationError`.
fn is_to_json_fallback_callee<C: ThrowsAnalysisContext>(
    context: &C,
    callee: ExprId,
    body: &ExprBody,
) -> bool {
    is_to_json_call_callee(&body.exprs[callee])
        && context
            .expression_type(callee)
            .is_none_or(|t| matches!(t, Ty::Unknown { .. } | Ty::Error { .. }))
}

/// Whether `expr` has the shape of a `Type.from_json(j)` static-call callee: a
/// `from_json` member access, or a dotted path whose last segment is `from_json`.
/// The deserialize analog of [`is_to_json_call_callee`]; the static-constructor
/// counterpart of the instance-style `to_json` sugar. Each site layers its own
/// guards (type-name receiver, callee left untyped, exactly one arg).
pub fn is_from_json_call_callee(expr: &Expr) -> bool {
    match expr {
        Expr::MemberAccess { member, .. } => member.as_str() == "from_json",
        Expr::Path(segs) => {
            segs.len() >= 2 && segs.last().is_some_and(|s| s.as_str() == "from_json")
        }
        _ => false,
    }
}

/// Whether `callee` is the callee of a `Type.from_json(j)` sugar fallback — a
/// `from_json` call shape the type checker left untyped because the receiver type
/// has no real `from_json` method (no `implements baml.FromJson` override). Such a
/// call lowers to `baml.json.to<Type>(j)`, which throws `JsonDecodeError`.
fn is_from_json_fallback_callee<C: ThrowsAnalysisContext>(
    context: &C,
    callee: ExprId,
    body: &ExprBody,
) -> bool {
    is_from_json_call_callee(&body.exprs[callee])
        && context
            .expression_type(callee)
            .is_none_or(|t| matches!(t, Ty::Unknown { .. } | Ty::Error { .. }))
}

/// True when `expr` is the bare `$id` special form used as an assignment
/// target — the shape that lowers to an implicit `baml.id.set` call.
fn is_runtime_id_path(expr_id: ExprId, body: &ExprBody) -> bool {
    matches!(&body.exprs[expr_id], Expr::Path(segments)
        if segments.len() == 1 && segments[0].as_str() == "$id")
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
    collect_escaping_throws_from(context, body, body.root_expr)
}

/// [`collect_escaping_throws`] rooted at an arbitrary expression of `body`.
///
/// A lambda's body is an expression inside the enclosing function's arena, so
/// its throw surface must be collected from *its* root — walking `body.root_expr`
/// would compute the enclosing function's surface instead.
pub(crate) fn collect_escaping_throws_from<C: ThrowsAnalysisContext>(
    context: &C,
    body: &ExprBody,
    root: Option<ExprId>,
) -> BTreeSet<Ty> {
    let mut out = BTreeSet::new();
    if let Some(root) = root {
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
    call_expr_id: ExprId,
    callee_expr_id: ExprId,
    args: &[ExprId],
    body: &ExprBody,
    unwrap_optional_callee: bool,
    out: &mut BTreeSet<Ty>,
) {
    let mut accounted = false;

    if let Some(throws) = context.instantiated_callee_throws(
        call_expr_id,
        callee_expr_id,
        args,
        unwrap_optional_callee,
    ) {
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

/// The throws a `to_string`/`to_json`/`from_json` *sugar fallback* call charges,
/// or `None` when the call is not one.
///
/// These three call shapes never resolve to a real method — the type checker
/// leaves their callee `Unknown`, and MIR rewrites them into a concrete stdlib
/// call (`string.from` / `baml.json.from` / `baml.json.to`). Routing them through
/// [`collect_callee_escaping_throws`] would therefore hit its unaccounted-callee
/// default and charge a bogus `Ty::Unknown`, so every throw-fact walker must
/// consult this first and, on `Some`, charge exactly what it returns instead of
/// the callee's escaping throws. (The receiver/argument expressions' own throws
/// are collected by the caller's recursion, independently of this.)
///
/// Shared by both walkers so they cannot drift: `collect_from_expr` (a function's
/// inferred `throws` set) and `Builder::collect_throw_facts_from_expr` (a
/// `catch`/`catch_all` binder's type). A missing guard on the latter made
/// `f().to_string() catch_all (e) { ... }` type `e` as `... | Unknown`, which then
/// ICE'd in MIR runtime-type lowering.
pub(crate) fn sugar_fallback_call_throws<C: ThrowsAnalysisContext>(
    context: &C,
    callee: ExprId,
    arg_exprs: &[ExprId],
    body: &ExprBody,
) -> Option<BTreeSet<Ty>> {
    // `recv.to_string()` lowers to `string.from(recv)`, which is `throws never`.
    if arg_exprs.is_empty() && is_to_string_fallback_callee(context, callee, body) {
        return Some(BTreeSet::new());
    }
    // `recv.to_json()` lowers to `baml.json.from(recv)`, which throws
    // `JsonSerializationError`.
    if arg_exprs.is_empty() && is_to_json_fallback_callee(context, callee, body) {
        return Some(context.to_json_fallback_throws().unwrap_or_default());
    }
    // `Type.from_json(j)` lowers to `baml.json.to<Type>(j)`, which throws
    // `JsonDecodeError`.
    if arg_exprs.len() == 1 && is_from_json_fallback_callee(context, callee, body) {
        return Some(context.from_json_fallback_throws().unwrap_or_default());
    }
    None
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
        Stmt::WhileLet {
            scrutinee,
            body: while_body,
            ..
        } => {
            collect_from_expr(context, *scrutinee, body, out);
            collect_from_expr(context, *while_body, body, out);
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
            // `$id = e` is an implicit `baml.id.set(e)` call; its declared
            // throws escape exactly as a direct call's would. Mirror the
            // unaccounted-callee convention: no summary -> Unknown.
            if is_runtime_id_path(*target, body) {
                match context.runtime_id_set_throws() {
                    Some(summary) => out.extend(summary),
                    None => {
                        out.insert(Ty::Unknown {
                            attr: TyAttr::default(),
                        });
                    }
                }
            }
        }
        Stmt::Throw { value } => {
            collect_from_expr(context, *value, body, out);
            collect_value_throw_facts(context, *value, out);
        }
        Stmt::Defer { body: defer_body } => {
            // A `throw` inside a defer body propagates, so the defer body's
            // throws contribute to the enclosing function's throw surface.
            collect_from_expr(context, *defer_body, body, out);
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
        Expr::Return { value } => {
            // Evaluating the returned value may throw; walk it. The value is
            // returned, not raised, so do not charge it as a thrown error.
            if let Some(value) = value {
                collect_from_expr(context, *value, body, out);
            }
        }
        Expr::Call { callee, args, .. } => {
            collect_from_expr(context, *callee, body, out);
            let arg_exprs: Vec<_> = args.iter().map(|arg| arg.expr).collect();
            for arg in args {
                collect_from_expr(context, arg.expr, body, out);
            }
            // A `to_string`/`to_json`/`from_json` sugar fallback charges the
            // throws of the stdlib call it rewrites into, not the callee's
            // (unaccounted -> `unknown`) escaping throws. The receiver/args were
            // already walked above.
            if let Some(sugar) = sugar_fallback_call_throws(context, *callee, &arg_exprs, body) {
                out.extend(sugar);
                return;
            }
            // When the callee is an `OptionalMemberAccess` (`obj?.method`), the
            // inferred callee type is a nullable `Union([Ty::Function { ... }, Null])`
            // (the post-`Ty::Optional`-removal encoding).
            // `instantiated_callee_throws` only handles `Ty::Function`, so we
            // must strip the nullable wrapper to get the actual throws.  This
            // mirrors the type-inference fast-path in `builder.rs` that routes
            // `Call { callee: OptionalMemberAccess }` through
            // `finalize_optional_callee_call`.
            let unwrap_optional = matches!(&body.exprs[*callee], Expr::OptionalMemberAccess { .. });
            collect_callee_escaping_throws(
                context,
                expr_id,
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
            collect_callee_escaping_throws(context, expr_id, *callee, &arg_exprs, body, true, out);
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
        Expr::Spawn {
            name,
            with_exprs,
            body: spawn_body,
        } => {
            // Throws from a spawned body do NOT escape the spawning
            // function — they are captured into the resulting
            // `Future<T, E>`'s E parameter and only re-thrown at an
            // `await` site. The name and `with` expressions are evaluated
            // eagerly in the spawning function, so their throws DO escape —
            // walk them; do not walk spawn_body.
            if let Some(name_id) = name {
                collect_from_expr(context, *name_id, body, out);
            }
            for with_id in with_exprs {
                collect_from_expr(context, *with_id, body, out);
                // The middleware pipeline INVOKES each transformer eagerly at
                // the spawn site (`b(a(SpawnParams { ... }))`), so a
                // transformer's own callable `throws` escapes the spawning
                // function — not just whatever evaluating the expression
                // throws.
                if let Some(Ty::Function { throws, .. }) = context.expression_type(*with_id) {
                    match throws.as_ref() {
                        Ty::Never { .. } | Ty::Unknown { .. } | Ty::Error { .. } => {}
                        Ty::Null { .. } => {}
                        t => out.extend(flatten_ty_to_facts(t)),
                    }
                }
            }
            let _ = spawn_body;
        }
        Expr::Await { future } => {
            // `await f` re-throws the awaited future's error: `f: Future<T, E>`
            // contributes `E` to the body's escaping throws — and `await`
            // distributes over a UNION of futures (BEP-034), contributing
            // every member's error type. `null`/`never` mark a future that
            // cannot fail (BEP-034 v1 spells `never` as `null`);
            // `Unknown`/`Error` add no information — skip those.
            fn add_error_facts(error: &Ty, out: &mut BTreeSet<Ty>) {
                match error {
                    Ty::Never { .. } | Ty::Unknown { .. } | Ty::Error { .. } => {}
                    Ty::Null { .. } => {}
                    e => out.extend(flatten_ty_to_facts(e)),
                }
            }
            collect_from_expr(context, *future, body, out);
            match context.expression_type(*future) {
                Some(Ty::Future(_, error, _)) => add_error_facts(&error, out),
                Some(Ty::Union(members, _)) => {
                    for member in &members {
                        if let Ty::Future(_, error, _) = member {
                            add_error_facts(error, out);
                        }
                    }
                }
                _ => {}
            }
        }
        Expr::Template { tag, segments } => {
            if let ast::TemplateTag::Custom { tag, .. } = tag {
                collect_from_expr(context, *tag, body, out);
                // A tagged template invokes the tag fn, so its declared `throws`
                // escape just like a direct call's would.
                collect_callee_escaping_throws(context, expr_id, *tag, &[], body, false, out);
            }
            collect_from_template_segments(context, segments, body, out);
        }
        Expr::GenericApply { base, .. } => {
            collect_from_expr(context, *base, body, out);
        }
        Expr::Lambda(_)
        | Expr::Literal(_)
        | Expr::ByteStringLiteral(_)
        | Expr::Null
        | Expr::Path(_)
        | Expr::Missing => {}
    }
}

/// Recursive walk of a template segment tree collecting throw facts from
/// each interp/condition/iter expression and any nested for/if bodies.
fn collect_from_template_segments<C: ThrowsAnalysisContext>(
    context: &C,
    segments: &[ast::TemplateSegment],
    body: &ExprBody,
    out: &mut BTreeSet<Ty>,
) {
    for seg in segments {
        match seg {
            ast::TemplateSegment::Text(_) => {}
            ast::TemplateSegment::Interp(e) => collect_from_expr(context, *e, body, out),
            ast::TemplateSegment::For {
                collection,
                body: inner,
                ..
            } => {
                collect_from_expr(context, *collection, body, out);
                collect_from_template_segments(context, inner, body, out);
            }
            ast::TemplateSegment::CStyleFor {
                init,
                cond,
                step,
                body: inner,
            } => {
                // `init`/`step` are statements (the `let` and the assignment) that
                // can throw, so traverse them alongside `cond` and the body.
                collect_from_stmt(context, *init, body, out);
                collect_from_expr(context, *cond, body, out);
                if let Some(step_stmt) = step {
                    collect_from_stmt(context, *step_stmt, body, out);
                }
                collect_from_template_segments(context, inner, body, out);
            }
            ast::TemplateSegment::If {
                branches,
                else_body,
            } => {
                for branch in branches {
                    collect_from_expr(context, branch.condition, body, out);
                    collect_from_template_segments(context, &branch.body, body, out);
                }
                if let Some(eb) = else_body {
                    collect_from_template_segments(context, eb, body, out);
                }
            }
        }
    }
}
