//! Snapshot tests for `baml_compiler2_tir`.
//!
//! Each test creates a minimal DB, adds a `.baml` file, runs type inference,
//! and snapshots the fully-typed output using the same format as the onion skin
//! tool's `run_tir2` renderer.

#[cfg(test)]
mod inference;
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
        CatchClauseKind, Expr, ExprBody, ExprId, Literal, PatId, Pattern, Stmt, StmtId, TypeExpr,
    };
    use baml_compiler2_hir::{
        body::{FunctionBody, function_body},
        contributions::Definition,
        loc::FunctionLoc,
        scope::ScopeKind,
        signature::function_signature,
    };
    use baml_compiler2_ppir::file_semantic_index;
    use baml_compiler2_tir::{
        inference::{
            ScopeInference, infer_scope_types, render_scope_diagnostics, resolve_class_fields,
            resolve_type_alias,
        },
        lower_type_expr::lower_type_expr,
    };
    use baml_project::ProjectDatabase;

    // ── Rendering helpers ────────────────────────────────────────────────────

    fn type_expr_to_string(ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Path(segments) => segments
                .iter()
                .map(|n| n.as_str())
                .collect::<Vec<_>>()
                .join("."),
            TypeExpr::Int => "int".into(),
            TypeExpr::Float => "float".into(),
            TypeExpr::String => "string".into(),
            TypeExpr::Bool => "bool".into(),
            TypeExpr::Null => "null".into(),
            TypeExpr::Never => "never".into(),
            TypeExpr::Media(k) => format!("{:?}", k).to_lowercase(),
            TypeExpr::Optional(inner) => format!("{}?", type_expr_to_string(inner)),
            TypeExpr::List(inner) => format!("{}[]", type_expr_to_string(inner)),
            TypeExpr::Map { key, value } => format!(
                "map<{}, {}>",
                type_expr_to_string(key),
                type_expr_to_string(value)
            ),
            TypeExpr::Union(members) => members
                .iter()
                .map(type_expr_to_string)
                .collect::<Vec<_>>()
                .join(" | "),
            TypeExpr::Literal(lit) => lit.to_string(),
            TypeExpr::Function { params, ret } => {
                let ps: Vec<String> = params
                    .iter()
                    .map(|p| {
                        p.name
                            .as_ref()
                            .map(|n| format!("{}: {}", n.as_str(), type_expr_to_string(&p.ty)))
                            .unwrap_or_else(|| type_expr_to_string(&p.ty))
                    })
                    .collect();
                format!("({}) -> {}", ps.join(", "), type_expr_to_string(ret))
            }
            TypeExpr::BuiltinUnknown => "unknown".into(),
            TypeExpr::Type => "type".into(),
            TypeExpr::Rust => "$rust_type".into(),
            TypeExpr::Error => "error".into(),
            TypeExpr::Unknown => "?".into(),
        }
    }

    fn pat_desc(pat_id: PatId, body: &ExprBody) -> String {
        let pat = &body.patterns[pat_id];
        match pat {
            Pattern::Binding(n) => n.to_string(),
            Pattern::TypedBinding { name, ty } => {
                format!("{name}: {}", type_expr_to_string(ty))
            }
            Pattern::Literal(lit) => lit.to_string(),
            Pattern::Null => "null".into(),
            Pattern::EnumVariant { enum_name, variant } => format!("{enum_name}.{variant}"),
            Pattern::Union(pats) => pats
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
                    let truncated = if s.len() > 20 {
                        format!("{}...", &s[..17])
                    } else {
                        s.clone()
                    };
                    format!("\"{}\"", truncated)
                }
                Literal::Int(i) => i.to_string(),
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
            Expr::Binary { op, lhs, rhs } => {
                format!("{} {op:?} {}", expr_desc(*lhs, body), expr_desc(*rhs, body))
            }
            Expr::Unary { op, expr: inner } => format!("{op:?} {}", expr_desc(*inner, body)),
            Expr::Call { callee, args } => {
                let callee_str = expr_desc(*callee, body);
                let arg_strs: Vec<String> = args.iter().map(|a| expr_desc(*a, body)).collect();
                format!("{callee_str}({})", arg_strs.join(", "))
            }
            Expr::Object {
                type_name, fields, ..
            } => {
                let tn = type_name.as_ref().map(|n| n.as_str()).unwrap_or("_");
                let field_strs: Vec<String> = fields
                    .iter()
                    .map(|(name, val)| format!("{name}: {}", expr_desc(*val, body)))
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
                    .map(|(k, v)| format!("{}: {}", expr_desc(*k, body), expr_desc(*v, body)))
                    .collect();
                format!("map {{ {} }}", entry_strs.join(", "))
            }
            Expr::Block { stmts, tail_expr } => {
                let tail = if tail_expr.is_some() { " + tail" } else { "" };
                format!("{{ {} stmts{tail} }}", stmts.len())
            }
            Expr::FieldAccess { base, field } => {
                format!("{}.{field}", expr_desc(*base, body))
            }
            Expr::Index { base, index } => {
                format!("{}[{}]", expr_desc(*base, body), expr_desc(*index, body))
            }
            Expr::Missing => "<missing>".into(),
        }
    }

    /// Returns true if an expression is "compound" and should be rendered
    /// with recursive indented output rather than a single-line `expr_desc`.
    fn is_compound(expr: &Expr) -> bool {
        matches!(
            expr,
            Expr::Block { .. } | Expr::If { .. } | Expr::Match { .. } | Expr::Catch { .. }
        )
    }

    /// Format an expression's inferred type as a string.
    fn expr_ty(inference: &ScopeInference, expr_id: ExprId) -> String {
        inference
            .expression_type(expr_id)
            .map(|t| t.to_string())
            .unwrap_or_else(|| "unknown".into())
    }

    fn render_expr(
        expr_id: ExprId,
        body: &ExprBody,
        inference: &ScopeInference,
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
                let base_desc = expr_desc(*base, body);
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
            _ => {
                let desc = expr_desc(expr_id, body);
                writeln!(output, "{pad}{desc} : {ty}").ok();
            }
        }
    }

    fn render_stmt(
        stmt_id: StmtId,
        body: &ExprBody,
        inference: &ScopeInference,
        indent: usize,
        output: &mut String,
    ) {
        let pad = " ".repeat(indent);
        let stmt = &body.stmts[stmt_id];
        match stmt {
            Stmt::Let {
                pattern,
                initializer,
                ..
            } => {
                let pat_name = match &body.patterns[*pattern] {
                    Pattern::Binding(n) => n.to_string(),
                    Pattern::TypedBinding { name, ty } => {
                        format!("{name}: {}", type_expr_to_string(ty))
                    }
                    other => format!("{other:?}"),
                };
                if let Some(init) = initializer {
                    let init_ty = expr_ty(inference, *init);
                    let binding_ty = inference.binding_type(*pattern).map(|t| t.to_string());
                    let ty_display = match &binding_ty {
                        Some(bt) if *bt != init_ty => format!("{init_ty} -> {bt}"),
                        _ => init_ty,
                    };
                    if is_compound(&body.exprs[*init]) {
                        writeln!(output, "{pad}let {pat_name} = : {ty_display}").ok();
                        render_expr(*init, body, inference, indent + 2, output);
                    } else {
                        let init_desc = expr_desc(*init, body);
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
                    let desc = expr_desc(*expr_id, body);
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
                    let desc = expr_desc(*value, body);
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
            Stmt::Assert { condition } => {
                let desc = expr_desc(*condition, body);
                writeln!(output, "{pad}assert {desc}").ok();
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
    pub fn render_tir(db: &ProjectDatabase, file: baml_base::SourceFile) -> String {
        use baml_compiler2_hir::package::PackageId;
        use baml_compiler2_ppir::package_items;
        use baml_compiler2_tir::inference::{
            detect_invalid_alias_cycles, detect_invalid_class_cycles,
        };

        let mut output = String::new();
        let index = file_semantic_index(db, file);

        // Get package items for resolving TypeExpr -> Ty in signatures
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let pkg_items = package_items(db, pkg_id);

        // Pre-compute invalid alias cycles for the package
        let invalid_cycles = detect_invalid_alias_cycles(db, pkg_id);

        // Pre-compute invalid class cycles — build a map from class QN → cycle_path
        let class_cycles_info = detect_invalid_class_cycles(db, pkg_id);
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
                                let resolved = resolve_class_fields(db, class_loc);
                                writeln!(output, "{kind_str} {fqn} {{").ok();
                                for (fname, fty) in &resolved.fields {
                                    writeln!(output, "  {fname}: {fty}").ok();
                                }
                                writeln!(output, "}}").ok();
                                // Render class cycle diagnostic if applicable
                                let qn = baml_compiler2_tir::lower_type_expr::qualify(
                                    pkg_info.package.as_str(),
                                    name,
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
                                let resolved = resolve_type_alias(db, alias_loc);
                                writeln!(output, "{kind_str} {fqn} = {}", resolved.ty).ok();
                                // Render type-lowering diagnostics
                                for (diag, span) in &resolved.diagnostics {
                                    let start = u32::from(span.start());
                                    let end = u32::from(span.end());
                                    writeln!(output, "  !! {start}..{end}: {diag}").ok();
                                }
                                // Render cycle diagnostic if this alias is in an invalid cycle
                                let qn = baml_compiler2_tir::lower_type_expr::qualify(
                                    pkg_info.package.as_str(),
                                    name,
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
            let inference = infer_scope_types(db, scope_id);

            let mut func_body_opt: Option<std::sync::Arc<FunctionBody>> = None;
            let mut sig_display = String::new();
            if matches!(scope.kind, ScopeKind::Function) {
                let item_tree = &index.item_tree;
                for (local_id, func_data) in &item_tree.functions {
                    if func_data.span == scope.range {
                        let func_loc = FunctionLoc::new(db, file, *local_id);
                        func_body_opt = Some(function_body(db, func_loc));
                        let sig = function_signature(db, func_loc);
                        let params: Vec<String> = sig
                            .params
                            .iter()
                            .map(|(pname, ptype)| {
                                let mut diags = Vec::new();
                                let ty = lower_type_expr(db, ptype, pkg_items, &mut diags);
                                format!("{}: {}", pname, ty)
                            })
                            .collect();
                        let ret = sig
                            .return_type
                            .as_ref()
                            .map(|t| {
                                let mut diags = Vec::new();
                                lower_type_expr(db, t, pkg_items, &mut diags).to_string()
                            })
                            .unwrap_or_else(|| "?".into());
                        let throws = sig
                            .throws
                            .as_ref()
                            .map(|t| {
                                let mut diags = Vec::new();
                                lower_type_expr(db, t, pkg_items, &mut diags).to_string()
                            })
                            .map(|t| format!(" throws {t}"))
                            .unwrap_or_default();
                        sig_display = format!("({}) -> {ret}{throws}", params.join(", "));
                        break;
                    }
                }
            }

            // Collect expression types for this scope — skip if none
            let mut has_expr_types = false;
            for (_expr_id, owner_scope) in &index.expr_scopes {
                if owner_scope.index() as usize == i {
                    has_expr_types = true;
                    break;
                }
            }
            if !has_expr_types {
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

            // Per-scope diagnostics
            let rendered = render_scope_diagnostics(db, scope_id);
            for rd in &rendered {
                let marker = match rd.severity {
                    baml_compiler2_tir::infer_context::DiagnosticSeverity::Error => "!!",
                    baml_compiler2_tir::infer_context::DiagnosticSeverity::Warning => "??",
                };
                writeln!(output, "  {marker} {rd}").ok();
            }

            writeln!(output, "}}").ok();
        }

        output
    }

    pub fn make_db() -> ProjectDatabase {
        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("."));
        db
    }
}
