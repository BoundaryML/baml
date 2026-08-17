//! Declaration-structural parameter-default rules (pre-S17 they lived in
//! TIR's builder): required-after-default ordering, `self` defaults, and
//! forward references from a default to a later parameter. The default's
//! TYPE check itself rides the `ParameterDefaults` body inference.

use baml_base::Name;
use baml_compiler2_ast as ast;
use baml_compiler2_ast::{Expr, ExprBody, ExprId, PatId, Stmt, StmtId};
use baml_compiler2_hir::loc::FunctionLoc;
use rustc_hash::FxHashSet;
use text_size::TextRange;

use crate::diagnostics::TirTypeError;

/// The structural default rules for one function, spans resolved against
/// the parameter spans and the defaults arena's own source map.
pub fn parameter_default_diagnostics<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: FunctionLoc<'db>,
) -> Vec<(TextRange, TirTypeError)> {
    let data = baml_compiler2_ppir::item_data::function_data(db, function);
    let source_map = baml_compiler2_ppir::item_data::function_source_map(db, function);
    let defaults = baml_compiler2_ppir::function_parameter_defaults(db, function);
    let mut out = Vec::new();
    let mut seen_default = false;
    for (index, param) in data.params.iter().enumerate() {
        let Some(default_ref) = defaults.param_default(index) else {
            if seen_default {
                out.push((
                    source_map.param_spans[index],
                    TirTypeError::RequiredParamAfterDefault {
                        name: param.name.clone(),
                    },
                ));
            }
            continue;
        };
        seen_default = true;
        let default_expr = default_ref.expr.expr();
        let default_span = defaults.defaults.source_map.expr_span(default_expr);
        if param.name.as_str() == "self" {
            out.push((default_span, TirTypeError::SelfParamDefault));
            continue;
        }
        let later_params: FxHashSet<Name> = data
            .params
            .iter()
            .skip(index + 1)
            .map(|param| param.name.clone())
            .collect();
        for referenced in
            default_expr_forward_references(default_expr, &defaults.defaults.exprs, &later_params)
        {
            out.push((
                default_span,
                TirTypeError::DefaultParamForwardReference {
                    param: param.name.clone(),
                    referenced,
                },
            ));
        }
    }
    out
}

fn default_expr_forward_references(
    expr_id: ExprId,
    body: &ExprBody,
    later_params: &FxHashSet<Name>,
) -> Vec<Name> {
    let mut shadowed = Vec::new();
    let mut refs = Vec::new();
    collect_default_expr_forward_references(expr_id, body, later_params, &mut shadowed, &mut refs);
    refs
}

fn collect_default_expr_forward_references(
    expr_id: ExprId,
    body: &ExprBody,
    later_params: &FxHashSet<Name>,
    shadowed: &mut Vec<Name>,
    refs: &mut Vec<Name>,
) {
    match &body.exprs[expr_id] {
        Expr::Path(segments) => {
            if let Some(root) = segments.first()
                && later_params.contains(root)
                && !shadowed.iter().rev().any(|name| name == root)
                && !refs.contains(root)
            {
                refs.push(root.clone());
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_default_expr_forward_references(*condition, body, later_params, shadowed, refs);
            collect_default_expr_forward_references(
                *then_branch,
                body,
                later_params,
                shadowed,
                refs,
            );
            if let Some(expr) = else_branch {
                collect_default_expr_forward_references(*expr, body, later_params, shadowed, refs);
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            collect_default_expr_forward_references(*scrutinee, body, later_params, shadowed, refs);
            for arm_id in arms {
                let arm = &body.match_arms[*arm_id];
                let saved_len = shadowed.len();
                collect_default_pattern_forward_references(
                    arm.pattern,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                push_pattern_bindings(arm.pattern, body, shadowed);
                if let Some(guard) = arm.guard {
                    collect_default_expr_forward_references(
                        guard,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
                collect_default_expr_forward_references(
                    arm.body,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                shadowed.truncate(saved_len);
            }
        }
        Expr::IfLet {
            pattern,
            scrutinee,
            then_branch,
            else_branch,
        } => {
            collect_default_expr_forward_references(*scrutinee, body, later_params, shadowed, refs);
            let saved_len = shadowed.len();
            collect_default_pattern_forward_references(
                *pattern,
                body,
                later_params,
                shadowed,
                refs,
            );
            push_pattern_bindings(*pattern, body, shadowed);
            collect_default_expr_forward_references(
                *then_branch,
                body,
                later_params,
                shadowed,
                refs,
            );
            shadowed.truncate(saved_len);
            if let Some(else_branch) = else_branch {
                collect_default_expr_forward_references(
                    *else_branch,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
            }
        }
        Expr::Is { scrutinee, pattern } => {
            collect_default_expr_forward_references(*scrutinee, body, later_params, shadowed, refs);
            collect_default_pattern_forward_references(
                *pattern,
                body,
                later_params,
                shadowed,
                refs,
            );
        }
        Expr::Catch { base, clauses } => {
            collect_default_expr_forward_references(*base, body, later_params, shadowed, refs);
            for clause in clauses {
                let clause_saved_len = shadowed.len();
                collect_default_pattern_forward_references(
                    clause.binding,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                push_pattern_bindings(clause.binding, body, shadowed);
                if let Some(stack_trace_binding) = clause.stack_trace_binding {
                    collect_default_pattern_forward_references(
                        stack_trace_binding,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                    push_pattern_bindings(stack_trace_binding, body, shadowed);
                }
                for arm_id in &clause.arms {
                    let arm = &body.catch_arms[*arm_id];
                    let arm_saved_len = shadowed.len();
                    collect_default_pattern_forward_references(
                        arm.pattern,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                    push_pattern_bindings(arm.pattern, body, shadowed);
                    collect_default_expr_forward_references(
                        arm.body,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                    shadowed.truncate(arm_saved_len);
                }
                shadowed.truncate(clause_saved_len);
            }
        }
        Expr::Throw { value } | Expr::Unary { expr: value, .. } => {
            collect_default_expr_forward_references(*value, body, later_params, shadowed, refs);
        }
        Expr::Return { value } => {
            if let Some(value) = value {
                collect_default_expr_forward_references(*value, body, later_params, shadowed, refs);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_default_expr_forward_references(*lhs, body, later_params, shadowed, refs);
            collect_default_expr_forward_references(*rhs, body, later_params, shadowed, refs);
        }
        Expr::Call {
            callee,
            type_args,
            args,
        } => {
            collect_default_expr_forward_references(*callee, body, later_params, shadowed, refs);
            for type_arg in type_args {
                if let ast::TypeArg::Unreflect(operand) = type_arg {
                    collect_default_expr_forward_references(
                        *operand,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
            }
            for arg in args {
                collect_default_expr_forward_references(
                    arg.expr,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
            }
        }
        Expr::OptionalCall { callee, args } => {
            collect_default_expr_forward_references(*callee, body, later_params, shadowed, refs);
            for arg in args {
                collect_default_expr_forward_references(
                    arg.expr,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
            }
        }
        Expr::Object {
            fields, spreads, ..
        } => {
            for field in fields {
                collect_default_expr_forward_references(
                    field.value,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
            }
            for spread in spreads {
                collect_default_expr_forward_references(
                    spread.expr,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
            }
        }
        Expr::Array { elements } => {
            for expr in elements {
                collect_default_expr_forward_references(*expr, body, later_params, shadowed, refs);
            }
        }
        Expr::Map { entries } => {
            for entry in entries {
                collect_default_expr_forward_references(
                    entry.key,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                collect_default_expr_forward_references(
                    entry.value,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
            }
        }
        Expr::Block { stmts, tail_expr } => {
            let saved_len = shadowed.len();
            for stmt in stmts {
                collect_default_stmt_forward_references(*stmt, body, later_params, shadowed, refs);
            }
            if let Some(expr) = tail_expr {
                collect_default_expr_forward_references(*expr, body, later_params, shadowed, refs);
            }
            shadowed.truncate(saved_len);
        }
        Expr::MemberAccess { base, .. }
        | Expr::Upcast { base, .. }
        | Expr::OptionalMemberAccess { base, .. }
        | Expr::OptionalChain { expr: base } => {
            collect_default_expr_forward_references(*base, body, later_params, shadowed, refs);
        }
        Expr::Index { base, index } | Expr::OptionalIndex { base, index } => {
            collect_default_expr_forward_references(*base, body, later_params, shadowed, refs);
            collect_default_expr_forward_references(*index, body, later_params, shadowed, refs);
        }
        Expr::Lambda(func_def) => {
            let saved_len = shadowed.len();
            for param in &func_def.params {
                shadowed.push(param.name.clone());
            }
            for param in &func_def.params {
                if let Some(default) = param.default {
                    collect_default_expr_forward_references(
                        default.expr(),
                        &func_def.defaults.exprs,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
            }
            if let Some(lambda_root) = func_def.body {
                collect_default_expr_forward_references(
                    lambda_root,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
            }
            shadowed.truncate(saved_len);
        }
        Expr::Spawn {
            name,
            with_exprs,
            body: spawn_body,
        } => {
            if let Some(name_id) = name {
                collect_default_expr_forward_references(
                    *name_id,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
            }
            for with_id in with_exprs {
                collect_default_expr_forward_references(
                    *with_id,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
            }
            // The spawn BODY is deferred (wrapped in a synthetic lambda
            // and evaluated on the spawned task) — only `name` and the
            // `with` transformers are evaluated eagerly, so a default
            // capturing a later parameter inside `spawn { ... }` is fine.
            let _ = spawn_body;
        }
        Expr::Await { future } => {
            collect_default_expr_forward_references(*future, body, later_params, shadowed, refs);
        }
        Expr::Template { tag, segments } => {
            if let ast::TemplateTag::Custom { tag, .. } = tag {
                collect_default_expr_forward_references(*tag, body, later_params, shadowed, refs);
            }
            collect_default_expr_forward_references_in_template_segments(
                segments,
                body,
                later_params,
                shadowed,
                refs,
            );
        }
        Expr::GenericApply { base, .. } => {
            collect_default_expr_forward_references(*base, body, later_params, shadowed, refs);
        }
        Expr::Literal(_) | Expr::ByteStringLiteral(_) | Expr::Null | Expr::Missing => {}
    }
}

/// Recursive walk over a tagged-template segment tree. Same forward-reference
/// collection as `collect_default_expr_forward_references` but threads
/// through nested for-bodies and if-branches, pushing for-bindings onto the
/// shadowed stack.
fn collect_default_expr_forward_references_in_template_segments(
    segments: &[ast::TemplateSegment],
    body: &ExprBody,
    later_params: &FxHashSet<Name>,
    shadowed: &mut Vec<Name>,
    refs: &mut Vec<Name>,
) {
    for seg in segments {
        match seg {
            ast::TemplateSegment::Text(_) => {}
            ast::TemplateSegment::Interp(e) => {
                collect_default_expr_forward_references(*e, body, later_params, shadowed, refs);
            }
            ast::TemplateSegment::For {
                binding,
                collection,
                body: inner,
            } => {
                collect_default_expr_forward_references(
                    *collection,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                let saved_len = shadowed.len();
                collect_default_pattern_forward_references(
                    *binding,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                push_pattern_bindings(*binding, body, shadowed);
                collect_default_expr_forward_references_in_template_segments(
                    inner,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                shadowed.truncate(saved_len);
            }
            ast::TemplateSegment::CStyleFor {
                init,
                cond,
                body: inner,
                ..
            } => {
                // Pull the loop var's pattern + initializer out of the `init`
                // `let` (releases the borrow before the collect calls below).
                let (init_initializer, init_pattern) = match &body.stmts[*init] {
                    ast::Stmt::Let {
                        initializer,
                        pattern,
                        ..
                    } => (*initializer, Some(*pattern)),
                    _ => (None, None),
                };
                // The initializer runs before the loop var is bound, so it
                // may reference outer names but never the loop var itself —
                // process it before shadowing.
                if let Some(e) = init_initializer {
                    collect_default_expr_forward_references(e, body, later_params, shadowed, refs);
                }
                // Shadow the loop var, then process `cond` and the body — both
                // see the binding (e.g. `i` in `for (let i = 0; i < n; …)`), so
                // a later param sharing the name must not flag them as
                // forward references.
                let saved_len = shadowed.len();
                if let Some(p) = init_pattern {
                    collect_default_pattern_forward_references(
                        p,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                    push_pattern_bindings(p, body, shadowed);
                }
                collect_default_expr_forward_references(*cond, body, later_params, shadowed, refs);
                collect_default_expr_forward_references_in_template_segments(
                    inner,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                shadowed.truncate(saved_len);
            }
            ast::TemplateSegment::If {
                branches,
                else_body,
            } => {
                for branch in branches {
                    collect_default_expr_forward_references(
                        branch.condition,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                    collect_default_expr_forward_references_in_template_segments(
                        &branch.body,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
                if let Some(eb) = else_body {
                    collect_default_expr_forward_references_in_template_segments(
                        eb,
                        body,
                        later_params,
                        shadowed,
                        refs,
                    );
                }
            }
        }
    }
}

fn push_pattern_bindings(pat_id: PatId, body: &ExprBody, shadowed: &mut Vec<Name>) {
    for name in body.patterns[pat_id].bound_names(&body.patterns) {
        shadowed.push(name.clone());
    }
}

fn collect_default_pattern_forward_references(
    pat_id: PatId,
    body: &ExprBody,
    later_params: &FxHashSet<Name>,
    shadowed: &mut Vec<Name>,
    refs: &mut Vec<Name>,
) {
    let mut operands = Vec::new();
    body.pattern_expr_children(pat_id, &mut operands);
    for operand in operands {
        let ast::traverse::BodyNode::Expr(operand) = operand else {
            unreachable!("patterns only contribute expression operands")
        };
        collect_default_expr_forward_references(operand, body, later_params, shadowed, refs);
    }
}

fn collect_default_stmt_forward_references(
    stmt_id: StmtId,
    body: &ExprBody,
    later_params: &FxHashSet<Name>,
    shadowed: &mut Vec<Name>,
    refs: &mut Vec<Name>,
) {
    match &body.stmts[stmt_id] {
        Stmt::Expr(expr)
        | Stmt::TypeBinding { value: expr, .. }
        | Stmt::Return(Some(expr))
        | Stmt::Throw { value: expr } => {
            collect_default_expr_forward_references(*expr, body, later_params, shadowed, refs);
        }
        Stmt::Let {
            pattern,
            initializer,
            else_branch,
            ..
        } => {
            if let Some(expr) = initializer {
                collect_default_expr_forward_references(*expr, body, later_params, shadowed, refs);
            }
            collect_default_pattern_forward_references(
                *pattern,
                body,
                later_params,
                shadowed,
                refs,
            );
            if let Some(else_expr) = else_branch {
                // The else branch runs before the pattern's bindings
                // exist, so it can't see them — recurse with the
                // pre-binding `shadowed` set, then re-truncate after.
                let saved_len = shadowed.len();
                collect_default_expr_forward_references(
                    *else_expr,
                    body,
                    later_params,
                    shadowed,
                    refs,
                );
                shadowed.truncate(saved_len);
            }
            push_pattern_bindings(*pattern, body, shadowed);
        }
        Stmt::While {
            condition,
            body: loop_body,
            after,
            ..
        } => {
            collect_default_expr_forward_references(*condition, body, later_params, shadowed, refs);
            let saved_len = shadowed.len();
            collect_default_expr_forward_references(*loop_body, body, later_params, shadowed, refs);
            if let Some(stmt) = after {
                collect_default_stmt_forward_references(*stmt, body, later_params, shadowed, refs);
            }
            shadowed.truncate(saved_len);
        }
        Stmt::WhileLet {
            pattern,
            scrutinee,
            body: loop_body,
        } => {
            // Scrutinee is evaluated outside the pattern's binding scope;
            // the pattern's names shadow within the body only — mirrors
            // `Stmt::For` (collection then pattern then body).
            collect_default_expr_forward_references(*scrutinee, body, later_params, shadowed, refs);
            let saved_len = shadowed.len();
            collect_default_pattern_forward_references(
                *pattern,
                body,
                later_params,
                shadowed,
                refs,
            );
            push_pattern_bindings(*pattern, body, shadowed);
            collect_default_expr_forward_references(*loop_body, body, later_params, shadowed, refs);
            shadowed.truncate(saved_len);
        }
        Stmt::For {
            binding,
            collection,
            body: loop_body,
            ..
        } => {
            collect_default_expr_forward_references(
                *collection,
                body,
                later_params,
                shadowed,
                refs,
            );
            let saved_len = shadowed.len();
            collect_default_pattern_forward_references(
                *binding,
                body,
                later_params,
                shadowed,
                refs,
            );
            push_pattern_bindings(*binding, body, shadowed);
            collect_default_expr_forward_references(*loop_body, body, later_params, shadowed, refs);
            shadowed.truncate(saved_len);
        }
        Stmt::Assign { target, value } | Stmt::AssignOp { target, value, .. } => {
            collect_default_expr_forward_references(*target, body, later_params, shadowed, refs);
            collect_default_expr_forward_references(*value, body, later_params, shadowed, refs);
        }
        Stmt::Defer { body: defer_body } => {
            collect_default_expr_forward_references(
                *defer_body,
                body,
                later_params,
                shadowed,
                refs,
            );
        }
        Stmt::Return(None)
        | Stmt::Break
        | Stmt::Continue
        | Stmt::Missing
        | Stmt::HeaderComment { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use baml_base::Literal;
    use baml_compiler2_ast::{CallArg, Pattern, TypeArg};

    use super::*;

    fn later_params() -> FxHashSet<Name> {
        [Name::new("later")].into_iter().collect()
    }

    #[test]
    fn hidden_runtime_operands_participate_in_default_forward_references() {
        let mut call_body = ExprBody::default();
        let callee = call_body
            .exprs
            .alloc(Expr::Path(vec![Name::new("identity")]));
        let operand = call_body.exprs.alloc(Expr::Path(vec![Name::new("later")]));
        let call = call_body.exprs.alloc(Expr::Call {
            callee,
            type_args: vec![TypeArg::Unreflect(operand)],
            args: Vec::<CallArg>::new(),
        });
        assert_eq!(
            default_expr_forward_references(call, &call_body, &later_params()),
            vec![Name::new("later")],
        );

        let mut binding_body = ExprBody::default();
        let value = binding_body
            .exprs
            .alloc(Expr::Path(vec![Name::new("later")]));
        let stmt = binding_body.stmts.alloc(Stmt::TypeBinding {
            name: Name::new("T"),
            value,
        });
        let block = binding_body.exprs.alloc(Expr::Block {
            stmts: vec![stmt],
            tail_expr: None,
        });
        assert_eq!(
            default_expr_forward_references(block, &binding_body, &later_params()),
            vec![Name::new("later")],
        );

        let mut pattern_body = ExprBody::default();
        let scrutinee = pattern_body.exprs.alloc(Expr::Literal(Literal::Int(1)));
        let operand = pattern_body
            .exprs
            .alloc(Expr::Path(vec![Name::new("later")]));
        let pattern = pattern_body.patterns.alloc(Pattern::Unreflect(operand));
        let test = pattern_body.exprs.alloc(Expr::Is { scrutinee, pattern });
        assert_eq!(
            default_expr_forward_references(test, &pattern_body, &later_params()),
            vec![Name::new("later")],
        );
    }
}
