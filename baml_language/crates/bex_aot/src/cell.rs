//! The cell behind a local a closure captures.
//!
//! On the VM a captured local's slot holds an `Object::Cell` and every read
//! or write of the binding goes through it, so a closure and the function
//! that created it (and every other closure capturing the same binding)
//! share one value: a write through either is seen through both. Natively
//! the slot is a [`Cell`]: a counted pointer to the value, cloned into every
//! closure that captures it. A `fresh_cell` in the MIR replaces the pointer
//! ([`fresh`], or [`carry`] with the old cell's value), which is how a
//! binding declared inside a loop body gives each iteration's closures a
//! cell of their own.
//!
//! The cell starts empty: lowering creates it where the binding is created
//! and stores the initializer afterwards, and a closure may capture the
//! cell in between (a binding is not in scope in its own initializer, so it
//! cannot read it there). A read of an empty cell ([`get`]) is therefore a
//! program the checker should have rejected, reported as the `Unreachable`
//! panic rather than a Rust panic.
//!
//! A cell is a [`Shared`] object, so the cycle collector sees it as one: a
//! cell whose type can be on a cycle (a closure stored in a value its own
//! cell reaches) is a [`CcCell`], tracked like any other such handle, and
//! a cycle through it is freed by [`crate::cc`].

use crate::{
    Panic, Thrown,
    cc::Tracked,
    handle::{Kind, Shared, Untracked},
    mem::{Trace, Visitor},
};

/// A captured local's cell: a heap object of its own, behind a [`Shared`]
/// handle of kind `H`.
#[repr(transparent)]
pub struct Cell<T, H: Kind = Untracked>(Shared<Option<T>, H>);

/// A cell of a type that can be part of a cycle: a closure stored in a
/// value the closure's own cell reaches.
pub type CcCell<T> = Cell<T, Tracked>;

impl<T, H: Kind> Clone for Cell<T, H> {
    #[inline]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: std::fmt::Debug, H: Kind> std::fmt::Debug for Cell<T, H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}

/// The cell is one edge, as any handle is.
impl<T: Trace, H: Kind> Trace for Cell<T, H> {
    #[inline]
    fn trace(&self, visitor: &mut dyn Visitor) {
        self.0.trace(visitor);
    }
}

/// A new, empty cell: `fresh_cell(_n)`.
#[inline]
pub fn fresh<T: Trace, H: Kind>() -> Cell<T, H> {
    Cell(Shared::new(None))
}

/// A new cell holding `value`: a captured parameter on entry.
#[inline]
pub fn with<T: Trace, H: Kind>(value: T) -> Cell<T, H> {
    Cell(Shared::new(Some(value)))
}

/// A new cell holding the value of `cell`: `fresh_cell(_n, carry)`, the
/// C-style `for` header binding copied into the next iteration.
#[inline]
pub fn carry<T: Clone + Trace, H: Kind>(cell: &Cell<T, H>) -> Cell<T, H> {
    Cell(Shared::new(cell.0.borrow().clone()))
}

/// The value in the cell, copied out: `copy *_n`.
#[inline]
pub fn get<T: Clone, H: Kind>(cell: &Cell<T, H>) -> Result<T, Thrown> {
    match &*cell.0.borrow() {
        Some(value) => Ok(value.clone()),
        None => Err(unset()),
    }
}

/// `*_n = value`.
#[inline]
pub fn set<T, H: Kind>(cell: &Cell<T, H>, value: T) {
    *cell.0.borrow_mut() = Some(value);
}

#[cold]
fn unset() -> Thrown {
    Thrown::Panic(Panic::Unreachable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_are_shared_and_replaced_by_fresh_ones() {
        let a: Cell<i64> = fresh();
        assert!(matches!(get(&a), Err(Thrown::Panic(Panic::Unreachable))));
        set(&a, 1);
        let b = a.clone();
        set(&b, 2);
        assert_eq!(get(&a).unwrap(), 2, "one value behind both handles");
        let c = carry(&a);
        set(&c, 3);
        assert_eq!(get(&a).unwrap(), 2, "a carried cell is a new cell");
        assert_eq!(get(&c).unwrap(), 3);
        assert_eq!(get(&with::<_, Untracked>(7)).unwrap(), 7);
        assert_eq!(format!("{a:?}"), "Some(2)");
    }
}
