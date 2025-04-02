use std::collections::HashSet;

use internal_baml_diagnostics::DatamodelError;
// use internal_baml_schema_ast::ast::expr;
use internal_baml_schema_ast::ast::{ClassConstructor, ClassConstructorField, Expression, Stmt};
use internal_baml_schema_ast::ast::{WithName, WithSpan};

use crate::validate::validation_pipeline::context::Context;

// An expr_fn is valid if:
//   - Its arguments have valid types.
//   - Its return type is valid.
//   - Its body is a valid function body (series of statements ending in an
//     expression). Bodies are valid if they refer only to variables defined
//     in the argument list and in the current scope.
//   - It does not share a name with any other expr_fn or LLM function.
pub(super) fn validate_expr_fns(ctx: &mut Context<'_>) {
    let mut defined_types = internal_baml_jinja_types::PredefinedTypes::default(
        internal_baml_jinja_types::JinjaContext::Prompt,
    );

    let mut taken_names = std::collections::HashSet::new();
    ctx.db.walk_classes().for_each(|class| {
        class.add_to_types(&mut defined_types);
        taken_names.insert(class.name().to_owned());
    });
    ctx.db.walk_toplevel_assignments().for_each(|assignment| {
        taken_names.insert(assignment.name().to_owned());
    });
    ctx.db.walk_functions().for_each(|function| {
        taken_names.insert(function.name().to_owned());
    });

    for expr_fn in ctx.db.walk_expr_fns() {
        if taken_names.contains(expr_fn.name()) {
            ctx.push_error(DatamodelError::new_validation_error(
                "Expr function name must be unique",
                expr_fn.name_span().clone(),
            ));
        }
        taken_names.insert(expr_fn.name().to_owned());
    }

    for expr_fn in ctx.db.walk_expr_fns() {
        let mut scope: HashSet<String> = expr_fn
            .expr_fn()
            .args
            .args
            .iter()
            .map(|(arg_name, _arg)| arg_name.to_string())
            .collect();

        scope.extend(taken_names.iter().cloned());
        expr_fn.expr_fn().body.stmts.iter().for_each(|s| {
            validate_stmt(ctx, s, &scope);
            scope.insert(s.identifier.name().to_string());
        });
        validate_expression(ctx, &expr_fn.expr_fn().body.expr, &scope);
    }

    for toplevel_assignment in ctx.db.walk_toplevel_assignments() {
        let scope: HashSet<String> = taken_names.clone();
        validate_stmt(
            ctx,
            &toplevel_assignment.top_level_assignment().stmt,
            &scope,
        );
    }
}

fn validate_stmt(ctx: &mut Context<'_>, stmt: &Stmt, scope: &HashSet<String>) {
    // Make a copy of the scope above, for augmenting an passing down.
    let mut scope_names = scope.clone();
    for sub_stmt in stmt.body.stmts.iter() {
        validate_stmt(ctx, sub_stmt, &scope_names);
        scope_names.insert(sub_stmt.identifier.name().to_owned());
    }

    // Validate the expression.
    validate_expression(ctx, &stmt.body.expr, &scope_names);
}

fn validate_expression(ctx: &mut Context<'_>, expr: &Expression, scope: &HashSet<String>) {
    match &expr {
        Expression::Identifier(identifier) => {
            if !scope.contains(&identifier.to_string()) {
                ctx.push_error(DatamodelError::new_anyhow_error(
                    anyhow::anyhow!("Unknown variable {}", &identifier.to_string()),
                    identifier.span().clone(),
                ));
            }
        }
        Expression::Lambda(_args, _body, _span) => {}
        Expression::FnApp(fn_name, args, span) => {
            // Validate the function name.
            if !scope.contains(&fn_name.to_string()) {
                ctx.push_error(DatamodelError::new_anyhow_error(
                    anyhow::anyhow!("Unknown function {}", &fn_name.to_string()),
                    span.clone(),
                ));
            }
            for arg in args {
                validate_expression(ctx, arg, scope);
            }
        }
        Expression::Array(items, span) => {
            for item in items {
                validate_expression(ctx, item, scope);
            }
        }
        Expression::Map(fields, span) => {
            for (_key, value) in fields {
                validate_expression(ctx, value, scope);
            }
        }
        Expression::BoolValue(_, span) => {}
        Expression::StringValue(_, _) => {}
        Expression::NumericValue(_, _) => {}
        Expression::RawStringValue(_) => {}
        Expression::JinjaExpressionValue(_, _) => {}
        Expression::ClassConstructor(cc, span) => {
            let mut fields = cc.fields.clone();
            fields.reverse();
            let last_field = fields.pop();
            fields.reverse();
            if fields
                .iter()
                .any(|field| matches!(field, ClassConstructorField::Spread(_)))
            {
                ctx.push_error(DatamodelError::new_validation_error(
                    "Class constructor can have at most one spread field",
                    span.clone(),
                ));
            }

            let field_names = cc
                .fields
                .iter()
                .filter_map(|field| match field {
                    ClassConstructorField::Named(name, _) => Some(name.to_string()),
                    ClassConstructorField::Spread(_) => None,
                })
                .collect::<Vec<_>>();

            for field in cc.fields.iter() {
                match field {
                    ClassConstructorField::Named(name, value) => {
                        let n_matches = cc
                            .fields
                            .iter()
                            .filter_map(|f| match f {
                                ClassConstructorField::Named(name, _) => {
                                    if name.to_string() == name.to_string() {
                                        Some(())
                                    } else {
                                        None
                                    }
                                }
                                ClassConstructorField::Spread(_) => None,
                            })
                            .count();
                        if n_matches > 1 {
                            ctx.push_error(DatamodelError::new_anyhow_error(
                                anyhow::anyhow!("Duplicate field name: {}", name.to_string()),
                                span.clone(),
                            ));
                        }
                    }
                    ClassConstructorField::Spread(expr) => {
                        validate_expression(ctx, expr, scope);
                    }
                }
            }
        }
        Expression::ExprBlock(block, span) => {
            let mut scope = scope.clone();
            for stmt in block.stmts.iter() {
                validate_stmt(ctx, stmt, &mut scope);
                scope.insert(stmt.identifier.name().to_string());
            }
            validate_expression(ctx, &block.expr, &scope);
        }
    }
}
