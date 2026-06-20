use bex_vm_types::Value;

use super::{BamlClassInt, PackageBamlImpl};
use crate::errors::{VmBamlError, VmPanic, VmRustFnError};

impl BamlClassInt for PackageBamlImpl {
    fn to_json(int: i64) -> Value {
        Value::int(int)
    }

    // ── Comparisons / clamping ────────────────────────────────────────────────

    fn abs(int: i64) -> Result<i64, VmRustFnError> {
        // Operate on the *true* 64-bit value. The genuine overflow case is
        // `i64::MIN`, whose absolute value (`2^63`) cannot be represented; the
        // old guard wrongly fired at `Value::INT_MIN` (`-2^62`), which is a
        // perfectly representable value (and, because of the upstream 63-bit
        // truncation, also what a positive `2^62` literal decodes to). Guarding
        // there made `(2^62).abs()` throw on a normal positive integer.
        if int == i64::MIN {
            return Err(VmBamlError::InvalidArgument {
                message: "int.abs: cannot represent the absolute value of int.min_value() (would overflow)".to_string(),
            }
            .into());
        }
        // `int` is stored in the i63 tagged representation, where every int
        // operation (`+`, `-`, `*`) wraps two's-complement. `abs` wraps the
        // same way: the absolute value of `Value::INT_MIN` (`-2^62`) is `2^62`,
        // one past `Value::INT_MAX`, which wraps back to `Value::INT_MIN` —
        // exactly mirroring `i64::MIN.wrapping_abs() == i64::MIN`.
        if int == Value::INT_MIN {
            return Ok(Value::INT_MIN);
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

    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap,
        clippy::cast_possible_truncation
    )]
    fn random(lower: i64, upper: i64) -> Result<i64, VmRustFnError> {
        if lower >= upper {
            return Err(VmBamlError::InvalidArgument {
                message: format!(
                    "int.random: lower ({lower}) must be less than upper ({upper}); range [{lower}, {upper}) is empty"
                ),
            }
            .into());
        }
        // range fits in u128; for valid lower < upper as i64, range ∈ [1, 2^64 - 1].
        // The i128 difference is provably positive, so the cast to u128 is safe.
        let range = (i128::from(upper) - i128::from(lower)) as u128;
        // Rejection sampling for an unbiased result: accept r only if it lies
        // below the largest multiple of `range` that fits in 2^64. This rejects
        // the (typically tiny) tail that would otherwise modulo-bias toward
        // smaller values.
        let threshold = (1u128 << 64) / range * range;
        let mut buf = [0u8; 8];
        loop {
            getrandom::getrandom(&mut buf).map_err(|e| VmPanic::HostUnavailable {
                resource: "entropy".to_string(),
                message: format!("getrandom failed in int.random: {e}"),
            })?;
            let r = u128::from(u64::from_le_bytes(buf));
            if r < threshold {
                // r % range < range ≤ 2^64 - 1 < i64 width when added to lower,
                // and lower + offset reconstructs a value in [lower, upper) ⊂ i64.
                let offset = (r % range) as i128;
                return Ok((i128::from(lower) + offset) as i64);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `int.abs` returns the magnitude for ordinary values, wraps the i63
    /// minimum back to itself (consistent with how int `+`/`-`/`*` wrap), and
    /// only errors for the genuine `i64::MIN`.
    #[test]
    fn abs_matches_i63_wrapping_semantics() {
        let abs = <PackageBamlImpl as BamlClassInt>::abs;

        assert_eq!(abs(3).unwrap(), 3);
        assert_eq!(abs(-7).unwrap(), 7);
        assert_eq!(abs(0).unwrap(), 0);

        // `int.max_value()` is its own absolute value; `int.min_value() + 1`
        // negates to `int.max_value()` without wrapping.
        assert_eq!(abs(Value::INT_MAX).unwrap(), Value::INT_MAX);
        assert_eq!(abs(Value::INT_MIN + 1).unwrap(), Value::INT_MAX);

        // `|int.min_value()|` (`2^62`) is one past `INT_MAX` and wraps back to
        // `int.min_value()` — it must NOT throw (the old bug fired here, which
        // is also what a positive `2^62` literal decodes to after the upstream
        // 63-bit truncation).
        assert_eq!(abs(Value::INT_MIN).unwrap(), Value::INT_MIN);

        // The only genuine overflow is `i64::MIN`, whose `|x|` is `2^63`.
        assert!(abs(i64::MIN).is_err());
    }
}
