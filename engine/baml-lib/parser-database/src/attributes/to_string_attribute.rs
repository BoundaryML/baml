use crate::types::{DynamicStringAttributes, StaticStringAttributes};
use crate::{context::Context, types::ToStringAttributes};

use super::alias::visit_alias_attribute;
use super::description::visit_description_attribute;
use super::get::visit_get_attribute;
use super::meta::visit_meta_attribute;

pub(super) fn visit(ctx: &mut Context<'_>, as_block: bool) -> Option<ToStringAttributes> {
    let mut modified = false;

    if !as_block && ctx.visit_optional_single_attr("get") {
        let mut attributes = DynamicStringAttributes::default();
        visit_get_attribute(&mut attributes, ctx);
        ctx.validate_visited_arguments();
        Some(ToStringAttributes::Dynamic(attributes))
    } else {
        let mut attributes = StaticStringAttributes::default();
        // @alias or @@alias
        if ctx.visit_optional_single_attr("alias") {
            visit_alias_attribute(&mut attributes, ctx);
            modified = true;
            ctx.validate_visited_arguments();
        }

        // Only inner blocks can have meta/skip attributes.
        if !as_block {
            // @meta
            while ctx.visit_repeated_attr("meta") {
                visit_meta_attribute(&mut attributes, ctx, as_block);
                modified = true;
                ctx.validate_visited_arguments();
            }

            if ctx.visit_optional_single_attr("description") {
                visit_description_attribute(&mut attributes, ctx);
                modified = true;
                ctx.validate_visited_arguments();
            }

            // @skip
            if ctx.visit_optional_single_attr("skip") {
                attributes.set_skip(true);
                modified = true;
                ctx.validate_visited_arguments();
            }
        } else {
            if ctx.visit_optional_single_attr("dynamic") {
                attributes.set_dynamic_type();
                modified = true;
                ctx.validate_visited_arguments();
            }
        }

        if modified {
            Some(ToStringAttributes::Static(attributes))
        } else {
            None
        }
    }
}
