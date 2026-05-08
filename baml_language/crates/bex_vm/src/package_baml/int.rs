use bex_vm_types::Value;

use super::{BamlClassInt, PackageBamlImpl};
use crate::errors::{VmBamlError, VmPanic, VmRustFnError};

impl BamlClassInt for PackageBamlImpl {
    fn to_json(int: i64) -> Value {
        Value::Int(int)
    }
    // ── Comparisons / clamping ────────────────────────────────────────────────

    fn abs(int: i64) -> Result<i64, VmRustFnError> {
        int.checked_abs().ok_or_else(|| {
            VmBamlError::InvalidArgument {
                message: "int.abs: cannot represent the absolute value of int.min_value() (would overflow)".to_string(),
            }
            .into()
        })
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
        // Saturation value if checked_pow overflows: positive if base >= 0,
        // or if exp is even (i.e. mathematical sign of result).
        let saturated = if int >= 0 || exp_u32 % 2 == 0 {
            i64::MAX
        } else {
            i64::MIN
        };
        int.checked_pow(exp_u32).unwrap_or(saturated)
    }

    fn parse(text: &str) -> Result<i64, VmRustFnError> {
        text.parse::<i64>().map_err(|e| {
            VmBamlError::ParseError {
                message: format!("int.parse: cannot parse {text:?} as int: {e}"),
            }
            .into()
        })
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
