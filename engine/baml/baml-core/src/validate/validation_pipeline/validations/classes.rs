use internal_baml_diagnostics::DatamodelWarning;
use internal_baml_schema_ast::ast::{FieldType, WithAttributes};

use crate::{internal_baml_parser_database, validate::validation_pipeline::context::Context};
use std::collections::HashSet;

use super::common::validate_type_exists;

pub(super) fn validate(ctx: &mut Context<'_>) {
    for cls in ctx.db.walk_classes() {
        let ast_class = cls.ast_class();

        for c in cls.static_fields() {
            let field = c.ast_field();
            validate_type_exists(ctx, &field.field_type)
        }
        for c in cls.dynamic_fields() {
            let field = c.ast_field();
        }
    }
}
