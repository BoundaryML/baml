//! `baml.Int`.

use super::{NativeBamlInt, Natives, Root, Scope, Str, Thrown};

/// The range `int.min_value()..=int.max_value()` names, which `pow`
/// saturates to and `parse` accepts: `-2^62..2^62`.
const MIN: i64 = -(1 << 62);
const MAX: i64 = (1 << 62) - 1;

/// How many values `MIN..=MAX` holds.
const SPAN: u128 = 1 << 63;

impl NativeBamlInt for Natives {
    /// The smallest int has no positive counterpart, so its absolute value
    /// is a panic and not itself. Compiled code takes an absolute value
    /// where it is written, with the same check.
    fn abs<'s>(scope: &mut Scope<'s>, int: i64) -> Result<i64, Thrown<'s>> {
        int.checked_abs()
            .ok_or_else(|| scope.integer_overflow(&format!("abs({int}) overflows int")))
    }

    fn isqrt<'s>(scope: &mut Scope<'s>, int: i64) -> Result<i64, Thrown<'s>> {
        if int < 0 {
            return Err(scope.invalid_argument(&format!(
                "int.isqrt: negative input ({int}) has no integer square root"
            )));
        }
        Ok(int.isqrt())
    }

    /// `int ** exp`, saturating to the range's end on the result's side. A
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
        match int.checked_pow(exp) {
            Some(power) if (MIN..=MAX).contains(&power) => power,
            _ if int >= 0 || even => MAX,
            _ => MIN,
        }
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
        let parsed = text.get(scope).as_str().parse::<i64>();
        let message = match parsed {
            Ok(value) if (MIN..=MAX).contains(&value) => return Ok(value),
            Ok(_) => format!(
                "int.parse: {:?} is outside the representable BAML int range [{MIN}, {MAX}]",
                text.get(scope).as_str()
            ),
            Err(error) => format!(
                "int.parse: cannot parse {:?} as int: {error}",
                text.get(scope).as_str()
            ),
        };
        Err(scope.parse_error(&message))
    }

    /// Map a draw from `MIN..=MAX` onto `lower..upper`, or give `upper` for
    /// a draw to reject: one past the largest multiple of the range's size,
    /// which would favour the low end.
    fn _random_in_range(draw: i64, lower: i64, upper: i64) -> i64 {
        if lower >= upper {
            return upper;
        }
        let range = (i128::from(upper) - i128::from(lower)).cast_unsigned();
        // A draw below `MIN` wraps to far above the zone and is rejected.
        let offset = (i128::from(draw) - i128::from(MIN)).cast_unsigned();
        let zone = SPAN / range * range;
        if offset >= zone {
            return upper;
        }
        i64::try_from(i128::from(lower) + (offset % range).cast_signed()).unwrap_or(upper)
    }
}
