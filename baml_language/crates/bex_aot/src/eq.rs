//! The broad `==` (`baml.ops.equals_equals`) for native values.
//!
//! The VM's `EqualsDriver` (`bex_vm/src/package_baml/ops.rs`) decides `==`
//! between values of any two types: operands of different runtime kinds are
//! never equal, primitives, strings and bigints compare by value, `float`
//! with the reflexive order (`NaN == NaN`, [`bex_lang::float::eq`]), `null`
//! equals `null`, enums by variant. A generated union implements the trait
//! by delegating to the member both sides hold, so a union value compared
//! with a member value (`x == 1` on an `int | float`) goes through the same
//! rule after the member is lifted into the union.
//!
//! Class instances compare structurally on the VM (field by field, with a
//! class's own `baml.ops.Equals` dispatched when it has one); that is not
//! implemented here, and the backend rejects `==` on them.

use crate::{BigInt, Int63, Str, bigint, float, string};

/// The broad `==`, for the types that compare natively.
pub trait BamlEq {
    /// Whether `self == other` on the VM.
    fn baml_eq(&self, other: &Self) -> bool;
}

/// `a == b`.
pub fn equals<T: BamlEq + ?Sized>(a: &T, b: &T) -> bool {
    a.baml_eq(b)
}

impl BamlEq for Int63 {
    fn baml_eq(&self, other: &Self) -> bool {
        self == other
    }
}

impl BamlEq for bool {
    fn baml_eq(&self, other: &Self) -> bool {
        self == other
    }
}

impl BamlEq for f64 {
    fn baml_eq(&self, other: &Self) -> bool {
        float::eq(*self, *other)
    }
}

impl BamlEq for Str {
    fn baml_eq(&self, other: &Self) -> bool {
        string::eq(self, other)
    }
}

impl BamlEq for BigInt {
    fn baml_eq(&self, other: &Self) -> bool {
        bigint::eq(self, other)
    }
}

/// `null == null`.
impl BamlEq for () {
    fn baml_eq(&self, _other: &Self) -> bool {
        true
    }
}

/// `null` equals only `null`; two values compare as their type does.
impl<T: BamlEq> BamlEq for Option<T> {
    fn baml_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (None, None) => true,
            (Some(a), Some(b)) => a.baml_eq(b),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::string::from_literal;

    fn int(value: i64) -> Int63 {
        Int63::new(value).unwrap()
    }

    #[test]
    fn primitives_compare_by_value() {
        assert!(equals(&int(1), &int(1)));
        assert!(!equals(&int(1), &int(2)));
        assert!(equals(&true, &true));
        assert!(equals(&f64::NAN, &f64::NAN), "the reflexive float order");
        assert!(!equals(&0.0, &-0.0) || float::eq(0.0, -0.0));
        assert!(equals(&from_literal("a"), &from_literal("a")));
        assert!(!equals(&from_literal("a"), &from_literal("b")));
        assert!(equals(&bigint::from_i64(7), &bigint::from_i64(7)));
        assert!(equals(&(), &()));
    }

    #[test]
    fn null_equals_only_null() {
        assert!(equals(&None::<Int63>, &None));
        assert!(!equals(&None, &Some(int(1))));
        assert!(!equals(&Some(int(1)), &None));
        assert!(equals(&Some(int(1)), &Some(int(1))));
        assert!(!equals(&Some(int(1)), &Some(int(2))));
    }
}
