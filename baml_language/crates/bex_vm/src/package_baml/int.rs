use super::{BamlClassInt, PackageBamlImpl};
use crate::errors::{VmBamlError, VmRustFnError};

/// Number of values `Rng.random_int` draws from: every `int`. One more than
/// `u64::MAX`, so the rejection arithmetic is in `u128`.
const DRAW_SPAN: u128 = 1 << 64;

impl BamlClassInt for PackageBamlImpl {
    // ── Magnitude ─────────────────────────────────────────────────────────────

    fn abs(int: i64) -> i64 {
        // Wraps: the absolute value of the smallest int is itself.
        int.wrapping_abs()
    }

    // No `min` / `max` / `clamp` here: `int` gets them from
    // `baml.ops.Compare`. See the note in `int.baml`.

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
        // or if exp is even (i.e. mathematical sign of result). Use the
        // *original* `exp` for the parity check: clamping to `u32::MAX` (odd)
        // above would otherwise flip the sign for even exponents above u32
        // range.
        let saturated = if int >= 0 || exp & 1 == 0 {
            i64::MAX
        } else {
            i64::MIN
        };
        int.checked_pow(exp_u32).unwrap_or(saturated)
    }

    fn parse(text: &bex_str::BexStr) -> Result<i64, VmRustFnError> {
        let s = text.as_str();
        s.parse::<i64>().map_err(|e| -> VmRustFnError {
            VmBamlError::ParseError {
                message: format!("int.parse: cannot parse {s:?} as int: {e}"),
            }
            .into()
        })
    }

    fn _random_in_range(draw: i64, lower: i64, upper: i64) -> i64 {
        if lower >= upper {
            return upper;
        }
        // At most `2^64 - 1`: the range is half-open.
        let range = (i128::from(upper) - i128::from(lower)).cast_unsigned();
        // The draw's offset from `int.min_value()`, in `[0, DRAW_SPAN)`.
        let u = (i128::from(draw) - i128::from(i64::MIN)).cast_unsigned();

        // Reject the remainder above the largest multiple of `range`.
        let zone = DRAW_SPAN / range * range;
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

#[cfg(test)]
mod tests {
    use super::{BamlClassInt, PackageBamlImpl};

    fn map(draw: i64, lower: i64, upper: i64) -> i64 {
        PackageBamlImpl::_random_in_range(draw, lower, upper)
    }

    /// Draws span all 2^64 ints, so the widest range, `[MIN, MAX)`, maps
    /// each draw to itself and rejects only the one draw past its end.
    #[test]
    fn random_in_range_covers_the_full_int_range() {
        for draw in [i64::MIN, -1, 0, 1, i64::MAX - 1] {
            assert_eq!(map(draw, i64::MIN, i64::MAX), draw);
        }
        assert_eq!(map(i64::MAX, i64::MIN, i64::MAX), i64::MAX);
        // 2^64 = 1 (mod 5): exactly one draw, the largest, is rejected.
        assert_eq!(map(i64::MIN, 0, 5), 0);
        assert_eq!(map(i64::MIN + 4, 0, 5), 4);
        assert_eq!(map(i64::MAX - 1, 0, 5), 4);
        assert_eq!(map(i64::MAX, 0, 5), 5);
    }
}
