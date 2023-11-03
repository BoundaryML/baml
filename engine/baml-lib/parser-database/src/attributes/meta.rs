use crate::{coerce, context::Context, types::StaticStringAttributes};

pub(super) fn visit_meta_attribute(
    attributes: &mut StaticStringAttributes,
    ctx: &mut Context<'_>,
    as_block: bool,
) {
    let name = match ctx
        .visit_default_arg_with_idx("name")
        .map(|(_, value)| coerce::string(value, ctx.diagnostics))
    {
        Ok(Some(name)) => ctx.interner.intern(name),
        Ok(None) => return,
        Err(err) => {
            ctx.push_error(err);
            return;
        }
    };

    let value = match ctx
        .visit_default_arg_with_idx("value")
        .map(|(_, value)| coerce::string(value, ctx.diagnostics))
    {
        Ok(Some(name)) => ctx.interner.intern(name),
        Ok(None) => {
            ctx.push_attribute_validation_error("Missing value for meta attribute.", as_block);
            return;
        }
        Err(err) => {
            ctx.push_error(err);
            return;
        } // not flattened for error handing legacy reasons
    };

    if !attributes.add_meta(name, value) {
        ctx.push_attribute_validation_error("Duplicate meta attribute.", as_block);
    }
}
