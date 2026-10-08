use std::{fmt, str::FromStr};

/// A BAML `int`, represented as a signed 63-bit two's-complement value.
///
/// Keeping the range invariant in the value type lets compile-time evaluation,
/// the VM, and generated native code share integer semantics without
/// duplicating bit manipulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Int63(i64);

/// The only invalid shift count in BAML is a negative one. Non-negative counts
/// at or above the i63 width have defined truncating or saturating behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntShiftError {
    /// The shift count was negative.
    NegativeCount(i64),
}

/// The text was not a decimal integer in the BAML `int` range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseInt63Error {
    /// The text was not a valid decimal `i64` literal.
    Invalid(std::num::ParseIntError),
    /// The text parsed as an `i64` that falls outside the i63 range.
    OutOfRange(i64),
}

impl fmt::Display for ParseInt63Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(err) => write!(f, "{err}"),
            Self::OutOfRange(value) => write!(f, "{value} overflows int"),
        }
    }
}

impl std::error::Error for ParseInt63Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Invalid(err) => Some(err),
            Self::OutOfRange(_) => None,
        }
    }
}

impl Int63 {
    /// Number of bits in a BAML `int`, including its sign bit.
    pub const BITS: u32 = 63;
    /// Smallest representable BAML `int`.
    pub const MIN: Self = Self(-(1_i64 << 62));
    /// Largest representable BAML `int`.
    pub const MAX: Self = Self((1_i64 << 62) - 1);
    /// The BAML `int` zero.
    pub const ZERO: Self = Self(0);

    const BIT_MASK: u64 = (1_u64 << Self::BITS) - 1;

    /// Construct a value when `value` is representable as a BAML `int`.
    pub const fn new(value: i64) -> Option<Self> {
        if value >= Self::MIN.0 && value <= Self::MAX.0 {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Return the underlying sign-extended `i64`.
    pub const fn get(self) -> i64 {
        self.0
    }

    /// Checked addition in BAML's range, independent of the execution backend.
    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        // The sum of two i63 values always fits in i64.
        Self::new(self.0 + rhs.0)
    }

    /// Checked subtraction; `None` when the difference leaves the i63 range.
    pub const fn checked_sub(self, rhs: Self) -> Option<Self> {
        // The widest case, `MAX - MIN = 2^63 - 1`, is exactly `i64::MAX`.
        Self::new(self.0 - rhs.0)
    }

    /// Checked multiplication; `None` when the product leaves the i63 range.
    pub const fn checked_mul(self, rhs: Self) -> Option<Self> {
        match self.0.checked_mul(rhs.0) {
            Some(value) => Self::new(value),
            None => None,
        }
    }

    /// Truncates toward zero. Returns `None` for a zero divisor or an
    /// out-of-range quotient (`MIN / -1`).
    pub const fn checked_div(self, rhs: Self) -> Option<Self> {
        if rhs.0 == 0 {
            None
        } else {
            Self::new(self.0 / rhs.0)
        }
    }

    /// The remainder has the dividend's sign. Returns `None` for a zero
    /// divisor; `MIN % -1` is `0`, never an overflow.
    pub const fn checked_rem(self, rhs: Self) -> Option<Self> {
        if rhs.0 == 0 {
            None
        } else {
            Some(Self(self.0 % rhs.0))
        }
    }

    /// Checked negation; `None` only for `MIN`.
    pub const fn checked_neg(self) -> Option<Self> {
        Self::new(-self.0)
    }

    /// Bitwise AND. Closed over the i63 range.
    pub const fn bit_and(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }

    /// Bitwise OR. Closed over the i63 range.
    pub const fn bit_or(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }

    /// Bitwise XOR. Closed over the i63 range.
    pub const fn bit_xor(self, rhs: Self) -> Self {
        Self(self.0 ^ rhs.0)
    }

    /// Shift left modulo the i63 width and interpret the retained bits as a
    /// signed two's-complement value.
    pub fn shift_left(self, count: i64) -> Result<Self, IntShiftError> {
        match count {
            ..=-1 => Err(IntShiftError::NegativeCount(count)),
            0..63 => {
                let Ok(shift) = u32::try_from(count) else {
                    unreachable!("shift count in 0..63 always fits in u32")
                };
                let bits = (self.0.cast_unsigned() << shift) & Self::BIT_MASK;
                Ok(Self((bits << 1).cast_signed() >> 1))
            }
            63.. => Ok(Self(0)),
        }
    }

    /// Shift right arithmetically, saturating to the sign bit at or above the
    /// i63 width.
    pub fn shift_right(self, count: i64) -> Result<Self, IntShiftError> {
        match count {
            ..=-1 => Err(IntShiftError::NegativeCount(count)),
            0..63 => {
                let Ok(shift) = u32::try_from(count) else {
                    unreachable!("shift count in 0..63 always fits in u32")
                };
                Ok(Self(self.0 >> shift))
            }
            63.. => Ok(Self(if self.0 < 0 { -1 } else { 0 })),
        }
    }
}

impl fmt::Display for Int63 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl FromStr for Int63 {
    type Err = ParseInt63Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = s.parse::<i64>().map_err(ParseInt63Error::Invalid)?;
        Self::new(value).ok_or(ParseInt63Error::OutOfRange(value))
    }
}

#[cfg(test)]
mod tests {
    use super::{Int63, IntShiftError, ParseInt63Error};

    const BOUNDARIES: [i64; 9] = [
        Int63::MIN.get(),
        Int63::MIN.get() + 1,
        -63,
        -1,
        0,
        1,
        63,
        Int63::MAX.get() - 1,
        Int63::MAX.get(),
    ];

    #[test]
    fn arithmetic_matches_wide_integer_reference_at_boundaries() {
        let narrow = |value: i128| i64::try_from(value).ok().and_then(Int63::new);
        for a in BOUNDARIES {
            let left = Int63::new(a).unwrap();
            assert_eq!(left.checked_neg(), narrow(-i128::from(a)));
            for b in BOUNDARIES {
                let right = Int63::new(b).unwrap();
                assert_eq!(
                    left.checked_add(right),
                    narrow(i128::from(a) + i128::from(b))
                );
                assert_eq!(
                    left.checked_sub(right),
                    narrow(i128::from(a) - i128::from(b))
                );
                assert_eq!(
                    left.checked_mul(right),
                    narrow(i128::from(a) * i128::from(b))
                );
                assert_eq!(
                    left.checked_div(right),
                    i128::from(a).checked_div(i128::from(b)).and_then(narrow)
                );
                assert_eq!(
                    left.checked_rem(right),
                    i128::from(a).checked_rem(i128::from(b)).and_then(narrow)
                );
                assert_eq!(left.bit_and(right), Int63::new(a & b).unwrap());
                assert_eq!(left.bit_or(right), Int63::new(a | b).unwrap());
                assert_eq!(left.bit_xor(right), Int63::new(a ^ b).unwrap());
            }
        }
    }

    #[test]
    fn range_constants() {
        assert_eq!(Int63::MIN.get(), -(1_i64 << 62));
        assert_eq!(Int63::MAX.get(), (1_i64 << 62) - 1);
        assert_eq!(Int63::ZERO.get(), 0);
        assert_eq!(Int63::new(Int63::MAX.get() + 1), None);
        assert_eq!(Int63::new(Int63::MIN.get() - 1), None);
        assert_eq!(std::mem::size_of::<Int63>(), std::mem::size_of::<i64>());
    }

    #[test]
    fn division_edge_cases() {
        let minus_one = Int63::new(-1).unwrap();
        assert_eq!(Int63::MIN.checked_div(minus_one), None);
        assert_eq!(Int63::MIN.checked_rem(minus_one), Some(Int63::ZERO));
        assert_eq!(Int63::MIN.checked_neg(), None);
        assert_eq!(
            Int63::MAX.checked_neg(),
            Some(Int63::new(-Int63::MAX.get()).unwrap())
        );
        // Truncation toward zero, remainder takes the dividend's sign.
        let seven = Int63::new(7).unwrap();
        let two = Int63::new(2).unwrap();
        let minus_seven = Int63::new(-7).unwrap();
        assert_eq!(minus_seven.checked_div(two), Int63::new(-3));
        assert_eq!(minus_seven.checked_rem(two), Int63::new(-1));
        assert_eq!(seven.checked_rem(Int63::new(-2).unwrap()), Int63::new(1));
        assert_eq!(seven.checked_div(Int63::ZERO), None);
        assert_eq!(seven.checked_rem(Int63::ZERO), None);
    }

    #[test]
    fn shifts() {
        let one = Int63::new(1).unwrap();
        assert_eq!(one.shift_left(62), Ok(Int63::MIN));
        assert_eq!(one.shift_left(63), Ok(Int63::ZERO));
        assert_eq!(one.shift_left(-1), Err(IntShiftError::NegativeCount(-1)));
        assert_eq!(Int63::MIN.shift_right(63), Ok(Int63::new(-1).unwrap()));
        assert_eq!(Int63::MAX.shift_right(63), Ok(Int63::ZERO));
        assert_eq!(
            Int63::MAX.shift_right(-5),
            Err(IntShiftError::NegativeCount(-5))
        );
    }

    #[test]
    fn display_and_parse_round_trip() {
        for value in BOUNDARIES {
            let int = Int63::new(value).unwrap();
            assert_eq!(int.to_string(), value.to_string());
            assert_eq!(int.to_string().parse::<Int63>(), Ok(int));
        }
        assert_eq!("-4611686018427387904".parse::<Int63>(), Ok(Int63::MIN));
        assert_eq!("4611686018427387903".parse::<Int63>(), Ok(Int63::MAX));
        assert_eq!(
            "4611686018427387904".parse::<Int63>(),
            Err(ParseInt63Error::OutOfRange(4_611_686_018_427_387_904))
        );
        assert_eq!(
            "-4611686018427387905".parse::<Int63>(),
            Err(ParseInt63Error::OutOfRange(-4_611_686_018_427_387_905))
        );
        assert!(matches!(
            "9223372036854775808".parse::<Int63>(),
            Err(ParseInt63Error::Invalid(_))
        ));
        assert!(matches!(
            "abc".parse::<Int63>(),
            Err(ParseInt63Error::Invalid(_))
        ));
        assert!(matches!(
            "".parse::<Int63>(),
            Err(ParseInt63Error::Invalid(_))
        ));
        assert_eq!(
            ParseInt63Error::OutOfRange(4_611_686_018_427_387_904).to_string(),
            "4611686018427387904 overflows int"
        );
    }
}
