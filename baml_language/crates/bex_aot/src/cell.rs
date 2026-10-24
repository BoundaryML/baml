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
//! freed, like a cycle through class fields; see the crate README.

use std::{cell::RefCell, rc::Rc};

use crate::{Panic, Thrown};

/// A captured local's cell: shared, mutable, counted.
#[repr(transparent)]
pub struct Cell<T>(Rc<RefCell<Option<T>>>);

impl<T> Clone for Cell<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Cell<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0.try_borrow() {
            Ok(value) => std::fmt::Debug::fmt(&*value, f),
            Err(_) => f.write_str("<mutably borrowed>"),
        }
    }
}

/// A new, empty cell: `fresh_cell(_n)`.
#[inline]
pub fn fresh<T>() -> Cell<T> {
    Cell(Rc::new(RefCell::new(None)))
}

/// A new cell holding `value`: a captured parameter on entry.
#[inline]
pub fn with<T>(value: T) -> Cell<T> {
    Cell(Rc::new(RefCell::new(Some(value))))
}

/// A new cell holding the value of `cell`: `fresh_cell(_n, carry)`, the
/// C-style `for` header binding copied into the next iteration.
#[inline]
pub fn carry<T: Clone>(cell: &Cell<T>) -> Cell<T> {
    Cell(Rc::new(RefCell::new(cell.0.borrow().clone())))
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
