//! Reference-semantics handles for heap values (`T[]`, class instances).
//!
//! A BAML array or class instance is shared by reference: two variables can
//! name one object and a write through either is seen through both. A handle
//! is a counted pointer to a `RefCell`: the count frees the object when the
//! last handle goes, and the cell lets any handle mutate it. There is no
//! lock, because a native program runs on one OS thread. How the handle
//! counts is its [`Kind`]: an [`Untracked`] handle is a plain `Rc`, and a
//! [`Tracked`] one ([`Cc`]) is examined by the cycle collector in
//! [`crate::cc`], which frees cycles of tracked handles. The backend picks
//! the kind per type: tracked exactly when the type can be on a cycle.
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
//! | allocate | [`shared`]`(value)` / [`cc`]`(value)` (`T: Trace`), or [`Shared::new`] at an inferred kind; [`crate::array::new`] / [`new_cc`](crate::array::new_cc), [`crate::map::new`] / [`new_cc`](crate::map::new_cc), [`crate::cell::fresh`] / [`with`](crate::cell::with) / [`carry`](crate::cell::carry) build on it |
//! | read | [`Shared::borrow`] → `Ref<'_, T>` |
//! | write | [`Shared::borrow_mut`] → `RefMut<'_, T>` |
//! | copy the handle | `Clone` |
//! | identity | [`ptr_eq`], [`Shared::id`] |
//! | function values | [`crate::func::alloc`] (a closure's environment), [`crate::func::item`] (a declared function), [`crate::func::Func::call`] |
//! | collector view | [`crate::mem::Trace`] for the contents, [`crate::mem::HeapRef`] for a handle |
//!
//! With the `mem-stats` feature every allocation and free is counted
//! (`mem::stats`); without it an untracked handle is exactly
//! `Rc<RefCell<T>>`.

use std::{
    cell::{Ref, RefCell, RefMut},
    fmt,
    rc::Rc,
};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    cc::{Object, Tracked},
    func::Callable,
    mem::{HeapRef, Trace, Visitor},
};

/// How a handle counts: the representation behind [`Shared`],
/// [`crate::Map`], [`crate::cell::Cell`] and [`crate::func::Func`].
///
/// The backend picks one per type. A type no value can reach itself
/// through is [`Untracked`]: a plain `Rc`, freed when its count reaches
/// zero and never seen by the cycle collector. A type that can be part of
/// a cycle is [`Tracked`]: a counted box the collector in [`crate::cc`]
/// examines when a decrement leaves it alive. The members are hidden: a
/// kind is a marker type, chosen by name in generated code and inferred
/// everywhere else.
pub trait Kind: Sized + 'static {
    /// The owning, counted pointer to a heap object `T`.
    #[doc(hidden)]
    type Ptr<T: ?Sized>: Clone;

    /// Allocate `value` with a count of one.
    #[doc(hidden)]
    fn alloc<T: Trace>(value: T) -> Self::Ptr<T>;

    /// Allocate a function value's object and erase it to its call
    /// signature. `acyclic` says the object itself can never be on a cycle
    /// (it holds no tracked handle), whatever its signature's kind: a
    /// tracked kind then never buffers it.
    #[doc(hidden)]
    fn alloc_callable<A: 'static, R: 'static, C: Callable<A, R>>(
        object: C,
        acyclic: bool,
    ) -> Self::Ptr<dyn Callable<A, R>>;

    /// The object.
    #[doc(hidden)]
    fn get<T: ?Sized>(ptr: &Self::Ptr<T>) -> &T;

    /// The object's address: its identity.
    #[doc(hidden)]
    fn addr<T: ?Sized>(ptr: &Self::Ptr<T>) -> *const ();

    /// The number of handles to the object.
    #[doc(hidden)]
    fn strong_count<T: ?Sized>(ptr: &Self::Ptr<T>) -> usize;

    /// The collector's view of the object, for a tracked kind.
    #[doc(hidden)]
    fn object<T: ?Sized>(ptr: &Self::Ptr<T>) -> Option<Object>;
}

/// The plain reference-counted kind: `Rc`, for types that cannot be part
/// of a cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Untracked {}

impl Kind for Untracked {
    type Ptr<T: ?Sized> = Rc<Slot<T>>;

    #[inline]
    fn alloc<T: Trace>(value: T) -> Rc<Slot<T>> {
        let rc = Rc::new(Slot(value));
        #[cfg(feature = "mem-stats")]
        crate::mem::on_alloc(register(&rc));
        rc
    }

    #[inline]
    fn alloc_callable<A: 'static, R: 'static, C: Callable<A, R>>(
        object: C,
        _acyclic: bool,
    ) -> Rc<Slot<dyn Callable<A, R>>> {
        Self::alloc(object)
    }

    #[inline]
    fn get<T: ?Sized>(ptr: &Rc<Slot<T>>) -> &T {
        &ptr.0
    }

    #[inline]
    fn addr<T: ?Sized>(ptr: &Rc<Slot<T>>) -> *const () {
        Rc::as_ptr(ptr).cast::<()>()
    }

    #[inline]
    fn strong_count<T: ?Sized>(ptr: &Rc<Slot<T>>) -> usize {
        Rc::strong_count(ptr)
    }

    #[inline]
    fn object<T: ?Sized>(_ptr: &Rc<Slot<T>>) -> Option<Object> {
        None
    }
}

/// A shared, mutable heap value with identity semantics. `borrow` /
/// `borrow_mut` reach the value; equality of two handles is [`ptr_eq`],
/// never structural. `H` is how it counts ([`Kind`]): [`Untracked`] by
/// default, [`Tracked`] as [`Cc`].
#[repr(transparent)]
pub struct Shared<T, H: Kind = Untracked>(H::Ptr<RefCell<T>>);

/// A handle to an object of a type that can be part of a cycle: counted,
/// and examined by the cycle collector ([`crate::cc`]) when a decrement
/// leaves it alive.
pub type Cc<T> = Shared<T, Tracked>;

/// The allocation behind an untracked handle: the value in its cell.
/// Counted as freed when dropped, under `mem-stats`.
#[doc(hidden)]
#[repr(transparent)]
pub struct Slot<T: ?Sized>(pub(crate) T);

#[cfg(feature = "mem-stats")]
impl<T: ?Sized> Drop for Slot<T> {
    fn drop(&mut self) {
        crate::mem::on_free();
    }
}

/// Allocate a fresh untracked handle owning `value`.
#[inline]
pub fn shared<T: Trace>(value: T) -> Shared<T> {
    Shared::new(value)
}

/// Allocate a fresh tracked handle owning `value`.
#[inline]
pub fn cc<T: Trace>(value: T) -> Cc<T> {
    Shared::new(value)
}

/// The registry's weak view of a new object.
#[cfg(feature = "mem-stats")]
fn register<T: crate::mem::Erased + 'static>(rc: &Rc<T>) -> crate::mem::Registered {
    let weak: std::rc::Weak<T> = Rc::downgrade(rc);
    crate::mem::Registered::Rc(weak)
}

#[cfg(feature = "mem-stats")]
impl<T: Trace + ?Sized> crate::mem::Erased for Slot<T> {
    fn trace_contents(&self, visitor: &mut dyn Visitor) {
        self.0.trace(visitor);
    }
}

impl<T: Trace, H: Kind> Shared<T, H> {
    /// Allocate a fresh handle owning `value`, of the kind the context
    /// asks for.
    #[inline]
    pub fn new(value: T) -> Self {
        Self(H::alloc(RefCell::new(value)))
    }
}

impl<T, H: Kind> Shared<T, H> {
    /// Read the value. Panics (a backend bug) if a `borrow_mut` is live.
    #[inline]
    pub fn borrow(&self) -> Ref<'_, T> {
        H::get(&self.0).borrow()
    }

    /// Write the value. Panics (a backend bug) if any borrow is live.
    #[inline]
    pub fn borrow_mut(&self) -> RefMut<'_, T> {
        H::get(&self.0).borrow_mut()
    }

    /// Read the value unless a `borrow_mut` is live.
    #[inline]
    pub fn try_borrow(&self) -> Option<Ref<'_, T>> {
        H::get(&self.0).try_borrow().ok()
    }

    /// The object's identity, for matching a handle against a list of
    /// handles without keeping it alive.
    #[inline]
    pub fn id(&self) -> *const () {
        H::addr(&self.0)
    }
}

/// Whether two handles name the same object: BAML's `==` on classes and
/// arrays.
#[inline]
pub fn ptr_eq<T, H: Kind>(left: &Shared<T, H>, right: &Shared<T, H>) -> bool {
    left.id() == right.id()
}

impl<T, H: Kind> Clone for Shared<T, H> {
    #[inline]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// A handle is one edge: it reports itself, not its contents.
impl<T: Trace, H: Kind> Trace for Shared<T, H> {
    #[inline]
    fn trace(&self, visitor: &mut dyn Visitor) {
        match H::object(&self.0) {
            Some(object) => visitor.visit_tracked(object, self),
            None => visitor.visit(self),
        }
    }
}

impl<T: Trace, H: Kind> HeapRef for Shared<T, H> {
    #[inline]
    fn addr(&self) -> *const () {
        self.id()
    }

    fn trace_target(&self, visitor: &mut dyn Visitor) {
        self.borrow().trace(visitor);
    }

    fn strong_count(&self) -> usize {
        H::strong_count(&self.0)
    }

    #[inline]
    fn object(&self) -> Option<Object> {
        H::object(&self.0)
    }
}

impl<T: fmt::Debug, H: Kind> fmt::Debug for Shared<T, H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.try_borrow() {
            Some(value) => fmt::Debug::fmt(&*value, f),
            None => f.write_str("<mutably borrowed>"),
        }
    }
}

/// Through the pointee: a `T[]` is a JSON array, a class a JSON object.
impl<T: Serialize, H: Kind> Serialize for Shared<T, H> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.borrow().serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de> + Trace, H: Kind> Deserialize<'de> for Shared<T, H> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::new)
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
