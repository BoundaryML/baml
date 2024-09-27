use crate::{coerce, context::Context, types::Attributes};

pub(super) fn visit_alias_attribute(attributes: &mut Attributes, ctx: &mut Context<'_>) {
    match ctx
        .visit_default_arg_with_idx("alias")
        .map(|(_, value)| coerce::string(value, ctx.diagnostics))
    {
        Ok(Some(name)) => attributes.add_alias(ctx.interner.intern(name)),
        Err(err) => ctx.push_error(err), // not flattened for error handing legacy reasons
        Ok(None) => (),
    };
}
