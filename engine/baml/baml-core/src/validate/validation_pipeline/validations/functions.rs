use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::{FieldType, FuncArguementId};

use crate::validate::validation_pipeline::context::Context;

pub(super) fn validate(ctx: &mut Context<'_>) {
    for cls in ctx.db.walk_functions() {
        for input_args in cls.walk_input_args() {
            let arg = input_args.ast_arg();

            if let FieldType::Supported(identifier) = &arg.1.field_type {
                ctx.db.find_class(&identifier.name).map_or_else(
                    || {
                        let error = DatamodelError::new_validation_error(
                            "Hi there",
                            identifier.span.clone(),
                        );
                        ctx.push_error(error.clone());
                    },
                    |_| {},
                );
            }
        }
    }
}
