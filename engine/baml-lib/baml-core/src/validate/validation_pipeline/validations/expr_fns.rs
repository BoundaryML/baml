use std::collections::HashSet;

use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::expr;
use internal_baml_schema_ast::ast::Expression;
use internal_baml_schema_ast::ast::WithName;

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
        validate_expr(ctx, &expr_fn.expr_fn().body.expr, &scope);
    }
}

fn validate_stmt(ctx: &mut Context<'_>, stmt: &expr::Stmt, scope: &HashSet<String>) {
    // Make a copy of the scope above, for augmenting an passing down.
    let mut scope_names = scope.clone();
    for sub_stmt in stmt.body.stmts.iter() {
        validate_stmt(ctx, sub_stmt, &scope_names);
        scope_names.insert(sub_stmt.identifier.name().to_owned());
    }

    // Validate the expression.
    validate_expr(ctx, &stmt.body.expr, &scope_names);
}

fn validate_expr(ctx: &mut Context<'_>, expr: &expr::ExprWithSpan, scope: &HashSet<String>) {
    match &expr.expr {
        expr::Expr::Atom(Expression::Identifier(name)) => {
            if !scope.contains(&name.to_string()) {
                ctx.push_error(DatamodelError::new_anyhow_error(
                    anyhow::anyhow!("Unknown valiable {}", &name.to_string()),
                    expr.span.clone(),
                ));
            }
        }
        expr::Expr::Atom(_) => {}
        expr::Expr::Lambda(args, body) => {}
        expr::Expr::FnApp(name, args) => {
            // Validate the function name.
            if !scope.contains(&name.to_string()) {
                ctx.push_error(DatamodelError::new_anyhow_error(
                    anyhow::anyhow!("Unknown function {}", &name.to_string()),
                    expr.span.clone(),
                ));
            }
            for arg in args {
                validate_expr(ctx, arg, scope);
            }
        }
    }
}
