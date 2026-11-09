//! Heap-neutral scalar bodies shared by interpreter and typed packs.
use crate::Value;

use crate::errors::{VmBamlError, VmPanic, VmRustFnError};

/// Number of values in the BAML `int` domain.
const INT_DOMAIN_SPAN: u128 = (Value::INT_MAX as i128 - Value::INT_MIN as i128 + 1).cast_unsigned();

// BAML int is i63 (the runtime reserves one bit for the tagged-pointer
// Value tag). Range: `[-2^62, 2^62 - 1]`. `-2^62` and `2^62` are both
// powers of two, exactly representable in f64 (as integers with
// magnitudes far past f64's 53-bit mantissa but with no fractional part).
// So the in-range predicate is `MIN_F <= r < MAX_PLUS_ONE_F` — note the
// strict upper bound. NaN fails this check via the usual NaN-comparison
// rules.
const MIN_F: f64 = -4_611_686_018_427_387_904.0; // -2^62
const MAX_PLUS_ONE_F: f64 = 4_611_686_018_427_387_904.0; // 2^62

#[allow(clippy::cast_possible_truncation)]
fn float_to_int(value: f64, op: &str) -> Result<i64, VmRustFnError> {
    if value.is_nan() {
        return Err(VmBamlError::InvalidArgument {
            message: format!("float.{op}: cannot convert NaN to int"),
        }
        .into());
    }
    if !(MIN_F..MAX_PLUS_ONE_F).contains(&value) {
        return Err(VmBamlError::InvalidArgument {
            message: format!("float.{op}: {value} is out of int range"),
        }
        .into());
    }
    Ok(value as i64)
}

pub mod int {
    use super::*;
    // ── Magnitude ─────────────────────────────────────────────────────────────

    pub fn abs(int: i64) -> Result<i64, VmRustFnError> {
        // BAML int is i63 (low bit reserved for the Value tag). The
        // absolute value of `Value::INT_MIN` (`-2^62`) is `2^62`, which is
        // ONE PAST `Value::INT_MAX` and therefore not representable.
        if int == Value::INT_MIN {
            return Err(VmPanic::IntegerOverflow {
                message: format!("abs({int}) overflows int"),
            }
            .into());
        }
        Ok(int.wrapping_abs())
    }

    // No `min` / `max` / `clamp` here: `int` gets them from
    // `baml.ops.Compare`. See the note in `int.baml`.

    // ── Math ──────────────────────────────────────────────────────────────────

    pub fn isqrt(int: i64) -> Result<i64, VmRustFnError> {
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

    pub fn pow(int: i64, exp: i64) -> i64 {
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

    pub fn parse(text: &str) -> Result<i64, VmRustFnError> {
        let s = text;
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

    pub fn _random_in_range(draw: i64, lower: i64, upper: i64) -> i64 {
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

    pub fn ilog(int: i64, base: i64) -> Result<i64, VmRustFnError> {
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

    pub fn leading_zeros(int: i64) -> i64 {
        i64::from(int.leading_zeros())
    }

    pub fn leading_ones(int: i64) -> i64 {
        i64::from(int.leading_ones())
    }

    pub fn trailing_zeros(int: i64) -> i64 {
        i64::from(int.trailing_zeros())
    }

    pub fn trailing_ones(int: i64) -> i64 {
        i64::from(int.trailing_ones())
    }

    pub fn count_zeros(int: i64) -> i64 {
        i64::from(int.count_zeros())
    }

    pub fn count_ones(int: i64) -> i64 {
        i64::from(int.count_ones())
    }

    // Note: `max_value()` and `min_value()` are implemented directly in
    // `int.baml` as BAML literal expressions — no Rust trampoline needed.
}

pub mod float {
    use super::*;
    // ── Predicates ────────────────────────────────────────────────────────────

    pub fn is_nan(float: f64) -> bool {
        float.is_nan()
    }

    pub fn is_infinite(float: f64) -> bool {
        float.is_infinite()
    }

    // Note: `is_finite` is implemented directly in `float.baml`.

    // ── Magnitude ─────────────────────────────────────────────────────────────

    pub fn abs(float: f64) -> f64 {
        float.abs()
    }

    // No `min` / `max` / `clamp` here: `float` gets them from
    // `baml.ops.Compare`, whose defaults are driven by the total float order
    // (`bex_vm_types::float_order`). See the note in `float.baml`.

    pub fn signum(float: f64) -> f64 {
        // Native because BAML cannot observe the sign of a zero: the total
        // order treats `-0.0` and `0.0` as one value (`==` says equal, and
        // `-0.0 < 0.0` is false), and `1.0 / 0.0` throws rather than yielding
        // a signed infinity. `f64::signum` already gives ±1.0 for ±0.0; only
        // the NaN case is overridden, to +1.0, so signum is total and never
        // returns NaN — matching the order, which ranks NaN above every
        // number.
        if float.is_nan() { 1.0 } else { float.signum() }
    }

    // ── Rounding (returns float) ──────────────────────────────────────────────

    pub fn floor(float: f64) -> f64 {
        float.floor()
    }

    pub fn ceil(float: f64) -> f64 {
        float.ceil()
    }

    pub fn round(float: f64) -> f64 {
        float.round()
    }

    pub fn trunc(float: f64) -> f64 {
        float.trunc()
    }

    pub fn fract(float: f64) -> f64 {
        float.fract()
    }

    // ── Rounding to int (fallible) ────────────────────────────────────────────

    pub fn ifloor(float: f64) -> Result<i64, VmRustFnError> {
        float_to_int(float.floor(), "ifloor")
    }

    pub fn iceil(float: f64) -> Result<i64, VmRustFnError> {
        float_to_int(float.ceil(), "iceil")
    }

    pub fn iround(float: f64) -> Result<i64, VmRustFnError> {
        float_to_int(float.round(), "iround")
    }

    pub fn itrunc(float: f64) -> Result<i64, VmRustFnError> {
        float_to_int(float.trunc(), "itrunc")
    }

    // ── Power / logarithm ─────────────────────────────────────────────────────

    pub fn sqrt(float: f64) -> f64 {
        float.sqrt()
    }

    pub fn pow(float: f64, exp: f64) -> f64 {
        float.powf(exp)
    }

    pub fn log(float: f64, base: f64) -> f64 {
        float.log(base)
    }

    // Note: `exp`, `ln` and `log2` are implemented directly in `float.baml`.
    // `log10` is native because the `.baml`-level `self.log(10.0)` is a ratio
    // of logarithms and loses the last bits at clean inputs (`1000.0` gives
    // `2.9999999999999996`). Rust bounds the precision of neither, so this
    // buys a measured improvement on the cases tested, not a guarantee.

    pub fn log10(float: f64) -> f64 {
        float.log10()
    }

    pub fn cbrt(float: f64) -> f64 {
        // Native because `pow(1.0 / 3.0)` is NaN for every negative base (a
        // non-integer exponent) and misses perfect cubes (`1000.0` ->
        // `9.999999999999998`). The NaN half is definitional; the accuracy
        // half is measured.
        float.cbrt()
    }

    pub fn hypot(float: f64, other: f64) -> f64 {
        float.hypot(other)
    }

    // ── Trigonometry ──────────────────────────────────────────────────────────

    pub fn sin(float: f64) -> f64 {
        float.sin()
    }

    pub fn cos(float: f64) -> f64 {
        float.cos()
    }

    pub fn tan(float: f64) -> f64 {
        float.tan()
    }

    pub fn asin(float: f64) -> f64 {
        float.asin()
    }

    pub fn acos(float: f64) -> f64 {
        float.acos()
    }

    pub fn atan(float: f64) -> f64 {
        float.atan()
    }

    pub fn atan2(float: f64, other: f64) -> f64 {
        float.atan2(other)
    }

    pub fn sinh(float: f64) -> f64 {
        float.sinh()
    }

    pub fn cosh(float: f64) -> f64 {
        float.cosh()
    }

    pub fn tanh(float: f64) -> f64 {
        float.tanh()
    }

    pub fn asinh(float: f64) -> f64 {
        float.asinh()
    }

    pub fn acosh(float: f64) -> f64 {
        float.acosh()
    }

    pub fn atanh(float: f64) -> f64 {
        float.atanh()
    }

    // Note: `to_radians` and `to_degrees` are implemented directly in
    // `float.baml` — no Rust trampoline needed.

    // ── Parsing / randomness ──────────────────────────────────────────────────

    pub fn parse(text: &str) -> Result<f64, VmRustFnError> {
        let s = text;
        s.parse::<f64>().map_err(|e| {
            VmBamlError::ParseError {
                message: format!("float.parse: cannot parse {s:?} as float: {e}"),
            }
            .into()
        })
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "bits <= 2^53 - 1 fits f64's mantissa exactly"
    )]
    pub fn _unit_from_draw(draw: i64) -> f64 {
        // Bit 63 only repeats the i63 sign bit, so use bits 10..=62.
        let bits = (draw.cast_unsigned() >> 10) & ((1u64 << 53) - 1); // <= 2^53 - 1
        bits as f64 * (1.0 / (1u64 << 53) as f64)
    }

    // ── Constants ─────────────────────────────────────────────────────────────

    pub fn pi() -> f64 {
        std::f64::consts::PI
    }

    pub fn e() -> f64 {
        std::f64::consts::E
    }

    pub fn golden_ratio() -> f64 {
        // (1 + sqrt(5)) / 2, rounded to f64.
        1.618_033_988_749_895
    }

    pub fn nan() -> f64 {
        f64::NAN
    }

    pub fn inf() -> f64 {
        f64::INFINITY
    }

    pub fn max_finite() -> f64 {
        f64::MAX
    }

    pub fn min_finite() -> f64 {
        f64::MIN
    }

    pub fn epsilon() -> f64 {
        f64::EPSILON
    }
}
