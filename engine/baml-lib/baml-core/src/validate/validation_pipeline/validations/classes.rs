use crate::validate::validation_pipeline::context::Context;

use super::types::validate_type;

pub(super) fn validate(ctx: &mut Context<'_>) {
    for cls in ctx.db.walk_classes() {
        let _ast_class = cls.ast_class();

        for c in cls.static_fields() {
            let field = c.ast_field();
            if let Some(ft) = &field.expr {
                validate_type(ctx, &ft);
            }
        }
        for c in cls.dynamic_fields() {
            let field = c.ast_field();
            if let Some(ft) = &field.expr {
                validate_type(ctx, &ft);
            }
        }
    }
}
