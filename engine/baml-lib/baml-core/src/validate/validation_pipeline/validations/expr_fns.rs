use std::collections::HashSet;

use internal_baml_ast::ast::{
    ClassConstructor, ClassConstructorField, Expression, Stmt, WithName, WithSpan,
};
use internal_baml_diagnostics::{DatamodelError, DatamodelWarning};
use itertools::Itertools;

use crate::{
    ir, ir::builtin::is_builtin_identifier, validate::validation_pipeline::context::Context,
};

/// Builtin functions.
///
/// TODO: Define this somewhere else like their own std.baml file or something,
/// but we don't have modules yet.
fn baml_prelude() -> HashSet<String> {
    let builtin_functions = [ir::builtin::functions::FETCH_VALUE];

    let builtin_classes = [ir::builtin::classes::REQUEST];

    HashSet::from_iter(
        builtin_functions
            .iter()
            .chain(builtin_classes.iter())
            .map(ToString::to_string),
    )
}

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

    let mut taken_names = baml_prelude();

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
        ctx.push_warning(DatamodelWarning::new(
            "Workflow functions are experimental, and will break in the future.".to_string(),
            expr_fn.name_span().clone(),
        ));
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
        ctx.push_warning(DatamodelWarning::new(
            "Variable assignment is experimental, and will break in the future.".to_string(),
            toplevel_assignment.expr().span().clone(),
        ));
        validate_stmt(
            ctx,
            &toplevel_assignment.top_level_assignment().stmt,
            &scope,
        );
    }
}

fn validate_stmt(ctx: &mut Context<'_>, stmt: &Stmt, scope: &HashSet<String>) {
    validate_expression(ctx, &stmt.body, scope);
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
        Expression::App(app) => {
            // Validate the function name.
            if !scope.contains(app.name.name()) {
                ctx.push_error(DatamodelError::new_anyhow_error(
                    anyhow::anyhow!("Unknown function {}", &app.name.to_string()),
                    app.span().clone(),
                ));
            }

            // Validate generics.
            if is_builtin_identifier(app.name.name()) {
                if app.type_args.len() == 0 {
                    ctx.push_error(DatamodelError::new_anyhow_error(
                        anyhow::anyhow!(
                            "Generic function {} must have a type argument. Try adding a type argument like this: {}<Type>",
                            app.name.name(),
                            app.name.name()
                        ),
                        app.span().clone(),
                    ));
                }
            }
            for arg in &app.args {
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
            let fields = cc.fields.clone();

            if fields.iter().len()
                != fields
                    .iter()
                    .map(|f| format!("{:?}", f))
                    .dedup()
                    .collect::<Vec<_>>()
                    .len()
            {
                ctx.push_error(DatamodelError::new_validation_error(
                    "Class constructor fields must be unique",
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

            for field in &cc.fields {
                match field {
                    ClassConstructorField::Named(field_name, value) => {}
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
        Expression::If(cond, then, else_, span) => {
            validate_expression(ctx, cond, scope);
            validate_expression(ctx, then, scope);
            if let Some(else_) = else_ {
                validate_expression(ctx, else_, scope);
            }
        }
        Expression::ForLoop {
            identifier,
            iterator,
            body,
            span,
        } => {
            validate_expression(ctx, iterator, scope);
            let mut body_scope = scope.clone();
            body_scope.insert(identifier.to_string());
            for stmt in body.stmts.iter() {
                validate_stmt(ctx, stmt, &mut body_scope);
                validate_expression(ctx, &stmt.body, &body_scope);
            }
        }
    }
}
