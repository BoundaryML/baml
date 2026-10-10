//! `bigint` for compiled BAML: arbitrary precision over `num-bigint`, with
//! the VM's checks and messages.
//!
//! A [`BigInt`] is a reference-counted immutable value (the VM's
//! `Object::Bigint` is an `Arc<num_bigint::BigInt>`), so a read clones a
//! pointer and every operation allocates its result. The operations are
//! those of `bex_vm`'s `bigint_binop` and `package_baml/bigint.rs`:
//!
//! - `+ - & | ^` never fail; `*`, `<<` and `pow` refuse a result past the
//!   workspace cap [`MAX_BIGINT_BITS`] with `baml.panics.AllocFailure`
//!   before computing it (`bits(a * b) <= bits(a) + bits(b)`);
//! - `/` and `%` by zero raise `baml.panics.DivisionByZero` with the
//!   dividend; `/` truncates toward zero and `%` keeps the dividend's sign,
//!   as `num-bigint` does;
//! - `<<` and `>>` by a negative count raise `baml.panics.NegativeBitShift`;
//!   `>>` past every bit saturates to `0` or `-1`;
//! - `to_int`, `isqrt` and `ilog` throw `baml.errors.InvalidArgument` and
//!   `parse` throws `baml.errors.ParseError`, with the VM's messages.
//!
//! A mixed `int` / `bigint` operation widens the `int` first
//! ([`from_int`]), as the bytecode does.

use std::{cmp::Ordering, fmt, rc::Rc};

pub use baml_type::MAX_BIGINT_BITS;
use num_bigint::Sign;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, MapAccess, Unexpected, Visitor},
};

use crate::{
    Int63, Panic, Str, Thrown,
    errors::{InvalidArgument, ParseError},
    render::ToBaml,
};

/// A BAML `bigint`. Equality and order are numeric.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BigInt(Rc<num_bigint::BigInt>);

impl BigInt {
    /// The number.
    #[inline]
    pub fn get(&self) -> &num_bigint::BigInt {
        &self.0
    }

    /// The bit length of the magnitude (`0` for zero).
    #[inline]
    pub fn bits(&self) -> u64 {
        self.0.bits()
    }
}

impl From<num_bigint::BigInt> for BigInt {
    #[inline]
    fn from(value: num_bigint::BigInt) -> Self {
        Self(Rc::new(value))
    }
}

impl fmt::Debug for BigInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}n", self.0)
    }
}

impl fmt::Display for BigInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Decimal, like `int`.
impl ToBaml for BigInt {
    fn render(&self, out: &mut String, _nested: bool) {
        use fmt::Write;
        let _ = write!(out, "{}", self.0);
    }
}

// ─── Construction ────────────────────────────────────────────────────────────

/// A `bigint` literal that fits an `i64`.
#[inline]
pub fn from_i64(value: i64) -> BigInt {
    BigInt::from(num_bigint::BigInt::from(value))
}

/// A `bigint` literal from its decimal digits, as the program text spells
/// it (an optional `-`, then digits). The backend only emits digits that
/// parse; a malformed literal is a bug in the emitter.
pub fn lit(digits: &str) -> BigInt {
    match num_bigint::BigInt::parse_bytes(digits.as_bytes(), 10) {
        Some(value) => BigInt::from(value),
        None => panic!("bigint literal {digits:?} is not decimal digits"),
    }
}

/// An `int` widened to a `bigint`: the implicit widening of a mixed
/// operation.
#[inline]
pub fn from_int(value: Int63) -> BigInt {
    from_i64(value.get())
}

/// `bigint.to_int()`: `InvalidArgument` outside the `int` range.
pub fn to_int(value: &BigInt) -> Result<Int63, Thrown> {
    i64::try_from(value.get())
        .ok()
        .and_then(Int63::new)
        .ok_or_else(|| {
            Thrown::error(InvalidArgument {
                message: format!(
                    "bigint.to_int: a {}-bit value is outside int's range (int is 63-bit signed)",
                    value.bits()
                ),
            })
        })
}

/// `bigint.parse(text)`: an optional sign and ASCII digits, nothing else.
pub fn parse(text: &Str) -> Result<BigInt, Thrown> {
    let text = text.as_str();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Thrown::error(ParseError {
            message: format!("bigint.parse: cannot parse {text:?} as bigint"),
        }));
    }
    if digits.len() > baml_type::MAX_BIGINT_DECIMAL_DIGITS {
        return Err(alloc_failure(format!(
            "bigint.parse: input has {} decimal digits, more than the {}-digit limit (bigint cap: {} bits)",
            digits.len(),
            baml_type::MAX_BIGINT_DECIMAL_DIGITS,
            MAX_BIGINT_BITS
        )));
    }
    let magnitude = num_bigint::BigInt::parse_bytes(digits.as_bytes(), 10).ok_or_else(|| {
        Thrown::error(ParseError {
            message: format!("bigint.parse: cannot parse {text:?} as bigint"),
        })
    })?;
    Ok(BigInt::from(if negative { -magnitude } else { magnitude }))
}

// ─── Arithmetic ──────────────────────────────────────────────────────────────

/// `left + right`.
#[inline]
pub fn add(left: &BigInt, right: &BigInt) -> BigInt {
    BigInt::from(left.get() + right.get())
}

/// `left - right`.
#[inline]
pub fn sub(left: &BigInt, right: &BigInt) -> BigInt {
    BigInt::from(left.get() - right.get())
}

/// `left * right`, or `AllocFailure` when the product could pass the cap.
pub fn mul(left: &BigInt, right: &BigInt) -> Result<BigInt, Thrown> {
    let estimated_bits = left.bits().saturating_add(right.bits());
    if estimated_bits > MAX_BIGINT_BITS {
        return Err(alloc_failure(format!(
            "bigint mul: result of bigint multiplication would require ~{estimated_bits} bits (limit: {MAX_BIGINT_BITS})"
        )));
    }
    Ok(BigInt::from(left.get() * right.get()))
}

/// `left / right`, truncating toward zero; `DivisionByZero` for a zero
/// divisor.
pub fn div(left: &BigInt, right: &BigInt) -> Result<BigInt, Thrown> {
    if right.get().sign() == Sign::NoSign {
        return Err(division_by_zero(left));
    }
    Ok(BigInt::from(left.get() / right.get()))
}

/// `left % right` with the dividend's sign; `DivisionByZero` for a zero
/// divisor.
pub fn rem(left: &BigInt, right: &BigInt) -> Result<BigInt, Thrown> {
    if right.get().sign() == Sign::NoSign {
        return Err(division_by_zero(left));
    }
    Ok(BigInt::from(left.get() % right.get()))
}

/// `-value`.
#[inline]
pub fn neg(value: &BigInt) -> BigInt {
    BigInt::from(-value.get())
}

/// `value.abs()`.
pub fn abs(value: &BigInt) -> BigInt {
    if value.get().sign() == Sign::Minus {
        neg(value)
    } else {
        value.clone()
    }
}

/// `base.pow(exp)`: `0` for a negative exponent, `1` for `0 ** 0`, and
/// `AllocFailure` when the result could pass the cap.
pub fn pow(base: &BigInt, exp: &BigInt) -> Result<BigInt, Thrown> {
    let exp_value = exp.get();
    if exp_value.sign() == Sign::Minus {
        return Ok(from_i64(0));
    }
    let base_value = base.get();
    if base_value.sign() == Sign::NoSign {
        // `0 ** 0` is `1` by convention; `0 ** n` is `0`.
        return Ok(from_i64(i64::from(exp_value.sign() == Sign::NoSign)));
    }
    if *base_value == num_bigint::BigInt::from(1) {
        return Ok(from_i64(1));
    }
    if *base_value == num_bigint::BigInt::from(-1) {
        let even = (exp_value % 2u32).sign() == Sign::NoSign;
        return Ok(from_i64(if even { 1 } else { -1 }));
    }
    let exp_u32 = u32::try_from(exp_value).map_err(|_| {
        alloc_failure(format!(
            "bigint.pow: exponent ({exp_value}) exceeds memory limits"
        ))
    })?;
    let estimated_bits = base.bits().saturating_mul(u64::from(exp_u32));
    if estimated_bits > MAX_BIGINT_BITS {
        return Err(alloc_failure(format!(
            "bigint.pow: result of {base_value}^{exp_value} would require ~{estimated_bits} bits (limit: {MAX_BIGINT_BITS})"
        )));
    }
    Ok(BigInt::from(base_value.pow(exp_u32)))
}

/// `value.isqrt()`: the floor of the square root; `InvalidArgument` for a
/// negative value.
pub fn isqrt(value: &BigInt) -> Result<BigInt, Thrown> {
    if value.get().sign() == Sign::Minus {
        return Err(Thrown::error(InvalidArgument {
            message: format!(
                "bigint.isqrt: negative input ({}) has no integer square root",
                value.get()
            ),
        }));
    }
    Ok(BigInt::from(value.get().sqrt()))
}

/// `value.ilog(base)`: the floor of the logarithm; `InvalidArgument` unless
/// `value > 0` and `base >= 2`.
pub fn ilog(value: &BigInt, base: &BigInt) -> Result<BigInt, Thrown> {
    let (value, base) = (value.get(), base.get());
    if value.sign() != Sign::Plus {
        return Err(Thrown::error(InvalidArgument {
            message: format!("bigint.ilog: input ({value}) must be positive"),
        }));
    }
    if *base < num_bigint::BigInt::from(2u32) {
        return Err(Thrown::error(InvalidArgument {
            message: format!("bigint.ilog: base ({base}) must be at least 2"),
        }));
    }
    if *base == num_bigint::BigInt::from(2u32) {
        return Ok(BigInt::from(num_bigint::BigInt::from(value.bits() - 1)));
    }
    // Binary search for the largest `k` with `base^k <= value`, as the VM.
    let denominator = base.bits().saturating_sub(1).max(1);
    let k_max = u32::try_from(
        (value.bits() / denominator)
            .saturating_add(1)
            .min(u64::from(u32::MAX)),
    )
    .unwrap_or(u32::MAX);
    let (mut low, mut high) = (0u32, k_max);
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if base.pow(mid) <= *value {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    Ok(BigInt::from(num_bigint::BigInt::from(low)))
}

// ─── Bits ────────────────────────────────────────────────────────────────────

/// `left & right` over infinite two's complement.
#[inline]
pub fn bit_and(left: &BigInt, right: &BigInt) -> BigInt {
    BigInt::from(left.get() & right.get())
}

/// `left | right`.
#[inline]
pub fn bit_or(left: &BigInt, right: &BigInt) -> BigInt {
    BigInt::from(left.get() | right.get())
}

/// `left ^ right`.
#[inline]
pub fn bit_xor(left: &BigInt, right: &BigInt) -> BigInt {
    BigInt::from(left.get() ^ right.get())
}

/// `left << count`: `NegativeBitShift` for a negative count, `AllocFailure`
/// when the result could pass the cap.
pub fn shl(left: &BigInt, count: &BigInt) -> Result<BigInt, Thrown> {
    let count_value = count.get();
    if count_value.sign() == Sign::Minus {
        return Err(negative_bit_shift("shl", count_value));
    }
    let shift = usize::try_from(count_value).map_err(|_| {
        alloc_failure(format!(
            "bigint shl: shift count ({count_value}) does not fit in usize"
        ))
    })?;
    let estimated_bits = left.bits().saturating_add(shift as u64);
    if estimated_bits > MAX_BIGINT_BITS {
        return Err(alloc_failure(format!(
            "bigint shl: result of {} << {shift} would require ~{estimated_bits} bits (limit: {MAX_BIGINT_BITS})",
            left.get()
        )));
    }
    Ok(BigInt::from(left.get() << shift))
}

/// `left >> count`, arithmetic (toward negative infinity): `NegativeBitShift`
/// for a negative count; a count past every bit gives `0` or `-1`.
pub fn shr(left: &BigInt, count: &BigInt) -> Result<BigInt, Thrown> {
    let count_value = count.get();
    if count_value.sign() == Sign::Minus {
        return Err(negative_bit_shift("shr", count_value));
    }
    Ok(BigInt::from(match usize::try_from(count_value) {
        Ok(shift) => left.get() >> shift,
        Err(_) if left.get().sign() == Sign::Minus => num_bigint::BigInt::from(-1),
        Err(_) => num_bigint::BigInt::ZERO,
    }))
}

// ─── Comparison ──────────────────────────────────────────────────────────────

/// `left == right`, numerically.
#[inline]
pub fn eq(left: &BigInt, right: &BigInt) -> bool {
    left.get() == right.get()
}

/// `<`, `<=`, `>`, `>=`.
#[inline]
pub fn cmp(left: &BigInt, right: &BigInt) -> Ordering {
    left.get().cmp(right.get())
}

#[cold]
fn division_by_zero(dividend: &BigInt) -> Thrown {
    Thrown::Panic(Panic::BigintDivisionByZero {
        dividend: dividend.get().to_string(),
    })
}

#[cold]
fn negative_bit_shift(op: &str, count: &num_bigint::BigInt) -> Thrown {
    Thrown::Panic(Panic::NegativeBitShift {
        message: format!("bigint {op}: negative shift count ({count})"),
    })
}

#[cold]
fn alloc_failure(message: String) -> Thrown {
    Thrown::Panic(Panic::AllocFailure { message })
}

// ─── JSON ────────────────────────────────────────────────────────────────────

/// A JSON number with every digit (`serde_json`'s `arbitrary_precision`),
/// as the VM writes it.
impl Serialize for BigInt {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let number: serde_json::Number = self
            .0
            .to_string()
            .parse()
            .map_err(|_| serde::ser::Error::custom("a bigint's digits are a JSON number"))?;
        number.serialize(serializer)
    }
}

/// Signed decimal digits within the cap, or `None`: the VM's
/// `parse_decimal_bigint`.
fn parse_decimal(text: &str) -> Option<num_bigint::BigInt> {
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    if digits.is_empty()
        || digits.len() > baml_type::MAX_BIGINT_DECIMAL_DIGITS
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let magnitude = num_bigint::BigInt::parse_bytes(digits.as_bytes(), 10)?;
    let value = if negative { -magnitude } else { magnitude };
    (value.bits() <= MAX_BIGINT_BITS).then_some(value)
}

/// An integral JSON number of any width, or a string of decimal digits:
/// the VM's `parse_typed_json_bigint`. A fraction or an exponent is a
/// decode error.
impl<'de> Deserialize<'de> for BigInt {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(BigIntVisitor)
    }
}

struct BigIntVisitor;

const EXPECTED: &str = "expected integral JSON number or numeric string";

impl<'de> Visitor<'de> for BigIntVisitor {
    type Value = BigInt;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(EXPECTED)
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<BigInt, E> {
        Ok(from_i64(value))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<BigInt, E> {
        Ok(BigInt::from(num_bigint::BigInt::from(value)))
    }

    fn visit_i128<E: de::Error>(self, value: i128) -> Result<BigInt, E> {
        Ok(BigInt::from(num_bigint::BigInt::from(value)))
    }

    fn visit_u128<E: de::Error>(self, value: u128) -> Result<BigInt, E> {
        Ok(BigInt::from(num_bigint::BigInt::from(value)))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<BigInt, E> {
        Err(E::invalid_type(Unexpected::Float(value), &self))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<BigInt, E> {
        parse_decimal(value)
            .map(BigInt::from)
            .ok_or_else(|| E::custom(EXPECTED))
    }

    /// `serde_json` with `arbitrary_precision` hands a number it cannot
    /// represent as a primitive over as a one-entry map holding its text.
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<BigInt, A::Error> {
        let mut parsed = None;
        while let Some(key) = map.next_key::<String>()? {
            let text: String = map.next_value()?;
            if key == "$serde_json::private::Number" {
                parsed = parse_decimal(&text);
            }
        }
        parsed
            .map(BigInt::from)
            .ok_or_else(|| de::Error::custom(EXPECTED))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{json, render::to_std, string::from_literal};

    fn big(text: &str) -> BigInt {
        lit(text)
    }

    fn rendered<T: fmt::Debug>(result: Result<T, Thrown>) -> String {
        result.unwrap_err().render_readable()
    }

    #[test]
    fn arithmetic_and_rendering() {
        let a = big("123456789012345678901234567890");
        let b = from_i64(-7);
        assert_eq!(to_std(&add(&a, &b)), "123456789012345678901234567883");
        assert_eq!(to_std(&sub(&b, &a)), "-123456789012345678901234567897");
        assert_eq!(
            to_std(&mul(&a, &b).unwrap()),
            "-864197523086419752308641975230"
        );
        assert_eq!(
            to_std(&div(&a, &b).unwrap()),
            "-17636684144620811271604938270"
        );
        assert_eq!(to_std(&rem(&a, &b).unwrap()), "0");
        // `%` keeps the dividend's sign, `/` truncates toward zero.
        assert_eq!(to_std(&rem(&from_i64(100), &b).unwrap()), "2");
        assert_eq!(to_std(&rem(&from_i64(-100), &from_i64(7)).unwrap()), "-2");
        assert_eq!(to_std(&div(&from_i64(-100), &from_i64(7)).unwrap()), "-14");
        assert_eq!(to_std(&neg(&b)), "7");
        assert_eq!(to_std(&abs(&b)), "7");
        assert_eq!(format!("{b:?}"), "-7n");
        assert_eq!(to_std(&from_int(Int63::MIN)), "-4611686018427387904");
        assert_eq!(to_int(&from_int(Int63::MAX)).unwrap(), Int63::MAX);
        assert_eq!(
            rendered(to_int(&a)),
            r#"baml.errors.InvalidArgument {message: "bigint.to_int: a 97-bit value is outside int's range (int is 63-bit signed)"}"#
        );
        assert_eq!(
            rendered(to_int(&big("4611686018427387904"))),
            r#"baml.errors.InvalidArgument {message: "bigint.to_int: a 63-bit value is outside int's range (int is 63-bit signed)"}"#
        );
    }

    #[test]
    fn division_by_zero_carries_the_bigint_dividend() {
        let a = big("-99999999999999999999");
        assert_eq!(
            rendered(div(&a, &from_i64(0))),
            "baml.panics.DivisionByZero {dividend: -99999999999999999999}"
        );
        assert_eq!(
            rendered(rem(&from_i64(5), &from_i64(0))),
            "baml.panics.DivisionByZero {dividend: 5}"
        );
    }

    #[test]
    fn comparison_is_numeric() {
        let (a, b) = (big("10"), big("-10"));
        assert!(eq(&a, &from_i64(10)));
        assert!(!eq(&a, &b));
        assert_eq!(cmp(&b, &a), Ordering::Less);
        assert_eq!(cmp(&a, &a), Ordering::Equal);
        assert!(a > b);
    }

    #[test]
    fn bits_and_shifts() {
        let minus_one = from_i64(-1);
        assert_eq!(to_std(&bit_and(&minus_one, &from_i64(1))), "1");
        assert_eq!(to_std(&bit_or(&minus_one, &from_i64(0))), "-1");
        assert_eq!(to_std(&bit_xor(&minus_one, &from_i64(0))), "-1");
        assert_eq!(
            to_std(&shl(&from_i64(1), &from_i64(100)).unwrap()),
            "1267650600228229401496703205376"
        );
        assert_eq!(to_std(&shr(&from_i64(-9), &from_i64(1)).unwrap()), "-5");
        assert_eq!(
            to_std(&shr(&from_i64(9), &big("99999999999999999999")).unwrap()),
            "0"
        );
        assert_eq!(
            to_std(&shr(&from_i64(-9), &big("99999999999999999999")).unwrap()),
            "-1"
        );
        assert_eq!(
            rendered(shl(&from_i64(1), &from_i64(-1))),
            r#"baml.panics.NegativeBitShift {message: "bigint shl: negative shift count (-1)"}"#
        );
        assert_eq!(
            rendered(shr(&from_i64(1), &from_i64(-2))),
            r#"baml.panics.NegativeBitShift {message: "bigint shr: negative shift count (-2)"}"#
        );
        assert_eq!(
            rendered(shl(&from_i64(1), &from_i64(300_000_000))),
            r#"baml.panics.AllocFailure {message: "bigint shl: result of 1 << 300000000 would require ~300000001 bits (limit: 268435456)"}"#
        );
        assert_eq!(
            shl(&from_i64(1), &big("99999999999999999999"))
                .unwrap_err()
                .class_fqn(),
            "baml.panics.AllocFailure"
        );
    }

    #[test]
    fn cap_guards_multiplication_and_powers() {
        let wide = shl(&from_i64(1), &from_i64(200_000_000)).unwrap();
        assert_eq!(
            mul(&wide, &wide).unwrap_err().class_fqn(),
            "baml.panics.AllocFailure"
        );
        assert_eq!(to_std(&pow(&from_i64(2), &from_i64(10)).unwrap()), "1024");
        assert_eq!(to_std(&pow(&from_i64(2), &from_i64(0)).unwrap()), "1");
        assert_eq!(to_std(&pow(&from_i64(0), &from_i64(0)).unwrap()), "1");
        assert_eq!(to_std(&pow(&from_i64(0), &from_i64(3)).unwrap()), "0");
        assert_eq!(to_std(&pow(&from_i64(2), &from_i64(-1)).unwrap()), "0");
        assert_eq!(to_std(&pow(&from_i64(-2), &from_i64(3)).unwrap()), "-8");
        assert_eq!(
            to_std(&pow(&from_i64(-1), &big("1000000000000")).unwrap()),
            "1"
        );
        assert_eq!(
            to_std(&pow(&from_i64(-1), &big("1000000000001")).unwrap()),
            "-1"
        );
        assert_eq!(
            to_std(&pow(&from_i64(1), &big("1000000000000")).unwrap()),
            "1"
        );
        assert_eq!(
            rendered(pow(&from_i64(2), &from_i64(300_000_000))),
            r#"baml.panics.AllocFailure {message: "bigint.pow: result of 2^300000000 would require ~600000000 bits (limit: 268435456)"}"#
        );
        assert_eq!(
            rendered(pow(&from_i64(2), &big("99999999999999999999"))),
            r#"baml.panics.AllocFailure {message: "bigint.pow: exponent (99999999999999999999) exceeds memory limits"}"#
        );
    }

    #[test]
    fn roots_and_logs() {
        assert_eq!(to_std(&isqrt(&from_i64(10)).unwrap()), "3");
        assert_eq!(to_std(&isqrt(&from_i64(16)).unwrap()), "4");
        assert_eq!(
            rendered(isqrt(&from_i64(-1))),
            r#"baml.errors.InvalidArgument {message: "bigint.isqrt: negative input (-1) has no integer square root"}"#
        );
        assert_eq!(to_std(&ilog(&from_i64(1000), &from_i64(10)).unwrap()), "3");
        assert_eq!(to_std(&ilog(&from_i64(999), &from_i64(10)).unwrap()), "2");
        assert_eq!(to_std(&ilog(&from_i64(1024), &from_i64(2)).unwrap()), "10");
        assert_eq!(to_std(&ilog(&from_i64(1), &from_i64(10)).unwrap()), "0");
        assert_eq!(
            to_std(&ilog(&big("1000000000000000000000000000000"), &from_i64(1000)).unwrap()),
            "10"
        );
        assert_eq!(
            rendered(ilog(&from_i64(0), &from_i64(10))),
            r#"baml.errors.InvalidArgument {message: "bigint.ilog: input (0) must be positive"}"#
        );
        assert_eq!(
            rendered(ilog(&from_i64(10), &from_i64(1))),
            r#"baml.errors.InvalidArgument {message: "bigint.ilog: base (1) must be at least 2"}"#
        );
    }

    #[test]
    fn parsing() {
        assert_eq!(to_std(&parse(&from_literal("42")).unwrap()), "42");
        assert_eq!(to_std(&parse(&from_literal("-7")).unwrap()), "-7");
        assert_eq!(to_std(&parse(&from_literal("+0")).unwrap()), "0");
        assert_eq!(
            to_std(&parse(&from_literal("99999999999999999999")).unwrap()),
            "99999999999999999999"
        );
        for bad in ["", "12a", "0x2a", "1_000", " 5 ", "-", "+"] {
            let message = format!("bigint.parse: cannot parse {bad:?} as bigint");
            assert_eq!(
                rendered(parse(&Str::from(bad))),
                format!("baml.errors.ParseError {{message: {message:?}}}")
            );
        }
        let long = Str::from("9".repeat(baml_type::MAX_BIGINT_DECIMAL_DIGITS + 1));
        assert_eq!(
            parse(&long).unwrap_err().class_fqn(),
            "baml.panics.AllocFailure"
        );
    }

    #[test]
    fn json_is_a_number_with_every_digit() {
        let a = big("-123456789012345678901234567890");
        assert_eq!(
            json::to_string(&a).unwrap().as_str(),
            "-123456789012345678901234567890"
        );
        assert_eq!(json::to_string(&from_i64(7)).unwrap().as_str(), "7");
        assert_eq!(
            json::to_string(&vec![from_i64(1), a]).unwrap().as_str(),
            "[1,-123456789012345678901234567890]"
        );
        for (input, expected) in [
            ("42", "42"),
            ("-42", "-42"),
            ("18446744073709551616", "18446744073709551616"),
            (
                "-123456789012345678901234567890",
                "-123456789012345678901234567890",
            ),
            ("\"42\"", "42"),
            ("\"-99999999999999999999\"", "-99999999999999999999"),
        ] {
            let value: BigInt = json::deserialize(&Str::from(input)).unwrap();
            assert_eq!(to_std(&value), expected, "{input}");
        }
        for bad in [
            "12.5", "1e3", "1.0", "\"12.5\"", "\"1e3\"", "\" 42\"", "true", "[1]", "{}",
        ] {
            let thrown = json::deserialize::<BigInt>(&Str::from(bad)).unwrap_err();
            assert_eq!(thrown.class_fqn(), "baml.json.DecodeError", "{bad}");
        }
    }
}
