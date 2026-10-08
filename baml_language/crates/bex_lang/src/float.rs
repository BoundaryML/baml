//! `float` semantics shared by interpreted and compiled BAML: the total order,
//! the `to_string` rendering, and the fallible conversions to `int`.
//!
//! # The order
//!
//! BAML's `baml.ops.Equals` is **reflexive** and `baml.ops.Compare` is a **total
//! order** — the language has no partial counterpart of either, so `float` must
//! satisfy both. That forces one deliberate departure from IEEE 754, which
//! leaves NaN unordered and unequal to itself:
//!
//! ```text
//! -inf  <  …negative reals…  <  0.0  <  …positive reals…  <  +inf  <  NaN
//! ```
//!
//! - **NaN is a single value, greater than every number.** Every NaN compares
//!   `Equal` to every other NaN and `Greater` than everything else, so `x == x`
//!   holds for all floats. Sign and payload are *not* observable here: the
//!   NaN a hardware invalid-operation produces differs across targets
//!   (`(-1.0).sqrt()` yields a sign-set NaN on x86-64 and a sign-clear one on
//!   `AArch64`), so a sign- or payload-sensitive order — IEEE 754's `totalOrder`
//!   predicate, i.e. [`f64::total_cmp`] — would make comparison results
//!   platform-dependent.
//! - **`-0.0` and `0.0` are `Equal`**, as under IEEE 754. Collapsing the two
//!   adjacent `totalOrder` classes keeps the order total and keeps `x == 0.0`
//!   true for a negatively-signed zero.
//!
//! Every other pair orders exactly as IEEE 754 does, so this differs from the
//! `<` / `<=` / `>` / `>=` / `==` / `!=` of most languages only where a NaN is
//! involved.
//!
//! This module is the single definition of that order. Everything that orders
//! or equates BAML floats routes through it: the specialized `CmpFloat*`
//! opcodes and the generic `exec_cmpop` float arm in the VM (via
//! `bex_vm_types::float_order`), the `baml.ops.Equals for float` builtin, the
//! `==` driver's float leaf, the natural-sort comparator behind
//! `float[].sort()`, and the comparisons generated native code emits.

use std::cmp::Ordering;

use crate::{Int63, Str, Thrown, errors::InvalidArgument};

/// Three-way compare two floats in BAML's total float order (module docs).
///
/// Total, and consistent with [`eq`]: `cmp(a, b) == Ordering::Equal` exactly
/// when `eq(a, b)`.
#[inline]
#[must_use]
pub fn cmp(a: f64, b: f64) -> Ordering {
    // `partial_cmp` is `None` exactly when either operand is NaN, and orders
    // `-0.0` and `0.0` as `Equal`. The fallback then places NaN above every
    // number and ties NaN with NaN: `false < true` gives `Less` when only `b`
    // is NaN, `Greater` when only `a` is, and `Equal` when both are.
    a.partial_cmp(&b)
        .unwrap_or_else(|| a.is_nan().cmp(&b.is_nan()))
}

/// Whether two floats are equal in BAML's reflexive float equality: IEEE 754
/// equality (so `-0.0 == 0.0`) extended to make every NaN equal to every other
/// NaN, and therefore to itself.
#[inline]
#[must_use]
pub fn eq(a: f64, b: f64) -> bool {
    // Not `cmp(a, b).is_eq()`: this compiles to a branchless `ucomisd` plus an
    // or of the two NaN tests, and `==` on floats is far hotter than ordering.
    // (IEEE `==` is the intended base case; `clippy::float_cmp` is exempt in an
    // `eq`-named fn, so it needs no attribute.)
    a == b || (a.is_nan() && b.is_nan())
}

/// `a != b` under the total order.
#[inline]
#[must_use]
pub fn ne(a: f64, b: f64) -> bool {
    !eq(a, b)
}

/// `a < b` under the total order.
#[inline]
#[must_use]
pub fn lt(a: f64, b: f64) -> bool {
    cmp(a, b).is_lt()
}

/// `a <= b` under the total order.
#[inline]
#[must_use]
pub fn le(a: f64, b: f64) -> bool {
    cmp(a, b).is_le()
}

/// `a > b` under the total order.
#[inline]
#[must_use]
pub fn gt(a: f64, b: f64) -> bool {
    cmp(a, b).is_gt()
}

/// `a >= b` under the total order.
#[inline]
#[must_use]
pub fn ge(a: f64, b: f64) -> bool {
    cmp(a, b).is_ge()
}

/// `float.to_string()` as a Rust `String`: Rust's shortest round-trip
/// `Display` with `.0` appended when it printed no decimal point (`1.0`, not
/// `1`), and the JavaScript spellings `NaN`, `Infinity`, `-Infinity`. Rust's
/// `Display` never uses exponent notation, so `1e21` prints all its digits and
/// `1e-7` prints as `0.0000001`.
///
/// Identical to `bex_vm_types::format_float`; that function is kept for the
/// VM's call sites and tested against this one.
#[must_use]
pub fn format_std(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_positive() {
            "Infinity".to_owned()
        } else {
            "-Infinity".to_owned()
        };
    }
    let text = value.to_string();
    if text.contains('.') {
        text
    } else {
        format!("{text}.0")
    }
}

/// `float.to_string()`; see [`format_std`].
#[must_use]
pub fn format(value: f64) -> Str {
    Str::from(format_std(value))
}

/// `float.floor()`. Never throws.
#[inline]
#[must_use]
pub fn floor(value: f64) -> f64 {
    value.floor()
}

/// `float.ceil()`. Never throws.
#[inline]
#[must_use]
pub fn ceil(value: f64) -> f64 {
    value.ceil()
}

/// `float.round()`: half away from zero, as Rust's `f64::round`. Never throws.
#[inline]
#[must_use]
pub fn round(value: f64) -> f64 {
    value.round()
}

/// `float.trunc()`. Never throws.
#[inline]
#[must_use]
pub fn trunc(value: f64) -> f64 {
    value.trunc()
}

/// `float.itrunc()`: truncate toward zero and convert to `int`, throwing
/// `baml.errors.InvalidArgument` for NaN or a result outside the `int` range.
#[inline]
pub fn itrunc(value: f64) -> Result<Int63, Thrown> {
    to_int(value.trunc(), "itrunc")
}

/// `float.ifloor()`; errors as [`itrunc`].
#[inline]
pub fn ifloor(value: f64) -> Result<Int63, Thrown> {
    to_int(value.floor(), "ifloor")
}

/// `float.iceil()`; errors as [`itrunc`].
#[inline]
pub fn iceil(value: f64) -> Result<Int63, Thrown> {
    to_int(value.ceil(), "iceil")
}

/// `float.iround()`; errors as [`itrunc`].
#[inline]
pub fn iround(value: f64) -> Result<Int63, Thrown> {
    to_int(value.round(), "iround")
}

// BAML int is i63. `-2^62` and `2^62` are powers of two, exactly representable
// in f64, so the in-range predicate is `MIN_F <= r < MAX_PLUS_ONE_F` with a
// strict upper bound. NaN fails it through the usual comparison rules, but is
// reported separately because the VM words that case differently.
const MIN_F: f64 = -4_611_686_018_427_387_904.0; // -2^62
const MAX_PLUS_ONE_F: f64 = 4_611_686_018_427_387_904.0; // 2^62

/// Convert an already-rounded float to `int`, with the VM's exact
/// `InvalidArgument` messages: `"float.{op}: cannot convert NaN to int"` and
/// `"float.{op}: {value} is out of int range"` (`value` in Rust `Display`
/// form, so `inf`, `-inf`, `1e300` printed in full).
#[inline]
fn to_int(value: f64, op: &str) -> Result<Int63, Thrown> {
    if value.is_nan() {
        return Err(nan_to_int(op));
    }
    if !(MIN_F..MAX_PLUS_ONE_F).contains(&value) {
        return Err(out_of_int_range(value, op));
    }
    // The range check above guarantees the cast is exact.
    #[allow(clippy::cast_possible_truncation)]
    let raw = value as i64;
    Int63::new(raw).ok_or_else(|| out_of_int_range(value, op))
}

#[cold]
fn nan_to_int(op: &str) -> Thrown {
    Thrown::error(InvalidArgument {
        message: format!("float.{op}: cannot convert NaN to int"),
    })
}

#[cold]
fn out_of_int_range(value: f64, op: &str) -> Thrown {
    Thrown::error(InvalidArgument {
        message: format!("float.{op}: {value} is out of int range"),
    })
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    /// A NaN with the sign bit set and a non-default payload — the shape a
    /// hardware invalid-operation can hand back on one target and not another.
    fn odd_nan() -> f64 {
        f64::from_bits(0xFFF8_0000_DEAD_BEEF)
    }

    fn invalid_argument(result: Result<Int63, Thrown>) -> String {
        match result {
            Err(Thrown::Error(error)) => {
                assert_eq!(error.class_fqn(), "baml.errors.InvalidArgument");
                error.render_readable()
            }
            other => panic!("expected InvalidArgument, got {other:?}"),
        }
    }

    #[test]
    fn equality_is_reflexive() {
        for x in [
            0.0,
            -0.0,
            1.5,
            -1.5,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
            odd_nan(),
        ] {
            assert!(eq(x, x), "{x} should equal itself");
            assert_eq!(cmp(x, x), Ordering::Equal, "{x} should compare equal");
        }
    }

    #[test]
    fn every_nan_is_one_value() {
        assert!(eq(f64::NAN, odd_nan()));
        assert!(eq(odd_nan(), -f64::NAN));
        assert_eq!(cmp(f64::NAN, odd_nan()), Ordering::Equal);
    }

    #[test]
    fn nan_is_the_greatest_float() {
        for x in [0.0, -0.0, 1.5, -1.5, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(cmp(f64::NAN, x), Ordering::Greater);
            assert_eq!(cmp(x, f64::NAN), Ordering::Less);
            assert!(!eq(f64::NAN, x));
            assert!(gt(f64::NAN, x));
            assert!(lt(x, f64::NAN));
        }
    }

    #[test]
    fn signed_zeros_are_one_value() {
        assert!(eq(-0.0, 0.0));
        assert_eq!(cmp(-0.0, 0.0), Ordering::Equal);
        assert!(le(-0.0, 0.0) && ge(-0.0, 0.0) && !lt(-0.0, 0.0));
    }

    #[test]
    fn numbers_order_as_ieee() {
        assert_eq!(cmp(1.0, 2.0), Ordering::Less);
        assert_eq!(cmp(2.0, 1.0), Ordering::Greater);
        assert_eq!(cmp(f64::NEG_INFINITY, -1e308), Ordering::Less);
        assert_eq!(cmp(1e308, f64::INFINITY), Ordering::Less);
    }

    /// The order must be a total order: antisymmetric, and transitive across
    /// the special values (which is what a naive `partial_cmp` unwrap breaks).
    #[test]
    fn order_is_total() {
        let domain = [
            f64::NEG_INFINITY,
            -1.5,
            -0.0,
            0.0,
            1.5,
            f64::INFINITY,
            f64::NAN,
            odd_nan(),
        ];
        for &a in &domain {
            for &b in &domain {
                assert_eq!(cmp(a, b), cmp(b, a).reverse(), "antisymmetry: {a} vs {b}");
                for &c in &domain {
                    if cmp(a, b).is_le() && cmp(b, c).is_le() {
                        assert!(cmp(a, c).is_le(), "transitivity: {a} <= {b} <= {c}");
                    }
                }
            }
        }
    }

    #[test]
    fn operators_agree_with_the_order() {
        let domain = [
            f64::NEG_INFINITY,
            -1.5,
            -0.0,
            0.0,
            1.5,
            f64::INFINITY,
            f64::NAN,
        ];
        for &a in &domain {
            for &b in &domain {
                assert_eq!(eq(a, b), cmp(a, b).is_eq());
                assert_eq!(ne(a, b), !cmp(a, b).is_eq());
                assert_eq!(lt(a, b), cmp(a, b).is_lt());
                assert_eq!(le(a, b), cmp(a, b).is_le());
                assert_eq!(gt(a, b), cmp(a, b).is_gt());
                assert_eq!(ge(a, b), cmp(a, b).is_ge());
            }
        }
    }

    #[test]
    fn format_matches_the_vm() {
        let cases: &[(f64, &str)] = &[
            (1.0, "1.0"),
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1.5, "1.5"),
            (-2.25, "-2.25"),
            (100.0, "100.0"),
            (0.1, "0.1"),
            (0.1 + 0.2, "0.30000000000000004"),
            (1e21, "1000000000000000000000.0"),
            (1e-7, "0.0000001"),
            (123_456_789.0, "123456789.0"),
            (1e300, format!("1{}.0", "0".repeat(300)).leak()),
            (f64::MAX, format!("{}.0", f64::MAX).leak()),
            (f64::NAN, "NaN"),
            (-f64::NAN, "NaN"),
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
            (f64::MIN_POSITIVE, format!("{}", f64::MIN_POSITIVE).leak()),
        ];
        for (value, expected) in cases {
            assert_eq!(format_std(*value), *expected, "{value:?}");
            assert_eq!(format(*value).as_str(), *expected, "{value:?}");
        }
        assert!(format_std(f64::MAX).ends_with(".0"));
        assert!(format_std(f64::MIN_POSITIVE).starts_with("0.0000"));
    }

    #[test]
    fn rounding_never_throws() {
        assert_eq!(floor(-1.5), -2.0);
        assert_eq!(ceil(-1.5), -1.0);
        assert_eq!(round(2.5), 3.0);
        assert_eq!(round(-2.5), -3.0);
        assert_eq!(trunc(-1.9), -1.0);
        assert!(floor(f64::NAN).is_nan());
        assert_eq!(floor(f64::INFINITY), f64::INFINITY);
    }

    #[test]
    fn int_conversions_round_then_convert() {
        let int = |v: i64| Int63::new(v).unwrap();
        assert_eq!(itrunc(-1.9).unwrap(), int(-1));
        assert_eq!(ifloor(-1.1).unwrap(), int(-2));
        assert_eq!(iceil(1.1).unwrap(), int(2));
        assert_eq!(iround(2.5).unwrap(), int(3));
        assert_eq!(iround(-2.5).unwrap(), int(-3));
        assert_eq!(itrunc(0.0).unwrap(), Int63::ZERO);
        assert_eq!(itrunc(-0.0).unwrap(), Int63::ZERO);
        assert_eq!(itrunc(MIN_F).unwrap(), Int63::MIN);
        // 2^62 - 1 is not representable; the largest float below 2^62 is.
        let below_max = f64::from_bits(MAX_PLUS_ONE_F.to_bits() - 1);
        assert_eq!(itrunc(below_max).unwrap(), int(4_611_686_018_427_387_392));
        // The floor lands in range even though the input is just outside.
        assert_eq!(itrunc(-4_611_686_018_427_387_904.5).unwrap(), Int63::MIN);
    }

    #[test]
    fn int_conversion_errors_match_the_vm() {
        assert_eq!(
            invalid_argument(itrunc(f64::NAN)),
            r#"baml.errors.InvalidArgument {message: "float.itrunc: cannot convert NaN to int"}"#
        );
        assert_eq!(
            invalid_argument(ifloor(f64::NAN)),
            r#"baml.errors.InvalidArgument {message: "float.ifloor: cannot convert NaN to int"}"#
        );
        // `{value}` is Rust's shortest round-trip form, so 2^62 prints rounded.
        assert_eq!(
            invalid_argument(itrunc(MAX_PLUS_ONE_F)),
            r#"baml.errors.InvalidArgument {message: "float.itrunc: 4611686018427388000 is out of int range"}"#
        );
        assert_eq!(
            invalid_argument(iceil(f64::INFINITY)),
            r#"baml.errors.InvalidArgument {message: "float.iceil: inf is out of int range"}"#
        );
        assert_eq!(
            invalid_argument(iround(f64::NEG_INFINITY)),
            r#"baml.errors.InvalidArgument {message: "float.iround: -inf is out of int range"}"#
        );
        assert_eq!(
            invalid_argument(itrunc(1e19)),
            r#"baml.errors.InvalidArgument {message: "float.itrunc: 10000000000000000000 is out of int range"}"#
        );
        // The rounded value is what the message reports.
        assert_eq!(
            invalid_argument(iceil(4_611_686_018_427_387_903.5)),
            r#"baml.errors.InvalidArgument {message: "float.iceil: 4611686018427388000 is out of int range"}"#
        );
        assert_eq!(itrunc(f64::NAN).unwrap_err().exit_code(), 1);
    }
}
