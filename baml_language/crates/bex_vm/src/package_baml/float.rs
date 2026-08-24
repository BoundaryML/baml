use super::{BamlClassFloat, PackageBamlImpl};
use crate::errors::{VmBamlError, VmRustFnError};

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

impl BamlClassFloat for PackageBamlImpl {
    // ── Predicates ────────────────────────────────────────────────────────────

    fn is_nan(float: f64) -> bool {
        float.is_nan()
    }

    fn is_infinite(float: f64) -> bool {
        float.is_infinite()
    }

    // Note: `is_finite` is implemented directly in `float.baml`.

    // ── Comparisons / clamping ────────────────────────────────────────────────

    fn abs(float: f64) -> f64 {
        float.abs()
    }

    fn min(float: f64, other: f64) -> f64 {
        // `f64::min` returns the non-NaN operand if exactly one is NaN
        // (NaN-suppressing). See doc on the .baml side.
        float.min(other)
    }

    fn max(float: f64, other: f64) -> f64 {
        float.max(other)
    }

    fn clamp(float: f64, min: f64, max: f64) -> f64 {
        // Two-step (cap then floor) to avoid `f64::clamp`'s `min <= max`
        // requirement. NaN propagates through `f64::min`/`max`.
        float.min(max).max(min)
    }

    // ── Rounding (returns float) ──────────────────────────────────────────────

    fn floor(float: f64) -> f64 {
        float.floor()
    }

    fn ceil(float: f64) -> f64 {
        float.ceil()
    }

    fn round(float: f64) -> f64 {
        float.round()
    }

    fn trunc(float: f64) -> f64 {
        float.trunc()
    }

    fn fract(float: f64) -> f64 {
        float.fract()
    }

    // ── Rounding to int (fallible) ────────────────────────────────────────────

    fn ifloor(float: f64) -> Result<i64, VmRustFnError> {
        float_to_int(float.floor(), "ifloor")
    }

    fn iceil(float: f64) -> Result<i64, VmRustFnError> {
        float_to_int(float.ceil(), "iceil")
    }

    fn iround(float: f64) -> Result<i64, VmRustFnError> {
        float_to_int(float.round(), "iround")
    }

    fn itrunc(float: f64) -> Result<i64, VmRustFnError> {
        float_to_int(float.trunc(), "itrunc")
    }

    // ── Power / logarithm ─────────────────────────────────────────────────────

    fn sqrt(float: f64) -> f64 {
        float.sqrt()
    }

    fn pow(float: f64, exp: f64) -> f64 {
        float.powf(exp)
    }

    fn log(float: f64, base: f64) -> f64 {
        float.log(base)
    }

    fn hypot(float: f64, other: f64) -> f64 {
        float.hypot(other)
    }

    // ── Trigonometry ──────────────────────────────────────────────────────────

    fn sin(float: f64) -> f64 {
        float.sin()
    }

    fn cos(float: f64) -> f64 {
        float.cos()
    }

    fn tan(float: f64) -> f64 {
        float.tan()
    }

    fn asin(float: f64) -> f64 {
        float.asin()
    }

    fn acos(float: f64) -> f64 {
        float.acos()
    }

    fn atan(float: f64) -> f64 {
        float.atan()
    }

    fn atan2(float: f64, other: f64) -> f64 {
        float.atan2(other)
    }

    fn sinh(float: f64) -> f64 {
        float.sinh()
    }

    fn cosh(float: f64) -> f64 {
        float.cosh()
    }

    fn tanh(float: f64) -> f64 {
        float.tanh()
    }

    fn asinh(float: f64) -> f64 {
        float.asinh()
    }

    fn acosh(float: f64) -> f64 {
        float.acosh()
    }

    fn atanh(float: f64) -> f64 {
        float.atanh()
    }

    // Note: `to_radians` and `to_degrees` are implemented directly in
    // `float.baml` — no Rust trampoline needed.

    // ── Parsing / randomness ──────────────────────────────────────────────────

    fn parse(text: &bex_str::BexStr) -> Result<f64, VmRustFnError> {
        let s = text.as_str();
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
    fn _unit_from_draw(draw: i64) -> f64 {
        // Uniform draw on [0, 1) using 53 mantissa bits: take 53 uniform bits
        // and multiply by 2^-53.
        //
        // Bits 10..=62 are the top 53 of the 63 the draw actually carries. A BAML
        // `int` is i63 held sign-extended in a 64-bit word, so bit 63 merely
        // repeats bit 62; taking the top 53 bits *of the word* would hand back
        // two identical leading bits, hence the shift by 10 rather than 11. Any
        // 53 of bits 0..=62 are equally uniform, so this prefers the high end
        // purely as insurance: a generator with weak low-order bits (a classic
        // LCG, say) breaks `random_int`'s uniformity contract, but only the low
        // bits, and nothing forces a user-supplied `Rng` to be well-behaved.
        let bits = (draw.cast_unsigned() >> 10) & ((1u64 << 53) - 1); // <= 2^53 - 1
        // 2^-53 = 1.0 / (1u64 << 53). The cast `bits as f64` is lossless
        // because bits <= 2^53 - 1 fits in f64's 53-bit mantissa, and scaling by
        // a power of two is exact, so every result is a true multiple of 2^-53.
        bits as f64 * (1.0 / (1u64 << 53) as f64)
    }

    // ── Constants ─────────────────────────────────────────────────────────────

    fn pi() -> f64 {
        std::f64::consts::PI
    }

    fn e() -> f64 {
        std::f64::consts::E
    }

    fn golden_ratio() -> f64 {
        // (1 + sqrt(5)) / 2, rounded to f64.
        1.618_033_988_749_895
    }

    fn nan() -> f64 {
        f64::NAN
    }

    fn inf() -> f64 {
        f64::INFINITY
    }
}
