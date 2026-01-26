//! Epoch protection token for safe ObjectIndex access.
//!
//! This module provides the `EpochGuard` type, which proves that code is
//! running in an epoch-protected context where GC cannot invalidate
//! ObjectIndex values.

use std::marker::PhantomData;

/// Token proving caller is in an active epoch.
///
/// Cannot be constructed outside of epoch-protected code paths.
/// Zero-sized - no runtime overhead.
///
/// # Safety Guarantee
///
/// Code holding an `EpochGuard` is guaranteed that:
/// - The current async task is registered as "active" in an epoch
/// - GC will wait for this task to park or complete before running
/// - ObjectIndex values resolved while holding the guard remain valid
///   until the guard is dropped (and the task parks or completes)
///
/// # Usage
///
/// ```ignore
/// // Only engine can create guards (inside call_function)
/// let guard = unsafe { EpochGuard::new() };
///
/// // Pass guard to methods that need epoch protection
/// let idx = handle.object_index(&guard)?;
/// ```
pub struct EpochGuard<'a> {
    _marker: PhantomData<&'a ()>,
}

impl<'a> EpochGuard<'a> {
    /// Create a new epoch guard.
    ///
    /// # Safety
    ///
    /// This must only be called after registering with an epoch
    /// (incrementing `epoch_states[slot].active`).
    /// Only `BexEngine` should call this.
    #[doc(hidden)]
    pub unsafe fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}
