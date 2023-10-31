use internal_baml_diagnostics::{DatamodelError, DatamodelWarning};
use internal_baml_parser_database::WithSerialize;
use internal_baml_prompt_parser::ast::WithSpan;
use internal_baml_schema_ast::ast::TopId;

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

        let span = &variant.properties().prompt.key_span;
        let (input, output) = &variant.properties().replacers;
        // Validation already done that the prompt is valid.
        // We should just ensure that atleast one of the input or output replacers is used.
        if input.is_empty() {
            ctx.push_warning(DatamodelWarning::prompt_variable_unused(
                "Never uses {#input}",
                span.clone(),
            ));
        }

        // TODO: We should ensure every enum is used here.
        if output.is_empty() {
            ctx.push_warning(DatamodelWarning::prompt_variable_unused(
                "Never uses {#print_type(..)} or {#print_enum(..)}",
                span.clone(),
            ));
        }
    }
}
