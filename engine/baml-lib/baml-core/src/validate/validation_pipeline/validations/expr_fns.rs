use std::collections::HashSet;

use internal_baml_ast::ast::WithName;
use internal_baml_diagnostics::{DatamodelError, DatamodelWarning};

use crate::{
    ir, validate::validation_pipeline::context::Context,
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

    // Expression validation is now handled by HIR-based typechecking in the validation pipeline
    // Only keep the experimental warnings for toplevel assignments
    for toplevel_assignment in ctx.db.walk_toplevel_assignments() {
        ctx.push_warning(DatamodelWarning::new(
            "Variable assignment is experimental, and will break in the future.".to_string(),
            toplevel_assignment.expr().span().clone(),
        ));
    }
}

