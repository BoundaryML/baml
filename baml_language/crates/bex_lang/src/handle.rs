//! Reference-semantics handles for heap values (`T[]`, class instances).
//!
//! Native programs run on one OS thread in v2 (there is no `spawn` yet), so a
//! handle is a plain `Rc<RefCell<T>>` with no locking: an array access costs
//! the same as on a Rust `Vec`. When `spawn` arrives the executor is a tokio
//! `LocalSet`, so handles stay `Rc`. Should BAML's `spawn` ever be required to
//! run CPU-parallel inside one process, this type's representation changes and
//! every heap operation takes a lock; that is a deliberate, measured decision
//! for a later version, not this one.
//!
//! Borrows are always short: MIR evaluates operands into temps before any
//! assignment or call, so `arr[i] = f(arr)` never holds a `RefCell` borrow
//! across the call.

use std::{cell::RefCell, fmt, ops::Deref, rc::Rc};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A shared, mutable heap value with identity semantics. Dereferences to the
/// [`RefCell`], so `handle.borrow()` / `handle.borrow_mut()` reach the value.
///
/// `PartialEq` compares the pointees structurally (after a pointer fast path);
/// BAML's reference equality for classes is [`ptr_eq`].
#[repr(transparent)]
pub struct Shared<T>(Rc<RefCell<T>>);

/// Allocate a fresh handle owning `value`.
#[inline]
pub fn shared<T>(value: T) -> Shared<T> {
    Shared(Rc::new(RefCell::new(value)))
}

/// Whether two handles are the same object (BAML's reference equality for
/// classes without an `Equals` implementation).
#[inline]
pub fn ptr_eq<T>(left: &Shared<T>, right: &Shared<T>) -> bool {
    Rc::ptr_eq(&left.0, &right.0)
}

impl<T> Shared<T> {
    /// Allocate a fresh handle owning `value`.
    #[inline]
    pub fn new(value: T) -> Self {
        shared(value)
    }

    /// Whether `self` and `other` are the same object.
    #[inline]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        ptr_eq(self, other)
    }
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

impl<T: PartialEq> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        ptr_eq(self, other) || *self.0.borrow() == *other.0.borrow()
    }
}

impl<T: Eq> Eq for Shared<T> {}

impl<T: Default> Default for Shared<T> {
    fn default() -> Self {
        shared(T::default())
    }
}

impl<T> From<T> for Shared<T> {
    fn from(value: T) -> Self {
        shared(value)
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
        assert!(!a.ptr_eq(&c));
        assert_eq!(a, c, "structural equality");
        assert_ne!(a, shared(vec![]));
        assert_eq!(format!("{a:?}"), "[1, 2, 3]");
        let _guard = a.borrow_mut();
        assert_eq!(format!("{a:?}"), "<mutably borrowed>");
    }
}
