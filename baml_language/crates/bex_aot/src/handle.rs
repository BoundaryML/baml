//! Reference-semantics handles for heap values (`T[]`, class instances).
//!
//! A BAML array or class instance is shared by reference: two variables can
//! name one object and a write through either is seen through both. A handle
//! is an `Rc<RefCell<T>>`: the count frees the object when the last handle
//! goes, and the cell lets any handle mutate it. There is no lock, because a
//! native program runs on one OS thread, and no cycle collector, so a cycle
//! of handles is never freed.
//!
//! Borrows are always short. MIR evaluates operands into temporaries before
//! any assignment or call, so `arr[i] = f(arr)` never holds a borrow across
//! the call, and a `borrow_mut` for a store sees no live `borrow`.

use std::{cell::RefCell, fmt, ops::Deref, rc::Rc};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A shared, mutable heap value with identity semantics. Dereferences to the
/// [`RefCell`], so `handle.borrow()` / `handle.borrow_mut()` reach the value.
/// Equality of two handles is [`ptr_eq`], never structural.
#[repr(transparent)]
pub struct Shared<T>(Rc<RefCell<T>>);

/// Allocate a fresh handle owning `value`.
#[inline]
pub fn shared<T>(value: T) -> Shared<T> {
    Shared(Rc::new(RefCell::new(value)))
}

/// Whether two handles name the same object: BAML's `==` on classes and
/// arrays.
#[inline]
pub fn ptr_eq<T>(left: &Shared<T>, right: &Shared<T>) -> bool {
    Rc::ptr_eq(&left.0, &right.0)
}

impl<T> Clone for Shared<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T> Deref for Shared<T> {
    type Target = RefCell<T>;

    #[inline]
    fn deref(&self) -> &RefCell<T> {
        &self.0
    }
}

impl<T: fmt::Debug> fmt::Debug for Shared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.try_borrow() {
            Ok(value) => fmt::Debug::fmt(&*value, f),
            Err(_) => f.write_str("<mutably borrowed>"),
        }
    }
}

/// Through the pointee: a `T[]` is a JSON array, a class a JSON object.
impl<T: Serialize> Serialize for Shared<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.borrow().serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Shared<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(shared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_share_one_value() {
        let a = shared(vec![1, 2]);
        let b = a.clone();
        b.borrow_mut().push(3);
        assert_eq!(*a.borrow(), vec![1, 2, 3]);
        assert!(ptr_eq(&a, &b));
        let c = shared(vec![1, 2, 3]);
        assert!(!ptr_eq(&a, &c), "equal contents are not the same object");
        assert_eq!(format!("{a:?}"), "[1, 2, 3]");
        let _guard = a.borrow_mut();
        assert_eq!(format!("{a:?}"), "<mutably borrowed>");
    }
}
