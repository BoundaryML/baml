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
//! A cycle through a cell (a closure stored in a cell it captures) is never
//! freed, like a cycle through class fields; see the crate README. The cell
//! is a [`Shared`] object, so a memory manager sees it as one.

use crate::{
    Panic, Thrown,
    handle::{Shared, shared},
    mem::{Trace, Visitor},
};

/// A captured local's cell: a heap object of its own, behind a [`Shared`]
/// handle.
#[repr(transparent)]
pub struct Cell<T>(Shared<Option<T>>);

impl<T> Clone for Cell<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Cell<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}

/// The cell is one edge, as any handle is.
impl<T: Trace> Trace for Cell<T> {
    #[inline]
    fn trace(&self, visitor: &mut dyn Visitor) {
        self.0.trace(visitor);
    }
}

/// A new, empty cell: `fresh_cell(_n)`.
#[inline]
pub fn fresh<T: Trace>() -> Cell<T> {
    Cell(shared(None))
}

/// A new cell holding `value`: a captured parameter on entry.
#[inline]
pub fn with<T: Trace>(value: T) -> Cell<T> {
    Cell(shared(Some(value)))
}

/// A new cell holding the value of `cell`: `fresh_cell(_n, carry)`, the
/// C-style `for` header binding copied into the next iteration.
#[inline]
pub fn carry<T: Clone + Trace>(cell: &Cell<T>) -> Cell<T> {
    Cell(shared(cell.0.borrow().clone()))
}

/// The value in the cell, copied out: `copy *_n`.
#[inline]
pub fn get<T: Clone>(cell: &Cell<T>) -> Result<T, Thrown> {
    match &*cell.0.borrow() {
        Some(value) => Ok(value.clone()),
        None => Err(unset()),
    }
}

/// `*_n = value`.
#[inline]
pub fn set<T>(cell: &Cell<T>, value: T) {
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
        assert_eq!(get(&with(7)).unwrap(), 7);
        assert_eq!(format!("{a:?}"), "Some(2)");
    }
}
