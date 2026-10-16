//! The one place a language's conversions are written down.
//!
//! A scalar either is a member of a slot's type as it is, becomes one by a
//! conversion the language allows, or does not fit. Python's `int` becomes a
//! `float` when the conversion is exact; JavaScript's `number` is a double
//! that is an `int` when integral. Nothing else in the crate knows a
//! language: the walker asks [`Rules::fit`] and ranks the answer.

use baml_type::Int63;
use num_bigint::BigInt;
use num_traits::{FromPrimitive, ToPrimitive};

use crate::host::Scalar;

/// Whose conversions apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rules {
    Python,
    JavaScript,
}

/// The scalar type a slot asks for. [`ScalarSlot::Open`] asks for the scalar
/// the value naturally is, when the slot fixes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScalarSlot {
    Int,
    Bigint,
    Float,
    Bool,
    String,
    Bytes,
    Open,
}

/// How a value relates to the slot it was asked about.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Answer<T> {
    /// A member of the slot's type as it is.
    AsIs(T),
    /// The language turned it into a member of the slot's type.
    Converted(T),
    /// Not a member, and the language does not convert it.
    No,
}

impl Rules {
    /// `scalar` in `slot`, by this language's rules.
    pub(crate) fn fit(self, scalar: &Scalar, slot: ScalarSlot) -> Answer<Scalar> {
        match self {
            Rules::Python => python(scalar, slot),
            Rules::JavaScript => javascript(scalar, slot),
        }
    }
}

/// A value that is exactly one BAML scalar fits that slot or an open one.
fn only(scalar: &Scalar, kind: ScalarSlot, slot: ScalarSlot) -> Answer<Scalar> {
    if slot == kind || slot == ScalarSlot::Open {
        Answer::AsIs(scalar.clone())
    } else {
        Answer::No
    }
}

/// Python: `bool` is its own type; an `int` is an `int` in range and
/// converts exactly to `float` or widens to `bigint`; a `float` is only a
/// `float`.
fn python(scalar: &Scalar, slot: ScalarSlot) -> Answer<Scalar> {
    match (scalar, slot) {
        (Scalar::Bool(_), _) => only(scalar, ScalarSlot::Bool, slot),
        (Scalar::Int(_), ScalarSlot::Int | ScalarSlot::Open) => Answer::AsIs(scalar.clone()),
        (Scalar::Int(int), ScalarSlot::Float) => match exact_float(int.get()) {
            Some(float) => Answer::Converted(Scalar::Float(float)),
            None => Answer::No,
        },
        (Scalar::Int(int), ScalarSlot::Bigint) => {
            Answer::Converted(Scalar::BigInt(BigInt::from(int.get())))
        }
        (Scalar::Int(_), ScalarSlot::Bool | ScalarSlot::String | ScalarSlot::Bytes) => Answer::No,
        // Beyond the VM's integers: a `bigint`, or a `float` when exact.
        (Scalar::BigInt(_), ScalarSlot::Bigint | ScalarSlot::Open) => Answer::AsIs(scalar.clone()),
        (Scalar::BigInt(big), ScalarSlot::Float) => match exact_float_big(big) {
            Some(float) => Answer::Converted(Scalar::Float(float)),
            None => Answer::No,
        },
        (
            Scalar::BigInt(_),
            ScalarSlot::Int | ScalarSlot::Bool | ScalarSlot::String | ScalarSlot::Bytes,
        ) => Answer::No,
        (Scalar::Float(_), _) => only(scalar, ScalarSlot::Float, slot),
        (Scalar::String, _) => only(scalar, ScalarSlot::String, slot),
        (Scalar::Bytes, _) => only(scalar, ScalarSlot::Bytes, slot),
    }
}

/// JavaScript: a `number` is a double that fits `int` as it is when integral
/// and in range, and `float` by conversion when integral (so `int | float`
/// takes an integral number as an `int`); a `bigint` fits `int` by
/// conversion when it fits the VM's integers and never `float`.
fn javascript(scalar: &Scalar, slot: ScalarSlot) -> Answer<Scalar> {
    match (scalar, slot) {
        (Scalar::Bool(_), _) => only(scalar, ScalarSlot::Bool, slot),
        (Scalar::Float(value), ScalarSlot::Int) => match integral(*value) {
            Some(int) => Answer::AsIs(Scalar::Int(int)),
            None => Answer::No,
        },
        (Scalar::Float(value), ScalarSlot::Float) => {
            if integral(*value).is_some() {
                Answer::Converted(scalar.clone())
            } else {
                Answer::AsIs(scalar.clone())
            }
        }
        (Scalar::Float(value), ScalarSlot::Open) => match integral(*value) {
            Some(int) => Answer::AsIs(Scalar::Int(int)),
            None => Answer::AsIs(scalar.clone()),
        },
        (
            Scalar::Float(_),
            ScalarSlot::Bigint | ScalarSlot::Bool | ScalarSlot::String | ScalarSlot::Bytes,
        ) => Answer::No,
        // An integer the host already classified as a VM integer.
        (Scalar::Int(_), ScalarSlot::Int | ScalarSlot::Open) => Answer::AsIs(scalar.clone()),
        (Scalar::Int(int), ScalarSlot::Float) => match exact_float(int.get()) {
            Some(float) => Answer::Converted(Scalar::Float(float)),
            None => Answer::No,
        },
        (
            Scalar::Int(_),
            ScalarSlot::Bigint | ScalarSlot::Bool | ScalarSlot::String | ScalarSlot::Bytes,
        ) => Answer::No,
        (Scalar::BigInt(_), ScalarSlot::Bigint | ScalarSlot::Open) => Answer::AsIs(scalar.clone()),
        (Scalar::BigInt(big), ScalarSlot::Int) => match big.to_i64().and_then(Int63::new) {
            Some(int) => Answer::Converted(Scalar::Int(int)),
            None => Answer::No,
        },
        (
            Scalar::BigInt(_),
            ScalarSlot::Float | ScalarSlot::Bool | ScalarSlot::String | ScalarSlot::Bytes,
        ) => Answer::No,
        (Scalar::String, _) => only(scalar, ScalarSlot::String, slot),
        (Scalar::Bytes, _) => only(scalar, ScalarSlot::Bytes, slot),
    }
}

/// An integer as a double, when the double is exactly that integer.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "the round trip checks the cast was exact"
)]
fn exact_float(int: i64) -> Option<f64> {
    let float = int as f64;
    // `as i64` saturates, so a value past the range that rounds to a power of
    // two would compare equal; the magnitude bound rules that out.
    (float.abs() < 9_223_372_036_854_775_808.0 && float as i64 == int).then_some(float)
}

/// A big integer as a double, when the double is exactly that integer.
fn exact_float_big(big: &BigInt) -> Option<f64> {
    let float = big.to_f64()?;
    (float.is_finite() && BigInt::from_f64(float).as_ref() == Some(big)).then_some(float)
}

/// An integral double as a VM integer, when the double is exactly one.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::float_cmp,
    reason = "the round trip checks the cast was exact"
)]
fn integral(value: f64) -> Option<Int63> {
    if !value.is_finite() || value.fract() != 0.0 {
        return None;
    }
    let int = value as i64;
    (int as f64 == value).then(|| Int63::new(int)).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(value: i64) -> Scalar {
        Scalar::Int(Int63::new(value).unwrap())
    }

    #[test]
    fn python_ints_convert_exactly_or_not_at_all() {
        assert_eq!(
            Rules::Python.fit(&int(3), ScalarSlot::Float),
            Answer::Converted(Scalar::Float(3.0))
        );
        assert_eq!(
            Rules::Python.fit(&int(1 << 53), ScalarSlot::Float),
            Answer::Converted(Scalar::Float(9_007_199_254_740_992.0))
        );
        assert_eq!(
            Rules::Python.fit(&int((1 << 53) + 1), ScalarSlot::Float),
            Answer::No
        );
        // `i64::MAX` is not representable; the saturating cast used to say it was.
        assert_eq!(
            Rules::Python.fit(&int(i64::MAX >> 1), ScalarSlot::Float),
            Answer::No
        );
        // `2^63` is beyond the VM's integers and exactly representable.
        let big: BigInt = BigInt::from(1u64) << 63;
        assert_eq!(
            Rules::Python.fit(&Scalar::BigInt(big.clone()), ScalarSlot::Float),
            Answer::Converted(Scalar::Float(9_223_372_036_854_775_808.0))
        );
        assert_eq!(
            Rules::Python.fit(&Scalar::BigInt(big), ScalarSlot::Int),
            Answer::No
        );
        assert_eq!(
            Rules::Python.fit(&Scalar::Bool(true), ScalarSlot::Int),
            Answer::No
        );
        assert_eq!(
            Rules::Python.fit(&Scalar::Float(1.0), ScalarSlot::Int),
            Answer::No
        );
    }

    #[test]
    fn javascript_numbers_are_ints_when_integral() {
        assert_eq!(
            Rules::JavaScript.fit(&Scalar::Float(3.0), ScalarSlot::Int),
            Answer::AsIs(int(3))
        );
        assert_eq!(
            Rules::JavaScript.fit(&Scalar::Float(3.0), ScalarSlot::Float),
            Answer::Converted(Scalar::Float(3.0))
        );
        assert_eq!(
            Rules::JavaScript.fit(&Scalar::Float(2.5), ScalarSlot::Float),
            Answer::AsIs(Scalar::Float(2.5))
        );
        assert_eq!(
            Rules::JavaScript.fit(&Scalar::Float(2.5), ScalarSlot::Int),
            Answer::No
        );
        assert_eq!(
            Rules::JavaScript.fit(&Scalar::Float(3.0), ScalarSlot::Open),
            Answer::AsIs(int(3))
        );
        assert_eq!(
            Rules::JavaScript.fit(&Scalar::Float(3.0), ScalarSlot::Bigint),
            Answer::No
        );
        assert_eq!(
            Rules::JavaScript.fit(&Scalar::BigInt(BigInt::from(5)), ScalarSlot::Int),
            Answer::Converted(int(5))
        );
        assert_eq!(
            Rules::JavaScript.fit(&Scalar::BigInt(BigInt::from(1u64) << 63), ScalarSlot::Int),
            Answer::No
        );
    }
}
