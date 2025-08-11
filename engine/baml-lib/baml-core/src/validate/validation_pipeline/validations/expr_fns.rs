use std::collections::{HashMap, HashSet};

use internal_baml_ast::ast::WithSpan;
use internal_baml_ast::ast::{
    ClassConstructorField, Expression, ExpressionBlock, LetStmt, Stmt, WithName,
};
use internal_baml_diagnostics::{DatamodelError, DatamodelWarning};
use itertools::Itertools;

use crate::{ir, validate::validation_pipeline::context::Context};

/// Builtin functions.
///
/// TODO: Define this somewhere else like their own std.baml file or something,
/// but we don't have modules yet.
fn baml_prelude() -> HashSet<String> {
    let builtin_functions = [ir::builtin::functions::FETCH_VALUE];

    let builtin_classes = [ir::builtin::classes::REQUEST];

    builtin_functions
        .into_iter()
        .chain(builtin_classes)
        .map(ToString::to_string)
        .collect()
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

    taken_names.insert("true".to_string());
    taken_names.insert("false".to_string());

    // Expression validation is now handled by HIR-based typechecking in the validation pipeline
    // Only keep the experimental warnings for toplevel assignments
    for expr_fn in ctx.db.walk_expr_fns() {
        // NOTE: (Jesus) perf of this is hideous. string clones + hashset clones (inside
        // validate_*).

        // start by declaring all top-level as non-mutable
        let top_level = taken_names.iter().cloned().map(|arg| (arg, false));

        // then declare arguments, so that they shadow the globals if required.
        let arg_local = expr_fn
            .expr_fn()
            .args
            .args
            .iter()
            .map(|(name, arg)| (name.to_string(), arg.is_mutable));

        let mut scope: Scope = top_level.chain(arg_local).collect();

        validate_expr_block(ctx, &expr_fn.expr_fn().body, scope);
    }

    {
        let scope: Scope = taken_names
            .iter()
            .cloned()
            .map(|name| (name, false))
            .collect();
        for toplevel_assignment in ctx.db.walk_toplevel_assignments() {
            ctx.push_warning(DatamodelWarning::new(
                "Variable assignment is experimental, and will break in the future.".to_string(),
                toplevel_assignment.expr().span().clone(),
            ));

            validate_expression(ctx, toplevel_assignment.expr(), &scope);
        }
    }
}

fn validate_stmt(ctx: &mut Context<'_>, stmt: &Stmt, scope: &Scope) {
    match stmt {
        Stmt::Assign(stmt) => {
            validate_expression(ctx, &stmt.expr, scope);

            let var_name = stmt.identifier.name();
            match scope.get(var_name) {
                Some(true) => {}
                Some(false) => ctx.diagnostics.push_error(DatamodelError::new_anyhow_error(
                    anyhow::format_err!(
                    "'{var_name}' is not assignable. Perhaps you meant to declare it with `let mut`?"),
                    stmt.span.clone(),
                )),
                None => ctx.diagnostics.push_error(DatamodelError::new_anyhow_error(
                    anyhow::format_err!("cannot resolve '{var_name}' to a variable"),
                    stmt.identifier.span().clone(),
                )),
            }
        }
        Stmt::Let(stmt) => {
            validate_expression(ctx, &stmt.expr, scope);
        }
        Stmt::ForLoop(stmt) => {
            // First validate the iterator expression
            validate_expression(ctx, &stmt.iterator, scope);

            // Create a new scope that includes the loop variable
            let mut loop_scope = scope.clone();
            loop_scope.insert(stmt.identifier.name().to_string(), false);

            validate_expr_block(ctx, &stmt.body, loop_scope);
        }
        Stmt::Expression(expr) => {
            validate_expression(ctx, expr, scope);
        }
    }
}

fn validate_expr_block(
    ctx: &mut Context<'_>,
    body: &ExpressionBlock,
    mut scope_for_block: HashMap<String, bool>,
) {
    // Validate statements in the loop body
    for stmt in &body.stmts {
        validate_stmt(ctx, stmt, &scope_for_block);

        insert_var_if_declared(&mut scope_for_block, stmt);
    }

    // Validate the loop body expression
    if let Some(expr) = body.expr.as_ref() {
        validate_expression(ctx, expr, &scope_for_block);
    }
}

fn insert_var_if_declared(scope: &mut HashMap<String, bool>, stmt: &Stmt) {
    if let Stmt::Let(LetStmt {
        identifier,
        is_mutable,
        ..
    }) = &stmt
    {
        scope.insert(identifier.name().to_string(), *is_mutable);
    }
}

// NOTE: (Jesus) ideally this should be a (newtyped) Vec<HashMap>, (or two sets for
// immutable/mutable vars).
// Also a good want would be a reference to where the binding has been defined so that we can
// helpfully point to the defn.
type Scope = HashMap<String, bool>;

fn validate_expression(ctx: &mut Context<'_>, expr: &Expression, scope: &Scope) {
    match &expr {
        Expression::Identifier(identifier) => {
            if !scope.contains_key(identifier.name()) {
                ctx.push_error(DatamodelError::new_anyhow_error(
                    anyhow::anyhow!("Unknown variable {}", &identifier.to_string()),
                    identifier.span().clone(),
                ));
            }
        }
        Expression::Lambda(_args, _body, _span) => {}
        Expression::App(app) => {
            // Validate the function name.
            if !scope.contains_key(app.name.name()) {
                ctx.push_error(DatamodelError::new_anyhow_error(
                    anyhow::anyhow!("Unknown function {}", &app.name.to_string()),
                    app.span().clone(),
                ));
            }

            // Validate generics.
            if ir::builtin::is_builtin_identifier(app.name.name()) && app.type_args.is_empty() {
                ctx.push_error(DatamodelError::new_anyhow_error(
                    anyhow::anyhow!(
                        "Generic function {} must have a type argument. Try adding a type argument like this: {}<Type>",
                        app.name.name(),
                        app.name.name()
                    ),
                    app.span().clone(),
                ));
            }
            for arg in &app.args {
                validate_expression(ctx, arg, scope);
            }
        }
        Expression::Array(items, _span) => {
            for item in items {
                validate_expression(ctx, item, scope);
            }
        }
        Expression::Map(fields, _span) => {
            for (_key, value) in fields {
                validate_expression(ctx, value, scope);
            }
        }
        Expression::BoolValue(_, _span) => {}
        Expression::StringValue(_, _) => {}
        Expression::NumericValue(_, _) => {}
        Expression::RawStringValue(_) => {}
        Expression::JinjaExpressionValue(_, _) => {}
        Expression::ClassConstructor(cc, span) => {
            let fields = cc.fields.clone();

            if fields.iter().len()
                != fields
                    .iter()
                    .map(|f| format!("{f:?}"))
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
                    ClassConstructorField::Named(_field_name, _value) => {}
                    ClassConstructorField::Spread(expr) => {
                        validate_expression(ctx, expr, scope);
                    }
                }
            }
        }
        Expression::ExprBlock(block, span) => {
            let scope = scope.clone();

            validate_expr_block(ctx, &block, scope);
        }
        Expression::If(cond, then, else_, _span) => {
            validate_expression(ctx, cond, scope);
            validate_expression(ctx, then, scope);
            if let Some(else_) = else_ {
                validate_expression(ctx, else_, scope);
            }
        }
        _ => {} // Handle other expression variants
    }
}
