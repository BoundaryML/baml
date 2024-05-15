
use internal_baml_diagnostics::{DatamodelError, Span};
use internal_baml_schema_ast::ast::{
    ConfigurationId, PrinterConfig, RetryPolicyConfig, WithIdentifier, WithName, WithSpan,
};
use regex::Regex;

use crate::{coerce, coerce_array, coerce_expression::coerce_map, context::Context};

use super::{
    ContantDelayStrategy, ExponentialBackoffStrategy, Printer, PrinterType, RetryPolicy,
    RetryPolicyStrategy,
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
    let dedent_pattern = format!(r"(?m)^\s{{1,{}}}", shortest_indent);
    Regex::new(&dedent_pattern)
        .unwrap()
        .replace_all(s, "")
        .trim()
        .to_string()
}

pub(crate) fn visit_printer<'db>(
    idx: ConfigurationId,
    config: &'db PrinterConfig,
    ctx: &mut Context<'db>,
) {
    let mut template = None;

    config
        .iter_fields()
        .for_each(|(_idx, f)| match (f.name(), &f.value) {
            (name, None) => {
                ctx.push_error(DatamodelError::new_config_property_missing_value_error(
                    name,
                    config.name(),
                    "printer",
                    f.identifier().span().clone(),
                ))
            }
            ("template", Some(val)) => match coerce::string_with_span(val, ctx.diagnostics) {
                Some((t, span)) => template = Some((dedent(t), span.clone())),
                None => {}
            },
            (name, Some(_)) => ctx.push_error(DatamodelError::new_property_not_known_error(
                name,
                f.identifier().span().clone(),
                ["template"].to_vec(),
            )),
        });

    match (
        template,
        coerce::string_with_span(&config.printer_type, ctx.diagnostics),
    ) {
        (None, _) => ctx.push_error(DatamodelError::new_validation_error(
            "Missing `template` property",
            config.identifier().span().clone(),
        )),
        (Some(template), Some(("enum", _))) => {
            ctx.types
                .printers
                .insert(idx, PrinterType::Enum(Printer { template }));
        }
        (Some(template), Some(("type", _))) => {
            ctx.types
                .printers
                .insert(idx, PrinterType::Type(Printer { template }));
        }
        (Some(_), Some((name, span))) => {
            ctx.push_error(DatamodelError::new_validation_error(
                &format!(
                    "Unknown printer type: {}. Options are `type` or `enum`",
                    name
                ),
                span.clone(),
            ));
        }
        (Some(_), None) => {
            // errors are handled by coerce::string_with_span
        }
    }
}

pub(crate) fn visit_retry_policy<'db>(
    idx: ConfigurationId,
    config: &'db RetryPolicyConfig,
    ctx: &mut Context<'db>,
) {
    let mut max_reties = None;

    let mut strategy = Some(RetryPolicyStrategy::ConstantDelay(
        super::ContantDelayStrategy { delay_ms: 200 },
    ));
    let mut options = None;

    config
        .iter_fields()
        .for_each(|(_idx, f)| match (f.name(), &f.value) {
            (name, None) => {
                ctx.push_error(DatamodelError::new_config_property_missing_value_error(
                    name,
                    config.name(),
                    "retry_policy",
                    f.identifier().span().clone(),
                ))
            }
            ("max_retries", Some(val)) => match coerce::integer(val, ctx.diagnostics) {
                Some(val) => max_reties = Some(val as u32),
                None => {}
            },
            ("strategy", Some(val)) => {
                match coerce_map(val, &coerce::string_with_span, ctx.diagnostics) {
                    Some(val) => match visit_strategy(f.span(), val, ctx.diagnostics) {
                        Some(val) => strategy = Some(val),
                        None => {}
                    },
                    None => {}
                }
            }
            ("options", Some(val)) => {
                match coerce_map(val, &coerce::string_with_span, ctx.diagnostics) {
                    Some(val) => {
                        options = Some(
                            val.iter()
                                .map(|(k, v)| ((k.0.to_string(), k.1.clone()), (*v).clone()))
                                .collect::<Vec<_>>(),
                        );
                    }
                    None => {}
                }
            }
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
    val: Vec<((&str, &Span), &internal_baml_schema_ast::ast::Expression)>,
    diagnostics: &mut internal_baml_diagnostics::Diagnostics,
) -> Option<RetryPolicyStrategy> {
    let mut r#type = None;
    let mut delay_ms = None;
    let mut max_delay_ms = None;
    let mut multiplier = None;

    val.iter()
        .for_each(|(name_and_span, val)| match name_and_span.0 {
            "type" => match coerce::string_with_span(val, diagnostics) {
                Some(val) => r#type = Some(val),
                None => {}
            },
            "delay_ms" => match coerce::integer(val, diagnostics) {
                Some(val) => delay_ms = Some(val),
                None => {}
            },
            "max_delay_ms" => match coerce::integer(val, diagnostics) {
                Some(_val) => max_delay_ms = Some((_val, val.span())),
                None => {}
            },
            "multiplier" => match coerce::float(val, diagnostics) {
                Some(_val) => multiplier = Some((_val, val.span())),
                None => {}
            },
            _ => {}
        });

    match r#type {
        Some(("constant_delay", _)) => {
            match multiplier {
              Some((_, span)) =>
                diagnostics.push_error(
                    internal_baml_diagnostics::DatamodelError::new_validation_error(
                        "The `multiplier` option is not supported for the `constant_delay` strategy",
                        span.clone(),
                    ),
                ),
                None => {}
            }
            match max_delay_ms {
                Some((_, span)) =>
                  diagnostics.push_error(
                      internal_baml_diagnostics::DatamodelError::new_validation_error(
                          "The `max_delay_ms` option is not supported for the `constant_delay` strategy",
                          span.clone(),
                      ),
                  ),
                  None => {}
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
                    &format!("Unknown retry strategy type: {}. Options are `constant_delay` or `exponential_backoff`", name),
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

pub(crate) fn visit_test_case<'db>(
    idx: ConfigurationId,
    config: &'db RetryPolicyConfig,
    ctx: &mut Context<'db>,
) {
    let mut functions = None;
    let mut args = None;

    config
        .iter_fields()
        .for_each(|(_idx, f)| match (f.name(), &f.value) {
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
                } else {
                    match coerce::string_with_span(val, ctx.diagnostics) {
                        Some((t, span)) => functions = Some(vec![(t.to_string(), span.clone())]),
                        None => {}
                    }
                }
            }
            ("functions", Some(val)) => {
                if functions.is_some() {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "Duplicate `functions` property",
                        f.identifier().span().clone(),
                    ));
                } else {
                    match coerce_array(val, &coerce::string_with_span, ctx.diagnostics) {
                        Some(val) => {
                            functions = Some(
                                val.iter()
                                    .map(|&(t, span)| (t.to_string(), span.clone()))
                                    .collect::<Vec<_>>(),
                            );
                        }
                        None => {}
                    }
                }
            }
            ("input", Some(val)) => {
                match coerce_map(val, &coerce::string_with_span, ctx.diagnostics) {
                    Some(val) => {
                        let params = val
                            .iter()
                            .map(|(k, v)| ((k.0.to_string(), (k.1.clone(), (*v).clone()))))
                            .collect();
                        args = Some((f.span(), params));
                    }
                    None => ctx.push_error(DatamodelError::new_property_not_known_error(
                        "input",
                        f.identifier().span().clone(),
                        ["functions", "args"].to_vec(),
                    )),
                }
            }
            ("args", Some(val)) => {
                match coerce_map(val, &coerce::string_with_span, ctx.diagnostics) {
                    Some(val) => {
                        let params = val
                            .iter()
                            .map(|(k, v)| ((k.0.to_string(), (k.1.clone(), (*v).clone()))))
                            .collect();
                        args = Some((f.span(), params));
                    }
                    None => {}
                }
            }
            (name, Some(_)) => ctx.push_error(DatamodelError::new_property_not_known_error(
                name,
                f.identifier().span().clone(),
                ["functions", "args"].to_vec(),
            )),
        });

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
                },
            );
        }
    }
}
