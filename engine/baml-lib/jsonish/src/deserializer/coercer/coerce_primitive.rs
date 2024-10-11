use anyhow::Result;
use baml_types::BamlMediaType;
use internal_baml_core::ir::{FieldType, TypeValue};

use crate::deserializer::{
    coercer::TypeCoercer,
    deserialize_flags::{DeserializerConditions, Flag},
    types::BamlValueWithFlags,
};
use regex::Regex;

use super::{array_helper::coerce_array_to_singular, ParsingContext, ParsingError};

impl TypeCoercer for TypeValue {
    fn coerce(
        &self,
        ctx: &ParsingContext,
        target: &FieldType,
        // Parsed from JSONish
        value: Option<&crate::jsonish::Value>,
    ) -> Result<BamlValueWithFlags, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: {name} (current: {current})",
            name = target.to_string(),
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
        }
    }
}

fn coerce_null(
    _ctx: &ParsingContext,
    _target: &FieldType,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    match value {
        Some(crate::jsonish::Value::Null) | None => {
            Ok(BamlValueWithFlags::Null(Default::default()))
        }
        Some(v) => Ok(BamlValueWithFlags::Null(
            DeserializerConditions::new().with_flag(Flag::DefaultButHadValue(v.clone())),
        )),
    }
}

fn coerce_string(
    ctx: &ParsingContext,
    target: &FieldType,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    if let Some(value) = value {
        match value {
            crate::jsonish::Value::String(s) => {
                Ok(BamlValueWithFlags::String(s.to_string().into()))
            }
            crate::jsonish::Value::Null => Err(ctx.error_unexpected_null(target)),
            v => Ok(BamlValueWithFlags::String(
                (v.to_string(), Flag::JsonToString(v.clone())).into(),
            )),
        }
    } else {
        Err(ctx.error_unexpected_null(target))
    }
}

pub(super) fn coerce_int(
    ctx: &ParsingContext,
    target: &FieldType,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    if let Some(value) = value {
        match value {
            crate::jsonish::Value::Number(n) => {
                if let Some(n) = n.as_i64() {
                    Ok(BamlValueWithFlags::Int(n.into()))
                } else if let Some(n) = n.as_u64() {
                    Ok(BamlValueWithFlags::Int((n as i64).into()))
                } else if let Some(n) = n.as_f64() {
                    Ok(BamlValueWithFlags::Int(
                        ((n.round() as i64), Flag::FloatToInt(n)).into(),
                    ))
                } else {
                    Err(ctx.error_unexpected_type(target, value))
                }
            }
            crate::jsonish::Value::String(s) => {
                let s = s.trim();
                // Trim trailing commas
                let s = s.trim_end_matches(',');
                if let Ok(n) = s.parse::<i64>() {
                    Ok(BamlValueWithFlags::Int(n.into()))
                } else if let Ok(n) = s.parse::<u64>() {
                    Ok(BamlValueWithFlags::Int((n as i64).into()))
                } else if let Ok(n) = s.parse::<f64>() {
                    Ok(BamlValueWithFlags::Int(
                        ((n.round() as i64), Flag::FloatToInt(n)).into(),
                    ))
                } else if let Some(frac) = float_from_maybe_fraction(s) {
                    Ok(BamlValueWithFlags::Int(
                        ((frac.round() as i64), Flag::FloatToInt(frac)).into(),
                    ))
                } else if let Some(frac) = float_from_comma_separated(s) {
                    Ok(BamlValueWithFlags::Int(
                        ((frac.round() as i64), Flag::FloatToInt(frac)).into(),
                    ))
                } else {
                    Err(ctx.error_unexpected_type(target, value))
                }
            }
            crate::jsonish::Value::Array(items) => {
                coerce_array_to_singular(ctx, target, &items.iter().collect::<Vec<_>>(), &|value| {
                    coerce_int(ctx, target, Some(value))
                })
            }
            _ => Err(ctx.error_unexpected_type(target, value)),
        }
    } else {
        Err(ctx.error_unexpected_null(target))
    }
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
    target: &FieldType,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    if let Some(value) = value {
        match value {
            crate::jsonish::Value::Number(n) => {
                if let Some(n) = n.as_f64() {
                    Ok(BamlValueWithFlags::Float(n.into()))
                } else if let Some(n) = n.as_i64() {
                    Ok(BamlValueWithFlags::Float((n as f64).into()))
                } else if let Some(n) = n.as_u64() {
                    Ok(BamlValueWithFlags::Float((n as f64).into()))
                } else {
                    Err(ctx.error_unexpected_type(target, value))
                }
            }
            crate::jsonish::Value::String(s) => {
                let s = s.trim();
                // Trim trailing commas
                let s = s.trim_end_matches(',');
                if let Ok(n) = s.parse::<f64>() {
                    Ok(BamlValueWithFlags::Float(n.into()))
                } else if let Ok(n) = s.parse::<i64>() {
                    Ok(BamlValueWithFlags::Float((n as f64).into()))
                } else if let Ok(n) = s.parse::<u64>() {
                    Ok(BamlValueWithFlags::Float((n as f64).into()))
                } else if let Some(frac) = float_from_maybe_fraction(s) {
                    Ok(BamlValueWithFlags::Float(frac.into()))
                } else if let Some(frac) = float_from_comma_separated(s) {
                    Ok(BamlValueWithFlags::Float(frac.into()))
                } else {
                    Err(ctx.error_unexpected_type(target, value))
                }
            }
            crate::jsonish::Value::Array(items) => {
                coerce_array_to_singular(ctx, target, &items.iter().collect::<Vec<_>>(), &|value| {
                    coerce_float(ctx, target, Some(value))
                })
            }
            _ => Err(ctx.error_unexpected_type(target, value)),
        }
    } else {
        Err(ctx.error_unexpected_null(target))
    }
}

pub(super) fn coerce_bool(
    ctx: &ParsingContext,
    target: &FieldType,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    if let Some(value) = value {
        match value {
            crate::jsonish::Value::Boolean(b) => Ok(BamlValueWithFlags::Bool((*b).into())),
            crate::jsonish::Value::String(s) => match s.as_str() {
                "true" => Ok(BamlValueWithFlags::Bool(
                    (true, Flag::StringToBool(s.clone())).into(),
                )),
                "false" => Ok(BamlValueWithFlags::Bool(
                    (true, Flag::StringToBool(s.clone())).into(),
                )),
                _ => {
                    match super::match_string::match_string(
                        ctx,
                        target,
                        Some(value),
                        &[
                            ("true", vec!["true".into()]),
                            ("false", vec!["false".into()]),
                        ],
                    ) {
                        Ok(val) => match val.value().as_str() {
                            "true" => Ok(BamlValueWithFlags::Bool(
                                (true, Flag::StringToBool(val.value().clone())).into(),
                            )),
                            "false" => Ok(BamlValueWithFlags::Bool(
                                (false, Flag::StringToBool(val.value().clone())).into(),
                            )),
                            _ => Err(ctx.error_unexpected_type(target, value)),
                        },
                        Err(_) => Err(ctx.error_unexpected_type(target, value)),
                    }
                }
            },
            crate::jsonish::Value::Array(items) => {
                coerce_array_to_singular(ctx, target, &items.iter().collect::<Vec<_>>(), &|value| {
                    coerce_float(ctx, target, Some(value))
                })
            }
            _ => Err(ctx.error_unexpected_type(target, value)),
        }
    } else {
        Err(ctx.error_unexpected_null(target))
    }
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
            ("3.14%", Some(3.14)),
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

        for &(input, expected) in &test_cases {
            let result = float_from_comma_separated(input);
            assert_eq!(
                result, expected,
                "Failed to parse '{}'. Expected {:?}, got {:?}",
                input, expected, result
            );
        }
    }
}
