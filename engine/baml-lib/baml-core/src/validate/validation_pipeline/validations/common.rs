use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::{FieldType, Identifier, WithName, WithSpan};

use crate::validate::validation_pipeline::context::Context;

fn errors_with_names<'a>(ctx: &'a mut Context<'_>, idn: &Identifier) {
    // Push the error with the appropriate message
    ctx.push_error(DatamodelError::new_type_not_found_error(
        idn.name(),
        ctx.db.valid_type_names(),
        idn.span().clone(),
    ));
}

pub(crate) fn validate_type_exists(ctx: &mut Context<'_>, field_type: &FieldType) {
    field_type
        .flat_idns()
        .iter()
        .for_each(|f| match ctx.db.find_type(f) {
            Some(_) => {}
            None => match f {
                Identifier::Primitive(..) => {}
                _ => errors_with_names(ctx, f),
            },
        });
}
