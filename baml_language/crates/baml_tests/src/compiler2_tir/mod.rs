//! Snapshot tests for the type-provider surface (typed renders over
//! `baml_compiler2_hir_ty`'s tables; historically TIR's).
//!
//! Each test creates a minimal DB, adds a `.baml` file, runs type inference,
//! and snapshots the fully-typed output using the same format as the onion skin
//! tool's `run_tir2` renderer.

#[cfg(test)]
mod array_rest;
#[cfg(test)]
mod explicit_type_args;
#[cfg(test)]
mod inference;
#[cfg(test)]
// Re-enabled in Slice 3 after the enriched PackageInterface schema is native
// to hir_ty. The test source remains the contract for that slice.
#[cfg(any())]
mod package_interface;
#[cfg(test)]
mod phase3a;
mod phase3a_recursion;
#[cfg(test)]
mod phase5;
#[cfg(test)]
mod phase6;
#[cfg(test)]
mod phase7;
#[cfg(test)]
mod phase8_exceptions;
#[cfg(test)]
mod stream_expansion;

#[cfg(test)]
pub(crate) mod support {
    use std::fmt::Write;

    use baml_compiler2_ast::{
        CatchClauseKind, DefaultExprId, Expr, ExprBody, ExprId, FunctionDefaults, Literal, PatId,
        Stmt, StmtId,
    };
    use baml_compiler2_hir::{
        body::FunctionBody, contributions::Definition, item_tree::DefaultExprRef, scope::ScopeKind,
    };
    use baml_compiler2_hir_ty::infer::InferenceResult;
    use baml_db::ProjectDatabase;

    use crate::engine::TestDbExt;

    // ── Rendering helpers ────────────────────────────────────────────────────

    fn default_expr_suffix(default: Option<DefaultExprId>, defaults: &FunctionDefaults) -> String {
        default
            .map(|default| format!(" = {}", defaults.exprs.display_expr(default.expr())))
            .unwrap_or_default()
    }

    fn default_ref_suffix(default: Option<&DefaultExprRef>, defaults: &FunctionDefaults) -> String {
        default
            .map(|default| {
                let default_expr_id = default.expr.expr();
                format!(" = {}", defaults.exprs.display_expr(default_expr_id))
            })
            .unwrap_or_default()
    }

    fn pat_desc(pat_id: PatId, body: &ExprBody) -> String {
        use baml_compiler2_ast::Pattern;
        let pat = &body.patterns[pat_id];
        match pat {
            Pattern::Wildcard => "_".to_string(),
            Pattern::Bind { name, subpat } => match subpat {
                Some(sp) => format!("{name}: {}", pat_desc(*sp, body)),
                None => name.to_string(),
            },
            Pattern::Class {
                class,
                generic_args,
                fields,
                ..
            } => {
                let class_path = class
                    .iter()
                    .map(|n| n.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                let generic_args = if generic_args.is_empty() {
                    String::new()
                } else {
                    format!(
                        "<{}>",
                        generic_args
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                let fs = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.field, pat_desc(f.pat, body)))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{class_path}{generic_args} {{ {fs} }}")
            }
            Pattern::Array {
                prefix,
                rest,
                suffix,
                ascription,
            } => {
                let mut parts = Vec::new();
                parts.extend(prefix.iter().map(|p| pat_desc(*p, body)));
                if let Some(rest) = rest {
                    let rest_desc = rest
                        .pat
                        .map(|p| pat_desc(p, body))
                        .unwrap_or_else(String::new);
                    parts.push(format!("..{rest_desc}"));
                }
                parts.extend(suffix.iter().map(|p| pat_desc(*p, body)));
                let arr = format!("[{}]", parts.join(", "));
                match ascription {
                    Some(t) => format!("{arr}: {t}"),
                    None => arr,
                }
            }
            Pattern::Type(ty) => ty.to_string(),
            Pattern::Unreflect(expr) => format!("unreflect({})", expr_desc(*expr, body)),
            Pattern::Or(pats) => pats
                .iter()
                .map(|p| pat_desc(*p, body))
                .collect::<Vec<_>>()
                .join(" | "),
        }
    }

    fn expr_desc(expr_id: ExprId, body: &ExprBody) -> String {
        let expr = &body.exprs[expr_id];
        match expr {
            Expr::Literal(lit) => match lit {
                Literal::String(s) => {
                    let truncated: String = if s.chars().count() > 20 {
                        format!("{}...", s.chars().take(17).collect::<String>())
                    } else {
                        s.clone()
                    };
                    format!("{truncated:?}")
                }
                Literal::Int(i) => i.to_string(),
                Literal::Bigint(n) => format!("{n}n"),
                Literal::Float(f) => f.clone(),
                Literal::Bool(b) => b.to_string(),
            },
            Expr::Null => "null".into(),
            Expr::Path(segments) => segments
                .iter()
                .map(|n| n.as_str())
                .collect::<Vec<_>>()
                .join("."),
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond = expr_desc(*condition, body);
                let then_desc = expr_desc(*then_branch, body);
                match else_branch {
                    Some(eb) => format!("if ({cond}) {then_desc} else {}", expr_desc(*eb, body)),
                    None => format!("if ({cond}) {then_desc}"),
                }
            }
            Expr::IfLet {
                scrutinee,
                then_branch,
                else_branch,
                ..
            } => {
                let scrut = expr_desc(*scrutinee, body);
                let then_desc = expr_desc(*then_branch, body);
                match else_branch {
                    Some(eb) => format!(
                        "if let <pat> = {scrut} {then_desc} else {}",
                        expr_desc(*eb, body)
                    ),
                    None => format!("if let <pat> = {scrut} {then_desc}"),
                }
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                let scrut = expr_desc(*scrutinee, body);
                let arm_strs: Vec<String> = arms
                    .iter()
                    .map(|arm_id| {
                        let arm = &body.match_arms[*arm_id];
                        let pat = pat_desc(arm.pattern, body);
                        let body_desc = expr_desc(arm.body, body);
                        format!("{pat} => {body_desc}")
                    })
                    .collect();
                format!("match ({scrut}) {{ {} }}", arm_strs.join(", "))
            }
            Expr::Is { scrutinee, pattern } => {
                format!(
                    "{} is {}",
                    expr_desc(*scrutinee, body),
                    pat_desc(*pattern, body)
                )
            }
            Expr::Catch { base, clauses } => {
                let base_desc = expr_desc(*base, body);
                let clause_descs: Vec<String> = clauses
                    .iter()
                    .map(|clause| {
                        let kind = match clause.kind {
                            CatchClauseKind::Catch => "catch",
                            CatchClauseKind::CatchAll => "catch_all",
                            CatchClauseKind::CatchAllPanics => "catch_all_panics",
                        };
                        let binding = pat_desc(clause.binding, body);
                        let arms_desc = clause
                            .arms
                            .iter()
                            .map(|arm_id| {
                                let arm = &body.catch_arms[*arm_id];
                                format!(
                                    "{} => {}",
                                    pat_desc(arm.pattern, body),
                                    expr_desc(arm.body, body)
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("{kind} ({binding}) {{ {arms_desc} }}")
                    })
                    .collect();
                format!("{base_desc} {}", clause_descs.join(" "))
            }
            Expr::Throw { value } => format!("throw {}", expr_desc(*value, body)),
            Expr::Return { value } => match value {
                Some(value) => format!("return {}", expr_desc(*value, body)),
                None => "return".into(),
            },
            Expr::Binary { op, lhs, rhs } => {
                format!("{} {op} {}", expr_desc(*lhs, body), expr_desc(*rhs, body))
            }
            Expr::Unary { op, expr: inner } => format!("{op:?} {}", expr_desc(*inner, body)),
            Expr::Call {
                callee,
                type_args,
                args,
                ..
            } => {
                let callee_str = expr_desc(*callee, body);
                let ty_args_str = if type_args.is_empty() {
                    String::new()
                } else {
                    let tys: Vec<_> = type_args
                        .iter()
                        .map(|arg| match arg {
                            baml_compiler2_ast::TypeArg::Static(ty) => ty.to_string(),
                            baml_compiler2_ast::TypeArg::Unreflect(operand) => {
                                format!("unreflect({})", expr_desc(*operand, body))
                            }
                        })
                        .collect();
                    format!("<{}>", tys.join(", "))
                };
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|a| match &a.label {
                        Some(label) => format!("{label} = {}", expr_desc(a.expr, body)),
                        None => expr_desc(a.expr, body),
                    })
                    .collect();
                format!("{callee_str}{ty_args_str}({})", arg_strs.join(", "))
            }
            Expr::Object {
                type_name, fields, ..
            } => {
                let tn = type_name.to_string();
                let field_strs: Vec<String> = fields
                    .iter()
                    .map(|field| format!("{}: {}", field.name, expr_desc(field.value, body)))
                    .collect();
                format!("{tn} {{ {} }}", field_strs.join(", "))
            }
            Expr::Array { elements } => {
                let elem_strs: Vec<String> = elements.iter().map(|e| expr_desc(*e, body)).collect();
                format!("[{}]", elem_strs.join(", "))
            }
            Expr::Map { entries } => {
                let entry_strs: Vec<String> = entries
                    .iter()
                    .map(|entry| {
                        format!(
                            "{}: {}",
                            expr_desc(entry.key, body),
                            expr_desc(entry.value, body)
                        )
                    })
                    .collect();
                format!("map {{ {} }}", entry_strs.join(", "))
            }
            Expr::Block { stmts, tail_expr } => {
                let tail = if tail_expr.is_some() { " + tail" } else { "" };
                format!("{{ {} stmts{tail} }}", stmts.len())
            }
            Expr::MemberAccess { base, member } => {
                format!("{}.{member}", expr_desc(*base, body))
            }
            Expr::Upcast { base, target } => {
                format!("{}.as<{target}>", expr_desc(*base, body))
            }
            Expr::OptionalMemberAccess { base, member } => {
                format!("{}?.{member}", expr_desc(*base, body))
            }
            Expr::Index { base, index } => {
                format!("{}[{}]", expr_desc(*base, body), expr_desc(*index, body))
            }
            Expr::ByteStringLiteral(bytes) => format!("b\"<{} bytes>\"", bytes.len()),
            Expr::Lambda(func_def) => format_lambda_signature(func_def),
            Expr::OptionalIndex { base, index } => {
                format!("{}?.[{}]", expr_desc(*base, body), expr_desc(*index, body))
            }
            Expr::OptionalCall { callee, args } => {
                let callee_str = expr_desc(*callee, body);
                let args_str: Vec<String> = args
                    .iter()
                    .map(|a| match &a.label {
                        Some(label) => format!("{label} = {}", expr_desc(a.expr, body)),
                        None => expr_desc(a.expr, body),
                    })
                    .collect();
                format!("{}?.({})", callee_str, args_str.join(", "))
            }
            Expr::OptionalChain { expr } => expr_desc(*expr, body),
            Expr::Spawn {
                body: spawn_body, ..
            } => {
                format!("spawn {{ {} }}", expr_desc(*spawn_body, body))
            }
            Expr::Await { future } => format!("await {}", expr_desc(*future, body)),
            Expr::Template { tag, .. } => match tag {
                baml_compiler2_ast::TemplateTag::Custom { tag, .. } => {
                    format!("{}`...`", expr_desc(*tag, body))
                }
                baml_compiler2_ast::TemplateTag::Default { .. } => "`...`".into(),
            },
            Expr::GenericApply { base, .. } => format!("{}<...>", expr_desc(*base, body)),
            Expr::QualifiedPath {
                qself,
                interface,
                member,
            } => format!("({qself} as {interface}).{member}"),
            Expr::Missing => "<missing>".into(),
        }
    }

    fn format_lambda_signature(func_def: &baml_compiler2_ast::LambdaDef) -> String {
        let params: Vec<String> = func_def
            .params
            .iter()
            .map(|p| {
                let default_suffix = default_expr_suffix(p.default, &func_def.defaults);
                if let Some(ref te) = p.type_expr {
                    format!("{}: {}{}", p.name, te, default_suffix)
                } else {
                    format!("{}{}", p.name, default_suffix)
                }
            })
            .collect();
        let ret = func_def
            .return_type
            .as_ref()
            .map(|te| format!(" {}", te))
            .unwrap_or_default();
        let throws = func_def
            .throws
            .as_ref()
            .map(|te| format!(" throws {}", te))
            .unwrap_or_default();
        // A lambda never declares generics, so the signature has no `<…>`.
        format!("({}) ->{ret}{throws} {{ ... }}", params.join(", "))
    }

    /// HIR-aware version of `format_lambda_signature` that qualifies type names.
    fn format_lambda_signature_hir(
        func_def: &baml_compiler2_ast::LambdaDef,
        prefix: &str,
        local_type_names: &std::collections::HashSet<&str>,
    ) -> String {
        let qualify = |te: &baml_compiler2_ast::TypeExpr| -> String {
            let raw = te.to_string();
            if local_type_names.contains(raw.as_str()) {
                format!("{prefix}{raw}")
            } else {
                raw
            }
        };
        let params: Vec<String> = func_def
            .params
            .iter()
            .map(|p| {
                let default_suffix = default_expr_suffix(p.default, &func_def.defaults);
                if let Some(ref te) = p.type_expr {
                    format!("{}: {}{}", p.name, qualify(te), default_suffix)
                } else {
                    format!("{}{}", p.name, default_suffix)
                }
            })
            .collect();
        let ret = func_def
            .return_type
            .as_ref()
            .map(|te| format!(" {}", qualify(te)))
            .unwrap_or_default();
        let throws = func_def
            .throws
            .as_ref()
            .map(|te| format!(" throws {}", qualify(te)))
            .unwrap_or_default();
        // A lambda never declares generics, so the signature has no `<…>`.
        format!("({}) ->{ret}{throws} {{ ... }}", params.join(", "))
    }

    /// Like `expr_desc` but enriches Call expressions with type params from inference.
    fn expr_desc_rich(expr_id: ExprId, body: &ExprBody, inference: &InferenceResult) -> String {
        let expr = &body.exprs[expr_id];
        if let Expr::Call { callee, args, .. } = expr {
            let callee_str = expr_desc(*callee, body);
            let arg_strs: Vec<String> = args
                .iter()
                .map(|a| match &a.label {
                    Some(label) => format!("{label} = {}", expr_desc(a.expr, body)),
                    None => expr_desc(a.expr, body),
                })
                .collect();
            let type_params = if let Some(callee_ty) = inference.type_of_expr.get(callee) {
                collect_typevars(&callee_ty.to_plain())
            } else {
                Vec::new()
            };
            let tp_display = if type_params.is_empty() {
                String::new()
            } else {
                format!("<{}>", type_params.join(", "))
            };
            format!("{callee_str}{tp_display}({})", arg_strs.join(", "))
        } else {
            expr_desc(expr_id, body)
        }
    }

    /// Returns true if an expression is "compound" and should be rendered
    /// with recursive indented output rather than a single-line `expr_desc`.
    fn is_compound(expr: &Expr) -> bool {
        matches!(
            expr,
            Expr::Block { .. }
                | Expr::If { .. }
                | Expr::Match { .. }
                | Expr::Catch { .. }
                | Expr::Lambda(_)
        )
    }

    /// Format an expression's inferred type as a string.
    ///
    /// Uses `render_canonical()` (fully-qualified leaf names, including the
    /// implicit `user` package) so the TIR dump keeps `user.X` rather than the
    /// user-facing `Display`, which elides `user`.
    fn expr_ty(inference: &InferenceResult, expr_id: ExprId) -> String {
        inference
            .type_of_expr
            .get(&expr_id)
            .map(|t| t.to_plain().render_canonical())
            .unwrap_or_else(|| "unknown".into())
    }

    fn render_expr(
        expr_id: ExprId,
        body: &ExprBody,
        inference: &InferenceResult,
        indent: usize,
        output: &mut String,
    ) {
        let pad = " ".repeat(indent);
        let ty = expr_ty(inference, expr_id);
        let expr = &body.exprs[expr_id];

        match expr {
            Expr::Block { stmts, tail_expr } => {
                writeln!(output, "{pad}{{ : {ty}").ok();
                for stmt_id in stmts {
                    render_stmt(*stmt_id, body, inference, indent + 2, output);
                }
                if let Some(tail) = tail_expr {
                    render_expr(*tail, body, inference, indent + 2, output);
                }
                writeln!(output, "{pad}}}").ok();
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond_desc = expr_desc(*condition, body);
                let cond_ty = expr_ty(inference, *condition);
                writeln!(output, "{pad}if ({cond_desc} : {cond_ty}) : {ty}").ok();
                render_expr(*then_branch, body, inference, indent + 2, output);
                if let Some(else_expr) = else_branch {
                    writeln!(output, "{pad}else").ok();
                    render_expr(*else_expr, body, inference, indent + 2, output);
                }
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                let scrut_desc = expr_desc(*scrutinee, body);
                let scrut_ty = expr_ty(inference, *scrutinee);
                writeln!(output, "{pad}match ({scrut_desc} : {scrut_ty}) : {ty}").ok();
                for arm_id in arms {
                    let arm = &body.match_arms[*arm_id];
                    let pat = pat_desc(arm.pattern, body);
                    let guard = arm
                        .guard
                        .map(|g| format!(" if {}", expr_desc(g, body)))
                        .unwrap_or_default();
                    writeln!(output, "{pad}  {pat}{guard} =>").ok();
                    render_expr(arm.body, body, inference, indent + 4, output);
                }
            }
            Expr::Catch { base, clauses } => {
                let base_desc = expr_desc_rich(*base, body, inference);
                let base_ty = expr_ty(inference, *base);
                writeln!(output, "{pad}catch ({base_desc} : {base_ty}) : {ty}").ok();
                for clause in clauses {
                    let kind = match clause.kind {
                        CatchClauseKind::Catch => "catch",
                        CatchClauseKind::CatchAll => "catch_all",
                        CatchClauseKind::CatchAllPanics => "catch_all_panics",
                    };
                    let binding = pat_desc(clause.binding, body);
                    writeln!(output, "{pad}  {kind} ({binding})").ok();
                    for arm_id in &clause.arms {
                        let arm = &body.catch_arms[*arm_id];
                        let pat = pat_desc(arm.pattern, body);
                        writeln!(output, "{pad}    {pat} =>").ok();
                        render_expr(arm.body, body, inference, indent + 6, output);
                    }
                }
            }
            Expr::Lambda(func_def) => {
                let desc = expr_desc(expr_id, body);
                writeln!(output, "{pad}{desc} : {ty}").ok();
                // The body is an expression in this same arena.
                if let Some(root) = func_def.body {
                    render_expr_body_untyped(body, root, indent + 2, output);
                }
            }
            Expr::Call { callee, args, .. } => {
                // Show type params at call site when callee has TypeVars
                let callee_desc = expr_desc(*callee, body);
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|a| match &a.label {
                        Some(label) => format!("{label} = {}", expr_desc(a.expr, body)),
                        None => expr_desc(a.expr, body),
                    })
                    .collect();
                let type_params = if let Some(callee_ty) = inference.type_of_expr.get(callee) {
                    collect_typevars(&callee_ty.to_plain())
                } else {
                    Vec::new()
                };
                let tp_display = if type_params.is_empty() {
                    String::new()
                } else {
                    format!("<{}>", type_params.join(", "))
                };
                writeln!(
                    output,
                    "{pad}{callee_desc}{tp_display}({}) : {ty}",
                    arg_strs.join(", ")
                )
                .ok();
                // Expand compound arguments (e.g. lambdas) below the call
                for arg in args {
                    if is_compound(&body.exprs[arg.expr]) {
                        render_expr(arg.expr, body, inference, indent + 2, output);
                    }
                }
            }
            _ => {
                let desc = expr_desc(expr_id, body);
                writeln!(output, "{pad}{desc} : {ty}").ok();
            }
        }
    }

    /// Render a lambda's body without type information.
    ///
    /// The body shares the enclosing function's arena, but its types live in
    /// the lambda's own tables, which this renderer does not hold.
    fn render_expr_body_untyped(
        body: &ExprBody,
        expr_id: ExprId,
        indent: usize,
        output: &mut String,
    ) {
        use std::fmt::Write;
        let pad = " ".repeat(indent);
        let expr = &body.exprs[expr_id];

        match expr {
            Expr::Block { stmts, tail_expr } => {
                writeln!(output, "{pad}{{").ok();
                for stmt_id in stmts {
                    render_stmt_untyped(*stmt_id, body, indent + 2, output);
                }
                if let Some(tail) = tail_expr {
                    render_expr_body_untyped(body, *tail, indent + 2, output);
                }
                writeln!(output, "{pad}}}").ok();
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond_desc = expr_desc(*condition, body);
                writeln!(output, "{pad}if ({cond_desc})").ok();
                render_expr_body_untyped(body, *then_branch, indent + 2, output);
                if let Some(else_expr) = else_branch {
                    writeln!(output, "{pad}else").ok();
                    render_expr_body_untyped(body, *else_expr, indent + 2, output);
                }
            }
            Expr::Lambda(func_def) => {
                let desc = expr_desc(expr_id, body);
                writeln!(output, "{pad}{desc}").ok();
                if let Some(root) = func_def.body {
                    render_expr_body_untyped(body, root, indent + 2, output);
                }
            }
            _ => {
                let desc = expr_desc(expr_id, body);
                writeln!(output, "{pad}{desc}").ok();
            }
        }
    }

    fn render_stmt_untyped(
        stmt_id: baml_compiler2_ast::StmtId,
        body: &ExprBody,
        indent: usize,
        output: &mut String,
    ) {
        use std::fmt::Write;

        use baml_compiler2_ast::Stmt;
        let pad = " ".repeat(indent);
        let stmt = &body.stmts[stmt_id];
        match stmt {
            Stmt::TypeBinding { name, value } => {
                let operand = expr_desc(*value, body);
                writeln!(output, "{pad}type {name} = unreflect({operand})").ok();
            }
            Stmt::Let {
                pattern,
                initializer,
                ..
            } => {
                let pat = pat_desc(*pattern, body);
                let init = initializer
                    .map(|e| {
                        let desc = expr_desc(e, body);
                        if is_compound(&body.exprs[e]) {
                            " = ...".to_string()
                        } else {
                            format!(" = {desc}")
                        }
                    })
                    .unwrap_or_default();
                writeln!(output, "{pad}let {pat}{init}").ok();
                if let Some(e) = *initializer
                    && is_compound(&body.exprs[e])
                {
                    render_expr_body_untyped(body, e, indent + 2, output);
                }
            }
            Stmt::Expr(expr_id) => {
                render_expr_body_untyped(body, *expr_id, indent, output);
            }
            Stmt::For {
                binding,
                collection,
                body: for_body,
            } => {
                let pat = pat_desc(*binding, body);
                let iter_desc = expr_desc(*collection, body);
                writeln!(output, "{pad}for {pat} in {iter_desc}").ok();
                render_expr_body_untyped(body, *for_body, indent + 2, output);
            }
            Stmt::While {
                condition,
                body: while_body,
                ..
            } => {
                let cond = expr_desc(*condition, body);
                writeln!(output, "{pad}while ({cond})").ok();
                render_expr_body_untyped(body, *while_body, indent + 2, output);
            }
            Stmt::WhileLet {
                pattern,
                scrutinee,
                body: while_body,
            } => {
                let pat = pat_desc(*pattern, body);
                let scrut = expr_desc(*scrutinee, body);
                writeln!(output, "{pad}while let {pat} = {scrut}").ok();
                render_expr_body_untyped(body, *while_body, indent + 2, output);
            }
            Stmt::Return(Some(expr_id)) => {
                let desc = expr_desc(*expr_id, body);
                writeln!(output, "{pad}return {desc}").ok();
            }
            Stmt::Return(None) => {
                writeln!(output, "{pad}return").ok();
            }
            Stmt::Throw { value } => {
                let desc = expr_desc(*value, body);
                writeln!(output, "{pad}throw {desc}").ok();
            }
            Stmt::Assign { target, value } => {
                let t = expr_desc(*target, body);
                let v = expr_desc(*value, body);
                writeln!(output, "{pad}{t} = {v}").ok();
            }
            Stmt::AssignOp { target, op, value } => {
                let t = expr_desc(*target, body);
                let v = expr_desc(*value, body);
                writeln!(output, "{pad}{t} {op:?}= {v}").ok();
            }
            Stmt::Defer { body: defer_body } => {
                writeln!(output, "{pad}defer").ok();
                render_expr_body_untyped(body, *defer_body, indent + 2, output);
            }
            Stmt::Break => {
                writeln!(output, "{pad}break").ok();
            }
            Stmt::Continue => {
                writeln!(output, "{pad}continue").ok();
            }
            Stmt::Missing | Stmt::HeaderComment { .. } => {}
        }
    }

    /// Collect unique TypeVar names from a Ty (in order of appearance).
    fn collect_typevars(ty: &baml_type::Ty) -> Vec<String> {
        let mut result = Vec::new();
        collect_typevars_inner(ty, &mut result);
        result
    }

    fn collect_typevars_inner(ty: &baml_type::Ty, out: &mut Vec<String>) {
        use baml_type::Ty;
        match ty {
            Ty::TypeVar(name, _) => {
                let s = name.to_string();
                if !out.contains(&s) {
                    out.push(s);
                }
            }
            Ty::List(inner, _) => collect_typevars_inner(inner, out),
            Ty::Map {
                key: k, value: v, ..
            } => {
                collect_typevars_inner(k, out);
                collect_typevars_inner(v, out);
            }
            Ty::Union(members, _) => {
                for m in members {
                    collect_typevars_inner(m, out);
                }
            }
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => {
                for param in params {
                    collect_typevars_inner(&param.ty, out);
                }
                collect_typevars_inner(ret, out);
                collect_typevars_inner(throws, out);
            }
            _ => {}
        }
    }

    fn render_stmt(
        stmt_id: StmtId,
        body: &ExprBody,
        inference: &InferenceResult,
        indent: usize,
        output: &mut String,
    ) {
        let pad = " ".repeat(indent);
        let stmt = &body.stmts[stmt_id];
        match stmt {
            Stmt::TypeBinding { name, value } => {
                let operand = expr_desc_rich(*value, body, inference);
                let operand_ty = expr_ty(inference, *value);
                writeln!(
                    output,
                    "{pad}type {name} = unreflect({operand}) : {operand_ty}"
                )
                .ok();
            }
            Stmt::Let {
                pattern,
                initializer,
                ..
            } => {
                let pat_name = pat_desc(*pattern, body);
                if let Some(init) = initializer {
                    let init_ty = expr_ty(inference, *init);
                    let binding_ty = inference
                        .type_of_pat
                        .get(pattern)
                        .map(|t| t.to_plain().render_canonical());
                    let ty_display = match &binding_ty {
                        Some(bt) if *bt != init_ty => format!("{init_ty} -> {bt}"),
                        _ => init_ty,
                    };
                    if is_compound(&body.exprs[*init]) {
                        writeln!(output, "{pad}let {pat_name} = : {ty_display}").ok();
                        render_expr(*init, body, inference, indent + 2, output);
                    } else {
                        let init_desc = expr_desc_rich(*init, body, inference);
                        writeln!(output, "{pad}let {pat_name} = {init_desc} : {ty_display}").ok();
                    }
                } else {
                    writeln!(output, "{pad}let {pat_name}").ok();
                }
            }
            Stmt::Return(Some(expr_id)) => {
                let ty = expr_ty(inference, *expr_id);
                if is_compound(&body.exprs[*expr_id]) {
                    writeln!(output, "{pad}return : {ty}").ok();
                    render_expr(*expr_id, body, inference, indent + 2, output);
                } else {
                    let desc = expr_desc_rich(*expr_id, body, inference);
                    writeln!(output, "{pad}return {desc} : {ty}").ok();
                }
            }
            Stmt::Return(None) => {
                writeln!(output, "{pad}return").ok();
            }
            Stmt::Throw { value } => {
                let ty = expr_ty(inference, *value);
                if is_compound(&body.exprs[*value]) {
                    writeln!(output, "{pad}throw : {ty}").ok();
                    render_expr(*value, body, inference, indent + 2, output);
                } else {
                    let desc = expr_desc_rich(*value, body, inference);
                    writeln!(output, "{pad}throw {desc} : {ty}").ok();
                }
            }
            Stmt::Expr(expr_id) => {
                render_expr(*expr_id, body, inference, indent, output);
            }
            Stmt::While {
                condition,
                body: body_expr,
                ..
            } => {
                let cond_desc = expr_desc(*condition, body);
                writeln!(output, "{pad}while {cond_desc}").ok();
                render_expr(*body_expr, body, inference, indent + 2, output);
            }
            Stmt::WhileLet {
                pattern,
                scrutinee,
                body: body_expr,
            } => {
                let pat = pat_desc(*pattern, body);
                let scrut_desc = expr_desc(*scrutinee, body);
                writeln!(output, "{pad}while let {pat} = {scrut_desc}").ok();
                render_expr(*body_expr, body, inference, indent + 2, output);
            }
            Stmt::For {
                binding,
                collection,
                body: for_body,
            } => {
                let bind_name = pat_desc(*binding, body);
                let coll_desc = expr_desc(*collection, body);
                writeln!(output, "{pad}for {bind_name} in {coll_desc}").ok();
                render_expr(*for_body, body, inference, indent + 2, output);
            }
            Stmt::Assign { target, value } => {
                let target_desc = expr_desc(*target, body);
                let val_desc = expr_desc(*value, body);
                let val_ty = expr_ty(inference, *value);
                writeln!(output, "{pad}{target_desc} = {val_desc} : {val_ty}").ok();
            }
            Stmt::AssignOp { target, op, value } => {
                let target_desc = expr_desc(*target, body);
                let val_desc = expr_desc(*value, body);
                let val_ty = expr_ty(inference, *value);
                writeln!(output, "{pad}{target_desc} {op:?}= {val_desc} : {val_ty}").ok();
            }
            Stmt::Defer { body: defer_body } => {
                writeln!(output, "{pad}defer").ok();
                render_expr(*defer_body, body, inference, indent + 2, output);
            }
            Stmt::Break => {
                writeln!(output, "{pad}break").ok();
            }
            Stmt::Continue => {
                writeln!(output, "{pad}continue").ok();
            }
            Stmt::HeaderComment { name, level } => {
                writeln!(output, "{pad}// [{level}] {name}").ok();
            }
            Stmt::Missing => {
                writeln!(output, "{pad}<missing stmt>").ok();
            }
        }
    }

    fn qualified_name(scopes: &[baml_compiler2_hir::scope::Scope], scope_idx: usize) -> String {
        let mut parts = Vec::new();
        let mut cur = scope_idx;
        loop {
            let s = &scopes[cur];
            match s.kind {
                ScopeKind::Project => break,
                ScopeKind::File => {}
                _ => {
                    if let Some(ref name) = s.name {
                        parts.push(name.to_string());
                    }
                }
            }
            if let Some(parent) = s.parent {
                cur = parent.index() as usize;
            } else {
                break;
            }
        }
        parts.reverse();
        parts.join(".")
    }

    /// Render a file's TIR output in the same format as the onion skin tool.
    /// Uses the PPIR semantic index which includes synthetic stream_* types.
    pub fn render_tir(db: &ProjectDatabase, file: baml_base::SourceFile) -> String {
        use baml_compiler2_hir::package::PackageId;

        let mut output = String::new();
        let index = baml_compiler2_ppir::file_semantic_index(db, file);

        // Get package items for resolving TypeExpr -> Ty in signatures
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let pkg_items = baml_compiler2_ppir::package_items(db, pkg_id);

        // Pre-compute throw sets for the package
        let throw_sets = baml_compiler2_hir_ty::package_interface::function_throw_sets(db, pkg_id);

        // Pre-compute invalid alias/class cycles for the package (the
        // declaration cycle analysis over hir-lowered values).
        let mut aliases = std::collections::HashMap::new();
        let mut class_fields = std::collections::HashMap::new();
        for ns in pkg_items.namespaces.values() {
            for (name, def) in &ns.types {
                match def {
                    Definition::TypeAlias(loc) => {
                        aliases.insert(
                            baml_compiler2_hir_ty::lower::qualify_def(db, *def, name),
                            baml_compiler2_hir_ty::lower::type_alias_value(db, *loc).to_plain(),
                        );
                    }
                    Definition::Class(loc) => {
                        let fields = baml_compiler2_hir_ty::lower::resolve_class_fields(db, *loc)
                            .iter()
                            .map(|(n, ty, _)| (n.clone(), ty.clone()))
                            .collect::<Vec<_>>();
                        class_fields.insert(
                            baml_compiler2_hir_ty::lower::qualify_def(db, *def, name),
                            fields,
                        );
                    }
                    _ => {}
                }
            }
        }
        let invalid_cycles = baml_type::decl_cycles::find_invalid_alias_cycles(&aliases);
        let class_cycles_info =
            baml_type::decl_cycles::find_invalid_class_cycles(&class_fields, &aliases);
        let mut class_cycle_map = std::collections::HashMap::new();
        for cycle in &class_cycles_info {
            for member in &cycle.members {
                class_cycle_map.insert(member.clone(), cycle.cycle_path.clone());
            }
        }
        for (i, scope) in index.scopes.iter().enumerate() {
            let scope_id = index.scope_ids[i];
            let kind_str = match &scope.kind {
                ScopeKind::Function => "function",
                ScopeKind::Lambda => "lambda",
                ScopeKind::Block => "block",
                ScopeKind::Class => "class",
                ScopeKind::Enum => "enum",
                ScopeKind::TypeAlias => "type",
                _ => continue,
            };
            let fqn = qualified_name(&index.scopes, i);

            // ── Structural scopes (class/enum/type alias) ───────────
            if matches!(
                scope.kind,
                ScopeKind::Class | ScopeKind::Enum | ScopeKind::TypeAlias
            ) {
                let contrib = &index.symbol_contributions;
                match &scope.kind {
                    ScopeKind::Class => {
                        for (name, c) in &contrib.types {
                            if scope.name.as_ref() == Some(name)
                                && let Definition::Class(class_loc) = c.definition
                            {
                                let resolved = baml_compiler2_hir_ty::lower::resolve_class_fields(
                                    db, class_loc,
                                );
                                writeln!(output, "{kind_str} {fqn} {{").ok();
                                for (fname, fty, fattrs) in resolved {
                                    let ty_attr_names = fty.attr().attr_names();
                                    let field_attr_strs: Vec<String> = fattrs
                                        .iter()
                                        .map(|a| {
                                            if a.args.is_empty() {
                                                format!("@{}", a.name)
                                            } else {
                                                let args_str = a
                                                    .args
                                                    .iter()
                                                    .map(|arg| match &arg.key {
                                                        Some(k) => format!("{}={}", k, arg.value),
                                                        None => arg.value.clone(),
                                                    })
                                                    .collect::<Vec<_>>()
                                                    .join(", ");
                                                format!("@{}({})", a.name, args_str)
                                            }
                                        })
                                        .collect();
                                    // Format: field: (Ty @ty_attr) @field_attr
                                    let ty_str = if ty_attr_names.is_empty() {
                                        fty.render_canonical()
                                    } else {
                                        let ta = ty_attr_names
                                            .iter()
                                            .map(|a| format!("@{a}"))
                                            .collect::<Vec<_>>()
                                            .join(" ");
                                        format!("({} {ta})", fty.render_canonical())
                                    };
                                    if field_attr_strs.is_empty() {
                                        writeln!(output, "  {fname}: {ty_str}").ok();
                                    } else {
                                        let fa = field_attr_strs.join(" ");
                                        writeln!(output, "  {fname}: {ty_str} {fa}").ok();
                                    }
                                }
                                writeln!(output, "}}").ok();
                                // Render class cycle diagnostic if applicable
                                let qn = baml_type::QualifiedTypeName::new(
                                    pkg_info.package.clone(),
                                    pkg_info.namespace_path.clone(),
                                    name.clone(),
                                );
                                if let Some(cycle_path) = class_cycle_map.get(&qn) {
                                    let start = u32::from(scope.range.start());
                                    let end = u32::from(scope.range.end());
                                    writeln!(
                                        output,
                                        "  !! {start}..{end}: class cycle: {cycle_path}"
                                    )
                                    .ok();
                                }
                                break;
                            }
                        }
                    }
                    ScopeKind::TypeAlias => {
                        for (name, c) in &contrib.types {
                            if scope.name.as_ref() == Some(name)
                                && let Definition::TypeAlias(alias_loc) = c.definition
                            {
                                let resolved =
                                    baml_compiler2_hir_ty::lower::type_alias_value(db, alias_loc)
                                        .to_plain();
                                writeln!(
                                    output,
                                    "{kind_str} {fqn} = {}",
                                    resolved.render_canonical()
                                )
                                .ok();
                                // Render type-lowering diagnostics
                                for (span, diag) in
                                    baml_compiler2_hir_ty::lower::type_alias_lowering_diagnostics(
                                        db, alias_loc,
                                    )
                                {
                                    let start = u32::from(span.start());
                                    let end = u32::from(span.end());
                                    writeln!(output, "  !! {start}..{end}: {diag}").ok();
                                }
                                // Render cycle diagnostic if this alias is in an invalid cycle
                                let qn = baml_type::QualifiedTypeName::new(
                                    pkg_info.package.clone(),
                                    pkg_info.namespace_path.clone(),
                                    name.clone(),
                                );
                                if invalid_cycles.contains(&qn) {
                                    let start = u32::from(scope.range.start());
                                    let end = u32::from(scope.range.end());
                                    writeln!(
                                        output,
                                        "  !! {start}..{end}: recursive type alias cycle: {name}"
                                    )
                                    .ok();
                                }
                                break;
                            }
                        }
                    }
                    ScopeKind::Enum => {
                        writeln!(output, "{kind_str} {fqn}").ok();
                    }
                    _ => {}
                }
                continue;
            }

            // ── Function/Lambda/Block scopes ────────────────────────
            let Some(inference) = baml_compiler2_hir_ty::ide::infer_for_scope(db, scope_id) else {
                continue;
            };

            let mut func_body_opt: Option<std::sync::Arc<FunctionBody>> = None;
            let mut sig_display = String::new();
            if matches!(scope.kind, ScopeKind::Function) {
                // The authoritative scope→item link (replaces the fragile
                // `func.span == scope.range` join, which collided on companion spans).
                if let Some(baml_compiler2_ppir::item_data::ScopeOwner::Function(func_loc)) =
                    baml_compiler2_ppir::item_data::scope_owner(db, scope_id)
                {
                    let func_data = baml_compiler2_ppir::item_data::function_data(db, func_loc);
                    func_body_opt = Some(baml_compiler2_ppir::function_body(db, func_loc));
                    let sig = baml_compiler2_hir_ty::lower::function_signature(db, func_loc);

                    let gp = &func_data.generic_params;
                    let generics_display = if gp.is_empty() {
                        String::new()
                    } else {
                        let names: Vec<String> =
                            gp.iter().map(|param| param.name.to_string()).collect();
                        format!("<{}>", names.join(", "))
                    };

                    let parameter_defaults =
                        baml_compiler2_ppir::function_parameter_defaults(db, func_loc);
                    let params: Vec<String> = sig
                        .params
                        .iter()
                        .enumerate()
                        .map(|(index, param)| {
                            let default_suffix = default_ref_suffix(
                                parameter_defaults.param_default(index),
                                &parameter_defaults.defaults,
                            );
                            format!(
                                "{}: {}{}",
                                param.name,
                                param.ty.to_plain().render_canonical(),
                                default_suffix
                            )
                        })
                        .collect();
                    let ret = sig.ret.to_plain().render_canonical();
                    // Inferred throws from the package transitive throw set.
                    let inferred_throws: Option<String> = {
                        let key = baml_base::Name::new(&*fqn);
                        throw_sets
                            .transitive_for(&key)
                            .filter(|facts| !facts.is_empty())
                            .map(|facts| {
                                let types: Vec<String> =
                                    facts.iter().map(|f| f.render_canonical()).collect();
                                types.join(" | ")
                            })
                    };
                    let throws = if sig.throws_declared {
                        let declared = sig.throws.to_plain().render_canonical();
                        match &inferred_throws {
                            Some(inferred) => {
                                format!(" throws {declared} infers {inferred}")
                            }
                            None => format!(" throws {declared}"),
                        }
                    } else {
                        match &inferred_throws {
                            Some(inferred) => format!(" throws {inferred}"),
                            None => " throws never".to_string(),
                        }
                    };
                    sig_display =
                        format!("{generics_display}({}) -> {ret}{throws}", params.join(", "));
                }
            }

            // Collect expression types for this scope — skip if none
            if inference.type_of_expr.is_empty() {
                continue;
            }

            writeln!(output, "{kind_str} {fqn}{sig_display} {{").ok();

            let expr_body = func_body_opt.as_ref().and_then(|fb| {
                if let FunctionBody::Expr(body) = fb.as_ref() {
                    Some(body)
                } else {
                    None
                }
            });

            if let Some(body) = expr_body
                && let Some(root) = body.root_expr
            {
                render_expr(root, body, inference, 2, &mut output);
            }

            // Owner diagnostics (rendered once, on the owner's own scope):
            // the body run's, the PARAMETER-DEFAULTS run's, the structural
            // default rules, and the signature walk - the same union the
            // check layer assembles.
            if matches!(scope.kind, ScopeKind::Function)
                && let Some(owner) = baml_compiler2_hir_ty::ide::owner_for_scope(db, scope_id)
            {
                let source_map = baml_compiler2_ppir::body_source_map(db, owner);
                let type_ref_spans = baml_compiler2_ppir::body_type_ref_spans(db, owner);
                let mut rendered = Vec::new();
                for diagnostic in &inference.diagnostics {
                    rendered.push(diagnostic.render_with_body_type_refs(
                        db,
                        file,
                        source_map.as_ref(),
                        type_ref_spans.as_ref(),
                    ));
                }
                if let baml_compiler2_hir::body::BodyOwnerId::Function(func_loc) = owner {
                    let defaults_owner =
                        baml_compiler2_hir::body::BodyOwnerId::ParameterDefaults(func_loc);
                    let defaults = baml_compiler2_hir_ty::infer::infer_body(db, defaults_owner);
                    let defaults_map = baml_compiler2_ppir::body_source_map(db, defaults_owner);
                    let defaults_spans =
                        baml_compiler2_ppir::body_type_ref_spans(db, defaults_owner);
                    for diagnostic in &defaults.diagnostics {
                        rendered.push(diagnostic.render_with_body_type_refs(
                            db,
                            file,
                            defaults_map.as_ref(),
                            defaults_spans.as_ref(),
                        ));
                    }
                    for (range, error) in
                        baml_compiler2_hir_ty::defaults::parameter_default_diagnostics(db, func_loc)
                            .into_iter()
                            .chain(
                                baml_compiler2_hir_ty::lower::signature_lowering_diagnostics(
                                    db, func_loc,
                                ),
                            )
                    {
                        rendered.push(baml_compiler2_hir_ty::diagnostics::RenderedTirDiagnostic {
                            message: error.to_string(),
                            error,
                            range,
                            severity: baml_compiler2_hir_ty::diagnostics::DiagnosticSeverity::Error,
                            related: Vec::new(),
                        });
                    }
                }
                rendered.sort_by_key(|rd| (rd.range.start(), rd.range.end()));
                for rd in &rendered {
                    let marker = match rd.severity {
                        baml_compiler2_hir_ty::diagnostics::DiagnosticSeverity::Error => "!!",
                        baml_compiler2_hir_ty::diagnostics::DiagnosticSeverity::Warning => "??",
                    };
                    writeln!(output, "  {marker} {rd}").ok();
                }
            }

            writeln!(output, "}}").ok();
        }

        output
    }

    /// Render a file's PPIR (canonical, post-expansion item tree) as readable
    /// text — includes the synthesized `*$stream` companions.
    pub fn render_ppir(db: &ProjectDatabase, file: baml_base::SourceFile) -> String {
        use baml_compiler2_ast::{CatchClauseKind, Expr, ExprBody, Literal};
        use baml_compiler2_hir::{file_package::file_package, file_semantic_index};

        fn qualify_type_name(
            path: &baml_base::TypePath,
            pkg_prefix: &str,
            local_type_names: &std::collections::HashSet<&str>,
        ) -> String {
            if path.is_qualified() {
                path.to_string()
            } else {
                let leaf = path.leaf().as_str();
                if local_type_names.contains(leaf) {
                    format!("{pkg_prefix}{leaf}")
                } else {
                    leaf.into()
                }
            }
        }

        fn type_expr_to_string_hir(
            ty: &baml_compiler2_ast::TypeExpr,
            pkg_prefix: &str,
            local_type_names: &std::collections::HashSet<&str>,
        ) -> String {
            fn is_local_type_path(
                first: &str,
                local_type_names: &std::collections::HashSet<&str>,
            ) -> bool {
                local_type_names.contains(first)
                    || first
                        .strip_suffix("$stream")
                        .is_some_and(|base| local_type_names.contains(base))
            }

            match &ty.kind {
                baml_compiler2_ast::TypeExprKind::Path {
                    segments,
                    generic_args,
                    associated_type_bindings,
                    ..
                } => {
                    let path = segments
                        .iter()
                        .map(|n| n.as_str())
                        .collect::<Vec<_>>()
                        .join(".");
                    let first = segments.first().map(|n| n.as_str()).unwrap_or("");
                    let mut rendered = if is_local_type_path(first, local_type_names) {
                        format!("{pkg_prefix}{path}")
                    } else {
                        path
                    };
                    if !generic_args.is_empty() || !associated_type_bindings.is_empty() {
                        let mut args = generic_args
                            .iter()
                            .map(|arg| type_expr_to_string_hir(arg, pkg_prefix, local_type_names))
                            .collect::<Vec<_>>();
                        args.extend(associated_type_bindings.iter().map(|binding| {
                            format!(
                                "{} = {}",
                                binding.name,
                                type_expr_to_string_hir(&binding.ty, pkg_prefix, local_type_names)
                            )
                        }));
                        rendered.push('<');
                        rendered.push_str(&args.join(", "));
                        rendered.push('>');
                    }
                    rendered
                }
                baml_compiler2_ast::TypeExprKind::Int { .. } => "int".into(),
                baml_compiler2_ast::TypeExprKind::Bigint { .. } => "bigint".into(),
                baml_compiler2_ast::TypeExprKind::Float { .. } => "float".into(),
                baml_compiler2_ast::TypeExprKind::String { .. } => "string".into(),
                baml_compiler2_ast::TypeExprKind::Bool { .. } => "bool".into(),
                baml_compiler2_ast::TypeExprKind::Null { .. } => "null".into(),
                baml_compiler2_ast::TypeExprKind::Never { .. } => "never".into(),
                baml_compiler2_ast::TypeExprKind::Void { .. } => "void".into(),
                baml_compiler2_ast::TypeExprKind::Uint8Array { .. } => "uint8array".into(),
                baml_compiler2_ast::TypeExprKind::Media { kind: k, .. } => {
                    format!("{:?}", k).to_lowercase()
                }
                baml_compiler2_ast::TypeExprKind::Optional { inner, .. } => {
                    format!(
                        "{}?",
                        type_expr_to_string_hir(inner, pkg_prefix, local_type_names)
                    )
                }
                baml_compiler2_ast::TypeExprKind::List { inner, .. } => {
                    format!(
                        "{}[]",
                        type_expr_to_string_hir(inner, pkg_prefix, local_type_names)
                    )
                }
                baml_compiler2_ast::TypeExprKind::Map { key, value, .. } => format!(
                    "map<{}, {}>",
                    type_expr_to_string_hir(key, pkg_prefix, local_type_names),
                    type_expr_to_string_hir(value, pkg_prefix, local_type_names)
                ),
                baml_compiler2_ast::TypeExprKind::Union {
                    variants: members, ..
                } => members
                    .iter()
                    .map(|m| type_expr_to_string_hir(m, pkg_prefix, local_type_names))
                    .collect::<Vec<_>>()
                    .join(" | "),
                baml_compiler2_ast::TypeExprKind::Literal { value: lit, .. } => lit.to_string(),
                baml_compiler2_ast::TypeExprKind::Function {
                    params,
                    ret,
                    throws,
                    ..
                } => {
                    let ps: Vec<String> = params
                        .iter()
                        .map(|p| {
                            p.name
                                .as_ref()
                                .map(|n| {
                                    let optional_marker = if p.optional { "?" } else { "" };
                                    format!(
                                        "{}{}: {}",
                                        n.as_str(),
                                        optional_marker,
                                        type_expr_to_string_hir(
                                            &p.ty,
                                            pkg_prefix,
                                            local_type_names
                                        )
                                    )
                                })
                                .unwrap_or_else(|| {
                                    type_expr_to_string_hir(&p.ty, pkg_prefix, local_type_names)
                                })
                        })
                        .collect();
                    let throws = throws
                        .as_deref()
                        .map(|throws| type_expr_to_string_hir(throws, pkg_prefix, local_type_names))
                        .map(|throws| format!(" throws {throws}"))
                        .unwrap_or_default();
                    format!(
                        "({}) -> {}{}",
                        ps.join(", "),
                        type_expr_to_string_hir(ret, pkg_prefix, local_type_names),
                        throws
                    )
                }
                baml_compiler2_ast::TypeExprKind::BuiltinUnknown { .. } => "unknown".into(),
                baml_compiler2_ast::TypeExprKind::AssociatedTypeProjection {
                    base,
                    interface,
                    member,
                    ..
                } => {
                    let base = type_expr_to_string_hir(base, pkg_prefix, local_type_names);
                    if let Some(interface) = interface {
                        let interface =
                            type_expr_to_string_hir(interface, pkg_prefix, local_type_names);
                        format!("({base} as {interface}).{member}")
                    } else {
                        format!("{base}.{member}")
                    }
                }
                baml_compiler2_ast::TypeExprKind::Type { .. } => "reflect.Type".into(),
                baml_compiler2_ast::TypeExprKind::Rust { .. } => "$rust_type".into(),
                baml_compiler2_ast::TypeExprKind::Error { .. } => "error".into(),
                baml_compiler2_ast::TypeExprKind::Unknown { .. } => "?".into(),
                baml_compiler2_ast::TypeExprKind::Infer { .. } => "_".into(),
            }
        }

        /// The firewall-`TypeRef` twin of [`type_expr_to_string_hir`]: renders one
        /// type reference from an item's `type_refs` arena, byte-identical to the
        /// `ast::TypeExpr` renderer above.
        fn type_ref_to_string(
            store: &baml_compiler2_hir::type_ref::TypeRefStore,
            id: baml_compiler2_hir::type_ref::TypeRefId,
            pkg_prefix: &str,
            local_type_names: &std::collections::HashSet<&str>,
        ) -> String {
            use baml_compiler2_hir::type_ref::TypeRefKind as K;
            fn is_local_type_path(
                first: &str,
                local_type_names: &std::collections::HashSet<&str>,
            ) -> bool {
                local_type_names.contains(first)
                    || first
                        .strip_suffix("$stream")
                        .is_some_and(|base| local_type_names.contains(base))
            }

            match &store[id].kind {
                K::Path {
                    segments,
                    generic_args,
                    associated_type_bindings,
                } => {
                    let path = segments
                        .iter()
                        .map(|n| n.as_str())
                        .collect::<Vec<_>>()
                        .join(".");
                    let first = segments.first().map(|n| n.as_str()).unwrap_or("");
                    let mut rendered = if is_local_type_path(first, local_type_names) {
                        format!("{pkg_prefix}{path}")
                    } else {
                        path
                    };
                    if !generic_args.is_empty() || !associated_type_bindings.is_empty() {
                        let mut args = generic_args
                            .iter()
                            .map(|&arg| {
                                type_ref_to_string(store, arg, pkg_prefix, local_type_names)
                            })
                            .collect::<Vec<_>>();
                        args.extend(associated_type_bindings.iter().map(|binding| {
                            format!(
                                "{} = {}",
                                binding.name,
                                type_ref_to_string(store, binding.ty, pkg_prefix, local_type_names)
                            )
                        }));
                        rendered.push('<');
                        rendered.push_str(&args.join(", "));
                        rendered.push('>');
                    }
                    rendered
                }
                K::Int => "int".into(),
                K::Bigint => "bigint".into(),
                K::Float => "float".into(),
                K::String => "string".into(),
                K::Bool => "bool".into(),
                K::Null => "null".into(),
                K::Never => "never".into(),
                K::Void => "void".into(),
                K::Uint8Array => "uint8array".into(),
                K::Media { kind: k } => format!("{:?}", k).to_lowercase(),
                K::Optional { inner } => {
                    format!(
                        "{}?",
                        type_ref_to_string(store, *inner, pkg_prefix, local_type_names)
                    )
                }
                K::List { inner } => {
                    format!(
                        "{}[]",
                        type_ref_to_string(store, *inner, pkg_prefix, local_type_names)
                    )
                }
                K::Map { key, value } => format!(
                    "map<{}, {}>",
                    type_ref_to_string(store, *key, pkg_prefix, local_type_names),
                    type_ref_to_string(store, *value, pkg_prefix, local_type_names)
                ),
                K::Union { variants: members } => members
                    .iter()
                    .map(|&m| type_ref_to_string(store, m, pkg_prefix, local_type_names))
                    .collect::<Vec<_>>()
                    .join(" | "),
                K::Literal { value: lit } => lit.to_string(),
                K::Function {
                    params,
                    ret,
                    throws,
                } => {
                    let ps: Vec<String> = params
                        .iter()
                        .map(|p| {
                            p.name
                                .as_ref()
                                .map(|n| {
                                    let optional_marker = if p.optional { "?" } else { "" };
                                    format!(
                                        "{}{}: {}",
                                        n.as_str(),
                                        optional_marker,
                                        type_ref_to_string(
                                            store,
                                            p.ty,
                                            pkg_prefix,
                                            local_type_names
                                        )
                                    )
                                })
                                .unwrap_or_else(|| {
                                    type_ref_to_string(store, p.ty, pkg_prefix, local_type_names)
                                })
                        })
                        .collect();
                    let throws = throws
                        .map(|throws| {
                            type_ref_to_string(store, throws, pkg_prefix, local_type_names)
                        })
                        .map(|throws| format!(" throws {throws}"))
                        .unwrap_or_default();
                    format!(
                        "({}) -> {}{}",
                        ps.join(", "),
                        type_ref_to_string(store, *ret, pkg_prefix, local_type_names),
                        throws
                    )
                }
                K::BuiltinUnknown => "unknown".into(),
                K::AssociatedTypeProjection {
                    base,
                    interface,
                    member,
                } => {
                    let base = type_ref_to_string(store, *base, pkg_prefix, local_type_names);
                    if let Some(interface) = interface {
                        let interface =
                            type_ref_to_string(store, *interface, pkg_prefix, local_type_names);
                        format!("({base} as {interface}).{member}")
                    } else {
                        format!("{base}.{member}")
                    }
                }
                K::Type => "reflect.Type".into(),
                K::Rust => "$rust_type".into(),
                K::Error => "error".into(),
                K::Unknown => "?".into(),
                K::Infer => "_".into(),
            }
        }

        fn pat_desc_hir(
            pat_id: baml_compiler2_ast::PatId,
            body: &ExprBody,
            prefix: &str,
            local_type_names: &std::collections::HashSet<&str>,
        ) -> String {
            use baml_compiler2_ast::Pattern;
            let pat = &body.patterns[pat_id];
            match pat {
                Pattern::Wildcard => "_".to_string(),
                Pattern::Bind { name, subpat } => match subpat {
                    Some(sp) => format!(
                        "{name}: {}",
                        pat_desc_hir(*sp, body, prefix, local_type_names)
                    ),
                    None => name.to_string(),
                },
                Pattern::Or(pats) => pats
                    .iter()
                    .map(|p| pat_desc_hir(*p, body, prefix, local_type_names))
                    .collect::<Vec<_>>()
                    .join(" | "),
                Pattern::Type(ty) => type_expr_to_string_hir(ty, prefix, local_type_names),
                Pattern::Unreflect(expr) => format!(
                    "unreflect({})",
                    expr_desc_hir(*expr, body, prefix, local_type_names)
                ),
                Pattern::Class {
                    class,
                    generic_args,
                    fields,
                    ..
                } => {
                    let class_path = class
                        .iter()
                        .map(|n| n.as_str())
                        .collect::<Vec<_>>()
                        .join(".");
                    let generic_args = if generic_args.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "<{}>",
                            generic_args
                                .iter()
                                .map(|ty| type_expr_to_string_hir(ty, prefix, local_type_names))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    };
                    let field_strs: Vec<_> = fields
                        .iter()
                        .map(|f| {
                            format!(
                                "{}: {}",
                                f.field,
                                pat_desc_hir(f.pat, body, prefix, local_type_names)
                            )
                        })
                        .collect();
                    format!("{class_path}{generic_args} {{ {} }}", field_strs.join(", "))
                }
                Pattern::Array {
                    prefix: prefix_pats,
                    rest,
                    suffix,
                    ascription,
                } => {
                    let mut parts = Vec::new();
                    parts.extend(
                        prefix_pats
                            .iter()
                            .map(|p| pat_desc_hir(*p, body, prefix, local_type_names)),
                    );
                    if let Some(rest) = rest {
                        let rest_desc = rest
                            .pat
                            .map(|p| pat_desc_hir(p, body, prefix, local_type_names))
                            .unwrap_or_else(String::new);
                        parts.push(format!("..{rest_desc}"));
                    }
                    parts.extend(
                        suffix
                            .iter()
                            .map(|p| pat_desc_hir(*p, body, prefix, local_type_names)),
                    );
                    let arr = format!("[{}]", parts.join(", "));
                    match ascription {
                        Some(t) => format!(
                            "{arr}: {}",
                            type_expr_to_string_hir(t, prefix, local_type_names)
                        ),
                        None => arr,
                    }
                }
            }
        }

        fn expr_desc_hir(
            expr_id: baml_compiler2_ast::ExprId,
            body: &ExprBody,
            prefix: &str,
            local_type_names: &std::collections::HashSet<&str>,
        ) -> String {
            let expr = &body.exprs[expr_id];
            match expr {
                Expr::Literal(lit) => match lit {
                    Literal::String(s) => {
                        let truncated: String = if s.chars().count() > 20 {
                            format!("{}...", s.chars().take(17).collect::<String>())
                        } else {
                            s.clone()
                        };
                        format!("{truncated:?}")
                    }
                    Literal::Int(i) => i.to_string(),
                    Literal::Bigint(n) => format!("{n}n"),
                    Literal::Float(f) => f.clone(),
                    Literal::Bool(b) => b.to_string(),
                },
                Expr::Null => "null".into(),
                Expr::Path(segments) => segments
                    .iter()
                    .map(|n| n.as_str())
                    .collect::<Vec<_>>()
                    .join("."),
                Expr::If {
                    condition,
                    then_branch,
                    else_branch,
                } => {
                    let cond = expr_desc_hir(*condition, body, prefix, local_type_names);
                    let then_desc = expr_desc_hir(*then_branch, body, prefix, local_type_names);
                    match else_branch {
                        Some(eb) => format!(
                            "if ({cond}) {then_desc} else {}",
                            expr_desc_hir(*eb, body, prefix, local_type_names)
                        ),
                        None => format!("if ({cond}) {then_desc}"),
                    }
                }
                Expr::IfLet {
                    pattern,
                    scrutinee,
                    then_branch,
                    else_branch,
                } => {
                    let pat = pat_desc_hir(*pattern, body, prefix, local_type_names);
                    let scrut = expr_desc_hir(*scrutinee, body, prefix, local_type_names);
                    let then_desc = expr_desc_hir(*then_branch, body, prefix, local_type_names);
                    match else_branch {
                        Some(eb) => format!(
                            "if let {pat} = {scrut} {then_desc} else {}",
                            expr_desc_hir(*eb, body, prefix, local_type_names)
                        ),
                        None => format!("if let {pat} = {scrut} {then_desc}"),
                    }
                }
                Expr::Match {
                    scrutinee, arms, ..
                } => {
                    let scrut = expr_desc_hir(*scrutinee, body, prefix, local_type_names);
                    let arm_strs: Vec<String> = arms
                        .iter()
                        .map(|arm_id| {
                            let arm = &body.match_arms[*arm_id];
                            let pat = pat_desc_hir(arm.pattern, body, prefix, local_type_names);
                            let body_desc = expr_desc_hir(arm.body, body, prefix, local_type_names);
                            format!("{pat} => {body_desc}")
                        })
                        .collect();
                    format!("match ({scrut}) {{ {} }}", arm_strs.join(", "))
                }
                Expr::Is { scrutinee, pattern } => format!(
                    "{} is {}",
                    expr_desc_hir(*scrutinee, body, prefix, local_type_names),
                    pat_desc_hir(*pattern, body, prefix, local_type_names),
                ),
                Expr::Catch { base, clauses } => {
                    let base_desc = expr_desc_hir(*base, body, prefix, local_type_names);
                    let clause_descs: Vec<String> = clauses
                        .iter()
                        .map(|clause| {
                            let kind = match clause.kind {
                                CatchClauseKind::Catch => "catch",
                                CatchClauseKind::CatchAll => "catch_all",
                                CatchClauseKind::CatchAllPanics => "catch_all_panics",
                            };
                            let binding =
                                pat_desc_hir(clause.binding, body, prefix, local_type_names);
                            let arms_desc = clause
                                .arms
                                .iter()
                                .map(|arm_id| {
                                    let arm = &body.catch_arms[*arm_id];
                                    format!(
                                        "{} => {}",
                                        pat_desc_hir(arm.pattern, body, prefix, local_type_names),
                                        expr_desc_hir(arm.body, body, prefix, local_type_names)
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join(", ");
                            format!("{kind} ({binding}) {{ {arms_desc} }}")
                        })
                        .collect();
                    format!("{base_desc} {}", clause_descs.join(" "))
                }
                Expr::Throw { value } => {
                    format!(
                        "throw {}",
                        expr_desc_hir(*value, body, prefix, local_type_names)
                    )
                }
                Expr::Return { value } => match value {
                    Some(value) => format!(
                        "return {}",
                        expr_desc_hir(*value, body, prefix, local_type_names)
                    ),
                    None => "return".into(),
                },
                Expr::Binary { op, lhs, rhs } => format!(
                    "{} {op:?} {}",
                    expr_desc_hir(*lhs, body, prefix, local_type_names),
                    expr_desc_hir(*rhs, body, prefix, local_type_names)
                ),
                Expr::Unary { op, expr: inner } => {
                    format!(
                        "{op:?} {}",
                        expr_desc_hir(*inner, body, prefix, local_type_names)
                    )
                }
                Expr::Call { callee, args, .. } => {
                    let callee_str = expr_desc_hir(*callee, body, prefix, local_type_names);
                    let arg_strs: Vec<String> = args
                        .iter()
                        .map(|a| match &a.label {
                            Some(label) => {
                                format!(
                                    "{label} = {}",
                                    expr_desc_hir(a.expr, body, prefix, local_type_names)
                                )
                            }
                            None => expr_desc_hir(a.expr, body, prefix, local_type_names),
                        })
                        .collect();
                    format!("{callee_str}({})", arg_strs.join(", "))
                }
                Expr::Object {
                    type_name, fields, ..
                } => {
                    let tn = qualify_type_name(type_name, prefix, local_type_names);
                    let field_strs: Vec<String> = fields
                        .iter()
                        .map(|field| {
                            format!(
                                "{}: {}",
                                field.name,
                                expr_desc_hir(field.value, body, prefix, local_type_names)
                            )
                        })
                        .collect();
                    format!("{tn} {{ {} }}", field_strs.join(", "))
                }
                Expr::Array { elements } => {
                    let elem_strs: Vec<String> = elements
                        .iter()
                        .map(|e| expr_desc_hir(*e, body, prefix, local_type_names))
                        .collect();
                    format!("[{}]", elem_strs.join(", "))
                }
                Expr::Map { entries } => {
                    let entry_strs: Vec<String> = entries
                        .iter()
                        .map(|entry| {
                            format!(
                                "{}: {}",
                                expr_desc_hir(entry.key, body, prefix, local_type_names),
                                expr_desc_hir(entry.value, body, prefix, local_type_names)
                            )
                        })
                        .collect();
                    format!("map {{ {} }}", entry_strs.join(", "))
                }
                Expr::Block { stmts, tail_expr } => {
                    let stmt_strs: Vec<String> = stmts
                        .iter()
                        .map(|s| stmt_desc_hir(*s, body, prefix, local_type_names))
                        .collect();
                    let tail = tail_expr
                        .map(|e| format!(" {}", expr_desc_hir(e, body, prefix, local_type_names)))
                        .unwrap_or_default();
                    format!("{{ {} }}{tail}", stmt_strs.join("; "))
                }
                Expr::MemberAccess { base, member } => {
                    format!(
                        "{}.{member}",
                        expr_desc_hir(*base, body, prefix, local_type_names)
                    )
                }
                Expr::Upcast { base, target } => {
                    format!(
                        "{}.as<{}>",
                        expr_desc_hir(*base, body, prefix, local_type_names),
                        type_expr_to_string_hir(target, prefix, local_type_names)
                    )
                }
                Expr::OptionalMemberAccess { base, member } => {
                    format!(
                        "{}?.{member}",
                        expr_desc_hir(*base, body, prefix, local_type_names)
                    )
                }
                Expr::OptionalIndex { base, index } => format!(
                    "{}?.[{}]",
                    expr_desc_hir(*base, body, prefix, local_type_names),
                    expr_desc_hir(*index, body, prefix, local_type_names)
                ),
                Expr::OptionalCall { callee, args } => {
                    let callee_str = expr_desc_hir(*callee, body, prefix, local_type_names);
                    let arg_strs: Vec<String> = args
                        .iter()
                        .map(|a| match &a.label {
                            Some(label) => {
                                format!(
                                    "{label} = {}",
                                    expr_desc_hir(a.expr, body, prefix, local_type_names)
                                )
                            }
                            None => expr_desc_hir(a.expr, body, prefix, local_type_names),
                        })
                        .collect();
                    format!("{callee_str}?.({})", arg_strs.join(", "))
                }
                Expr::Index { base, index } => format!(
                    "{}[{}]",
                    expr_desc_hir(*base, body, prefix, local_type_names),
                    expr_desc_hir(*index, body, prefix, local_type_names)
                ),
                Expr::Lambda(func_def) => {
                    let sig = format_lambda_signature_hir(func_def, prefix, local_type_names);
                    let body_desc = func_def.body.map_or_else(
                        || "<no body>".into(),
                        |root| expr_desc_hir(root, body, prefix, local_type_names),
                    );
                    // Replace "{ ... }" placeholder with actual body
                    sig.replace("{ ... }", &format!("{{ {body_desc} }}"))
                }
                Expr::OptionalChain { expr } => {
                    expr_desc_hir(*expr, body, prefix, local_type_names)
                }
                Expr::ByteStringLiteral(bytes) => format!("b\"<{} bytes>\"", bytes.len()),
                Expr::Spawn {
                    body: spawn_body, ..
                } => {
                    format!(
                        "spawn {{ {} }}",
                        expr_desc_hir(*spawn_body, body, prefix, local_type_names)
                    )
                }
                Expr::Await { future } => format!(
                    "await {}",
                    expr_desc_hir(*future, body, prefix, local_type_names)
                ),
                Expr::Template { tag, .. } => match tag {
                    baml_compiler2_ast::TemplateTag::Custom { tag, .. } => format!(
                        "{}`...`",
                        expr_desc_hir(*tag, body, prefix, local_type_names)
                    ),
                    baml_compiler2_ast::TemplateTag::Default { .. } => "`...`".into(),
                },
                Expr::GenericApply { base, .. } => {
                    format!(
                        "{}<...>",
                        expr_desc_hir(*base, body, prefix, local_type_names)
                    )
                }
                Expr::QualifiedPath {
                    qself,
                    interface,
                    member,
                } => format!(
                    "({} as {}).{member}",
                    type_expr_to_string_hir(qself, prefix, local_type_names),
                    type_expr_to_string_hir(interface, prefix, local_type_names)
                ),
                Expr::Missing => "<missing>".into(),
            }
        }

        fn stmt_desc_hir(
            stmt_id: baml_compiler2_ast::StmtId,
            body: &ExprBody,
            prefix: &str,
            local_type_names: &std::collections::HashSet<&str>,
        ) -> String {
            use baml_compiler2_ast::Stmt;
            let stmt = &body.stmts[stmt_id];
            match stmt {
                Stmt::TypeBinding { name, value } => format!(
                    "type {name} = unreflect({})",
                    expr_desc_hir(*value, body, prefix, local_type_names)
                ),
                Stmt::Let {
                    pattern,
                    initializer,
                    ..
                } => {
                    // The annotation is now part of the pattern (a `Chain`
                    // link), so `pat_desc_hir` already prints it.
                    let pat = pat_desc_hir(*pattern, body, prefix, local_type_names);
                    let init = initializer
                        .map(|e| format!(" = {}", expr_desc_hir(e, body, prefix, local_type_names)))
                        .unwrap_or_default();
                    format!("let {pat}{init}")
                }
                Stmt::Return(Some(expr_id)) => {
                    format!(
                        "return {}",
                        expr_desc_hir(*expr_id, body, prefix, local_type_names)
                    )
                }
                Stmt::Return(None) => "return".into(),
                Stmt::Throw { value } => {
                    format!(
                        "throw {}",
                        expr_desc_hir(*value, body, prefix, local_type_names)
                    )
                }
                Stmt::Expr(expr_id) => expr_desc_hir(*expr_id, body, prefix, local_type_names),
                Stmt::While {
                    condition,
                    body: be,
                    ..
                } => format!(
                    "while {} {}",
                    expr_desc_hir(*condition, body, prefix, local_type_names),
                    expr_desc_hir(*be, body, prefix, local_type_names)
                ),
                Stmt::WhileLet {
                    pattern,
                    scrutinee,
                    body: be,
                } => format!(
                    "while let {} = {} {}",
                    pat_desc_hir(*pattern, body, prefix, local_type_names),
                    expr_desc_hir(*scrutinee, body, prefix, local_type_names),
                    expr_desc_hir(*be, body, prefix, local_type_names)
                ),
                Stmt::For {
                    binding,
                    collection,
                    body: for_body,
                } => {
                    let bind = pat_desc_hir(*binding, body, prefix, local_type_names);
                    let coll = expr_desc_hir(*collection, body, prefix, local_type_names);
                    format!(
                        "for {bind} in {coll} {}",
                        expr_desc_hir(*for_body, body, prefix, local_type_names)
                    )
                }
                Stmt::Assign { target, value } => format!(
                    "{} = {}",
                    expr_desc_hir(*target, body, prefix, local_type_names),
                    expr_desc_hir(*value, body, prefix, local_type_names)
                ),
                Stmt::AssignOp { target, op, value } => format!(
                    "{} {op:?}= {}",
                    expr_desc_hir(*target, body, prefix, local_type_names),
                    expr_desc_hir(*value, body, prefix, local_type_names)
                ),
                Stmt::Defer { body: defer_body } => format!(
                    "defer {}",
                    expr_desc_hir(*defer_body, body, prefix, local_type_names)
                ),
                Stmt::Break => "break".into(),
                Stmt::Continue => "continue".into(),
                Stmt::HeaderComment { name, level } => format!("// [{level}] {name}"),
                Stmt::Missing => "<missing stmt>".into(),
            }
        }

        let mut output = String::new();
        let pkg_info = file_package(db, file);
        let prefix = if pkg_info.namespace_path.is_empty() {
            format!("{}.", pkg_info.package)
        } else {
            format!(
                "{}.{}.",
                pkg_info.package,
                pkg_info
                    .namespace_path
                    .iter()
                    .map(|n| n.as_str())
                    .collect::<Vec<_>>()
                    .join(".")
            )
        };

        use baml_compiler2_ppir::item_data::{
            class_data, enum_data, file_classes, file_enums, file_functions, file_type_aliases,
            function_data, function_llm_meta, type_alias_data,
        };

        let mut local_type_names = std::collections::HashSet::new();
        for &loc in file_classes(db, file) {
            local_type_names.insert(class_data(db, loc).name.as_str());
        }
        for &loc in file_enums(db, file) {
            local_type_names.insert(enum_data(db, loc).name.as_str());
        }
        for &loc in file_type_aliases(db, file) {
            local_type_names.insert(type_alias_data(db, loc).name.as_str());
        }

        let mut classes = file_classes(db, file).to_vec();
        classes.sort_by_key(|&loc| class_data(db, loc).name.as_str().to_string());
        for loc in classes {
            let class = class_data(db, loc);
            writeln!(output, "class {prefix}{} {{", class.name).ok();
            for field in &class.fields {
                let ty = type_ref_to_string(
                    &class.type_refs,
                    field.type_ref,
                    &prefix,
                    &local_type_names,
                );
                writeln!(output, "  {}: {}", field.name, ty).ok();
            }
            writeln!(output, "}}").ok();
        }

        let mut enums = file_enums(db, file).to_vec();
        enums.sort_by_key(|&loc| enum_data(db, loc).name.as_str().to_string());
        for loc in enums {
            let enum_def = enum_data(db, loc);
            write!(output, "enum {prefix}{} {{", enum_def.name).ok();
            for (i, v) in enum_def.variants.iter().enumerate() {
                if i > 0 {
                    write!(output, ", ").ok();
                }
                write!(output, "{}", v.name).ok();
            }
            writeln!(output, "}}").ok();
        }

        let mut type_aliases = file_type_aliases(db, file).to_vec();
        type_aliases.sort_by_key(|&loc| type_alias_data(db, loc).name.as_str().to_string());
        for loc in type_aliases {
            let ta = type_alias_data(db, loc);
            let ty = ta
                .value
                .map(|id| type_ref_to_string(&ta.type_refs, id, &prefix, &local_type_names))
                .unwrap_or_else(|| "?".into());
            writeln!(output, "type {prefix}{} = {}", ta.name, ty).ok();
        }

        let mut functions = file_functions(db, file).to_vec();
        functions.sort_by_key(|&loc| function_data(db, loc).name.as_str().to_string());
        for loc in functions {
            let func = function_data(db, loc);
            let defaults = baml_compiler2_ppir::function_parameter_defaults(db, loc);
            let params: Vec<String> = func
                .params
                .iter()
                .enumerate()
                .map(|(index, p)| {
                    let default_suffix =
                        default_ref_suffix(defaults.param_default(index), &defaults.defaults);
                    let ty = p
                        .type_ref
                        .map(|id| {
                            type_ref_to_string(&func.type_refs, id, &prefix, &local_type_names)
                        })
                        .unwrap_or_else(|| "?".into());
                    format!("{}: {}{}", p.name, ty, default_suffix)
                })
                .collect();
            let ret = func
                .return_type
                .map(|id| type_ref_to_string(&func.type_refs, id, &prefix, &local_type_names))
                .unwrap_or_else(|| "?".into());
            let func_body = baml_compiler2_ppir::function_body(db, loc);
            let body_kind = if function_llm_meta(db, loc).is_some() {
                "llm"
            } else {
                match func_body.as_ref() {
                    baml_compiler2_hir::body::FunctionBody::Expr(_) => "expr",
                    baml_compiler2_hir::body::FunctionBody::Builtin(_) => "builtin",
                    baml_compiler2_hir::body::FunctionBody::Missing => "missing",
                }
            };
            write!(
                output,
                "function {prefix}{}({}) -> {}  [{}]",
                func.name,
                params.join(", "),
                ret,
                body_kind
            )
            .ok();
            if let baml_compiler2_hir::body::FunctionBody::Expr(body) = func_body.as_ref() {
                if let Some(root) = body.root_expr {
                    writeln!(output, " {{").ok();
                    writeln!(
                        output,
                        "  {}",
                        expr_desc_hir(root, body, &prefix, &local_type_names)
                    )
                    .ok();
                    writeln!(output, "}}").ok();
                } else {
                    writeln!(output).ok();
                }
            } else {
                writeln!(output).ok();
            }
        }

        // ── Lambda capture annotations ──────────────────────────────────────
        let index = file_semantic_index(db, file);
        let mut has_captures = false;
        for (i, scope) in index.scopes.iter().enumerate() {
            if !matches!(scope.kind, baml_compiler2_hir::scope::ScopeKind::Lambda) {
                continue;
            }
            let bindings = &index.scope_bindings[i];
            if bindings.captures.is_empty() {
                continue;
            }
            if !has_captures {
                writeln!(output, "\n--- captures ---").ok();
                has_captures = true;
            }
            // Build a descriptive path for the lambda scope
            let parent_name = scope
                .parent
                .and_then(|pid| {
                    let parent = &index.scopes[pid.index() as usize];
                    parent.name.as_ref().map(|n| n.to_string())
                })
                .unwrap_or_else(|| "?".into());
            let params: Vec<&str> = bindings.params.iter().map(|(n, _)| n.as_str()).collect();
            let capture_names: Vec<&str> =
                bindings.captures.iter().map(|(n, _)| n.as_str()).collect();
            writeln!(
                output,
                "lambda ({}) in {}: captures [{}]",
                params.join(", "),
                parent_name,
                capture_names.join(", ")
            )
            .ok();
        }

        output
    }

    pub fn expr_type_in_function(
        db: &ProjectDatabase,
        file: baml_base::SourceFile,
        function_name: &str,
        expr_text: &str,
    ) -> String {
        let func_loc = *baml_compiler2_ppir::item_data::file_functions(db, file)
            .iter()
            .find(|&&loc| {
                baml_compiler2_ppir::item_data::function_data(db, loc)
                    .name
                    .as_str()
                    == function_name
            })
            .unwrap_or_else(|| panic!("function `{function_name}` not found"));
        let func_body = baml_compiler2_ppir::function_body(db, func_loc);
        let body = match func_body.as_ref() {
            FunctionBody::Expr(body) => body,
            _ => panic!("function `{function_name}` has no expression body"),
        };

        // The authoritative function→scope link, replacing a `scope.range ==
        // func.span` join.
        let inference = baml_compiler2_hir_ty::infer::infer_body(
            db,
            baml_compiler2_hir::body::BodyOwnerId::Function(func_loc),
        );

        let matches: Vec<_> = body
            .exprs
            .iter()
            .filter_map(|(expr_id, _)| (expr_desc(expr_id, body) == expr_text).then_some(expr_id))
            .collect();
        let expr_id = match matches.as_slice() {
            [expr_id] => *expr_id,
            [] => panic!("expression `{expr_text}` not found in function `{function_name}`"),
            _ => panic!(
                "expression `{expr_text}` matched multiple nodes in function `{function_name}`"
            ),
        };

        inference
            .type_of_expr
            .get(&expr_id)
            .map(|ty| ty.to_plain().render_canonical())
            .unwrap_or_else(|| {
                panic!(
                    "expression `{expr_text}` in function `{function_name}` has no inferred type"
                )
            })
    }

    pub fn make_db() -> ProjectDatabase {
        let mut db = ProjectDatabase::new();
        db.workspace(std::path::Path::new("."));
        db
    }
}
