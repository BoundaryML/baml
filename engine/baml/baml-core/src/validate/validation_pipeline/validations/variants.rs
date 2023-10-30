use internal_baml_diagnostics::{DatamodelError, DatamodelWarning};

use crate::{ast::WithSpan, validate::validation_pipeline::context::Context};

pub(super) fn validate(ctx: &mut Context<'_>) {
    for variant in ctx.db.walk_variants() {
        let client = &variant.properties().client;

        if ctx.db.find_client(client).is_none() {
            ctx.push_error(DatamodelError::new_validation_error(
                &format!("Unknown client `{}`", client),
                variant.ast_variant().span().clone(),
            ));
        }

        let span = &variant.properties().prompt.1;
        let (input, output) = &variant.properties().replacers;
        // Validation already done that the prompt is valid.
        // We should just ensure that atleast one of the input or output replacers is used.

        if input.len() == 0 {
            ctx.push_warning(DatamodelWarning::prompt_variable_unused(
                "Never uses {#input}",
                span.clone(),
            ));
        }

        if output.len() == 0 {
            ctx.push_warning(DatamodelWarning::prompt_variable_unused(
                "Never uses {#print_type(..)} or {#print_enum(..)}",
                span.clone(),
            ));
        }
    }
}
