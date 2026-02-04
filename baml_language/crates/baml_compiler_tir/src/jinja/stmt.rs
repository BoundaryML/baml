//! Statement validation for Jinja templates.
//!
//! This module performs type checking on Jinja statements (control flow),
//! including:
//! - Variable assignment and scoping
//! - For loop iteration with proper type narrowing
//! - If/elif/else branching with type guards
//! - Type narrowing based on predicates
//!
//! Ported from `engine/baml-lib/jinja/src/evaluate_type/stmt.rs`.

use minijinja::machinery::ast;

use super::{JinjaType, JinjaTypeEnv, LiteralValue, TypeError, infer_expression_type};

/// Validate a Jinja statement and track variable types through control flow.
///
/// This is the main entry point for statement validation.
pub fn validate_statement(stmt: &ast::Stmt, env: &mut JinjaTypeEnv) -> Vec<TypeError> {
    let mut errors = Vec::new();
    walk_statement(stmt, env, &mut errors);
    errors
}

/// Walk a statement tree, tracking types and collecting errors.
fn walk_statement(stmt: &ast::Stmt, env: &mut JinjaTypeEnv, errors: &mut Vec<TypeError>) {
    match stmt {
        ast::Stmt::Template(template_stmt) => {
            // Process template children in a new scope
            env.push_scope();
            for child in &template_stmt.children {
                walk_statement(child, env, errors);
            }
            env.pop_scope();
        }

        ast::Stmt::EmitExpr(emit_expr) => {
            // Validate the expression being emitted
            match infer_expression_type(&emit_expr.expr, env) {
                Ok(_) => {}
                Err(expr_errors) => errors.extend(expr_errors),
            }
        }

        ast::Stmt::EmitRaw(_) => {
            // Raw text emission - no validation needed
        }

        ast::Stmt::ForLoop(for_stmt) => {
            // Infer the type of the iterable
            let iter_type = match infer_expression_type(&for_stmt.iter, env) {
                Ok(ty) => match ty {
                    JinjaType::List(elem_type) => *elem_type,
                    JinjaType::Map(key_type, _) => *key_type,
                    _ => JinjaType::Unknown,
                },
                Err(expr_errors) => {
                    errors.extend(expr_errors);
                    JinjaType::Unknown
                }
            };

            // Validate filter expression if present
            if let Some(filter_expr) = &for_stmt.filter_expr {
                match infer_expression_type(filter_expr, env) {
                    Ok(_) => {}
                    Err(expr_errors) => errors.extend(expr_errors),
                }
            }

            // Create a new scope for the loop body
            env.push_scope();

            // Add the loop variable to the scope
            match &for_stmt.target {
                ast::Expr::Var(var) => {
                    env.add_variable(var.id, iter_type);
                }
                ast::Expr::List(list) => {
                    // Tuple unpacking in for loop
                    match iter_type {
                        JinjaType::List(elem_type) => {
                            // Each item gets the same type
                            for item in &list.items {
                                if let ast::Expr::Var(var) = item {
                                    env.add_variable(var.id, *elem_type.clone());
                                }
                            }
                        }
                        JinjaType::Tuple(item_types) => {
                            if list.items.len() != item_types.len() {
                                errors.push(TypeError {
                                    message: format!("Expected {} items", item_types.len()),
                                    span: list.span(),
                                });
                            } else {
                                for (item, item_type) in list.items.iter().zip(item_types.iter()) {
                                    if let ast::Expr::Var(var) = item {
                                        env.add_variable(var.id, item_type.clone());
                                    }
                                }
                            }
                        }
                        _ => {
                            // Unknown iteration type, add unknowns
                            for item in &list.items {
                                if let ast::Expr::Var(var) = item {
                                    env.add_variable(var.id, JinjaType::Unknown);
                                }
                            }
                        }
                    }
                }
                _ => {
                    errors.push(TypeError {
                        message: "Invalid for loop target".to_string(),
                        span: for_stmt.span(),
                    });
                }
            }

            // Add special `loop` variable
            env.push_scope();
            // TODO: Define a proper loop object type
            env.add_variable("loop", JinjaType::Unknown);

            // Process loop body
            for body_stmt in &for_stmt.body {
                walk_statement(body_stmt, env, errors);
            }
            env.pop_scope();

            // Process else body in a separate scope
            env.push_scope();
            for else_stmt in &for_stmt.else_body {
                walk_statement(else_stmt, env, errors);
            }
            env.pop_scope();

            env.pop_scope();
        }

        ast::Stmt::IfCond(if_stmt) => {
            // Validate the condition expression
            match infer_expression_type(&if_stmt.expr, env) {
                Ok(_) => {}
                Err(expr_errors) => errors.extend(expr_errors),
            }

            // Compute type narrowing implications
            let true_implications = predicate_implications(&if_stmt.expr, env, true);
            let false_implications = predicate_implications(&if_stmt.expr, env, false);

            // True branch with type narrowing
            env.push_scope();
            for (var_name, narrowed_type) in true_implications {
                env.add_variable(var_name, narrowed_type);
            }
            for true_stmt in &if_stmt.true_body {
                walk_statement(true_stmt, env, errors);
            }
            env.pop_scope();

            // False/else branch with type narrowing
            env.push_scope();
            for (var_name, narrowed_type) in false_implications {
                env.add_variable(var_name, narrowed_type);
            }
            for false_stmt in &if_stmt.false_body {
                walk_statement(false_stmt, env, errors);
            }
            env.pop_scope();
        }

        ast::Stmt::WithBlock(with_stmt) => {
            // TODO: Implement with block validation
            errors.push(TypeError {
                message: "With blocks are not yet supported".to_string(),
                span: with_stmt.span(),
            });
        }

        ast::Stmt::Set(set_stmt) => {
            // Validate the expression being assigned
            let expr_type = match infer_expression_type(&set_stmt.expr, env) {
                Ok(ty) => ty,
                Err(expr_errors) => {
                    errors.extend(expr_errors);
                    JinjaType::Unknown
                }
            };

            // Add the variable to the current scope
            if let ast::Expr::Var(var) = &set_stmt.target {
                env.add_variable(var.id, expr_type);
            } else {
                errors.push(TypeError {
                    message: "Invalid set target".to_string(),
                    span: set_stmt.span(),
                });
            }
        }

        ast::Stmt::SetBlock(set_block_stmt) => {
            // Validate the target expression
            match infer_expression_type(&set_block_stmt.target, env) {
                Ok(_) => {}
                Err(expr_errors) => errors.extend(expr_errors),
            }

            // Validate filter if present
            if let Some(filter) = &set_block_stmt.filter {
                match infer_expression_type(filter, env) {
                    Ok(_) => {}
                    Err(expr_errors) => errors.extend(expr_errors),
                }
            }

            // Process block body
            for body_stmt in &set_block_stmt.body {
                walk_statement(body_stmt, env, errors);
            }
        }

        ast::Stmt::AutoEscape(autoescape_stmt) => {
            // TODO: Implement autoescape validation
            errors.push(TypeError {
                message: "Autoescape blocks are not yet supported".to_string(),
                span: autoescape_stmt.span(),
            });
        }

        ast::Stmt::FilterBlock(filter_stmt) => {
            // TODO: Implement filter block validation
            errors.push(TypeError {
                message: "Filter blocks are not yet supported".to_string(),
                span: filter_stmt.span(),
            });
        }

        ast::Stmt::Macro(_) => {
            // Macros are not validated for now
        }

        ast::Stmt::CallBlock(call_stmt) => {
            // TODO: Implement call block validation
            errors.push(TypeError {
                message: "Call blocks are not yet supported".to_string(),
                span: call_stmt.span(),
            });
        }

        ast::Stmt::Do(do_stmt) => {
            // TODO: Implement do statement validation
            errors.push(TypeError {
                message: "Do statements are not yet supported".to_string(),
                span: do_stmt.span(),
            });
        }
    }
}

// ============================================================================
// Type Narrowing (Simplified Implementation)
// ============================================================================

/// Compute type implications from a predicate expression.
///
/// For example, if `x: Number | null` and the predicate is `x`,
/// then when the predicate is true, we can narrow `x` to `Number`.
///
/// This analyzes conditions like:
/// - `if variable` -> narrows nullable unions
/// - `if variable.kind == "value"` -> narrows discriminated unions
/// - `if a and b` -> combines implications
/// - `if a or b` -> merges implications
fn predicate_implications(
    expr: &ast::Expr,
    env: &JinjaTypeEnv,
    branch: bool,
) -> Vec<(String, JinjaType)> {
    match expr {
        // Simple variable reference: `if x` narrows nullable types
        ast::Expr::Var(var) => {
            if let Some(var_type) = env.resolve_variable(var.id) {
                if let Some(truthy) = truthy_type(&var_type) {
                    if branch {
                        vec![(var.id.to_string(), truthy)]
                    } else {
                        vec![(var.id.to_string(), JinjaType::None)]
                    }
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        }

        // Unary NOT: flip the branch
        ast::Expr::UnaryOp(unary_op) => match unary_op.op {
            ast::UnaryOpKind::Not => predicate_implications(&unary_op.expr, env, !branch),
            ast::UnaryOpKind::Neg => predicate_implications(&unary_op.expr, env, branch),
        },

        // Binary operations
        ast::Expr::BinOp(bin_op) => {
            match bin_op.op {
                // AND: both conditions must be true
                ast::BinOpKind::ScAnd => {
                    let mut left = predicate_implications(&bin_op.left, env, branch);
                    let right = predicate_implications(&bin_op.right, env, branch);
                    left.extend(right);
                    left
                }

                // OR: at least one condition is true
                ast::BinOpKind::ScOr => {
                    if branch {
                        // For OR being true: merge implications by creating unions
                        let left = predicate_implications(&bin_op.left, env, true);
                        let right = predicate_implications(&bin_op.right, env, true);

                        let mut merged = std::collections::HashMap::new();
                        for (var_name, var_type) in left {
                            merged.insert(var_name, var_type);
                        }
                        for (var_name, var_type) in right {
                            merged
                                .entry(var_name)
                                .and_modify(|existing| {
                                    *existing =
                                        merge_types(vec![existing.clone(), var_type.clone()]);
                                })
                                .or_insert(var_type);
                        }
                        merged.into_iter().collect()
                    } else {
                        // For OR being false: both must be false (like AND on false branches)
                        let mut left = predicate_implications(&bin_op.left, env, false);
                        let right = predicate_implications(&bin_op.right, env, false);
                        left.extend(right);
                        left
                    }
                }

                // != null: narrows nullable types
                ast::BinOpKind::Ne => {
                    let maybe_var = match (&bin_op.left, &bin_op.right) {
                        (ast::Expr::Var { .. }, ast::Expr::Const(c)) if is_null_const(c) => {
                            Some(&bin_op.left)
                        }
                        (ast::Expr::Const(c), ast::Expr::Var { .. }) if is_null_const(c) => {
                            Some(&bin_op.right)
                        }
                        _ => None,
                    };

                    if let Some(var_expr) = maybe_var {
                        predicate_implications(var_expr, env, branch)
                    } else {
                        vec![]
                    }
                }

                // == "literal": attribute-based union narrowing
                ast::BinOpKind::Eq => {
                    if !branch {
                        return vec![];
                    }

                    // Must be exactly `var.attr == "literal"`
                    let (get_attr, const_expr) = match (&bin_op.left, &bin_op.right) {
                        (ast::Expr::GetAttr(attr), ast::Expr::Const(c)) => (attr, c),
                        (ast::Expr::Const(c), ast::Expr::GetAttr(attr)) => (attr, c),
                        _ => return vec![],
                    };

                    // Extract variable name
                    let ast::Expr::Var(var) = &get_attr.expr else {
                        return vec![];
                    };

                    // Perform attribute-based narrowing
                    narrow_attr_access_on_union_var(var, get_attr, const_expr, env)
                }

                _ => vec![],
            }
        }

        _ => vec![],
    }
}

/// Narrow a union type based on attribute value comparison.
///
/// For example: `if message.type == "user"` where message is `UserMessage | AssistantMessage`
/// narrows message to `UserMessage` in the true branch.
fn narrow_attr_access_on_union_var(
    var: &ast::Spanned<ast::Var>,
    get_attr: &ast::Spanned<ast::GetAttr>,
    const_expr: &ast::Spanned<ast::Const>,
    env: &JinjaTypeEnv,
) -> Vec<(String, JinjaType)> {
    let Some(var_type) = env.resolve_variable(var.id) else {
        return vec![];
    };

    // Extract union items
    let union_items = match &var_type {
        JinjaType::Union(items) => items,
        JinjaType::Alias { resolved, .. } => match resolved.as_ref() {
            JinjaType::Union(items) => items,
            _ => return vec![],
        },
        _ => return vec![],
    };

    let mut matched_types = Vec::new();
    let mut all_have_attr = true;

    for union_item in union_items {
        match union_item {
            JinjaType::ClassRef(class_name) => {
                // Check if this class has the attribute
                if let Some(attr_type) = env.get_class_property(class_name, get_attr.name) {
                    // Check if the attribute matches the literal value
                    if let (JinjaType::Literal(LiteralValue::String(lit)), Some(val)) =
                        (&attr_type, const_expr.value.as_str())
                    {
                        if lit == val {
                            matched_types.push(union_item.clone());
                        }
                    }
                } else {
                    all_have_attr = false;
                }
            }
            JinjaType::Union(_nested) => {
                // Recursively check nested unions
                // Simplified: don't recurse for now
                all_have_attr = false;
            }
            JinjaType::Alias { resolved: _, .. } => {
                // Would need to resolve aliases
                all_have_attr = false;
            }
            _ => {
                all_have_attr = false;
            }
        }
    }

    // Only narrow if all union members have the attribute and exactly one matches
    if all_have_attr && matched_types.len() == 1 {
        vec![(var.id.to_string(), matched_types[0].clone())]
    } else {
        vec![]
    }
}

/// Check if a constant is null/none.
fn is_null_const(c: &ast::Spanned<ast::Const>) -> bool {
    c.value.to_string().to_lowercase() == "none"
}

/// Merge multiple types into a union.
fn merge_types(types: Vec<JinjaType>) -> JinjaType {
    let mut result: Option<JinjaType> = None;

    for ty in types {
        result = Some(match result {
            None => ty,
            Some(prev) => {
                if ty == prev {
                    prev
                } else {
                    match prev {
                        JinjaType::Union(mut items) => {
                            items.push(ty);
                            JinjaType::Union(items)
                        }
                        _ => JinjaType::Union(vec![prev, ty]),
                    }
                }
            }
        });
    }

    result.unwrap_or(JinjaType::Unknown)
}

/// Check if a type becomes narrower when truthy.
///
/// For example, `Number | null` becomes `Number` when truthy.
#[allow(dead_code)]
fn truthy_type(ty: &JinjaType) -> Option<JinjaType> {
    match ty {
        JinjaType::Union(variants) => {
            let non_nullish: Vec<_> = variants
                .iter()
                .filter(|v| !matches!(v, JinjaType::None | JinjaType::Undefined))
                .cloned()
                .collect();

            match non_nullish.len() {
                0 => None,
                1 => Some(non_nullish[0].clone()),
                _ => Some(JinjaType::Union(non_nullish)),
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truthy_type_nullable() {
        let ty = JinjaType::Union(vec![JinjaType::Int, JinjaType::None]);
        let narrowed = truthy_type(&ty);
        assert_eq!(narrowed, Some(JinjaType::Int));
    }

    #[test]
    fn test_truthy_type_non_nullable() {
        let ty = JinjaType::Int;
        let narrowed = truthy_type(&ty);
        assert_eq!(narrowed, None); // No narrowing possible
    }
}
