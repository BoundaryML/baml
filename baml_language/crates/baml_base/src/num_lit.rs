//! Shared parsing and validation for numeric literal token text.
//!
//! The lexer deliberately over-accepts numeric literals so that a malformed
//! literal stays a single token with a good span (mirroring rustc's design):
//!
//! - `0b`/`0o` literals consume any decimal digits, so `0b123` is one token
//!   and the invalid digits are reported here with per-digit spans.
//! - Base prefixes also match uppercase (`0X1F`), so the fix can be suggested
//!   instead of the text splitting into `0` + `X1F`.
//! - A bare prefix (`0x`, `0b__`) still lexes as a literal and is reported
//!   here as having no digits.
//!
//! Underscore digit separators are allowed everywhere Rust allows them and
//! are stripped before value parsing.
//!
//! All compile-time consumers of `INTEGER_LITERAL`, `BIGINT_LITERAL`, and
//! `FLOAT_LITERAL` token text must go through this module so that base
//! prefixes and underscores are handled identically.

use num_bigint::BigInt;

/// Validation failure for an integer or bigint literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntLitError {
    /// Base prefix written uppercase (`0X1F`); `fixed` is the lowercase spelling.
    UppercaseBasePrefix { fixed: String },
    /// Prefix with no digits after it (`0x`, `0b__`).
    NoDigits,
    /// Digits invalid for the base (`0b12`); byte offset and char of each
    /// offending digit within the token text.
    InvalidDigits {
        base: u32,
        positions: Vec<(usize, char)>,
    },
    /// Magnitude exceeds `i64::MAX`. Never produced for bigints.
    TooLarge,
}

impl IntLitError {
    /// Human-readable message for the error, matching rustc's wording where
    /// it has an equivalent.
    pub fn message(&self) -> String {
        match self {
            IntLitError::UppercaseBasePrefix { fixed } => {
                format!(
                    "invalid base prefix for number literal; base prefixes are lowercase: `{fixed}`"
                )
            }
            IntLitError::NoDigits => "no valid digits found for number".to_string(),
            IntLitError::InvalidDigits { base, .. } => {
                format!("invalid digit for a base {base} literal")
            }
            IntLitError::TooLarge => "integer literal is too large".to_string(),
        }
    }
}

/// Determine the base and the byte offset where digits start.
fn base_and_digits_start(text: &str) -> (u32, usize) {
    let b = text.as_bytes();
    if b.len() >= 2 && b[0] == b'0' {
        match b[1] {
            b'x' | b'X' => (16, 2),
            b'o' | b'O' => (8, 2),
            b'b' | b'B' => (2, 2),
            _ => (10, 0),
        }
    } else {
        (10, 0)
    }
}

/// Validate token text and return `(base, digits)` where `digits` is the
/// text after any base prefix, still containing underscores.
fn validate(text: &str) -> Result<(u32, &str), IntLitError> {
    let (base, start) = base_and_digits_start(text);
    if start == 2 && text.as_bytes()[1].is_ascii_uppercase() {
        let mut fixed = text.to_string();
        fixed.replace_range(1..2, &text[1..2].to_ascii_lowercase());
        return Err(IntLitError::UppercaseBasePrefix { fixed });
    }
    let digits = &text[start..];
    if !digits.chars().any(|c| c != '_') {
        return Err(IntLitError::NoDigits);
    }
    let positions: Vec<(usize, char)> = digits
        .char_indices()
        .filter(|(_, c)| *c != '_' && c.to_digit(base).is_none())
        .map(|(i, c)| (start + i, c))
        .collect();
    if !positions.is_empty() {
        return Err(IntLitError::InvalidDigits { base, positions });
    }
    Ok((base, digits))
}

/// Parse an `INTEGER_LITERAL` token's text (`42`, `1_000`, `0xFF`, `0o755`,
/// `0b1010`) into its non-negative value. Signs are handled by callers.
///
/// The value is bounded by `i64::MAX`; the VM's stricter i63 `int` range is
/// enforced later in type inference, where negation context is known.
pub fn parse_int_literal(text: &str) -> Result<i64, IntLitError> {
    let (base, digits) = validate(text)?;
    let cleaned: String = digits.chars().filter(|c| *c != '_').collect();
    i64::from_str_radix(&cleaned, base).map_err(|_| IntLitError::TooLarge)
}

/// Parse a `BIGINT_LITERAL` token's text with the trailing `n` already
/// stripped (`42`, `0xFFFF_FFFF`). An optional leading `-` is accepted
/// because type-level literals carry the sign in the text.
pub fn parse_bigint_literal(text: &str) -> Result<BigInt, IntLitError> {
    let (negated, magnitude) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (base, digits) = validate(magnitude)?;
    let cleaned: String = digits.chars().filter(|c| *c != '_').collect();
    let value = BigInt::parse_bytes(cleaned.as_bytes(), base)
        .unwrap_or_else(|| unreachable!("validated bigint digits failed to parse: {text:?}"));
    Ok(if negated { -value } else { value })
}

/// Normalize a `FLOAT_LITERAL` token's text by stripping underscore
/// separators, so downstream `f64` parsing (which rejects `_`) always works.
pub fn normalize_float_literal(text: &str) -> String {
    if text.contains('_') {
        text.chars().filter(|c| *c != '_').collect()
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal() {
        assert_eq!(parse_int_literal("0"), Ok(0));
        assert_eq!(parse_int_literal("42"), Ok(42));
        assert_eq!(parse_int_literal("0123"), Ok(123));
        assert_eq!(parse_int_literal("1_000_000"), Ok(1_000_000));
        assert_eq!(parse_int_literal("1_"), Ok(1));
        assert_eq!(parse_int_literal("9223372036854775807"), Ok(i64::MAX));
    }

    #[test]
    fn hex_octal_binary() {
        assert_eq!(parse_int_literal("0xFF"), Ok(255));
        assert_eq!(parse_int_literal("0xff"), Ok(255));
        assert_eq!(parse_int_literal("0xCAFE"), Ok(0xCAFE));
        assert_eq!(parse_int_literal("0xFF_FF"), Ok(0xFFFF));
        assert_eq!(parse_int_literal("0o755"), Ok(0o755));
        assert_eq!(parse_int_literal("0b1010"), Ok(10));
        assert_eq!(parse_int_literal("0b10_10"), Ok(10));
        assert_eq!(parse_int_literal("0x_F"), Ok(15));
    }

    #[test]
    fn no_digits() {
        assert_eq!(parse_int_literal("0x"), Err(IntLitError::NoDigits));
        assert_eq!(parse_int_literal("0b"), Err(IntLitError::NoDigits));
        assert_eq!(parse_int_literal("0o"), Err(IntLitError::NoDigits));
        assert_eq!(parse_int_literal("0b__"), Err(IntLitError::NoDigits));
    }

    #[test]
    fn invalid_digits() {
        assert_eq!(
            parse_int_literal("0b123"),
            Err(IntLitError::InvalidDigits {
                base: 2,
                positions: vec![(3, '2'), (4, '3')],
            })
        );
        assert_eq!(
            parse_int_literal("0b10_10301"),
            Err(IntLitError::InvalidDigits {
                base: 2,
                positions: vec![(7, '3')],
            })
        );
        assert_eq!(
            parse_int_literal("0o18"),
            Err(IntLitError::InvalidDigits {
                base: 8,
                positions: vec![(3, '8')],
            })
        );
        assert_eq!(
            parse_int_literal("0o1234_9_5670"),
            Err(IntLitError::InvalidDigits {
                base: 8,
                positions: vec![(7, '9')],
            })
        );
    }

    #[test]
    fn uppercase_prefix() {
        assert_eq!(
            parse_int_literal("0X1F"),
            Err(IntLitError::UppercaseBasePrefix {
                fixed: "0x1F".to_string()
            })
        );
        assert_eq!(
            parse_int_literal("0B10"),
            Err(IntLitError::UppercaseBasePrefix {
                fixed: "0b10".to_string()
            })
        );
        assert_eq!(
            parse_int_literal("0O7"),
            Err(IntLitError::UppercaseBasePrefix {
                fixed: "0o7".to_string()
            })
        );
    }

    #[test]
    fn too_large() {
        assert_eq!(
            parse_int_literal("9223372036854775808"),
            Err(IntLitError::TooLarge)
        );
        assert_eq!(
            parse_int_literal("99999999999999999999"),
            Err(IntLitError::TooLarge)
        );
        assert_eq!(
            parse_int_literal("0x8000000000000000"),
            Err(IntLitError::TooLarge)
        );
        assert_eq!(
            parse_int_literal(
                "0b11111111111111111111111111111111111111111111111111111111111111111"
            ),
            Err(IntLitError::TooLarge)
        );
        assert_eq!(
            parse_int_literal("0o1000000000000000000000"),
            Err(IntLitError::TooLarge)
        );
        assert_eq!(parse_int_literal("0x7FFF_FFFF_FFFF_FFFF"), Ok(i64::MAX));
    }

    #[test]
    fn bigint() {
        assert_eq!(parse_bigint_literal("42"), Ok(BigInt::from(42)));
        assert_eq!(parse_bigint_literal("-42"), Ok(BigInt::from(-42)));
        assert_eq!(parse_bigint_literal("0xFF"), Ok(BigInt::from(255)));
        assert_eq!(parse_bigint_literal("-0xFF"), Ok(BigInt::from(-255)));
        assert_eq!(parse_bigint_literal("0b1010"), Ok(BigInt::from(10)));
        assert_eq!(parse_bigint_literal("0o755"), Ok(BigInt::from(493)));
        assert_eq!(parse_bigint_literal("1_000"), Ok(BigInt::from(1000)));
        // No TooLarge for bigints.
        assert_eq!(
            parse_bigint_literal("99999999999999999999"),
            Ok("99999999999999999999".parse::<BigInt>().unwrap())
        );
        assert_eq!(parse_bigint_literal("0x"), Err(IntLitError::NoDigits));
        assert_eq!(
            parse_bigint_literal("0b2"),
            Err(IntLitError::InvalidDigits {
                base: 2,
                positions: vec![(2, '2')]
            })
        );
    }

    #[test]
    fn float_normalization() {
        assert_eq!(normalize_float_literal("1.5"), "1.5");
        assert_eq!(normalize_float_literal("1_000.000_1"), "1000.0001");
        assert_eq!(normalize_float_literal("1_0e1_0"), "10e10");
        assert_eq!(normalize_float_literal("1e-1_0"), "1e-10");
    }
}
