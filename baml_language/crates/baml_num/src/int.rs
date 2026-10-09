use std::fmt;

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

    /// An `int` literal. The compiler range-checks every literal it emits,
    /// so the panic is unreachable from generated code; it keeps the
    /// function total for a hand-written call with a bad constant.
    ///
    /// # Panics
    ///
    /// When `value` is outside the i63 range.
    #[must_use]
    pub const fn lit(value: i64) -> Self {
        match Self::new(value) {
            Some(value) => value,
            None => panic!("int literal is outside the int range"),
        }
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
    #[must_use]
    pub const fn bit_and(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }

    /// Bitwise OR. Closed over the i63 range.
    #[must_use]
    pub const fn bit_or(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }

    /// Bitwise XOR. Closed over the i63 range.
    #[must_use]
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

/// serde support (`serde` feature): an `int` is a JSON integer in the i63
/// range. An integer outside the range fails with `"{value} is outside the
/// int range [-2^62, 2^62 - 1]"`; any other JSON value fails with serde's
/// `invalid type` message.
#[cfg(feature = "serde")]
mod serde_impls {
    use core::fmt;

    use serde::{
        Deserialize, Deserializer, Serialize, Serializer,
        de::{self, IgnoredAny, MapAccess, Unexpected, Visitor},
    };

    use super::Int63;

    impl Serialize for Int63 {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_i64(self.0)
        }
    }

    impl<'de> Deserialize<'de> for Int63 {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer.deserialize_any(Int63Visitor)
        }
    }

    struct Int63Visitor;

    fn in_range<E: de::Error>(value: impl TryInto<i64> + fmt::Display + Copy) -> Result<Int63, E> {
        value.try_into().ok().and_then(Int63::new).ok_or_else(|| {
            E::custom(format_args!(
                "{value} is outside the int range [-2^62, 2^62 - 1]"
            ))
        })
    }

    impl<'de> Visitor<'de> for Int63Visitor {
        type Value = Int63;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("an int in [-2^62, 2^62 - 1]")
        }

        fn visit_i64<E: de::Error>(self, value: i64) -> Result<Int63, E> {
            in_range(value)
        }

        fn visit_u64<E: de::Error>(self, value: u64) -> Result<Int63, E> {
            in_range(value)
        }

        fn visit_i128<E: de::Error>(self, value: i128) -> Result<Int63, E> {
            in_range(value)
        }

        fn visit_u128<E: de::Error>(self, value: u128) -> Result<Int63, E> {
            in_range(value)
        }

        fn visit_f64<E: de::Error>(self, value: f64) -> Result<Int63, E> {
            Err(E::invalid_type(Unexpected::Float(value), &self))
        }

        /// `serde_json` with `arbitrary_precision` hands a number it cannot
        /// represent as a primitive (an integer wider than 64 bits) over as
        /// a one-entry map, so a map here is either that or an object.
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Int63, A::Error> {
            while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
            Err(de::Error::custom(
                "expected an int, found an object or a number wider than 64 bits",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Int63, IntShiftError};

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
    fn display_matches_i64() {
        for value in BOUNDARIES {
            assert_eq!(Int63::new(value).unwrap().to_string(), value.to_string());
        }
    }
}
