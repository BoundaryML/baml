//! Reference-semantics handles for heap values (`T[]`, class instances).
//!
//! A BAML array or class instance is shared by reference: two variables can
//! name one object and a write through either is seen through both. A handle
//! is an `Rc` over a `RefCell`: the count frees the object when the last
//! handle goes, and the cell lets any handle mutate it. There is no lock,
//! because a native program runs on one OS thread, and no cycle collector,
//! so a cycle of handles is never freed.
//!
//! Borrows are always short. MIR evaluates operands into temporaries before
//! any assignment or call, so `arr[i] = f(arr)` never holds a borrow across
//! the call, and a `borrow_mut` for a store sees no live `borrow`.
//!
//! # The seam
//!
//! Generated code and the rest of this crate reach heap objects only
//! through the API below, so a memory manager can change what a handle is
//! here (and in [`crate::func`] for function values) without touching the
//! backend. Nothing outside this module and `func` names `Rc` or `RefCell`
//! for a heap object.
//!
//! | operation | API |
//! |---|---|
//! | allocate | [`shared`]`(value)` (`T: Trace`); [`crate::array::new`], [`crate::map::new`], [`crate::cell::fresh`] / [`with`](crate::cell::with) / [`carry`](crate::cell::carry) build on it |
//! | read | [`Shared::borrow`] → `Ref<'_, T>` |
//! | write | [`Shared::borrow_mut`] → `RefMut<'_, T>` |
//! | copy the handle | `Clone` |
//! | identity | [`ptr_eq`], [`Shared::id`] |
//! | function values | [`crate::func::alloc`] (a closure's environment), [`crate::func::item`] (a declared function), [`crate::func::Func::call`] |
//! | collector view | [`crate::mem::Trace`] for the contents, [`crate::mem::HeapRef`] for a handle |
//!
//! With the `mem-stats` feature every allocation and free is counted
//! (`mem::stats`); without it the handle is exactly
//! `Rc<RefCell<T>>`.

use std::{
    cell::{Ref, RefCell, RefMut},
    fmt,
    rc::Rc,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::mem::{HeapRef, Trace, Visitor};

/// A shared, mutable heap value with identity semantics. `borrow` /
/// `borrow_mut` reach the value; equality of two handles is [`ptr_eq`],
/// never structural.
#[repr(transparent)]
pub struct Shared<T>(Rc<Slot<RefCell<T>>>);

/// The allocation behind a handle: the value in its cell. Counted as freed
/// when dropped, under `mem-stats`.
#[repr(transparent)]
pub(crate) struct Slot<T: ?Sized>(pub(crate) T);

#[cfg(feature = "mem-stats")]
impl<T: ?Sized> Drop for Slot<T> {
    fn drop(&mut self) {
        crate::mem::on_free();
    }
}

/// Allocate a fresh handle owning `value`.
#[inline]
pub fn shared<T: Trace>(value: T) -> Shared<T> {
    let rc = Rc::new(Slot(RefCell::new(value)));
    #[cfg(feature = "mem-stats")]
    crate::mem::on_alloc(register(&rc));
    Shared(rc)
}

/// The registry's weak view of a new object.
#[cfg(feature = "mem-stats")]
pub(crate) fn register<T: crate::mem::Erased + 'static>(rc: &Rc<T>) -> crate::mem::Registered {
    let weak: std::rc::Weak<T> = Rc::downgrade(rc);
    weak
}

#[cfg(feature = "mem-stats")]
impl<T: Trace + ?Sized> crate::mem::Erased for Slot<T> {
    fn trace_contents(&self, visitor: &mut dyn Visitor) {
        self.0.trace(visitor);
    }
}

impl<T> Shared<T> {
    /// Read the value. Panics (a backend bug) if a `borrow_mut` is live.
    #[inline]
    pub fn borrow(&self) -> Ref<'_, T> {
        self.0.0.borrow()
    }

    /// Write the value. Panics (a backend bug) if any borrow is live.
    #[inline]
    pub fn borrow_mut(&self) -> RefMut<'_, T> {
        self.0.0.borrow_mut()
    }

    /// Read the value unless a `borrow_mut` is live.
    #[inline]
    pub fn try_borrow(&self) -> Option<Ref<'_, T>> {
        self.0.0.try_borrow().ok()
    }

    /// The object's identity, for matching a handle against a list of
    /// handles without keeping it alive.
    #[inline]
    pub fn id(&self) -> *const () {
        Rc::as_ptr(&self.0).cast::<()>()
    }
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

/// A handle is one edge: it reports itself, not its contents.
impl<T: Trace> Trace for Shared<T> {
    #[inline]
    fn trace(&self, visitor: &mut dyn Visitor) {
        visitor.visit(self);
    }
}

impl<T: Trace> HeapRef for Shared<T> {
    #[inline]
    fn addr(&self) -> *const () {
        self.id()
    }

    fn trace_target(&self, visitor: &mut dyn Visitor) {
        self.borrow().trace(visitor);
    }

    fn strong_count(&self) -> usize {
        Rc::strong_count(&self.0)
    }
}

impl<T: fmt::Debug> fmt::Debug for Shared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.try_borrow() {
            Some(value) => fmt::Debug::fmt(&*value, f),
            None => f.write_str("<mutably borrowed>"),
        }
    }
}

/// Through the pointee: a `T[]` is a JSON array, a class a JSON object.
impl<T: Serialize> Serialize for Shared<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.borrow().serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de> + Trace> Deserialize<'de> for Shared<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(shared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Int63;

    fn int(v: i64) -> Int63 {
        Int63::new(v).unwrap()
    }

    #[test]
    fn handles_share_one_value() {
        let a = shared(vec![1_i64, 2]);
        let b = a.clone();
        b.borrow_mut().push(3);
        assert_eq!(*a.borrow(), vec![1, 2, 3]);
        assert!(ptr_eq(&a, &b));
        let c = shared(vec![1_i64, 2, 3]);
        assert!(!ptr_eq(&a, &c), "equal contents are not the same object");
        assert_eq!(format!("{a:?}"), "[1, 2, 3]");
        let _guard = a.borrow_mut();
        assert_eq!(format!("{a:?}"), "<mutably borrowed>");
    }

    #[test]
    fn a_handle_traces_as_one_edge_and_its_target_as_its_contents() {
        let leaf = shared(vec![int(1)]);
        let outer = shared(vec![leaf.clone(), leaf.clone(), shared(Vec::new())]);
        let mut seen = Vec::new();
        outer.trace(&mut |object: &dyn HeapRef| seen.push(object.addr()));
        assert_eq!(seen, vec![outer.id()], "the handle itself, not followed");
        seen.clear();
        outer.trace_target(&mut |object: &dyn HeapRef| seen.push(object.addr()));
        assert_eq!(seen.len(), 3);
        assert_eq!(seen[0], leaf.id());
        assert_eq!(seen[1], leaf.id());
        assert_eq!(HeapRef::strong_count(&leaf), 3);
    }
}
