//! `baml.Float`.

use super::{NativeBamlFloat, Natives, Root, Scope, Str, Thrown};

/// The `int`s a `float` converts to: `-2^62 <= value < 2^62`.
const MIN: f64 = -4_611_686_018_427_387_904.0;
const MAX_PLUS_ONE: f64 = 4_611_686_018_427_387_904.0;

fn to_int<'s>(scope: &mut Scope<'s>, value: f64, operation: &str) -> Result<i64, Thrown<'s>> {
    if value.is_nan() {
        return Err(
            scope.invalid_argument(&format!("float.{operation}: cannot convert NaN to int"))
        );
    }
    if !(MIN..MAX_PLUS_ONE).contains(&value) {
        return Err(
            scope.invalid_argument(&format!("float.{operation}: {value} is out of int range"))
        );
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the value is whole and in range"
    )]
    Ok(value as i64)
}

impl NativeBamlFloat for Natives {
    fn is_nan(float: f64) -> bool {
        float.is_nan()
    }

    fn is_infinite(float: f64) -> bool {
        float.is_infinite()
    }

    /// Total: a zero keeps its sign, and NaN, which the order ranks above
    /// every number, is positive.
    fn signum(float: f64) -> f64 {
        if float.is_nan() { 1.0 } else { float.signum() }
    }

    fn sqrt(float: f64) -> f64 {
        float.sqrt()
    }

    fn abs(float: f64) -> f64 {
        float.abs()
    }

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

    fn log10(float: f64) -> f64 {
        float.log10()
    }

    fn cbrt(float: f64) -> f64 {
        float.cbrt()
    }

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

    fn pow(float: f64, exp: f64) -> f64 {
        float.powf(exp)
    }

    fn log(float: f64, base: f64) -> f64 {
        float.log(base)
    }

    fn hypot(float: f64, other: f64) -> f64 {
        float.hypot(other)
    }

    fn atan2(float: f64, other: f64) -> f64 {
        float.atan2(other)
    }

    fn ifloor<'s>(scope: &mut Scope<'s>, float: f64) -> Result<i64, Thrown<'s>> {
        to_int(scope, float.floor(), "ifloor")
    }

    fn iceil<'s>(scope: &mut Scope<'s>, float: f64) -> Result<i64, Thrown<'s>> {
        to_int(scope, float.ceil(), "iceil")
    }

    fn iround<'s>(scope: &mut Scope<'s>, float: f64) -> Result<i64, Thrown<'s>> {
        to_int(scope, float.round(), "iround")
    }

    fn itrunc<'s>(scope: &mut Scope<'s>, float: f64) -> Result<i64, Thrown<'s>> {
        to_int(scope, float.trunc(), "itrunc")
    }

    fn parse<'s>(scope: &mut Scope<'s>, text: Root<'s, Str>) -> Result<f64, Thrown<'s>> {
        let parsed = text.get(scope).as_str().parse::<f64>();
        parsed.map_err(|error| {
            let message = format!(
                "float.parse: cannot parse {:?} as float: {error}",
                text.get(scope).as_str()
            );
            scope.parse_error(&message)
        })
    }

    /// A float in `[0, 1)` from the 53 bits of `draw` above its low ten.
    fn _unit_from_draw(draw: i64) -> f64 {
        const BITS: u32 = 53;
        let bits = (draw.cast_unsigned() >> 10) & ((1 << BITS) - 1);
        #[expect(
            clippy::cast_precision_loss,
            reason = "53 bits fit a float's mantissa exactly"
        )]
        let unit = bits as f64 / (1u64 << BITS) as f64;
        unit
    }

    fn pi() -> f64 {
        std::f64::consts::PI
    }

    fn e() -> f64 {
        std::f64::consts::E
    }

    /// `(1 + sqrt(5)) / 2`.
    fn golden_ratio() -> f64 {
        1.618_033_988_749_895
    }

    fn nan() -> f64 {
        f64::NAN
    }

    fn inf() -> f64 {
        f64::INFINITY
    }

    fn max_finite() -> f64 {
        f64::MAX
    }

    fn min_finite() -> f64 {
        f64::MIN
    }

    fn epsilon() -> f64 {
        f64::EPSILON
    }
}
