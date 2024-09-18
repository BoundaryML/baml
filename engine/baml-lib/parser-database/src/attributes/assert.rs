use internal_baml_schema_ast::ast::Expression;

use crate::{context::Context, types::StaticStringAttributes};

pub(super) fn visit_assert_or_check_attributes(
    attribute_name: &'static str,
    attributes: &mut StaticStringAttributes,
    ctx: &mut Context<'_>,
) {
    let arg1 = ctx.visit_default_arg_with_idx("expression").map_err(|err| {
        ctx.push_error(err);
    });
    let arg2 = ctx.visit_default_arg_with_idx("name");
    let assert_name = match arg2 {
        Ok((_, Expression::StringValue(descr,_))) => descr,
        _ => attribute_name,
    };
    match arg1 {
        Ok((_,expr@Expression::JinjaExpression(_,_))) => {
            match attribute_name {
                "assert" => {
                    attributes.add_assert(assert_name.to_string(), expr.clone());
                },
                "check" => {
                    attributes.add_check(assert_name.to_string(), expr.clone());
                }
                _ => {
                    panic!("Internal error: Only \"assert\" and \"check\" are valid attribute names in this context.");
                }
            }
        },
        _ => panic!("The impossible happened: Reached arguments that are ruled out by the tokenizer."),
    }
}
