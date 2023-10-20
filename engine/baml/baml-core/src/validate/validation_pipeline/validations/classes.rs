use crate::{internal_baml_parser_database, validate::validation_pipeline::context::Context};
use std::collections::HashSet;

pub(super) fn validate(ctx: &mut Context<'_>) {
    for cls in ctx.db.walk_classes() {
        let ast_class = cls.ast_class();
        for c in cls.static_fields() {
            let field = c.ast_field();
        }
        for c in cls.dynamic_fields() {
            let field = c.ast_field();
        }
    }
}
