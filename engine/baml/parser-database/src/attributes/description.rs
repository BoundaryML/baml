use crate::{coerce, context::Context, types::StaticStringAttributes};

pub(super) fn visit_description_attribute(
    attributes: &mut StaticStringAttributes,
    ctx: &mut Context<'_>,
) {
    match ctx
        .visit_default_arg_with_idx("description")
        .map(|(_, value)| coerce::string(value, ctx.diagnostics))
    {
        Ok(Some(name)) => {
            if !attributes.add_meta(
                ctx.interner.intern("description"),
                ctx.interner.intern(name),
            ) {
                ctx.push_attribute_validation_error("Duplicate meta attribute.", true);
            }
        }
        Err(err) => ctx.push_error(err), // not flattened for error handing legacy reasons
        Ok(None) => (),
    };
}
