use internal_baml_schema_ast::ast::FieldType;

use crate::validate::validation_pipeline::context::Context;

pub(super) fn validate(ctx: &mut Context<'_>) {
    for cls in ctx.db.walk_clients() {
        let ast_client = cls.ast_client();

        let mut provider_exists = false;

        ast_client.iter_fields().for_each(|field| {
            if &field.1.name.name == "provider" {
                provider_exists = true;
            }
        });

        if !provider_exists {
            let error =
                internal_baml_diagnostics::DatamodelError::new_missing_required_property_error(
                    "provider",
                    cls.name(),
                    ast_client.span.clone(),
                );
            ctx.push_error(error);
        }
    }
}
