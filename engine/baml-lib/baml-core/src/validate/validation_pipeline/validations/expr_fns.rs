use std::{collections::HashSet, mem::MaybeUninit};

use internal_baml_ast::ast::{
    AssertStmt, ClassConstructorField, Expression, LetStmt, ReturnStmt, Stmt, WithName, WithSpan,
};
use internal_baml_diagnostics::{DatamodelError, DatamodelWarning};
use itertools::Itertools;

use crate::{
    ir,
    validate::validation_pipeline::{
        context::Context,
        validations::{
            identifiers::validate_identifier_not_keyword, reserved_names::baml_keywords,
        },
    },
};

/// Builtin functions.
///
/// TODO: Define this somewhere else like their own baml.baml file or something,
/// but we don't have modules yet.
fn baml_prelude() -> HashSet<String> {
    let builtin_functions = [ir::builtin::functions::FETCH_AS];

    let builtin_classes = [ir::builtin::classes::REQUEST];

    HashSet::from_iter(
        builtin_functions
            .iter()
            .chain(builtin_classes.iter())
            .map(ToString::to_string),
    )
}

/// Validate that the left-hand side of an assignment refers to a declared variable
/// when it is a bare identifier. This is used to enforce that C-style for-loop
/// headers either declare their iterator with `let` in the init statement, or
/// use a variable already declared in the containing scope.
fn validate_assign_lhs_in_scope(ctx: &mut Context<'_>, left: &Expression, scope: &HashSet<String>) {
    if let Expression::Identifier(identifier) = left {
        if !scope.contains(&identifier.to_string()) {
            ctx.push_error(DatamodelError::new_anyhow_error(
                anyhow::anyhow!("Unknown variable {}", &identifier.to_string()),
                identifier.span().clone(),
            ));
        }
    }
}

// An expr_fn is valid if:
//   - Its parameters have valid types.
//   - Its parameter names are not reserved keywords.
//   - Its return type is valid.
//   - Its body is a valid function body (series of statements ending in an
//     expression). Bodies are valid if they refer only to variables defined
//     in the parameter list and in the current scope.
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
        // Only show experimental warning if beta features are NOT enabled
        if !ctx.feature_flags().is_beta_enabled() {
            ctx.push_warning(DatamodelWarning::new(
                "Workflow functions are experimental, and will break in the future.".to_string(),
                expr_fn.name_span().clone(),
            ));
        }
        if taken_names.contains(expr_fn.name()) {
            ctx.push_error(DatamodelError::new_validation_error(
                "Expr function name must be unique",
                expr_fn.name_span().clone(),
            ));
        }
        taken_names.insert(expr_fn.name().to_owned());
    }

    // Expression validation is now handled by HIR-based typechecking in the validation pipeline
    // Only keep the experimental warnings for toplevel assignments
    for expr_fn in ctx.db.walk_expr_fns() {
        let mut scope: HashSet<String> = expr_fn
            .expr_fn()
            .args
            .args
            .iter()
            .map(|(arg_name, _arg)| arg_name.to_string())
            .collect();

        // Check for reserved keywords in argument names.
        for arg_name in expr_fn
            .expr_fn()
            .args
            .args
            .iter()
            .map(|(arg_name, _arg)| arg_name.to_string())
        {
            if baml_keywords().contains(arg_name.as_str()) {
                ctx.push_error(DatamodelError::new_validation_error(
                    &format!("'{arg_name}' is a reserved keyword."),
                    expr_fn.expr_fn().span.clone(),
                ));
            }
        }

        scope.insert("true".to_string());
        scope.insert("false".to_string());

        scope.extend(taken_names.iter().cloned());
        expr_fn.expr_fn().body.stmts.iter().for_each(|s| {
            validate_stmt(ctx, s, &scope);
            match s {
                Stmt::Let(_) => {
                    scope.insert(s.identifier().name().to_string());
                }
                Stmt::ForLoop(fl) => {
                    // Only treat as declaration if header included `let`
                    if fl.has_let {
                        scope.insert(fl.identifier.name().to_string());
                    }
                }
                _ => {}
            }
        });
        if let Some(expr) = &expr_fn.expr_fn().body.expr {
            validate_expression(ctx, expr, &scope);
        }
    }

    for toplevel_assignment in ctx.db.walk_toplevel_assignments() {
        // Only show experimental warning if beta features are NOT enabled
        if !ctx.feature_flags().is_beta_enabled() {
            ctx.push_warning(DatamodelWarning::new(
                "Variable assignment is experimental, and will break in the future.".to_string(),
                toplevel_assignment.expr().span().clone(),
            ));
        }

        // Create a scope for toplevel assignments that includes all taken names
        let scope = taken_names.clone();
        validate_expression(ctx, toplevel_assignment.expr(), &scope);
    }
}

fn validate_stmt(ctx: &mut Context<'_>, stmt: &Stmt, scope: &HashSet<String>) {
    match stmt {
        Stmt::WhileLoop(stmt) => {
            validate_expression(ctx, &stmt.condition, scope);

            validate_expr_block(ctx, &stmt.body, scope.clone());
        }
        Stmt::Assign(stmt) => {
            // re: validation is handled by HIR-based typechecking.
            validate_expression(ctx, &stmt.expr, scope);
        }
        Stmt::AssignOp(stmt) => {
            // re: validation is handled by HIR-based typechecking.
            validate_expression(ctx, &stmt.expr, scope);
        }
        Stmt::Let(stmt) => {
            validate_identifier_not_keyword(ctx, &stmt.identifier);

            validate_expression(ctx, &stmt.expr, scope);
        }
        Stmt::ForLoop(stmt) => {
            // First validate the iterator expression
            validate_expression(ctx, &stmt.iterator, scope);

            // Create loop scope. If `let` is present, introduce the loop variable.
            // Otherwise, require it to already exist in the outer scope.
            let mut loop_scope = scope.clone();
            if stmt.has_let {
                loop_scope.insert(stmt.identifier.name().to_string());
            } else if !scope.contains(&stmt.identifier.name().to_string()) {
                ctx.push_error(DatamodelError::new_anyhow_error(
                    anyhow::anyhow!("Unknown variable {}", &stmt.identifier.to_string()),
                    stmt.identifier.span().clone(),
                ));
            }

            let body = &stmt.body;
            validate_expr_block(ctx, body, loop_scope);
        }
        Stmt::Expression(es) => {
            validate_expression(ctx, &es.expr, scope);
        }
        Stmt::Semicolon(expr) => {
            validate_expression(ctx, &expr.expr, scope);
        }
        Stmt::Break(_) | Stmt::Continue(_) => {}
        Stmt::CForLoop(stmt) => {
            // we have to clone the scope anyway for the inner expression block.
            let mut loop_scope = scope.clone();

            if let Some(init) = &stmt.init_stmt {
                validate_stmt(ctx, init, scope);

                let init: &Stmt = init;

                if let Stmt::Let(LetStmt { identifier, .. }) = init {
                    loop_scope.insert(identifier.to_string());
                }

                // If init is an assignment without declaration, ensure the LHS is declared
                // in the containing scope. This enforces `for (let i = ...)` unless `i` is
                // already declared outside.
                match init {
                    Stmt::Assign(assign) => {
                        validate_assign_lhs_in_scope(ctx, &assign.left, scope);
                    }
                    Stmt::AssignOp(assign_op) => {
                        validate_assign_lhs_in_scope(ctx, &assign_op.left, scope);
                    }
                    _ => {}
                }
            }

            // validate the condition & after statement in the loop header's scope:
            // bindings declared inside the loop header are available, things from inside the loop
            // body aren't.

            if let Some(condition) = &stmt.condition {
                validate_expression(ctx, condition, &loop_scope);
            }

            if let Some(after) = &stmt.after_stmt {
                // For `i += 1` (or similar) in the after-statement, ensure `i` is declared
                match after.as_ref() {
                    Stmt::Assign(assign) => {
                        validate_assign_lhs_in_scope(ctx, &assign.left, &loop_scope);
                    }
                    Stmt::AssignOp(assign_op) => {
                        validate_assign_lhs_in_scope(ctx, &assign_op.left, &loop_scope);
                    }
                    _ => {}
                }
                validate_stmt(ctx, after, &loop_scope);
            }

            validate_expr_block(ctx, &stmt.body, loop_scope);
        }
        Stmt::Return(ReturnStmt { value, .. }) | Stmt::Assert(AssertStmt { value, .. }) => {
            validate_expression(ctx, value, scope);
        }
        Stmt::WatchNotify(_) => {}
        Stmt::WatchOptions(_) => {}
    }
}

fn validate_expr_block(
    ctx: &mut Context<'_>,
    body: &internal_baml_ast::ast::ExpressionBlock,
    mut scope_for_block: HashSet<String>,
) {
    for stmt in &body.stmts {
        validate_stmt(ctx, stmt, &scope_for_block);
        match stmt {
            Stmt::Let(_) => {
                scope_for_block.insert(stmt.identifier().name().to_string());
            }
            Stmt::ForLoop(fl) => {
                if fl.has_let {
                    scope_for_block.insert(fl.identifier.name().to_string());
                }
            }
            _ => {}
        }
    }

    if let Some(expr) = &body.expr {
        validate_expression(ctx, expr, &scope_for_block);
    }
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
        Expression::ExprBlock(block, _span) => {
            validate_expr_block(ctx, block, scope.clone());
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
