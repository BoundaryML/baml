


use crate::{validate::validation_pipeline::context::Context};


use super::common::validate_type_exists;

pub(super) fn validate(ctx: &mut Context<'_>) {
    for cls in ctx.db.walk_classes() {
        let _ast_class = cls.ast_class();

        for c in cls.static_fields() {
            let field = c.ast_field();
            validate_type_exists(ctx, &field.field_type)
        }
        for c in cls.dynamic_fields() {
            let _field = c.ast_field();
        }
    }
}
