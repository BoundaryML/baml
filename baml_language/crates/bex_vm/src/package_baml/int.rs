use bex_vm_types::Value;

use super::{BamlClassInt, PackageBamlImpl};
use crate::errors::{VmBamlError, VmRustFnError};

/// Number of values in the BAML `int` domain.
const INT_DOMAIN_SPAN: u128 =
    (Value::INT_MAX as i128 - Value::INT_MIN as i128 + 1).cast_unsigned();

impl BamlClassInt for PackageBamlImpl {
    // ── Comparisons / clamping ────────────────────────────────────────────────

    fn abs(int: i64) -> Result<i64, VmRustFnError> {
        // BAML int is i63 (low bit reserved for the Value tag). The
        // absolute value of `Value::INT_MIN` (`-2^62`) is `2^62`, which is
        // ONE PAST `Value::INT_MAX` and therefore not representable.
        if int == Value::INT_MIN {
            return Err(VmBamlError::InvalidArgument {
                message: "int.abs: cannot represent the absolute value of int.min_value() (would overflow)".to_string(),
            }
            .into());
        }
        Ok(int.wrapping_abs())
    }

    fn min(int: i64, other: i64) -> i64 {
        std::cmp::min(int, other)
    }

    fn max(int: i64, other: i64) -> i64 {
        std::cmp::max(int, other)
    }

    fn clamp(int: i64, min: i64, max: i64) -> i64 {
        // Two-step (clamp) so we do not panic when min > max — Rust's
        // `i64::clamp` debug-asserts `min <= max`. The behavior matches:
        // first cap at max, then floor at min.
        let v = std::cmp::min(int, max);
        std::cmp::max(v, min)
    }

    // ── Math ──────────────────────────────────────────────────────────────────

    fn isqrt(int: i64) -> Result<i64, VmRustFnError> {
        if int < 0 {
            return Err(VmBamlError::InvalidArgument {
                message: format!("int.isqrt: negative input ({int}) has no integer square root"),
            }
            .into());
        }
        // Result of isqrt always fits in i63 since INT_MAX < 2^62 and
        // floor(sqrt(2^62)) = 2^31.
        Ok(int.isqrt())
    }

    fn pow(int: i64, exp: i64) -> i64 {
        if exp < 0 {
            return 0;
        }
        // -1 to any power is ±1 with parity determined by the exponent. Special-
        // cased so we use the original (i64) parity, since clamping to u32::MAX
        // (odd) would otherwise produce -1 for an even exp like 2^32.
        if int == -1 {
            return if exp & 1 == 0 { 1 } else { -1 };
        }
        // For any other base, exp > ~63 always overflows for |int| >= 2, so
        // clamping to u32::MAX is safe — checked_pow returns None and we
        // saturate. For |int| <= 1 it never overflows. checked_pow uses the
        // mathematical convention 0^0 == 1.
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let exp_u32 = if exp > i64::from(u32::MAX) {
            u32::MAX
        } else {
            exp as u32
        };
        // Saturation value if checked_pow overflows or if the result
        // doesn't fit in i63: positive if base >= 0, or if exp is even
        // (i.e. mathematical sign of result). Use the *original* `exp`
        // for the parity check: clamping to `u32::MAX` (odd) above would
        // otherwise flip the sign for even exponents above u32 range.
        let saturated = if int >= 0 || exp & 1 == 0 {
            Value::INT_MAX
        } else {
            Value::INT_MIN
        };
        match int.checked_pow(exp_u32) {
            Some(v) if (Value::INT_MIN..=Value::INT_MAX).contains(&v) => v,
            _ => saturated,
        }
    }

    fn parse(text: &bex_str::BexStr) -> Result<i64, VmRustFnError> {
        let s = text.as_str();
        let v: i64 = s.parse::<i64>().map_err(|e| -> VmRustFnError {
            VmBamlError::ParseError {
                message: format!("int.parse: cannot parse {s:?} as int: {e}"),
            }
            .into()
        })?;
        if !(Value::INT_MIN..=Value::INT_MAX).contains(&v) {
            return Err(VmBamlError::ParseError {
                message: format!(
                    "int.parse: {s:?} is outside the representable BAML int range [{}, {}]",
                    Value::INT_MIN,
                    Value::INT_MAX
                ),
            }
            .into());
        }
        Ok(v)
    }

    fn _random_in_range(draw: i64, lower: i64, upper: i64) -> i64 {
        if lower >= upper {
            return upper;
        }
        let range = (i128::from(upper) - i128::from(lower)).cast_unsigned();
        let u = (i128::from(draw) - i128::from(Value::INT_MIN)).cast_unsigned();

        // Reject the remainder above the largest multiple of `range`.
        let zone = INT_DOMAIN_SPAN / range * range;
        if u >= zone {
            return upper;
        }
        i64::try_from(i128::from(lower) + (u % range).cast_signed())
            .unwrap_or_else(|_| unreachable!("int._random_in_range: offset is below upper - lower"))
    }

    fn ilog(int: i64, base: i64) -> Result<i64, VmRustFnError> {
        if int <= 0 {
            return Err(VmBamlError::InvalidArgument {
                message: format!("int.ilog: undefined for non-positive input (self = {int})"),
            }
            .into());
        }
        if base < 2 {
            return Err(VmBamlError::InvalidArgument {
                message: format!("int.ilog: base must be >= 2, got {base}"),
            }
            .into());
        }
        // Both invariants hold above, so checked_ilog cannot return None.
        Ok(i64::from(int.checked_ilog(base).unwrap_or_else(|| {
            unreachable!("int.ilog: invariants self > 0 && base >= 2 already enforced")
        })))
    }

    // ── Bit operations ────────────────────────────────────────────────────────

    fn leading_zeros(int: i64) -> i64 {
        i64::from(int.leading_zeros())
    }

    fn leading_ones(int: i64) -> i64 {
        i64::from(int.leading_ones())
    }

    fn trailing_zeros(int: i64) -> i64 {
        i64::from(int.trailing_zeros())
    }

    fn trailing_ones(int: i64) -> i64 {
        i64::from(int.trailing_ones())
    }

    fn count_zeros(int: i64) -> i64 {
        i64::from(int.count_zeros())
    }

    fn count_ones(int: i64) -> i64 {
        i64::from(int.count_ones())
    }

    // Note: `max_value()` and `min_value()` are implemented directly in
    // `int.baml` as BAML literal expressions — no Rust trampoline needed.
}
