use crate::{context::Context, types::StaticStringAttributes};

pub(super) fn visit_description_attribute(
    attributes: &mut StaticStringAttributes,
    ctx: &mut Context<'_>,
) {
    match ctx
        .visit_default_arg_with_idx("description")
        .map(|(_, value)| value)
    {
        Ok(description) => {
            if attributes.description().is_some() {
                ctx.push_attribute_validation_error("Duplicate description attribute.", true);
            } else {
                attributes.add_description(description.clone())
            }
        }
        Err(err) => ctx.push_error(err), // not flattened for error handing legacy reasons
    };
}
