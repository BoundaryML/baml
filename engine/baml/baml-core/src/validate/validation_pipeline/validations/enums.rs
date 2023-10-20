use crate::validate::validation_pipeline::context::Context;

pub(super) fn validate(ctx: &mut Context<'_>) {
    // Name validation is done in the parser. See ../../parser-database/src/names.rs
    //
    // No further validation is needed.
}
