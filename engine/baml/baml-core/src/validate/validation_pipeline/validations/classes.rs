use internal_baml_diagnostics::DatamodelWarning;
use internal_baml_schema_ast::ast::{FieldType, WithAttributes};

use crate::{internal_baml_parser_database, validate::validation_pipeline::context::Context};
use std::collections::HashSet;

pub(super) fn validate(ctx: &mut Context<'_>) {
    for cls in ctx.db.walk_classes() {
        let ast_class = cls.ast_class();

        for c in cls.static_fields() {
            let field = c.ast_field();

            if let FieldType::Supported(identifier) = &field.field_type {
                ctx.db.find_class(&identifier.name).map_or_else(
                    || {
                        let error =
                            internal_baml_diagnostics::DatamodelError::new_type_not_found_error(
                                &identifier.name,
                                identifier.span.clone(),
                            );
                        ctx.push_error(error.clone());
                    },
                    |_| {},
                );
            }
        }
        for c in cls.dynamic_fields() {
            let field = c.ast_field();
        }
    }
}
