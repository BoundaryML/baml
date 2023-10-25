use internal_baml_schema_ast::ast::FieldType;

use crate::validate::validation_pipeline::context::Context;

pub(crate) fn validate_type_exists(ctx: &mut Context<'_>, field_type: &FieldType) {
    if let FieldType::Supported(identifier) = &field_type {
        if ctx.db.find_class(&identifier.name).is_none() {
            let error = internal_baml_diagnostics::DatamodelError::new_type_not_found_error(
                &identifier.name,
                identifier.span.clone(),
            );
            ctx.push_error(error);
        }
    }
}
