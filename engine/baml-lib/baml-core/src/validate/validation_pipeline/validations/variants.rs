use internal_baml_diagnostics::{DatamodelError};

use internal_baml_schema_ast::ast::{WithIdentifier, WithName, WithSpan};

use crate::validate::validation_pipeline::context::Context;

pub(super) fn validate(ctx: &mut Context<'_>) {
    for variant in ctx.db.walk_variants() {
        let client = &variant.properties().client;

        if variant.client().is_none() {
            ctx.push_error(DatamodelError::new_validation_error(
                &format!("Unknown client `{}`", client.value),
                client.span.clone(),
            ));
        }

        if let Some(_function) = variant.walk_function() {
            // Ensure that every serializer is valid.
            variant.ast_variant().iter_serializers().for_each(|(_, f)| {
                match ctx.db.find_type(f.identifier()) {
                    Some(_) => {}
                    None => {
                        ctx.push_error(DatamodelError::new_validation_error(
                            &format!("Unknown serializer `{}`", f.identifier().name()),
                            f.identifier().span().clone(),
                        ));
                    }
                }
            });
        } else {
            ctx.push_error(DatamodelError::new_type_not_found_error(
                variant.function_identifier().name(),
                ctx.db.valid_function_names(),
                variant.function_identifier().span().clone(),
            ));
        }
    }
}
