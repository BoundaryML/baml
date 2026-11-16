//! Semantics of a BAML `int`: a signed [`BITS`]-bit two's-complement value.
//!
//! Compile-time evaluation (type-checker and MIR folding), the VM and the
//! compiled runtime all take the range and the shift rules from here, so they
//! cannot disagree.

/// Number of bits in a BAML `int`, including its sign bit.
pub const BITS: u32 = 63;
/// Smallest BAML `int`.
pub const MIN: i64 = i64::MIN >> (i64::BITS - BITS);
/// Largest BAML `int`.
pub const MAX: i64 = i64::MAX >> (i64::BITS - BITS);

/// Bits of an `i64` that a BAML `int` does not use.
const UNUSED: u32 = i64::BITS - BITS;

/// The only invalid shift count in BAML is a negative one. Non-negative counts
/// at or above the int width have defined truncating or saturating behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntShiftError {
    /// The shift count was negative.
    NegativeCount(i64),
}

/// Whether `value` is a BAML `int`.
pub const fn fits(value: i64) -> bool {
    (value << UNUSED) >> UNUSED == value
}

/// `value << count`: the bits shifted past the int width are discarded and the
/// retained bits are read as a signed two's-complement value. A count at or
/// above the width yields `0`.
pub fn shl(value: i64, count: i64) -> Result<i64, IntShiftError> {
    match u32::try_from(count) {
        Err(_) if count < 0 => Err(IntShiftError::NegativeCount(count)),
        Ok(shift) if shift < BITS => {
            let bits = value.cast_unsigned() << shift;
            Ok((bits << UNUSED).cast_signed() >> UNUSED)
        }
        _ => Ok(0),
    }
}

/// `value >> count`, arithmetic. A count at or above the width saturates to
/// the sign: `0` for a non-negative value, `-1` for a negative one.
pub fn shr(value: i64, count: i64) -> Result<i64, IntShiftError> {
    match u32::try_from(count) {
        Err(_) if count < 0 => Err(IntShiftError::NegativeCount(count)),
        Ok(shift) if shift < BITS => Ok(value >> shift),
        _ => Ok(if value < 0 { -1 } else { 0 }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shifts_follow_the_int_width() {
        assert_eq!(shl(1, i64::from(BITS) - 1), Ok(MIN));
        assert_eq!(shl(1, i64::from(BITS)), Ok(0));
        assert_eq!(shl(-1, 1000), Ok(0));
        assert_eq!(shl(MAX, 1), Ok(-2));
        assert_eq!(shr(MIN, i64::from(BITS) - 1), Ok(-1));
        assert_eq!(shr(MIN, i64::from(BITS)), Ok(-1));
        assert_eq!(shr(MAX, i64::from(BITS)), Ok(0));
        assert_eq!(shr(-8, 2), Ok(-2));
        assert_eq!(shl(1, -1), Err(IntShiftError::NegativeCount(-1)));
        assert_eq!(
            shr(1, i64::MIN),
            Err(IntShiftError::NegativeCount(i64::MIN))
        );
        assert!(fits(MIN) && fits(MAX) && fits(0));
    }
}
