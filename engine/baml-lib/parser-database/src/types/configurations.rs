use std::{collections::HashSet, ops::Deref};

use baml_types::{Constraint, UnresolvedValue};
use internal_baml_ast::ast::{
    Attribute, ValExpId, ValueExprBlock, WithIdentifier, WithName, WithSpan,
};
use internal_baml_diagnostics::{DatamodelError, DatamodelWarning, Span};
use regex::Regex;

use super::{
    Attributes, ContantDelayStrategy, ExponentialBackoffStrategy, RetryPolicy, RetryPolicyStrategy,
};
use crate::{
    attributes::constraint::attribute_as_constraint, coerce, coerce_array,
    coerce_expression::coerce_map, context::Context,
};

fn dedent(s: &str) -> String {
    // Find the shortest indentation in the string (that's not an empty line).
    let shortest_indent = Regex::new(r"^(?m)\s*\S")
        .unwrap()
        .captures_iter(s.trim())
        .map(|cap| cap.get(0).unwrap().start())
        .min()
        .unwrap_or(0);

    if shortest_indent == 0 {
        return s.trim().to_string();
    }

    // Remove that amount of indentation from each line.
    let dedent_pattern = format!(r"(?m)^\s{{1,{shortest_indent}}}");
    Regex::new(&dedent_pattern)
        .unwrap()
        .replace_all(s, "")
        .trim()
        .to_string()
}

pub(crate) fn visit_retry_policy<'db>(
    idx: ValExpId,
    config: &'db ValueExprBlock,
    ctx: &mut Context<'db>,
) {
    let mut max_reties = None;

    let mut strategy = Some(RetryPolicyStrategy::ConstantDelay(
        super::ContantDelayStrategy { delay_ms: 200 },
    ));
    let mut options = None;

    config
        .iter_fields()
        .for_each(|(_idx, f)| match (f.name(), &f.expr) {
            (name, None) => {
                ctx.push_error(DatamodelError::new_config_property_missing_value_error(
                    name,
                    config.name(),
                    "retry_policy",
                    f.identifier().span().clone(),
                ))
            }
            ("max_retries", Some(val)) => {
                if let Some(val) = coerce::integer(val, ctx.diagnostics) {
                    max_reties = Some(val as u32)
                }
            }
            ("strategy", Some(val)) => {
                if let Some(val) = coerce_map(val, &coerce::string_with_span, ctx.diagnostics) {
                    if let Some(val) = visit_strategy(f.span(), val, ctx.diagnostics) {
                        strategy = Some(val)
                    }
                }
            }
            ("options", Some(val)) => match val.to_unresolved_value(ctx.diagnostics) {
                Some(UnresolvedValue::<Span>::Map(kv, _)) => options = Some(kv),
                Some(other) => {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "`options` must be a map",
                        other.meta().clone(),
                    ));
                }
                None => {}
            },
            (name, Some(_)) => ctx.push_error(DatamodelError::new_property_not_known_error(
                name,
                f.identifier().span().clone(),
                ["max_retries", "strategy", "options"].to_vec(),
            )),
        });
    match (max_reties, strategy) {
        (Some(max_retries), Some(strategy)) => {
            ctx.types.retry_policies.insert(
                idx,
                RetryPolicy {
                    max_retries,
                    strategy,
                    options,
                },
            );
        }
        (Some(_), None) => {
            unreachable!("max_retries is set but strategy is not");
        }
        (None, Some(_)) => ctx.push_error(DatamodelError::new_validation_error(
            "Missing `max_reties` property",
            config.identifier().span().clone(),
        )),
        (None, None) => ctx.push_error(DatamodelError::new_validation_error(
            "Missing `strategy` property",
            config.identifier().span().clone(),
        )),
    }
}

fn visit_strategy(
    field_span: &Span,
    val: Vec<((&str, &Span), &internal_baml_ast::ast::Expression)>,
    diagnostics: &mut internal_baml_diagnostics::Diagnostics,
) -> Option<RetryPolicyStrategy> {
    let mut r#type = None;
    let mut delay_ms = None;
    let mut max_delay_ms = None;
    let mut multiplier = None;

    val.iter()
        .for_each(|(name_and_span, val)| match name_and_span.0 {
            "type" => {
                if let Some(val) = coerce::string_with_span(val, diagnostics) {
                    r#type = Some(val)
                }
            }
            "delay_ms" => {
                if let Some(val) = coerce::integer(val, diagnostics) {
                    delay_ms = Some(val)
                }
            }
            "max_delay_ms" => {
                if let Some(_val) = coerce::integer(val, diagnostics) {
                    max_delay_ms = Some((_val, val.span()))
                }
            }
            "multiplier" => {
                if let Some(_val) = coerce::float(val, diagnostics) {
                    multiplier = Some((_val, val.span()))
                }
            }
            _ => {}
        });

    match r#type {
        Some(("constant_delay", _)) => {
            if let Some((_, span)) = multiplier {
                diagnostics.push_error(
                internal_baml_diagnostics::DatamodelError::new_validation_error(
                    "The `multiplier` option is not supported for the `constant_delay` strategy",
                    span.clone(),
                ),
            )
            }
            if let Some((_, span)) = max_delay_ms {
                diagnostics.push_error(
                internal_baml_diagnostics::DatamodelError::new_validation_error(
                    "The `max_delay_ms` option is not supported for the `constant_delay` strategy",
                    span.clone(),
                ),
            )
            }
            Some(RetryPolicyStrategy::ConstantDelay(ContantDelayStrategy {
                delay_ms: delay_ms.unwrap_or(200) as u32,
            }))
        }
        Some(("exponential_backoff", _)) => Some(RetryPolicyStrategy::ExponentialBackoff(
            ExponentialBackoffStrategy {
                delay_ms: delay_ms.unwrap_or(200) as u32,
                multiplier: multiplier.map(|(v, _)| v as f32).unwrap_or(1.5),
                max_delay_ms: max_delay_ms.map(|(v, _)| v as u32).unwrap_or(10000),
            },
        )),
        Some((name, span)) => {
            diagnostics.push_error(
                internal_baml_diagnostics::DatamodelError::new_validation_error(
                    &format!("Unknown retry strategy type: {name}. Options are `constant_delay` or `exponential_backoff`"),
                    span.clone(),
                ),
            );
            None
        }
        None => {
            diagnostics.push_error(
                internal_baml_diagnostics::DatamodelError::new_missing_required_property_error(
                    "type",
                    "strategy",
                    field_span.clone(),
                ),
            );
            None
        }
    }
}

// TODO: Are test cases "configurations"?
pub(crate) fn visit_test_case<'db>(
    idx: ValExpId,
    config: &'db ValueExprBlock,
    ctx: &mut Context<'db>,
) {
    let mut functions = None;
    let mut args = None;

    config
        .iter_fields()
        .for_each(|(_idx, f)| match (f.name(), &f.expr) {
            (name, None) => {
                ctx.push_error(DatamodelError::new_config_property_missing_value_error(
                    name,
                    config.name(),
                    "printer",
                    f.identifier().span().clone(),
                ))
            }
            ("function", Some(val)) => {
                if functions.is_some() {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "Duplicate `function` property",
                        f.identifier().span().clone(),
                    ));
                } else if let Some((t, span)) = coerce::string_with_span(val, ctx.diagnostics) {
                    functions = Some(vec![(t.to_string(), span.clone())])
                }
            }
            ("functions", Some(val)) => {
                if functions.is_some() {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "Duplicate `functions` property",
                        f.identifier().span().clone(),
                    ));
                } else if let Some(val) =
                    coerce_array(val, &coerce::string_with_span, ctx.diagnostics)
                {
                    functions = Some(
                        val.iter()
                            .map(|&(t, span)| (t.to_string(), span.clone()))
                            .collect::<Vec<_>>(),
                    );
                }
            }
            ("args", Some(val)) => match val.to_unresolved_value(ctx.diagnostics) {
                Some(UnresolvedValue::<Span>::Map(kv, span)) => args = Some((span, kv)),
                Some(other) => {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "`args` must be a map",
                        other.meta().clone(),
                    ));
                }
                None => {}
            },
            (name, Some(_)) => ctx.push_error(DatamodelError::new_property_not_known_error(
                name,
                f.identifier().span().clone(),
                ["functions", "args"].to_vec(),
            )),
        });

    let constraints: Vec<(Constraint, Span, Span)> = config
        .attributes
        .iter()
        .filter_map(|attribute| {
            let (maybe_constraint, errors) = attribute_as_constraint(attribute);
            for error in errors {
                ctx.push_error(error);
            }
            maybe_constraint
        })
        .collect();

    match (functions, args) {
        (None, _) => ctx.push_error(DatamodelError::new_validation_error(
            "Missing `functions` property",
            config.identifier().span().clone(),
        )),
        (Some(_function_name), None) => ctx.push_error(DatamodelError::new_validation_error(
            "Missing `args` property",
            config.identifier().span().clone(),
        )),
        (Some(functions), Some((args_field_span, args))) => {
            ctx.types.test_cases.insert(
                idx,
                super::TestCase {
                    functions,
                    args,
                    args_field_span: args_field_span.clone(),
                    constraints,
                    type_builder: config.type_builder.clone(),
                    type_builder_scoped_db: Default::default(),
                },
            );
        }
    }
}
