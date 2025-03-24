use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::WithName;

use crate::validate::validation_pipeline::context::Context;

// An expr_fn is valid if:
//   - Its arguments have valid types.
//   - Its return type is valid.
//   - Its body is a valid function body (series of statements ending in an
//     expression). Bodies are valid if they refer only to variables defined
//     in the argument list and in the current scope.
//   - It does not share a name with any other expr_fn or LLM function.
pub(super) fn validate_expr_fn(ctx: &mut Context<'_> ) {
    let mut defined_types = internal_baml_jinja_types::PredefinedTypes::default(
        internal_baml_jinja_types::JinjaContext::Prompt,
    );

    let mut taken_names = std::collections::HashSet::new();
    ctx.db.walk_classes().for_each(|class| {
        class.add_to_types(&mut defined_types);
        taken_names.insert(class.name().to_owned() );
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
    }
}
