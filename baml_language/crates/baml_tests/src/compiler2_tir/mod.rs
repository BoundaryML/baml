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
pub(crate) mod support {
    use std::fmt::Write;

    use baml_compiler2_ast::{
        CatchClauseKind, Expr, ExprBody, ExprId, Literal, PatId, Pattern, Stmt, StmtId, TypeExpr,
    };
    use baml_compiler2_hir::{
        body::{FunctionBody, function_body},
        contributions::Definition,
        file_semantic_index,
        loc::FunctionLoc,
        scope::ScopeKind,
        signature::function_signature,
    };
    use baml_compiler2_tir::{
        inference::{
            ScopeInference, infer_scope_types, render_scope_diagnostics, resolve_class_fields,
            resolve_type_alias,
        },
        lower_type_expr::lower_type_expr_in_ns,
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

    /// Like `expr_desc` but enriches Call expressions with type params from inference.
    fn expr_desc_rich(expr_id: ExprId, body: &ExprBody, inference: &ScopeInference) -> String {
        let expr = &body.exprs[expr_id];
        if let Expr::Call { callee, args } = expr {
            let callee_str = expr_desc(*callee, body);
            let arg_strs: Vec<String> = args.iter().map(|a| expr_desc(*a, body)).collect();
            let type_params = if let Some(callee_ty) = inference.expression_type(*callee) {
                collect_typevars(callee_ty)
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
            Expr::Call { callee, args } => {
                // Show type params at call site when callee has TypeVars
                let callee_desc = expr_desc(*callee, body);
                let arg_strs: Vec<String> = args.iter().map(|a| expr_desc(*a, body)).collect();
                let type_params = if let Some(callee_ty) = inference.expression_type(*callee) {
                    collect_typevars(callee_ty)
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
            }
            _ => {
                let desc = expr_desc(expr_id, body);
                writeln!(output, "{pad}{desc} : {ty}").ok();
            }
        }
    }

    /// Collect unique TypeVar names from a Ty (in order of appearance).
    fn collect_typevars(ty: &baml_compiler2_tir::ty::Ty) -> Vec<String> {
        use baml_compiler2_tir::ty::Ty;
        let mut result = Vec::new();
        collect_typevars_inner(ty, &mut result);
        result
    }

    fn collect_typevars_inner(ty: &baml_compiler2_tir::ty::Ty, out: &mut Vec<String>) {
        use baml_compiler2_tir::ty::Ty;
        match ty {
            Ty::TypeVar(name) => {
                let s = name.to_string();
                if !out.contains(&s) {
                    out.push(s);
                }
            }
            Ty::List(inner) | Ty::Optional(inner) => collect_typevars_inner(inner, out),
            Ty::Map(k, v) => {
                collect_typevars_inner(k, out);
                collect_typevars_inner(v, out);
            }
            Ty::Union(members) => {
                for m in members {
                    collect_typevars_inner(m, out);
                }
            }
            Ty::Function { params, ret } => {
                for (_, p) in params {
                    collect_typevars_inner(p, out);
                }
                collect_typevars_inner(ret, out);
            }
            _ => {}
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
            Stmt::For {
                binding,
                collection,
                body: for_body,
            } => {
                let bind_name = match &body.patterns[*binding] {
                    Pattern::Binding(n) => n.to_string(),
                    Pattern::TypedBinding { name, .. } => name.to_string(),
                    other => format!("{other:?}"),
                };
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
        use baml_compiler2_hir::package::{PackageId, package_items};
        use baml_compiler2_tir::inference::{
            detect_invalid_alias_cycles, detect_invalid_class_cycles,
        };

        let mut output = String::new();
        let index = file_semantic_index(db, file);

        // Get package items for resolving TypeExpr -> Ty in signatures
        let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
        let pkg_id = PackageId::new(db, pkg_info.package.clone());
        let pkg_items = package_items(db, pkg_id);

        // Pre-compute throw sets for the package
        let throw_sets = baml_compiler2_tir::throw_inference::function_throw_sets(db, pkg_id);

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
                            if scope.name.as_ref() == Some(name) {
                                if let Definition::Class(class_loc) = c.definition {
                                    let resolved = resolve_class_fields(db, class_loc);
                                    writeln!(output, "{kind_str} {fqn} {{").ok();
                                    for (fname, fty) in &resolved.fields {
                                        writeln!(output, "  {fname}: {fty}").ok();
                                    }
                                    writeln!(output, "}}").ok();
                                    // Render class cycle diagnostic if applicable
                                    let qn = baml_compiler2_tir::ty::QualifiedTypeName::new(
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
                    }
                    ScopeKind::TypeAlias => {
                        for (name, c) in &contrib.types {
                            if scope.name.as_ref() == Some(name) {
                                if let Definition::TypeAlias(alias_loc) = c.definition {
                                    let resolved = resolve_type_alias(db, alias_loc);
                                    writeln!(output, "{kind_str} {fqn} = {}", resolved.ty).ok();
                                    // Render type-lowering diagnostics
                                    for (diag, span) in &resolved.diagnostics {
                                        let start = u32::from(span.start());
                                        let end = u32::from(span.end());
                                        writeln!(output, "  !! {start}..{end}: {diag}").ok();
                                    }
                                    // Render cycle diagnostic if this alias is in an invalid cycle
                                    let qn = baml_compiler2_tir::ty::QualifiedTypeName::new(
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
                    let name_matches = scope.name.as_ref().map_or(true, |n| *n == func_data.name);
                    if func_data.span == scope.range && name_matches {
                        let func_loc = FunctionLoc::new(db, file, *local_id);
                        func_body_opt = Some(function_body(db, func_loc));
                        let sig = function_signature(db, func_loc);
                        let ns = &pkg_info.namespace_path;

                        let enclosing_class_ty: Option<baml_compiler2_tir::ty::Ty> =
                            scope.parent.and_then(|parent_idx| {
                                let parent = &index.scopes[parent_idx.index() as usize];
                                if matches!(parent.kind, ScopeKind::Class) {
                                    parent.name.as_ref().and_then(|cn| {
                                        let mut ns_path = ns.to_vec();
                                        ns_path.push(cn.clone());
                                        pkg_items.lookup_type(&ns_path).map(|def| {
                                            baml_compiler2_tir::ty::Ty::Class(
                                                baml_compiler2_tir::lower_type_expr::qualify_def(
                                                    db, def, cn,
                                                ),
                                            )
                                        })
                                    })
                                } else {
                                    None
                                }
                            });

                        let gp = &func_data.generic_params;
                        let generics_display = if gp.is_empty() {
                            String::new()
                        } else {
                            let names: Vec<String> = gp.iter().map(|n| n.to_string()).collect();
                            format!("<{}>", names.join(", "))
                        };

                        let params: Vec<String> = sig
                            .params
                            .iter()
                            .map(|(pname, ptype)| {
                                let ty = if pname.as_str() == "self"
                                    && matches!(ptype, baml_compiler2_ast::TypeExpr::Unknown)
                                {
                                    enclosing_class_ty
                                        .clone()
                                        .unwrap_or(baml_compiler2_tir::ty::Ty::Unknown)
                                } else {
                                    let mut diags = Vec::new();
                                    lower_type_expr_in_ns(db, ptype, &pkg_items, ns, gp, &mut diags)
                                };
                                format!("{}: {}", pname, ty)
                            })
                            .collect();
                        let ret = sig
                            .return_type
                            .as_ref()
                            .map(|t| {
                                let mut diags = Vec::new();
                                lower_type_expr_in_ns(db, t, &pkg_items, ns, gp, &mut diags)
                                    .to_string()
                            })
                            .unwrap_or_else(|| "?".into());
                        // Compute inferred throws from transitive throw set
                        let inferred_throws: Option<String> = {
                            let key = baml_base::Name::new(&*fqn);
                            throw_sets
                                .transitive_for(&key)
                                .filter(|facts| !facts.is_empty())
                                .map(|facts| {
                                    let types: Vec<String> =
                                        facts.iter().map(|f| f.to_string()).collect();
                                    types.join(" | ")
                                })
                        };

                        let throws = if let Some(t) = &sig.throws {
                            let mut diags = Vec::new();
                            let declared =
                                lower_type_expr_in_ns(db, t, &pkg_items, ns, gp, &mut diags);
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

            if let Some(body) = expr_body {
                if let Some(root) = body.root_expr {
                    render_expr(root, body, &inference, 2, &mut output);
                }
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

    /// Render a file's HIR2 (compiler2 item tree) as readable text.
    pub fn render_hir2(db: &ProjectDatabase, file: baml_base::SourceFile) -> String {
        use baml_compiler2_ast::{CatchClauseKind, Expr, ExprBody, Literal, Pattern};
        use baml_compiler2_hir::{
            file_item_tree,
            file_package::file_package,
            loc::{ClassLoc, EnumLoc, TypeAliasLoc},
        };

        fn qualify_type_name(
            name: &baml_base::Name,
            pkg_prefix: &str,
            local_type_names: &std::collections::HashSet<&str>,
        ) -> String {
            let s = name.as_str();
            if s.contains('.') {
                s.into()
            } else if local_type_names.contains(s) {
                format!("{pkg_prefix}{s}")
            } else {
                s.into()
            }
        }

        fn type_expr_to_string_hir(ty: &baml_compiler2_ast::TypeExpr, pkg_prefix: &str) -> String {
            match ty {
                baml_compiler2_ast::TypeExpr::Path(segments) => {
                    let path = segments
                        .iter()
                        .map(|n| n.as_str())
                        .collect::<Vec<_>>()
                        .join(".");
                    if segments.len() == 1 {
                        format!("{pkg_prefix}{path}")
                    } else {
                        path
                    }
                }
                baml_compiler2_ast::TypeExpr::Int => "int".into(),
                baml_compiler2_ast::TypeExpr::Float => "float".into(),
                baml_compiler2_ast::TypeExpr::String => "string".into(),
                baml_compiler2_ast::TypeExpr::Bool => "bool".into(),
                baml_compiler2_ast::TypeExpr::Null => "null".into(),
                baml_compiler2_ast::TypeExpr::Never => "never".into(),
                baml_compiler2_ast::TypeExpr::Media(k) => format!("{:?}", k).to_lowercase(),
                baml_compiler2_ast::TypeExpr::Optional(inner) => {
                    format!("{}?", type_expr_to_string_hir(inner, pkg_prefix))
                }
                baml_compiler2_ast::TypeExpr::List(inner) => {
                    format!("{}[]", type_expr_to_string_hir(inner, pkg_prefix))
                }
                baml_compiler2_ast::TypeExpr::Map { key, value } => format!(
                    "map<{}, {}>",
                    type_expr_to_string_hir(key, pkg_prefix),
                    type_expr_to_string_hir(value, pkg_prefix)
                ),
                baml_compiler2_ast::TypeExpr::Union(members) => members
                    .iter()
                    .map(|m| type_expr_to_string_hir(m, pkg_prefix))
                    .collect::<Vec<_>>()
                    .join(" | "),
                baml_compiler2_ast::TypeExpr::Literal(lit) => lit.to_string(),
                baml_compiler2_ast::TypeExpr::Function { params, ret } => {
                    let ps: Vec<String> = params
                        .iter()
                        .map(|p| {
                            p.name
                                .as_ref()
                                .map(|n| {
                                    format!(
                                        "{}: {}",
                                        n.as_str(),
                                        type_expr_to_string_hir(&p.ty, pkg_prefix)
                                    )
                                })
                                .unwrap_or_else(|| type_expr_to_string_hir(&p.ty, pkg_prefix))
                        })
                        .collect();
                    format!(
                        "({}) -> {}",
                        ps.join(", "),
                        type_expr_to_string_hir(ret, pkg_prefix)
                    )
                }
                baml_compiler2_ast::TypeExpr::BuiltinUnknown => "unknown".into(),
                baml_compiler2_ast::TypeExpr::Type => "type".into(),
                baml_compiler2_ast::TypeExpr::Rust => "$rust_type".into(),
                baml_compiler2_ast::TypeExpr::Error => "error".into(),
                baml_compiler2_ast::TypeExpr::Unknown => "?".into(),
            }
        }

        fn pat_desc_hir(
            pat_id: baml_compiler2_ast::PatId,
            body: &ExprBody,
            prefix: &str,
            local_type_names: &std::collections::HashSet<&str>,
        ) -> String {
            let pat = &body.patterns[pat_id];
            match pat {
                Pattern::Binding(n) => n.to_string(),
                Pattern::TypedBinding { name, ty } => {
                    format!("{name}: {}", type_expr_to_string_hir(ty, prefix))
                }
                Pattern::Literal(lit) => lit.to_string(),
                Pattern::Null => "null".into(),
                Pattern::EnumVariant { enum_name, variant } => {
                    format!(
                        "{}.{variant}",
                        qualify_type_name(enum_name, prefix, local_type_names)
                    )
                }
                Pattern::Union(pats) => pats
                    .iter()
                    .map(|p| pat_desc_hir(*p, body, prefix, local_type_names))
                    .collect::<Vec<_>>()
                    .join(" | "),
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
                Expr::Call { callee, args } => {
                    let callee_str = expr_desc_hir(*callee, body, prefix, local_type_names);
                    let arg_strs: Vec<String> = args
                        .iter()
                        .map(|a| expr_desc_hir(*a, body, prefix, local_type_names))
                        .collect();
                    format!("{callee_str}({})", arg_strs.join(", "))
                }
                Expr::Object {
                    type_name, fields, ..
                } => {
                    let tn = type_name
                        .as_ref()
                        .map(|n| qualify_type_name(n, prefix, local_type_names))
                        .unwrap_or_else(|| "_".into());
                    let field_strs: Vec<String> = fields
                        .iter()
                        .map(|(name, val)| {
                            format!(
                                "{name}: {}",
                                expr_desc_hir(*val, body, prefix, local_type_names)
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
                        .map(|(k, v)| {
                            format!(
                                "{}: {}",
                                expr_desc_hir(*k, body, prefix, local_type_names),
                                expr_desc_hir(*v, body, prefix, local_type_names)
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
                Expr::FieldAccess { base, field } => {
                    format!(
                        "{}.{field}",
                        expr_desc_hir(*base, body, prefix, local_type_names)
                    )
                }
                Expr::Index { base, index } => format!(
                    "{}[{}]",
                    expr_desc_hir(*base, body, prefix, local_type_names),
                    expr_desc_hir(*index, body, prefix, local_type_names)
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
                Stmt::Let {
                    pattern,
                    type_annotation,
                    initializer,
                    ..
                } => {
                    let pat = pat_desc_hir(*pattern, body, prefix, local_type_names);
                    let ty_annot = type_annotation
                        .map(|id| {
                            format!(
                                ": {}",
                                type_expr_to_string_hir(&body.type_annotations[id], prefix)
                            )
                        })
                        .unwrap_or_default();
                    let init = initializer
                        .map(|e| format!(" = {}", expr_desc_hir(e, body, prefix, local_type_names)))
                        .unwrap_or_default();
                    format!("let {pat}{ty_annot}{init}")
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
                Stmt::Assert { condition } => {
                    format!(
                        "assert {}",
                        expr_desc_hir(*condition, body, prefix, local_type_names)
                    )
                }
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

        let item_tree = file_item_tree(db, file);

        let mut local_type_names = std::collections::HashSet::new();
        for (_, class) in &item_tree.classes {
            local_type_names.insert(class.name.as_str());
        }
        for (_, enum_def) in &item_tree.enums {
            local_type_names.insert(enum_def.name.as_str());
        }
        for (_, ta) in &item_tree.type_aliases {
            local_type_names.insert(ta.name.as_str());
        }

        let mut classes: Vec<_> = item_tree.classes.iter().collect();
        classes.sort_by_key(|(_, c)| c.name.as_str().to_string());
        for (id, class) in classes {
            let _loc = ClassLoc::new(db, file, *id);
            writeln!(output, "class {prefix}{} {{", class.name).ok();
            for field in &class.fields {
                let ty = field
                    .type_expr
                    .as_ref()
                    .map(|te| type_expr_to_string_hir(&te.expr, &prefix))
                    .unwrap_or_else(|| "?".into());
                writeln!(output, "  {}: {}", field.name, ty).ok();
            }
            writeln!(output, "}}").ok();
        }

        let mut enums: Vec<_> = item_tree.enums.iter().collect();
        enums.sort_by_key(|(_, e)| e.name.as_str().to_string());
        for (id, enum_def) in enums {
            let _loc = EnumLoc::new(db, file, *id);
            write!(output, "enum {prefix}{} {{", enum_def.name).ok();
            for (i, v) in enum_def.variants.iter().enumerate() {
                if i > 0 {
                    write!(output, ", ").ok();
                }
                write!(output, "{}", v.name).ok();
            }
            writeln!(output, "}}").ok();
        }

        let mut type_aliases: Vec<_> = item_tree.type_aliases.iter().collect();
        type_aliases.sort_by_key(|(_, ta)| ta.name.as_str().to_string());
        for (id, ta) in type_aliases {
            let _loc = TypeAliasLoc::new(db, file, *id);
            let ty = ta
                .type_expr
                .as_ref()
                .map(|te| type_expr_to_string_hir(&te.expr, &prefix))
                .unwrap_or_else(|| "?".into());
            writeln!(output, "type {prefix}{} = {}", ta.name, ty).ok();
        }

        let mut functions: Vec<_> = item_tree.functions.iter().collect();
        functions.sort_by_key(|(_, f)| f.name.as_str().to_string());
        for (_, func) in functions {
            let params: Vec<String> = func
                .params
                .iter()
                .map(|p| {
                    let ty = p
                        .type_expr
                        .as_ref()
                        .map(|te| type_expr_to_string_hir(&te.expr, &prefix))
                        .unwrap_or_else(|| "?".into());
                    format!("{}: {}", p.name, ty)
                })
                .collect();
            let ret = func
                .return_type
                .as_ref()
                .map(|te| type_expr_to_string_hir(&te.expr, &prefix))
                .unwrap_or_else(|| "?".into());
            let body_kind = if func.declarative_meta.is_some() {
                "llm"
            } else {
                match &func.body {
                    Some(baml_compiler2_ast::FunctionBodyDef::Expr(_, _)) => "expr",
                    Some(baml_compiler2_ast::FunctionBodyDef::Builtin(_)) => "builtin",
                    None => "missing",
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
            if let Some(baml_compiler2_ast::FunctionBodyDef::Expr(body, _)) = &func.body {
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

        output
    }

    pub fn make_db() -> ProjectDatabase {
        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("."));
        db
    }
}
