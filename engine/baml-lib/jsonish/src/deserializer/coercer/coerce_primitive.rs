use anyhow::Result;
use baml_types::BamlMediaType;
use internal_baml_core::ir::{FieldType, TypeValue};

use crate::deserializer::{
    coercer::TypeCoercer,
    deserialize_flags::{DeserializerConditions, Flag},
    types::BamlValueWithFlags,
};

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

fn coerce_int(
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
    let trimmed = value.trim();

    // Return None if the input is empty or contains only whitespace
    if trimmed.is_empty() {
        return None;
    }

    // Define a set of currency symbols to ignore
    let currency_symbols = [
        '$', '€', '£', '¥', '₹', '₽', '₩', '₪', '₫', '₴', '₦', '₱', '฿', '₵', '₲', '₡', '₺', '₼',
        '₸', '₿',
    ];

    // Remove currency symbols from the string
    let without_currency: String = trimmed
        .chars()
        .filter(|c| !currency_symbols.contains(c))
        .collect();

    // Replace any non-breaking spaces or other Unicode spaces with nothing
    let normalized: String = without_currency
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    // Return None if the normalized string is empty
    if normalized.is_empty() {
        return None;
    }

    // Early rejection of inputs with disallowed characters
    if normalized
        .chars()
        .any(|c| !c.is_ascii_digit() && c != ',' && c != '.' && c != '-' && c != '+')
    {
        return None;
    }

    // Get an iterator over the characters
    let mut chars = normalized.chars();

    // Attempt to get the first character
    let first_char = match chars.next() {
        Some(c) => c,
        None => return None,
    };

    // Check for multiple signs
    if (first_char == '+' || first_char == '-')
        && match chars.clone().next() {
            Some(c) => c == '+' || c == '-',
            None => false,
        }
    {
        return None;
    }

    // Split the string into sign and rest
    let rest = if first_char == '+' || first_char == '-' {
        chars.as_str()
    } else {
        normalized.as_str()
    };
    let sign = if first_char == '+' || first_char == '-' {
        first_char.to_string()
    } else {
        "".to_string()
    };

    // Return None if rest is empty after removing sign and currency symbols
    if rest.is_empty() {
        return None;
    }

    // Count occurrences of ',' and '.'
    let comma_count = rest.matches(',').count();
    let dot_count = rest.matches('.').count();

    // Determine decimal and thousands separators
    let (decimal_sep, thousands_sep) = if rest.contains('.') && rest.contains(',') {
        // Both '.' and ',' present
        match (rest.rfind(','), rest.rfind('.')) {
            (Some(comma_pos), Some(dot_pos)) => {
                if comma_pos > dot_pos {
                    // ',' occurs after '.', so ',' is decimal separator
                    (Some(','), Some('.'))
                } else {
                    // '.' occurs after ',', so '.' is decimal separator
                    (Some('.'), Some(','))
                }
            }
            _ => (None, None),
        }
    } else if rest.contains('.') {
        if dot_count == 1 {
            if let Some(dot_pos) = rest.rfind('.') {
                // Check if the dot is within the last three characters
                if dot_pos > rest.len().saturating_sub(4) {
                    // Dot is near the end, likely a decimal separator
                    (Some('.'), None)
                } else {
                    // Dot is not near the end, treat as thousands separator
                    (None, Some('.'))
                }
            } else {
                // Should not reach here, default to treating as thousands separator
                (None, Some('.'))
            }
        } else {
            // Multiple dots, assume dots are thousands separators
            (None, Some('.'))
        }
    } else if rest.contains(',') {
        if comma_count == 1 {
            if let Some(comma_pos) = rest.rfind(',') {
                // Check if the comma is within the last three characters
                if comma_pos > rest.len().saturating_sub(4) {
                    // Comma is near the end, likely a decimal separator
                    (Some(','), None)
                } else {
                    // Comma is not near the end, treat as thousands separator
                    (None, Some(','))
                }
            } else {
                // Should not reach here, default to treating as thousands separator
                (None, Some(','))
            }
        } else {
            // Multiple commas, assume commas are thousands separators
            (None, Some(','))
        }
    } else {
        // No separators
        (None, None)
    };

    // Split rest into integer and fractional parts
    let (integer_part, fractional_part) = if let Some(decimal) = decimal_sep {
        let parts: Vec<&str> = rest.split(decimal).collect();
        if parts.len() != 2 {
            return None; // Invalid decimal format
        }
        (parts[0], parts[1])
    } else {
        (rest, "")
    };

    // Validate and clean the integer part
    let integer_digits = if let Some(thousands) = thousands_sep {
        let groups: Vec<&str> = integer_part.split(thousands).collect();

        // Validate grouping
        if groups.is_empty() || groups[0].is_empty() {
            return None;
        }

        if groups[0].len() > 3 {
            return None; // First group can't have more than 3 digits
        }

        for group in &groups[1..] {
            if group.len() != 3 {
                return None; // Subsequent groups must have exactly 3 digits
            }
        }

        // Reconstruct integer part without thousands separators
        groups.join("")
    } else {
        integer_part.to_string()
    };

    // Validate that integer and fractional parts contain only digits
    if !integer_digits.chars().all(|c| c.is_ascii_digit())
        || !fractional_part.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }

    // Reconstruct the cleaned number
    let mut cleaned = sign;
    cleaned.push_str(&integer_digits);
    if !fractional_part.is_empty() {
        cleaned.push('.');
        cleaned.push_str(fractional_part);
    }

    // Ensure there are no remaining separators
    if cleaned.matches(',').count() > 0 || cleaned.matches('.').count() > 1 {
        return None;
    }

    // Parse the cleaned string into f64
    cleaned.parse::<f64>().ok()
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

fn coerce_bool(
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
                _ => match s.to_ascii_lowercase().trim() {
                    "true" => Ok(BamlValueWithFlags::Bool(
                        (true, Flag::StringToBool(s.clone())).into(),
                    )),
                    "false" => Ok(BamlValueWithFlags::Bool(
                        (false, Flag::StringToBool(s.clone())).into(),
                    )),
                    _ => Err(ctx.error_unexpected_type(target, value)),
                },
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
        let test_cases = vec![
            // Valid German format (comma as decimal separator)
            ("3,14", Some(3.14)),
            ("1.234,56", Some(1234.56)),
            ("1.234.567,89", Some(1234567.89)),
            // Valid US format (comma as thousands separator)
            ("3,000", Some(3000.0)),
            ("3,100.00", Some(3100.00)),
            ("1,234.56", Some(1234.56)),
            ("1,234,567.89", Some(1234567.89)),
            // No separators
            ("314", Some(314.0)),
            // Invalid formats
            ("3,abc", None),
            ("1,234,567,89", None),
            ("1.234.567.89", None),
            ("", None),
            (",", None),
            (".", None),
            ("1234,56.78", None), // Mixed separators not following conventions
            ("1,23,4", None),     // Incorrect thousands separators
            ("1.23.4", None),     // Incorrect thousands separators
            // Additional Edge Cases
            ("12,345.67", Some(12345.67)),     // Valid US
            ("12,345.6789", Some(12345.6789)), // Valid US
            ("12.345,67", Some(12345.67)),     // Valid German
            ("12,111.123", Some(12111.123)),   // Valid German
            // big number
            ("10,000,000.1234", Some(10000000.1234)),
            ("100,000,000.1234", Some(100000000.1234)),
            ("1,000,000.1234", Some(1000000.1234)),
            ("1234567", Some(1234567.0)), // Large number without separators
            ("1,2345.67", None),          // Incorrect thousands separators
            ("1.2345,67", None),          // Incorrect thousands separators
            ("1,234.5.67", None),         // Multiple decimal separators
            ("1.234,5,67", None),         // Multiple decimal separators
            ("1,234,56.78", None),        // Mixed separators with incorrect grouping
            ("1.234.56,78", None),        // Mixed separators with incorrect grouping
            ("1,234", Some(1234.0)),      // US format without decimal
            ("1.234", Some(1234.0)),      // German format without decimal
            ("-1,234.56", Some(-1234.56)), // Negative number in US format
            ("   1,234.56   ", Some(1234.56)), // Leading and trailing spaces
            ("1 234,56", Some(1234.56)),  // Embedded space should be rejected
            ("1,,234.56", None),          // Consecutive thousands separators
            ("123,456,789,012,345.67", Some(123456789012345.67)), // Very large number
            // currencies
            // Valid US format without decimal places
            ("$1,234", Some(1234.0)),
            ("€1,234,567", Some(1234567.0)),
            ("£1.234", Some(1234.0)),
            ("¥1.234.567", Some(1234567.0)),
            // Valid German format with decimal places
            ("€1.234,56", Some(1234.56)),
            ("$1.234.567,89", Some(1234567.89)),
            // Valid US format with decimal places
            ("$1,234.56", Some(1234.56)),
            ("€1,234,567.89", Some(1234567.89)),
            // Currency symbols at the end
            ("1,234$", Some(1234.0)),
            ("1.234€", Some(1234.0)),
            ("1,234.56£", Some(1234.56)),
            ("1.234,56¥", Some(1234.56)),
            // Currency symbols with spaces
            ("$ 1,234.56", Some(1234.56)),
            ("1,234.56 €", Some(1234.56)),
            // No separators
            ("$314", Some(314.0)),
            // Negative numbers with currency symbols
            ("-$1,234.56", Some(-1234.56)),
            ("-€1.234,56", Some(-1234.56)),
            // Leading and trailing spaces with currency symbols
            ("   $1,234.56   ", Some(1234.56)),
            ("€   1.234,56   ", Some(1234.56)),
            // Invalid formats (should still be rejected)
            ("$1.23.456", None),
            ("€1,23,456", None),
            ("£1.234.567.890", Some(1234567890.0)),
            // Only currency symbols
            ("$", None),
            ("€", None),
            ("£", None),
            // Currency symbols with invalid numbers
            ("$abc", None),
            ("€1,2a3", None),
            ("£-1.23.45", None),
            // Multiple currency symbols
            ("$$1,234.56", Some(1234.56)),
            ("€€1.234,56", Some(1234.56)),
            ("$€1,234.56", Some(1234.56)),
            // Currency symbols in the middle of the number
            ("1,$234.56", Some(1234.56)),
            ("1€234,56", Some(1234.56)),
            // Valid numbers with different currency symbols
            ("₹1,234.56", Some(1234.56)),
            ("₽1.234,56", Some(1234.56)),
            ("1,234.56₩", Some(1234.56)),
            // Valid numbers with positive sign and currency symbols
            ("+$1,234.56", Some(1234.56)),
            ("+€1.234,56", Some(1234.56)),
            // Edge cases with only currency symbols and signs
            ("+$", None),
            ("-€", None),
            // Large numbers with currency symbols
            ("$9,999,999,999", Some(9999999999.0)),
            ("€9.999.999.999", Some(9999999999.0)),
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
