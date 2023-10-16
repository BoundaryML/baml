use crate::{
    internal_baml_diagnostics::DatamodelError,
    internal_baml_parser_database::{ast::WithSpan, walkers::EnumWalker},
    validate::validation_pipeline::context::Context,
};
use std::collections::HashSet;

pub(super) fn database_name_clashes(ctx: &mut Context<'_>) {
    let mut database_names: HashSet<&str> = HashSet::with_capacity(ctx.db.enums_count());

    for enm in ctx.db.walk_enums() {
        if !database_names.insert(enm.name()) {
            ctx.push_error(DatamodelError::new_duplicate_enum_database_name_error(
                enm.ast_enum().span(),
            ));
        }
    }
}
