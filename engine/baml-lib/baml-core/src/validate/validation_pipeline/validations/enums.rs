use super::types::validate_type;
use crate::validate::validation_pipeline::context::Context;
use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::{WithName, WithSpan};

pub(super) fn validate(ctx: &mut Context<'_>) {
    let mut defined_types = internal_baml_jinja::PredefinedTypes::default();
    for enm in ctx.db.walk_enums() {
        for args in enm.walk_input_args() {
            let arg = args.ast_arg();
            validate_type(ctx, &arg.1.field_type)
        }

        defined_types.start_scope();

        enm.walk_input_args().for_each(|arg| {
            let name = match arg.ast_arg().0 {
                Some(arg) => arg.name(),
                None => {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "Argument name is missing.",
                        arg.ast_arg().1.span().clone(),
                    ));
                    return;
                }
            };

            let field_type = ctx.db.to_jinja_type(&arg.ast_arg().1.field_type);

            defined_types.add_variable(&name, field_type);
        });

        defined_types.end_scope();
        defined_types.errors_mut().clear();
    }
}
