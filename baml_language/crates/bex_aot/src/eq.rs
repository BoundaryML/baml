//! The broad `==` (`baml.ops.equals_equals`) for native values: the VM's
//! `EqualsDriver` (`bex_vm/src/package_baml/ops.rs`) over native types.
//!
//! Operands of different runtime kinds are never equal; primitives, strings
//! and bigints compare by value, `float` with the reflexive order (`NaN ==
//! NaN`, [`bex_lang::float::eq`]), `null` equals `null`, an enum by variant
//! or by its own `baml.ops.Equals`. An object (a class instance, array or
//! map) is equal to itself without inspection; two objects are compared
//! structurally: arrays element by element, maps entry by entry (a key of
//! one looked up in the other, the values compared), class instances field
//! by field, or by the class's own `baml.ops.Equals` when it has one, called
//! under one frame (the VM's continuation frame). A pair of objects
//! already under comparison higher on the walk (a cycle) is taken as equal,
//! so the walk terminates, as the VM's visited set does. Children are
//! copied out of an object before they are compared, so no borrow is held
//! while a class's own `eq` runs. A nested `==` inside such an `eq` is a
//! walk of its own.

use std::collections::HashSet;

use crate::{BigInt, Int63, Str, Thrown, bigint, depth, float, handle::Shared, string};

/// One walk of `==`: the object pairs under comparison.
#[derive(Default)]
pub struct EqCx {
    visited: HashSet<(*const (), *const ())>,
}

impl EqCx {
    /// Note that `a` and `b` are now under comparison; `true` when they
    /// already were (a cycle), in which case they are taken as equal.
    pub(crate) fn visit(&mut self, a: *const (), b: *const ()) -> bool {
        let key = if a < b { (a, b) } else { (b, a) };
        !self.visited.insert(key)
    }
}

/// The broad `==`.
pub trait BamlEq {
    /// Whether `self == other` on the VM; `Err` is what a class's own `eq`
    /// threw.
    fn baml_eq(&self, other: &Self, cx: &mut EqCx) -> Result<bool, Thrown>;
}

/// `a == b`.
pub fn equals<T: BamlEq + ?Sized>(a: &T, b: &T) -> Result<bool, Thrown> {
    a.baml_eq(b, &mut EqCx::default())
}

/// What an object does under `==` once identity and cycles are settled: its
/// class's own `baml.ops.Equals`, or field by field. The generated code
/// implements this for every class; arrays take the defaults.
pub trait EqHandle: Sized {
    /// Whether the class provides its own `eq`.
    const OVERRIDES: bool = false;

    /// The class's own `eq` on the two objects.
    fn call_eq(_a: &Shared<Self>, _b: &Shared<Self>) -> Result<bool, Thrown> {
        unreachable!("call_eq on a class without an `eq` override")
    }

    /// Field by field (element by element), each pair copied out before it
    /// is compared.
    fn fields_eq(a: &Shared<Self>, b: &Shared<Self>, cx: &mut EqCx) -> Result<bool, Thrown>;
}

impl<T: BamlEq + Clone> EqHandle for Vec<T> {
    fn fields_eq(a: &Shared<Self>, b: &Shared<Self>, cx: &mut EqCx) -> Result<bool, Thrown> {
        let (xs, ys) = (a.borrow().clone(), b.borrow().clone());
        if xs.len() != ys.len() {
            return Ok(false);
        }
        for (x, y) in xs.iter().zip(&ys) {
            if !x.baml_eq(y, cx)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl<T: EqHandle> BamlEq for Shared<T> {
    fn baml_eq(&self, other: &Self, cx: &mut EqCx) -> Result<bool, Thrown> {
        if crate::handle::ptr_eq(self, other) || cx.visit(self.id(), other.id()) {
            return Ok(true);
        }
        if T::OVERRIDES {
            let _frame = depth::Guard::enter()?;
            return T::call_eq(self, other);
        }
        T::fields_eq(self, other, cx)
    }
}

macro_rules! by_value {
    ($($ty:ty),*) => {$(
        impl BamlEq for $ty {
            fn baml_eq(&self, other: &Self, _cx: &mut EqCx) -> Result<bool, Thrown> {
                Ok(self == other)
            }
        }
    )*};
}

by_value!(Int63, bool);

impl BamlEq for f64 {
    fn baml_eq(&self, other: &Self, _cx: &mut EqCx) -> Result<bool, Thrown> {
        Ok(float::eq(*self, *other))
    }
}

impl BamlEq for Str {
    fn baml_eq(&self, other: &Self, _cx: &mut EqCx) -> Result<bool, Thrown> {
        Ok(string::eq(self, other))
    }
}

impl BamlEq for BigInt {
    fn baml_eq(&self, other: &Self, _cx: &mut EqCx) -> Result<bool, Thrown> {
        Ok(bigint::eq(self, other))
    }
}

/// `null == null`.
impl BamlEq for () {
    fn baml_eq(&self, _other: &Self, _cx: &mut EqCx) -> Result<bool, Thrown> {
        Ok(true)
    }
}

/// `null` equals only `null`; two values compare as their type does.
impl<T: BamlEq> BamlEq for Option<T> {
    fn baml_eq(&self, other: &Self, cx: &mut EqCx) -> Result<bool, Thrown> {
        match (self, other) {
            (None, None) => Ok(true),
            (Some(a), Some(b)) => a.baml_eq(b, cx),
            _ => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{array, handle::shared, string::from_literal};

    fn int(value: i64) -> Int63 {
        Int63::new(value).unwrap()
    }

    #[test]
    fn primitives_compare_by_value() {
        assert!(equals(&int(1), &int(1)).unwrap());
        assert!(!equals(&int(1), &int(2)).unwrap());
        assert!(equals(&true, &true).unwrap());
        assert!(
            equals(&f64::NAN, &f64::NAN).unwrap(),
            "the reflexive float order"
        );
        assert!(equals(&from_literal("a"), &from_literal("a")).unwrap());
        assert!(!equals(&from_literal("a"), &from_literal("b")).unwrap());
        assert!(equals(&bigint::from_i64(7), &bigint::from_i64(7)).unwrap());
        assert!(equals(&(), &()).unwrap());
    }

    #[test]
    fn null_equals_only_null() {
        assert!(equals(&None::<Int63>, &None).unwrap());
        assert!(!equals(&None, &Some(int(1))).unwrap());
        assert!(!equals(&Some(int(1)), &None).unwrap());
        assert!(equals(&Some(int(1)), &Some(int(1))).unwrap());
    }

    /// A class with a self-referential field, compared structurally.
    struct Node {
        n: Int63,
        next: Option<Shared<Node>>,
    }
    crate::mem::trace_fields!(Node, n, next);

    impl EqHandle for Node {
        fn fields_eq(a: &Shared<Self>, b: &Shared<Self>, cx: &mut EqCx) -> Result<bool, Thrown> {
            let (x, y) = (a.borrow().n, b.borrow().n);
            if !x.baml_eq(&y, cx)? {
                return Ok(false);
            }
            let (x, y) = (a.borrow().next.clone(), b.borrow().next.clone());
            x.baml_eq(&y, cx)
        }
    }

    #[test]
    fn objects_compare_structurally_by_identity_first_and_through_cycles() {
        let a = array::new(vec![int(1), int(2)]);
        assert!(equals(&a, &a.clone()).unwrap(), "identity");
        assert!(equals(&a, &array::new(vec![int(1), int(2)])).unwrap());
        assert!(!equals(&a, &array::new(vec![int(1)])).unwrap());
        let p = shared(Node {
            n: int(1),
            next: None,
        });
        let q = shared(Node {
            n: int(1),
            next: None,
        });
        assert!(equals(&p, &q).unwrap());
        p.borrow_mut().next = Some(p.clone());
        q.borrow_mut().next = Some(q.clone());
        assert!(
            equals(&p, &q).unwrap(),
            "a cycle met again is taken as equal"
        );
        q.borrow_mut().n = int(2);
        assert!(!equals(&p, &q).unwrap());
        // Break the cycles so the test frees what it built (Miri checks).
        p.borrow_mut().next = None;
        q.borrow_mut().next = None;
    }
}
