//! `baml.Int`.

use super::{NativeBamlInt, Natives, Root, Scope, Str, Thrown};

/// The smallest draw of `Rng.random_int`, which draws 63 random bits:
/// `-2^62..2^62`.
const DRAW_MIN: i64 = -(1 << 62);

/// How many values `Rng.random_int` draws from.
const DRAW_SPAN: u128 = 1 << 63;

impl NativeBamlInt for Natives {
    /// The smallest int has no positive counterpart and is its own absolute
    /// value, as it is its own negation.
    fn abs(_scope: &mut Scope<'_>, int: i64) -> i64 {
        int.wrapping_abs()
    }

    fn isqrt<'s>(scope: &mut Scope<'s>, int: i64) -> Result<i64, Thrown<'s>> {
        if int < 0 {
            return Err(scope.invalid_argument(&format!(
                "int.isqrt: negative input ({int}) has no integer square root"
            )));
        }
        Ok(int.isqrt())
    }

    /// `int ** exp`, saturating to the largest or the smallest int, whichever
    /// is on the result's side. A
    /// negative `exp` gives zero.
    fn pow(int: i64, exp: i64) -> i64 {
        if exp < 0 {
            return 0;
        }
        let even = exp & 1 == 0;
        if int == -1 {
            return if even { 1 } else { -1 };
        }
        // Past 63 any base other than 0, 1 and -1 overflows, so a larger
        // exponent need not be told apart.
        let exp = u32::try_from(exp).unwrap_or(u32::MAX);
        int.checked_pow(exp)
            .unwrap_or(if int >= 0 || even { i64::MAX } else { i64::MIN })
    }

    fn ilog<'s>(scope: &mut Scope<'s>, int: i64, base: i64) -> Result<i64, Thrown<'s>> {
        if int <= 0 {
            return Err(scope.invalid_argument(&format!(
                "int.ilog: undefined for non-positive input (self = {int})"
            )));
        }
        if base < 2 {
            return Err(scope.invalid_argument(&format!("int.ilog: base must be >= 2, got {base}")));
        }
        // Both are in the logarithm's domain, so there is one.
        Ok(i64::from(int.checked_ilog(base).unwrap_or_default()))
    }

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

    fn parse<'s>(scope: &mut Scope<'s>, text: Root<'s, Str>) -> Result<i64, Thrown<'s>> {
        let text = text.get(scope).as_str();
        let message = match text.parse::<i64>() {
            Ok(value) => return Ok(value),
            Err(error) => format!("int.parse: cannot parse {text:?} as int: {error}"),
        };
        Err(scope.parse_error(&message))
    }

    /// Map a draw from the `DRAW_SPAN` values starting at `DRAW_MIN` onto
    /// `lower..upper`, or give `upper` for a draw to reject: one past the
    /// largest multiple of the range's size, which would favour the low end.
    fn _random_in_range(draw: i64, lower: i64, upper: i64) -> i64 {
        if lower >= upper {
            return upper;
        }
        let range = (i128::from(upper) - i128::from(lower)).cast_unsigned();
        // A draw outside those values lands past the zone and is rejected:
        // one below `DRAW_MIN` wraps to far above it.
        let offset = (i128::from(draw) - i128::from(DRAW_MIN)).cast_unsigned();
        let zone = DRAW_SPAN / range * range;
        if offset >= zone {
            return upper;
        }
        i64::try_from(i128::from(lower) + (offset % range).cast_signed()).unwrap_or(upper)
    }
}
