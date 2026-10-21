//! BAML's total order on `float`, and the equality derived from it.
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
//! opcodes and the generic `exec_cmpop` float arm in the VM, the
//! `baml.ops.Equals for float` builtin, the `==` driver's float leaf, and the
//! natural-sort comparator behind `float[].sort()`.

use std::cmp::Ordering;

use crate::bytecode::CmpOp;

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

/// Apply a comparison operator to two floats under BAML's float order.
///
/// The one place the six operators are derived from the order, shared by the
/// specialized `CmpFloat*` opcodes and the generic `exec_cmpop` float arm so
/// the fast path and the fallback cannot disagree.
#[inline]
#[must_use]
pub fn apply(op: CmpOp, a: f64, b: f64) -> bool {
    match op {
        CmpOp::Eq => eq(a, b),
        CmpOp::NotEq => !eq(a, b),
        CmpOp::Lt => cmp(a, b).is_lt(),
        CmpOp::LtEq => cmp(a, b).is_le(),
        CmpOp::Gt => cmp(a, b).is_gt(),
        CmpOp::GtEq => cmp(a, b).is_ge(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A NaN with the sign bit set and a non-default payload — the shape a
    /// hardware invalid-operation can hand back on one target and not another.
    fn odd_nan() -> f64 {
        f64::from_bits(0xFFF8_0000_DEAD_BEEF)
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
        }
    }

    #[test]
    fn signed_zeros_are_one_value() {
        assert!(eq(-0.0, 0.0));
        assert_eq!(cmp(-0.0, 0.0), Ordering::Equal);
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
                assert_eq!(apply(CmpOp::Eq, a, b), cmp(a, b).is_eq());
                assert_eq!(apply(CmpOp::NotEq, a, b), !cmp(a, b).is_eq());
                assert_eq!(apply(CmpOp::Lt, a, b), cmp(a, b).is_lt());
                assert_eq!(apply(CmpOp::LtEq, a, b), cmp(a, b).is_le());
                assert_eq!(apply(CmpOp::Gt, a, b), cmp(a, b).is_gt());
                assert_eq!(apply(CmpOp::GtEq, a, b), cmp(a, b).is_ge());
            }
        }
    }
}
