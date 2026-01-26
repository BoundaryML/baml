use anyhow::Result;
use baml_types::{BamlMediaType, CompletionState};
use internal_baml_core::ir::{TypeIR, TypeValue};
use regex::Regex;

use super::{array_helper::coerce_array_to_singular, ParsingContext, ParsingError};
use crate::deserializer::{
    coercer::TypeCoercer,
    deserialize_flags::{DeserializerConditions, Flag},
    types::BamlValueWithFlags,
};

impl TypeCoercer for TypeValue {
    fn coerce(
        &self,
        ctx: &ParsingContext,
        target: &TypeIR,
        // Parsed from JSONish
        value: Option<&crate::jsonish::Value>,
    ) -> Result<BamlValueWithFlags, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: {name} (current: {current})",
            name = target,
            scope = ctx.display_scope(),
            current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
        );
        log::trace!(
            "content: {}",
            value
                .map(|v| v.to_string())
                .unwrap_or_else(|| "<null>".into())
        );

        match self {
            TypeValue::String => coerce_string(ctx, target, value),
            TypeValue::Int => coerce_int(ctx, target, value),
            TypeValue::Float => coerce_float(ctx, target, value),
            TypeValue::Bool => coerce_bool(ctx, target, value),
            TypeValue::Null => coerce_null(ctx, target, value),
            TypeValue::Media(BamlMediaType::Image) => Err(ctx.error_image_not_supported()),
            TypeValue::Media(BamlMediaType::Audio) => Err(ctx.error_audio_not_supported()),
            TypeValue::Media(BamlMediaType::Pdf) => Err(ctx.error_pdf_not_supported()),
            TypeValue::Media(BamlMediaType::Video) => Err(ctx.error_video_not_supported()),
        }
    }

    fn try_cast(
        &self,
        ctx: &ParsingContext,
        target: &TypeIR,
        value: Option<&crate::jsonish::Value>,
    ) -> Option<BamlValueWithFlags> {
        // Early exit for null values
        if value.is_none() {
            return match self {
                TypeValue::Null => {
                    Some(BamlValueWithFlags::Null(target.clone(), Default::default()))
                }
                _ => None,
            };
        }

        let mut result = match self {
            TypeValue::String => match value {
                Some(crate::jsonish::Value::String(s, _)) => {
                    Some(BamlValueWithFlags::String((s.to_string(), target).into()))
                }
                _ => None,
            },
            TypeValue::Int => match value {
                Some(crate::jsonish::Value::Number(n, _)) => n
                    .as_i64()
                    .map(|i| BamlValueWithFlags::Int((i, target).into())),
                _ => None,
            },
            TypeValue::Float => match value {
                Some(crate::jsonish::Value::Number(n, _)) => n
                    .as_f64()
                    .map(|f| BamlValueWithFlags::Float((f, target).into())),
                _ => None,
            },
            TypeValue::Bool => match value {
                Some(crate::jsonish::Value::Boolean(b)) => {
                    Some(BamlValueWithFlags::Bool((*b, target).into()))
                }
                _ => None,
            },
            TypeValue::Null => match value {
                Some(crate::jsonish::Value::Null) | None => {
                    Some(BamlValueWithFlags::Null(target.clone(), Default::default()))
                }
                _ => None,
            },
            TypeValue::Media(_) => None,
        };

        // Check completion state exactly like coerce methods do
        if let Some(v) = value {
            match v.completion_state() {
                CompletionState::Complete => {}
                CompletionState::Incomplete => {
                    result
                        .iter_mut()
                        .for_each(|baml_value| baml_value.add_flag(Flag::Incomplete));
                }
                CompletionState::Pending => {
                    unreachable!("jsonish::Value may never be in a Pending state.")
                }
            }
        }

        result
    }
}

fn coerce_null(
    _ctx: &ParsingContext,
    target: &TypeIR,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    match value {
        Some(crate::jsonish::Value::Null) | None => {
            Ok(BamlValueWithFlags::Null(target.clone(), Default::default()))
        }
        Some(v) => Ok(BamlValueWithFlags::Null(
            target.clone(),
            DeserializerConditions::new().with_flag(Flag::DefaultButHadValue(v.clone())),
        )),
    }
}

fn coerce_string(
    ctx: &ParsingContext,
    target: &TypeIR,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    let Some(value) = value else {
        return Err(ctx.error_unexpected_null(target));
    };

    match value {
        crate::jsonish::Value::String(s, completion_state) => {
            let mut baml_value = BamlValueWithFlags::String((s.to_string(), target).into());
            if completion_state == &CompletionState::Incomplete {
                baml_value.add_flag(Flag::Incomplete);
            }
            Ok(baml_value)
        }
        crate::jsonish::Value::Null => Err(ctx.error_unexpected_null(target)),
        // Handle AnyOf explicitly to extract the string content.
        // If one of the variants is a String, prefer that over the raw input.
        // Otherwise, use the original raw string.
        crate::jsonish::Value::AnyOf(choices, original_string) => {
            // Prefer a String choice only when it looks like it comes from the original raw input.
            // In streaming/partial cases the String choice is often a prefix of the raw input.
            // Some parse paths can also produce derived String choices (e.g. extracted from an object);
            // in those cases fall back to the raw string to preserve the user's content.
            let string_value = choices
                .iter()
                .filter_map(|choice| match choice {
                    crate::jsonish::Value::String(s, completion_state)
                        if original_string.starts_with(s) || s == original_string =>
                    {
                        Some((s.clone(), completion_state.clone()))
                    }
                    _ => None,
                })
                .max_by_key(|(s, _)| s.len());

            let (string_val, completion_state) = string_value
                .unwrap_or_else(|| (original_string.clone(), value.completion_state().clone()));

            let mut baml_value = BamlValueWithFlags::String((string_val, target).into());
            if completion_state == CompletionState::Incomplete {
                baml_value.add_flag(Flag::Incomplete);
            }
            Ok(baml_value)
        }
        v => Ok(BamlValueWithFlags::String(
            (v.to_string(), target, Flag::JsonToString(v.clone())).into(),
        )),
    }
}

pub(super) fn coerce_int(
    ctx: &ParsingContext,
    target: &TypeIR,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    let Some(value) = value else {
        return Err(ctx.error_unexpected_null(target));
    };

    let mut result = match value {
        crate::jsonish::Value::Number(n, _) => {
            if let Some(n) = n.as_i64() {
                Ok(BamlValueWithFlags::Int((n, target).into()))
            } else if let Some(n) = n.as_u64() {
                Ok(BamlValueWithFlags::Int((n as i64, target).into()))
            } else if let Some(n) = n.as_f64() {
                Ok(BamlValueWithFlags::Int(
                    ((n.round() as i64), target, Flag::FloatToInt(n)).into(),
                ))
            } else {
                Err(ctx.error_unexpected_type(target, &value))
            }
        }
        crate::jsonish::Value::String(s, _) => {
            let s = s.trim();
            // Trim trailing commas
            let s = s.trim_end_matches(',');
            if let Ok(n) = s.parse::<i64>() {
                Ok(BamlValueWithFlags::Int((n, target).into()))
            } else if let Ok(n) = s.parse::<u64>() {
                Ok(BamlValueWithFlags::Int((n as i64, target).into()))
            } else if let Ok(n) = s.parse::<f64>() {
                Ok(BamlValueWithFlags::Int(
                    ((n.round() as i64), target, Flag::FloatToInt(n)).into(),
                ))
            } else if let Some(frac) = float_from_maybe_fraction(s) {
                Ok(BamlValueWithFlags::Int(
                    ((frac.round() as i64), target, Flag::FloatToInt(frac)).into(),
                ))
            } else if let Some(frac) = float_from_comma_separated(s) {
                Ok(BamlValueWithFlags::Int(
                    ((frac.round() as i64), target, Flag::FloatToInt(frac)).into(),
                ))
            } else {
                Err(ctx.error_unexpected_type(target, &value))
            }
        }
        crate::jsonish::Value::Array(items, _) => {
            coerce_array_to_singular(ctx, target, &items.iter().collect::<Vec<_>>(), &|value| {
                coerce_int(ctx, target, Some(value))
            })
        }
        _ => Err(ctx.error_unexpected_type(target, &value)),
    };
    match value.completion_state() {
        CompletionState::Complete => {}
        CompletionState::Incomplete => {
            result.iter_mut().for_each(|v| v.add_flag(Flag::Incomplete));
        }
        CompletionState::Pending => unreachable!("jsonish::Value may never be in a Pending state."),
    }
    result
}

fn float_from_maybe_fraction(value: &str) -> Option<f64> {
    if let Some((numerator, denominator)) = value.split_once('/') {
        match (
            numerator.trim().parse::<f64>(),
            denominator.trim().parse::<f64>(),
        ) {
            (Ok(num), Ok(denom)) if denom != 0.0 => Some(num / denom),
            _ => None,
        }
    } else {
        None
    }
}

fn float_from_comma_separated(value: &str) -> Option<f64> {
    let re = Regex::new(r"([-+]?)\$?(?:\d+(?:,\d+)*(?:\.\d+)?|\d+\.\d+|\d+|\.\d+)(?:e[-+]?\d+)?")
        .unwrap();
    let matches: Vec<_> = re.find_iter(value).collect();

    if matches.len() != 1 {
        return None;
    }

    let number_str = matches[0].as_str();
    let without_commas = number_str.replace(",", "");
    // Remove all Unicode currency symbols
    let re_currency = Regex::new(r"\p{Sc}").unwrap();
    let without_currency = re_currency.replace_all(&without_commas, "");

    without_currency.parse::<f64>().ok()
}

fn coerce_float(
    ctx: &ParsingContext,
    target: &TypeIR,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    let Some(value) = value else {
        return Err(ctx.error_unexpected_null(target));
    };
    let mut result = match value {
        crate::jsonish::Value::Number(n, _) => {
            if let Some(n) = n.as_f64() {
                Ok(BamlValueWithFlags::Float((n, target).into()))
            } else if let Some(n) = n.as_i64() {
                Ok(BamlValueWithFlags::Float(((n as f64), target).into()))
            } else if let Some(n) = n.as_u64() {
                Ok(BamlValueWithFlags::Float(((n as f64), target).into()))
            } else {
                Err(ctx.error_unexpected_type(target, &value))
            }
        }
        crate::jsonish::Value::String(s, _) => {
            let s = s.trim();
            // Trim trailing commas
            let s = s.trim_end_matches(',');
            if let Ok(n) = s.parse::<f64>() {
                Ok(BamlValueWithFlags::Float((n, target).into()))
            } else if let Ok(n) = s.parse::<i64>() {
                Ok(BamlValueWithFlags::Float(((n as f64), target).into()))
            } else if let Ok(n) = s.parse::<u64>() {
                Ok(BamlValueWithFlags::Float(((n as f64), target).into()))
            } else if let Some(frac) = float_from_maybe_fraction(s) {
                Ok(BamlValueWithFlags::Float((frac, target).into()))
            } else if let Some(frac) = float_from_comma_separated(s) {
                let mut baml_value = BamlValueWithFlags::Float((frac, target).into());
                // Add flag here to penalize strings like
                // "1 cup unsalted butter, room temperature".
                // If we're trying to parse this to a float it should work
                // anyway but unions like "float | string" should still coerce
                // this to a string.
                baml_value.add_flag(Flag::StringToFloat(s.to_string()));
                Ok(baml_value)
            } else {
                Err(ctx.error_unexpected_type(target, &value))
            }
        }
        crate::jsonish::Value::Array(items, _) => {
            coerce_array_to_singular(ctx, target, &items.iter().collect::<Vec<_>>(), &|value| {
                coerce_float(ctx, target, Some(value))
            })
        }
        _ => Err(ctx.error_unexpected_type(target, &value)),
    };
    match value.completion_state() {
        CompletionState::Complete => {}
        CompletionState::Incomplete => {
            result.iter_mut().for_each(|v| v.add_flag(Flag::Incomplete));
        }
        CompletionState::Pending => unreachable!("jsonish::Value may never be in pending state"),
    }
    result
}

pub(super) fn coerce_bool(
    ctx: &ParsingContext,
    target: &TypeIR,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    let Some(value) = value else {
        return Err(ctx.error_unexpected_null(target));
    };

    let mut result = match value {
        crate::jsonish::Value::Boolean(b) => Ok(BamlValueWithFlags::Bool((*b, target).into())),
        crate::jsonish::Value::String(s, _) => match s.to_lowercase().as_str() {
            "true" => Ok(BamlValueWithFlags::Bool(
                (true, target, Flag::StringToBool(s.clone())).into(),
            )),
            "false" => Ok(BamlValueWithFlags::Bool(
                (false, target, Flag::StringToBool(s.clone())).into(),
            )),
            _ => {
                match super::match_string::match_string(
                    ctx,
                    target,
                    Some(value),
                    &[
                        ("true", vec!["true".into(), "True".into(), "TRUE".into()]),
                        (
                            "false",
                            vec!["false".into(), "False".into(), "FALSE".into()],
                        ),
                    ],
                    true,
                ) {
                    Ok(val) => match val.value().as_str() {
                        "true" => Ok(BamlValueWithFlags::Bool(
                            (true, target, Flag::StringToBool(val.value().clone())).into(),
                        )),
                        "false" => Ok(BamlValueWithFlags::Bool(
                            (false, target, Flag::StringToBool(val.value().clone())).into(),
                        )),
                        _ => Err(ctx.error_unexpected_type(target, &value)),
                    },
                    Err(_) => Err(ctx.error_unexpected_type(target, &value)),
                }
            }
        },
        crate::jsonish::Value::Array(items, _) => {
            coerce_array_to_singular(ctx, target, &items.iter().collect::<Vec<_>>(), &|value| {
                coerce_bool(ctx, target, Some(value))
            })
        }
        _ => Err(ctx.error_unexpected_type(target, &value)),
    };
    match value.completion_state() {
        CompletionState::Complete => {}
        CompletionState::Incomplete => {
            result.iter_mut().for_each(|v| v.add_flag(Flag::Incomplete));
        }
        CompletionState::Pending => unreachable!("jsonish::Value may never be in pending state."),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_float_from_comma_separated() {
        // Note we don't handle european numbers correctly.
        let test_cases = vec![
            // European Formats
            // Valid German format (comma as decimal separator)
            ("3,14", Some(314.0)),
            ("1.234,56", None),
            ("1.234.567,89", None),
            ("€1.234,56", None),
            ("-€1.234,56", None),
            ("€1.234", Some(1.234)), // TODO - technically incorrect
            ("1.234€", Some(1.234)), // TODO - technically incorrect
            // Valid currencies with European formatting
            ("€1.234,56", None),
            ("€1,234.56", Some(1234.56)), // Incorrect format for Euro
            // US Formats
            // Valid US format (comma as thousands separator)
            ("3,000", Some(3000.0)),
            ("3,100.00", Some(3100.00)),
            ("1,234.56", Some(1234.56)),
            ("1,234,567.89", Some(1234567.89)),
            ("$1,234.56", Some(1234.56)),
            ("-$1,234.56", Some(-1234.56)),
            ("$1,234", Some(1234.0)),
            ("1,234$", Some(1234.0)),
            ("$1,234.56", Some(1234.56)),
            ("+$1,234.56", Some(1234.56)),
            ("-$1,234.56", Some(-1234.56)),
            ("$9,999,999,999", Some(9999999999.0)),
            ("$1.23.456", None),
            ("$1.234.567.890", None),
            // Valid currencies with US formatting
            ("$1,234", Some(1234.0)),
            ("$314", Some(314.0)),
            // Indian Formats
            // Assuming Indian numbering system (not present in original tests, added for categorization)
            ("$1,23,456", Some(123456.0)),
            // Additional Indian format test cases can be added here

            // Percentages and Strings with Numbers
            // Percentages
            ("50%", Some(50.0)),
            ("3.15%", Some(3.15)),
            (".009%", Some(0.009)),
            ("1.234,56%", None),
            ("$1,234.56%", Some(1234.56)),
            // Strings containing numbers
            ("The answer is 10,000", Some(10000.0)),
            ("The total is €1.234,56 today", None),
            ("You owe $3,000 for the service", Some(3000.0)),
            ("Save up to 20% on your purchase", Some(20.0)),
            ("Revenue grew by 1,234.56 this quarter", Some(1234.56)),
            ("Profit is -€1.234,56 in the last month", None),
            // Sentences with Multiple Numbers
            ("The answer is 10,000 and $3,000", None),
            ("We earned €1.234,56 and $2,345.67 this year", None),
            ("Increase of 5% and a profit of $1,000", None),
            ("Loss of -€500 and a gain of 1,200.50", None),
            ("Targets: 2,000 units and €3.000,75 revenue", None),
            // trailing periods and commas
            ("12,111,123.", Some(12111123.0)),
            ("12,111,123,", Some(12111123.0)),
        ];

        for (input, expected) in test_cases {
            let result = float_from_comma_separated(input);
            assert_eq!(
                result, expected,
                "Failed to parse '{input}'. Expected {expected:?}, got {result:?}"
            );
        }
    }

    #[test]
    fn test_coerce_anyof_to_string() {
        use crate::{
            helpers::{load_test_ir, render_output_format},
            jsonish::Value,
        };

        // Create an AnyOf value similar to what the parser creates
        let anyof_value = Value::AnyOf(
            vec![
                Value::String("[json\n".to_string(), CompletionState::Incomplete),
                Value::Object(vec![], CompletionState::Incomplete),
            ],
            "[json\nAnyOf[{,AnyOf[{,{},],]".to_string(), // This is the raw string
        );

        let ir = load_test_ir("");
        let target = TypeIR::Primitive(TypeValue::String, Default::default());
        let output_format = render_output_format(
            &ir,
            &target,
            &Default::default(),
            baml_types::StreamingMode::Streaming,
        )
        .unwrap();
        let ctx = ParsingContext::new(&output_format, baml_types::StreamingMode::Streaming);

        let result = coerce_string(&ctx, &target, Some(&anyof_value));

        // The bug would cause this to return "AnyOf[..."
        // The fix should prefer the String variant from the choices if available
        assert!(result.is_ok());
        let baml_value = result.unwrap();
        match baml_value {
            BamlValueWithFlags::String(v) => {
                // Should NOT start with "AnyOf[" - that's the bug!
                assert!(
                    !v.value.starts_with("AnyOf["),
                    "Got parsing artifact in string: {}",
                    v.value
                );
                // Should be the String variant from the choices, not the Display repr
                assert_eq!(v.value, "[json\n");
            }
            _ => panic!("Expected String, got {baml_value:?}"),
        }
    }

    #[test]
    fn test_coerce_anyof_to_string_no_string_variant() {
        use crate::{
            helpers::{load_test_ir, render_output_format},
            jsonish::Value,
        };

        // Create an AnyOf value with NO string variant - should fall back to raw string
        let anyof_value = Value::AnyOf(
            vec![
                Value::Object(vec![], CompletionState::Incomplete),
                Value::Array(vec![], CompletionState::Incomplete),
            ],
            "some raw input".to_string(),
        );

        let ir = load_test_ir("");
        let target = TypeIR::Primitive(TypeValue::String, Default::default());
        let output_format = render_output_format(
            &ir,
            &target,
            &Default::default(),
            baml_types::StreamingMode::Streaming,
        )
        .unwrap();
        let ctx = ParsingContext::new(&output_format, baml_types::StreamingMode::Streaming);

        let result = coerce_string(&ctx, &target, Some(&anyof_value));

        assert!(result.is_ok());
        let baml_value = result.unwrap();
        match baml_value {
            BamlValueWithFlags::String(v) => {
                // Should fall back to the raw input string
                assert_eq!(v.value, "some raw input");
            }
            _ => panic!("Expected String, got {baml_value:?}"),
        }
    }
}
