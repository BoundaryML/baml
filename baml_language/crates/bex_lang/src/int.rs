//! Catchable `int` operations shared by interpreted and compiled BAML.
//!
//! [`Int63`] owns the value invariant and the arithmetic. This module turns
//! failures into [`Panic`] payloads with the messages the VM has always
//! produced, allocating only on the failure path.

use baml_type::{Int63, IntShiftError};

use crate::Panic;

/// Narrow a raw `i64` into the `int` range, or raise `IntegerOverflow` with
/// the message `"{value} overflows int"`.
#[inline]
pub fn check(value: i64) -> Result<Int63, Panic> {
    Int63::new(value).ok_or_else(|| check_overflow(value))
}

/// `left + right`, or `IntegerOverflow`.
#[inline]
pub fn add(left: Int63, right: Int63) -> Result<Int63, Panic> {
    finish(left.checked_add(right), left, '+', right)
}

/// `left - right`, or `IntegerOverflow`.
#[inline]
pub fn sub(left: Int63, right: Int63) -> Result<Int63, Panic> {
    finish(left.checked_sub(right), left, '-', right)
}

/// `left * right`, or `IntegerOverflow`.
#[inline]
pub fn mul(left: Int63, right: Int63) -> Result<Int63, Panic> {
    finish(left.checked_mul(right), left, '*', right)
}

/// `left / right`, truncating toward zero. `DivisionByZero` for a zero
/// divisor; `IntegerOverflow` for `MIN / -1`.
#[inline]
pub fn div(left: Int63, right: Int63) -> Result<Int63, Panic> {
    if right == Int63::ZERO {
        return Err(division_by_zero(left));
    }
    finish(left.checked_div(right), left, '/', right)
}

/// `left % right` with the dividend's sign. `DivisionByZero` for a zero
/// divisor; `MIN % -1` is `0` and never overflows.
#[inline]
pub fn rem(left: Int63, right: Int63) -> Result<Int63, Panic> {
    left.checked_rem(right)
        .ok_or_else(|| division_by_zero(left))
}

/// `-value`, or `IntegerOverflow` for `MIN`.
#[inline]
pub fn neg(value: Int63) -> Result<Int63, Panic> {
    value.checked_neg().ok_or_else(|| neg_overflow(value))
}

/// `left << right` modulo the i63 width; `NegativeBitShift` for a negative
/// count.
#[inline]
pub fn shl(left: Int63, right: Int63) -> Result<Int63, Panic> {
    left.shift_left(right.get()).map_err(shift_error)
}

/// `left >> right`, arithmetic and saturating; `NegativeBitShift` for a
/// negative count.
#[inline]
pub fn shr(left: Int63, right: Int63) -> Result<Int63, Panic> {
    left.shift_right(right.get()).map_err(shift_error)
}

/// `left & right`. Cannot fail.
#[inline]
pub fn bit_and(left: Int63, right: Int63) -> Int63 {
    left.bit_and(right)
}

/// `left | right`. Cannot fail.
#[inline]
pub fn bit_or(left: Int63, right: Int63) -> Int63 {
    left.bit_or(right)
}

/// `left ^ right`. Cannot fail.
#[inline]
pub fn bit_xor(left: Int63, right: Int63) -> Int63 {
    left.bit_xor(right)
}

#[inline]
fn finish(value: Option<Int63>, left: Int63, op: char, right: Int63) -> Result<Int63, Panic> {
    value.ok_or_else(|| overflow(left, op, right))
}

/// The `IntegerOverflow` panic for a binary operation: message
/// `"{left} {op} {right} overflows int"`.
#[cold]
pub fn overflow(left: Int63, op: char, right: Int63) -> Panic {
    Panic::IntegerOverflow {
        message: overflow_message(left.get(), op, right.get()),
    }
}

/// The `IntegerOverflow` message for a binary operation on raw operands,
/// `"{left} {op} {right} overflows int"`. Exposed so backends that keep
/// operands as `i64` (the VM's tagged fast paths) format through the same
/// function.
#[cold]
pub fn overflow_message(left: i64, op: char, right: i64) -> String {
    format!("{left} {op} {right} overflows int")
}

/// The `IntegerOverflow` message for unary negation, `"-({value}) overflows int"`.
#[cold]
pub fn neg_overflow_message(value: i64) -> String {
    format!("-({value}) overflows int")
}

/// The `IntegerOverflow` message for an out-of-range raw value,
/// `"{value} overflows int"`.
#[cold]
pub fn check_overflow_message(value: i64) -> String {
    format!("{value} overflows int")
}

/// The `NegativeBitShift` message, `"bit shift count is negative: {count}"`.
/// Generic over the count's type because `bigint` shifts report a `BigInt`.
#[cold]
pub fn negative_bit_shift_message(count: impl std::fmt::Display) -> String {
    format!("bit shift count is negative: {count}")
}

#[cold]
fn check_overflow(value: i64) -> Panic {
    Panic::IntegerOverflow {
        message: check_overflow_message(value),
    }
}

#[cold]
fn neg_overflow(value: Int63) -> Panic {
    Panic::IntegerOverflow {
        message: neg_overflow_message(value.get()),
    }
}

#[cold]
fn division_by_zero(dividend: Int63) -> Panic {
    Panic::DivisionByZero { dividend }
}

#[cold]
fn shift_error(error: IntShiftError) -> Panic {
    let IntShiftError::NegativeCount(count) = error;
    Panic::NegativeBitShift {
        message: negative_bit_shift_message(count),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(value: i64) -> Int63 {
        Int63::new(value).expect("test value in range")
    }

    fn overflow_err(message: &str) -> Result<Int63, Panic> {
        Err(Panic::IntegerOverflow {
            message: message.into(),
        })
    }

    #[test]
    fn check_narrows_or_overflows() {
        assert_eq!(check(0), Ok(Int63::ZERO));
        assert_eq!(check(Int63::MAX.get()), Ok(Int63::MAX));
        assert_eq!(check(Int63::MIN.get()), Ok(Int63::MIN));
        assert_eq!(
            check(Int63::MAX.get() + 1),
            overflow_err("4611686018427387904 overflows int")
        );
        assert_eq!(
            check(Int63::MIN.get() - 1),
            overflow_err("-4611686018427387905 overflows int")
        );
        assert_eq!(
            check(i64::MIN),
            overflow_err("-9223372036854775808 overflows int")
        );
    }

    #[test]
    fn binary_overflow_messages_match_the_vm() {
        let one = int(1);
        let minus_one = int(-1);
        assert_eq!(
            add(Int63::MAX, one),
            overflow_err("4611686018427387903 + 1 overflows int")
        );
        assert_eq!(
            sub(Int63::MIN, one),
            overflow_err("-4611686018427387904 - 1 overflows int")
        );
        assert_eq!(
            sub(Int63::MAX, Int63::MIN),
            overflow_err("4611686018427387903 - -4611686018427387904 overflows int")
        );
        assert_eq!(
            mul(Int63::MAX, Int63::MAX),
            overflow_err("4611686018427387903 * 4611686018427387903 overflows int")
        );
        assert_eq!(
            mul(Int63::MIN, minus_one),
            overflow_err("-4611686018427387904 * -1 overflows int")
        );
        assert_eq!(
            div(Int63::MIN, minus_one),
            overflow_err("-4611686018427387904 / -1 overflows int")
        );
        assert_eq!(
            neg(Int63::MIN),
            overflow_err("-(-4611686018427387904) overflows int")
        );
        assert_eq!(
            overflow(int(2), '*', int(3)),
            Panic::IntegerOverflow {
                message: "2 * 3 overflows int".into()
            }
        );
        assert_eq!(overflow_message(2, '+', -3), "2 + -3 overflows int");
    }

    #[test]
    fn successful_arithmetic() {
        assert_eq!(add(int(2), int(3)), Ok(int(5)));
        assert_eq!(sub(int(2), int(3)), Ok(int(-1)));
        assert_eq!(mul(int(-4), int(3)), Ok(int(-12)));
        assert_eq!(div(int(-7), int(2)), Ok(int(-3)));
        assert_eq!(rem(int(-7), int(2)), Ok(int(-1)));
        assert_eq!(rem(Int63::MIN, int(-1)), Ok(Int63::ZERO));
        assert_eq!(neg(Int63::MAX), Ok(int(-Int63::MAX.get())));
        assert_eq!(add(Int63::MAX, Int63::MIN), Ok(int(-1)));
        assert_eq!(sub(Int63::MIN, Int63::MIN), Ok(Int63::ZERO));
    }

    #[test]
    fn division_by_zero_carries_the_dividend() {
        for operation in [div, rem] {
            assert_eq!(
                operation(int(1), Int63::ZERO),
                Err(Panic::DivisionByZero { dividend: int(1) })
            );
            assert_eq!(
                operation(Int63::MIN, Int63::ZERO),
                Err(Panic::DivisionByZero {
                    dividend: Int63::MIN
                })
            );
        }
        assert_eq!(
            div(int(42), Int63::ZERO).unwrap_err().render_readable(),
            "baml.panics.DivisionByZero {dividend: 42}"
        );
    }

    #[test]
    fn shifts_match_int63_and_reject_negative_counts() {
        let one = int(1);
        assert_eq!(shl(one, int(62)), Ok(Int63::MIN));
        assert_eq!(shl(one, int(63)), Ok(Int63::ZERO));
        assert_eq!(shr(Int63::MIN, int(62)), Ok(int(-1)));
        assert_eq!(shr(Int63::MAX, int(100)), Ok(Int63::ZERO));
        let negative = Err(Panic::NegativeBitShift {
            message: "bit shift count is negative: -1".into(),
        });
        assert_eq!(shl(one, int(-1)), negative);
        assert_eq!(shr(one, int(-1)), negative);
        assert_eq!(
            shr(one, Int63::MIN),
            Err(Panic::NegativeBitShift {
                message: "bit shift count is negative: -4611686018427387904".into(),
            })
        );
        assert_eq!(
            negative_bit_shift_message(-9),
            "bit shift count is negative: -9"
        );
    }

    #[test]
    fn bit_operations_never_fail() {
        assert_eq!(bit_and(int(0b1100), int(0b1010)), int(0b1000));
        assert_eq!(bit_or(int(0b1100), int(0b1010)), int(0b1110));
        assert_eq!(bit_xor(int(0b1100), int(0b1010)), int(0b0110));
        assert_eq!(bit_and(Int63::MIN, Int63::MAX), Int63::ZERO);
        assert_eq!(bit_or(Int63::MIN, Int63::MAX), int(-1));
        assert_eq!(bit_xor(Int63::MIN, int(-1)), Int63::MAX);
    }
}
