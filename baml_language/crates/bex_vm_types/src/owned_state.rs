//! State a `@@rust_type` instance owns outright.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// State the VM owns outright: copying the instance copies it.
///
/// Natives reach it through a shared borrow of the instance, and fibers
/// started with `spawn` may share the instance, so writes go through a mutex
/// that serializes them.
#[derive(Debug, Default)]
pub struct OwnedState<T>(Mutex<T>);

impl<T> OwnedState<T> {
    pub fn new(state: T) -> Self {
        Self(Mutex::new(state))
    }

    /// The state, held until the guard drops.
    ///
    /// A poisoned lock is taken anyway. The only writes are short calls into
    /// the state's own methods, which do not panic, so poisoning means the
    /// process is already unwinding; recovering keeps a panicking fiber from
    /// turning every other holder of the same instance into a second panic.
    pub fn lock(&self) -> MutexGuard<'_, T> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A copy holds the state as it is now, independent of the original from
/// then on.
impl<T: Clone> Clone for OwnedState<T> {
    fn clone(&self) -> Self {
        Self::new(self.lock().clone())
    }
}
