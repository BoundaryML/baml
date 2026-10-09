/// A BAML `int`, represented as a signed 63-bit two's-complement value.
///
/// Keeping the range invariant in the value type lets compile-time evaluation
/// and the VM share integer semantics without duplicating bit manipulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
