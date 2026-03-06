//! Runtime cycle detection for recursive type operations.

use std::cell::{Cell, RefCell};

use indexmap::IndexSet;

const MAX_DEPTH: u32 = 64;

/// Runtime cycle guard for recursive type operations.
///
/// Used during subtype-checking to detect recursive types like
/// `type JSON = string | int | JSON[]` without infinite looping.
pub struct CycleDetector<T: Eq + std::hash::Hash + Clone> {
    seen: RefCell<IndexSet<T>>,
    depth: Cell<u32>,
}

impl<T: Eq + std::hash::Hash + Clone> CycleDetector<T> {
    pub fn new() -> Self {
        Self {
            seen: RefCell::new(IndexSet::new()),
            depth: Cell::new(0),
        }
    }

    /// Visit a key, calling `f` if not already seen.
    ///
    /// Returns `default` if the key has been seen (cycle detected) or
    /// if the maximum depth has been reached.
    pub fn visit(&self, key: T, default: bool, f: impl FnOnce() -> bool) -> bool {
        if !self.seen.borrow_mut().insert(key.clone()) {
            return default;
        }
        if self.depth.get() >= MAX_DEPTH {
            self.seen.borrow_mut().swap_remove(&key);
            return default;
        }
        self.depth.set(self.depth.get() + 1);
        let result = f();
        self.depth.set(self.depth.get() - 1);
        self.seen.borrow_mut().swap_remove(&key);
        result
    }
}

impl<T: Eq + std::hash::Hash + Clone> Default for CycleDetector<T> {
    fn default() -> Self {
        Self::new()
    }
}
