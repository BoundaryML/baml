use baml_types::{Constraint, ConstraintLevel};
use internal_baml_schema_ast::ast::Expression;

use crate::{context::Context, types::Attributes};

pub(super) fn visit_constraint_attributes(
    attribute_name: String,
    attributes: &mut Attributes,
    ctx: &mut Context<'_>,
) {
    let expression_arg = ctx.visit_default_arg_with_idx("expression").map_err(|err| {
        ctx.push_error(err);
    });
    let label = ctx.visit_default_arg_with_idx("name");
    let label = match label {
        Ok((_, Expression::StringValue(descr, _))) => Some(descr.clone()),
        _ => None,
    };
    match expression_arg {
        Ok((_, Expression::JinjaExpressionValue(expression, _))) => {
            let level = match attribute_name.as_str() {
                "assert" => ConstraintLevel::Assert,
                "check" => ConstraintLevel::Check,
                _ => {
                    panic!("Internal error: Only \"assert\" and \"check\" are valid attribute names in this context.");
                }
            };
            attributes.constraints.push(Constraint {
                level,
                expression: expression.clone(),
                label,
            });
        }
        _ => panic!(
            "The impossible happened: Reached arguments that are ruled out by the tokenizer."
        ),
    }
}
