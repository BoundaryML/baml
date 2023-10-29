use crate::validate::validation_pipeline::context::Context;



use super::common::validate_type_exists;

pub(super) fn validate(ctx: &mut Context<'_>) {
    for cls in ctx.db.walk_functions() {
        for args in cls.walk_input_args().chain(cls.walk_output_args()) {
            let arg = args.ast_arg();
            validate_type_exists(ctx, &arg.1.field_type)
        }
    }
}
