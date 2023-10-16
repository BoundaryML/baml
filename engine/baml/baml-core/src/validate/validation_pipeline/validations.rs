mod enums;

use super::context::Context;

pub(super) fn validate(ctx: &mut Context<'_>) {
    enums::database_name_clashes(ctx);
}
