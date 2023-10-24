use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::ConfigBlockProperty;

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
    }
}
