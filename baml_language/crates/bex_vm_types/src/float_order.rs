//! BAML's total order on `float`, and the equality derived from it.
//!
//! The order is defined once, in [`bex_lang::float`] (see its docs for the
//! rules: NaN is a single value above every number, `-0.0 == 0.0`, everything
//! else as IEEE 754). This module re-exports it for the VM's call sites and
//! adds [`apply`], which maps a [`CmpOp`] onto the order so the specialized
//! `CmpFloat*` opcodes and the generic `exec_cmpop` float arm cannot disagree.

pub use bex_lang::float::{cmp, eq};

use crate::bytecode::CmpOp;

/// Apply a comparison operator to two floats under BAML's float order.
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

    /// `bex_lang::float::format_std` is the native backend's `float.to_string`;
    /// it must print exactly what the VM's `format_float` prints.
    #[test]
    fn float_formatting_matches_bex_lang() {
        for value in [
            0.0,
            -0.0,
            1.0,
            -1.0,
            1.5,
            0.1 + 0.2,
            1e21,
            1e-7,
            1e300,
            f64::MAX,
            f64::MIN_POSITIVE,
            f64::EPSILON,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            assert_eq!(
                crate::format_float(value),
                bex_lang::float::format_std(value),
                "{value:?}"
            );
        }
    }
}
