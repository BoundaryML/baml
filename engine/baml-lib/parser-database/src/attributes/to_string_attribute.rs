use crate::types::StaticStringAttributes;
use crate::{context::Context, types::ToStringAttributes};

use super::alias::visit_alias_attribute;
use super::assert::visit_assert_or_check_attributes;
use super::description::visit_description_attribute;

pub(super) fn visit(ctx: &mut Context<'_>, as_block: bool) -> Option<ToStringAttributes> {
    let mut modified = false;

    let mut attributes = StaticStringAttributes::default();

    // @alias or @@alias
    if ctx.visit_optional_single_attr("alias") {
        visit_alias_attribute(&mut attributes, ctx);
        modified = true;
        ctx.validate_visited_arguments();
    }

    if ctx.visit_optional_single_attr("description") {
        visit_description_attribute(&mut attributes, ctx);
        modified = true;
        ctx.validate_visited_arguments();
    }

    if ctx.visit_repeated_attr("assert") {
        visit_assert_or_check_attributes("assert", &mut attributes, ctx);
        modified = true;
        ctx.validate_visited_arguments();
    }

    if ctx.visit_repeated_attr("check") {
        visit_assert_or_check_attributes("check", &mut attributes, ctx);
        modified = true;
        ctx.validate_visited_arguments();
    }

    if modified {
        Some(ToStringAttributes::Static(attributes))
    } else {
        None
    }
}
