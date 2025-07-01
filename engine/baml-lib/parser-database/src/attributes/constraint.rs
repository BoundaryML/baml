use baml_types::{Constraint, ConstraintLevel};
use internal_baml_ast::ast::{Argument, Attribute, Expression};
use internal_baml_diagnostics::{DatamodelError, Span};

use crate::{context::Context, types::Attributes};

/// Interpret an attribute as a constraint, the whole constraint's span,
/// and the span of the constraint's jinja expression.
pub fn attribute_as_constraint(
    attribute: &Attribute,
) -> (Option<(Constraint, Span, Span)>, Vec<DatamodelError>) {
    let span = attribute.span.clone();
    let mut datamodel_errors = Vec::new();
    let attribute_name = attribute.name.to_string();
    let arguments: Vec<Expression> = attribute
        .arguments
        .arguments
        .iter()
        .map(|Argument { value, .. }| value)
        .cloned()
        .collect();

    let level = match attribute_name.as_str() {
        "assert" => ConstraintLevel::Assert,
        "check" => ConstraintLevel::Check,
        _ => {
            return (None, datamodel_errors);
        }
    };

    let (label, expression, expr_span) = match arguments.as_slice() {
        [Expression::JinjaExpressionValue(expression, expr_span)] => {
            if level == ConstraintLevel::Check {
                datamodel_errors.push(DatamodelError::new_attribute_validation_error(
                    "Checks must specify a label.",
                    attribute_name.as_str(),
                    span.clone(),
                ));
            }
            (None, expression.clone(), expr_span.clone())
        }
        [Expression::Identifier(label), Expression::JinjaExpressionValue(expression, expr_span)] => {
            (
                Some(label.to_string()),
                expression.clone(),
                expr_span.clone(),
            )
        }
        _ => {
            datamodel_errors.push(
                DatamodelError::new_attribute_validation_error(
                    "Checks and asserts may have either a label and an expression, or a lone expression.",
                    attribute_name.as_str(),
                    span
                )
            );
            return (None, datamodel_errors);
        }
    };
    let constraint = Constraint {
        label,
        expression,
        level,
    };
    (Some((constraint, span, expr_span)), datamodel_errors)
}

pub(super) fn visit_constraint_attributes(
    attribute_name: String,
    span: Span,
    attributes: &mut Attributes,
    ctx: &mut Context<'_>,
) {
    let arguments: Vec<&Expression> = ctx
        .get_all_args()
        .into_iter()
        .map(|(_arg_id, arg)| arg)
        .collect();

    let level = match attribute_name.as_str() {
        "assert" => ConstraintLevel::Assert,
        "check" => ConstraintLevel::Check,
        other_name => {
            ctx.push_error(DatamodelError::new_attribute_validation_error(
                "Internal error - the parser should have ruled out other attribute names.",
                other_name,
                span,
            ));
            return;
        }
    };

    let (label, expression) = match arguments.as_slice() {
        [Expression::JinjaExpressionValue(expression, _)] => {
            if level == ConstraintLevel::Check {
                ctx.push_error(DatamodelError::new_attribute_validation_error(
                    "Checks must specify a label.",
                    attribute_name.as_str(),
                    span,
                ));
            }
            (None, expression.clone())
        }
        [Expression::Identifier(label), Expression::JinjaExpressionValue(expression, _)] => {
            (Some(label.to_string()), expression.clone())
        }
        _ => {
            ctx.push_error(
                DatamodelError::new_attribute_validation_error(
                    "Checks and asserts may have either a label and an expression, or a lone expression.",
                    attribute_name.as_str(),
                    span
                )
            );
            return;
        }
    };

    attributes.constraints.push(Constraint {
        level,
        expression,
        label,
    });
}
