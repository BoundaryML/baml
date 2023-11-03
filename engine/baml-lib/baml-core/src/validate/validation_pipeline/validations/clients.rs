use internal_baml_schema_ast::ast::WithName;

use crate::validate::validation_pipeline::context::Context;

pub(super) fn validate(ctx: &mut Context<'_>) {
    // required props are already validated in visit_client. No other validations here.
}
