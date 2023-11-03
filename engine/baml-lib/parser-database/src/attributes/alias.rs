use crate::{coerce, context::Context, types::StaticStringAttributes};

pub(super) fn visit_alias_attribute(
    attributes: &mut StaticStringAttributes,
    ctx: &mut Context<'_>,
) {
    match ctx
        .visit_default_arg_with_idx("alias")
        .map(|(_, value)| coerce::string(value, ctx.diagnostics))
    {
        Ok(Some(name)) => attributes.add_alias(ctx.interner.intern(name)),
        Err(err) => ctx.push_error(err), // not flattened for error handing legacy reasons
        Ok(None) => (),
    };
}
