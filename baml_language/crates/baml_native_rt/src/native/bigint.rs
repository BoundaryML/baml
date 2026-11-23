//! `baml.Bigint`.

use num_bigint::{BigInt, Sign};

use super::{Big, BigRef, Bytes, NativeBamlBigint, Natives, Root, Scope, Str, Thrown};
use crate::bigint::MAX_BITS;

/// The most decimal digits a `bigint` within [`MAX_BITS`] can have.
const MAX_DECIMAL_DIGITS: usize = (MAX_BITS / 3 + 2) as usize;

impl NativeBamlBigint for Natives {
    fn abs<'s>(scope: &mut Scope<'s>, bigint: Root<'s, Big>) -> Root<'s, Big> {
        let value = bigint.get(scope).value();
        if value.sign() == Sign::Minus {
            // As many bits as the value it is made from.
            scope.make_big(&-value)
        } else {
            bigint
        }
    }

    fn isqrt<'s>(
        scope: &mut Scope<'s>,
        bigint: Root<'s, Big>,
    ) -> Result<Root<'s, Big>, Thrown<'s>> {
        let value = bigint.get(scope).value();
        if value.sign() == Sign::Minus {
            return Err(scope.invalid_argument(&format!(
                "bigint.isqrt: negative input ({value}) has no integer square root"
            )));
        }
        scope.alloc_big(&value.sqrt())
    }

    /// `bigint ** exp`. A negative `exp` gives zero.
    fn pow<'s>(
        scope: &mut Scope<'s>,
        bigint: Root<'s, Big>,
        exp: Root<'s, Big>,
    ) -> Result<Root<'s, Big>, Thrown<'s>> {
        let base = bigint.get(scope).value();
        let exp = exp.get(scope).value();
        let one = BigInt::from(1);
        // The bases whose powers stay small whatever the exponent come
        // first: the estimate below would refuse them.
        let small = if exp.sign() == Sign::Minus {
            Some(BigInt::ZERO)
        } else if base.sign() == Sign::NoSign {
            Some(if exp.sign() == Sign::NoSign {
                one
            } else {
                BigInt::ZERO
            })
        } else if base == one {
            Some(one)
        } else if base == BigInt::from(-1) {
            Some(if (&exp % 2_u32).sign() == Sign::NoSign {
                one
            } else {
                BigInt::from(-1)
            })
        } else {
            None
        };
        if let Some(power) = small {
            return scope.alloc_big(&power);
        }
        let Ok(count) = u32::try_from(&exp) else {
            return Err(scope.alloc_failure(&format!(
                "bigint.pow: exponent ({exp}) exceeds memory limits"
            )));
        };
        let bits = base.bits().saturating_mul(u64::from(count));
        if bits > MAX_BITS {
            return Err(scope.alloc_failure(&format!(
                "bigint.pow: result of {base}^{exp} would require ~{bits} bits (limit: {MAX_BITS})"
            )));
        }
        scope.alloc_big(&base.pow(count))
    }

    /// The largest `k` with `base ** k <= bigint`.
    fn ilog<'s>(
        scope: &mut Scope<'s>,
        bigint: Root<'s, Big>,
        base: Root<'s, Big>,
    ) -> Result<Root<'s, Big>, Thrown<'s>> {
        let value = bigint.get(scope).value();
        let base = base.get(scope).value();
        if value.sign() != Sign::Plus {
            return Err(
                scope.invalid_argument(&format!("bigint.ilog: input ({value}) must be positive"))
            );
        }
        if base < BigInt::from(2) {
            return Err(
                scope.invalid_argument(&format!("bigint.ilog: base ({base}) must be at least 2"))
            );
        }
        if base == BigInt::from(2) {
            return scope.alloc_big(&BigInt::from(value.bits() - 1));
        }
        // `base ** k >= 2 ** (k * (bits(base) - 1))` bounds `k` from above;
        // then a binary search for the largest that fits.
        let most = (value.bits() / base.bits().saturating_sub(1).max(1)).saturating_add(1);
        let mut low = 0_u32;
        let mut high = u32::try_from(most).unwrap_or(u32::MAX);
        while low < high {
            let middle = low + (high - low).div_ceil(2);
            if base.pow(middle) <= value {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        scope.alloc_big(&BigInt::from(low))
    }

    fn to_int<'s>(scope: &mut Scope<'s>, bigint: Root<'s, Big>) -> Result<i64, Thrown<'s>> {
        let value = bigint.get(scope).value();
        i64::try_from(&value).map_err(|_| {
            scope.invalid_argument(&format!(
                "bigint.to_int: a {}-bit value is outside int's range (int is 64-bit signed)",
                value.bits()
            ))
        })
    }

    /// An optional sign and then decimal digits, and nothing else.
    fn parse<'s>(scope: &mut Scope<'s>, text: Root<'s, Str>) -> Result<Root<'s, Big>, Thrown<'s>> {
        let text = text.get(scope).as_str().to_owned();
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text.strip_prefix('+').unwrap_or(&text)),
        };
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(
                scope.parse_error(&format!("bigint.parse: cannot parse {text:?} as bigint"))
            );
        }
        if digits.len() > MAX_DECIMAL_DIGITS {
            return Err(scope.alloc_failure(&format!(
                "bigint.parse: input has {} decimal digits, more than the {MAX_DECIMAL_DIGITS}-digit limit (bigint cap: {MAX_BITS} bits)",
                digits.len(),
            )));
        }
        match BigInt::parse_bytes(digits.as_bytes(), 10) {
            Some(value) if negative => scope.alloc_big(&-value),
            Some(value) => scope.alloc_big(&value),
            None => {
                Err(scope.parse_error(&format!("bigint.parse: cannot parse {text:?} as bigint")))
            }
        }
    }

    /// How many bytes one draw over `lower..upper` takes: none for an
    /// empty range.
    fn _random_byte_count(lower: BigRef<'_>, upper: BigRef<'_>) -> i64 {
        let (lower, upper) = (lower.value(), upper.value());
        if lower >= upper {
            return 0;
        }
        i64::try_from(draw_bits(&lower, &upper).div_ceil(8)).unwrap_or(i64::MAX)
    }

    /// Map the bytes of a draw onto `lower..upper`, or give `upper` for a
    /// draw to reject: one at or past the range's size, or too short.
    fn _random_in_range<'s>(
        scope: &mut Scope<'s>,
        draw: Root<'s, Bytes>,
        lower: Root<'s, Big>,
        upper: Root<'s, Big>,
    ) -> Root<'s, Big> {
        let (low, high) = (lower.get(scope).value(), upper.get(scope).value());
        if low >= high {
            return upper;
        }
        let bits = draw_bits(&low, &high);
        let Some(width) = usize::try_from(bits.div_ceil(8))
            .ok()
            .filter(|width| *width <= draw.get(scope).len())
        else {
            return upper;
        };
        let mut bytes = vec![0; width];
        draw.get(scope).copy_to(0, &mut bytes);
        // Without the high bits the range does not use, fewer than half
        // the draws are rejected.
        if let Some(top) = bytes.first_mut() {
            *top &= 0xFF >> (width as u64 * 8 - bits);
        }
        let sample = BigInt::from_bytes_be(Sign::Plus, &bytes);
        if sample < &high - &low {
            // Between two values that are within the limit.
            scope.make_big(&(sample + low))
        } else {
            upper
        }
    }
}

/// How many random bits one draw over `lower..upper`, which is not empty,
/// takes: the bits of the range's size less one, so that a range whose size
/// is a power of two rejects nothing.
fn draw_bits(lower: &BigInt, upper: &BigInt) -> u64 {
    (upper - lower - 1_u32).bits()
}
