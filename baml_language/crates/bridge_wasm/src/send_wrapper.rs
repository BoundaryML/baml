//! `SendFuture` and `SendWrapper` for WASM async compatibility.
//!
//! `SysOpFn` requires `Send + Sync` and `OpFuture` requires `Send`. But
//! `js_sys::Function` and `JsFuture` are `!Send`. On wasm32-unknown-unknown
//! (single-threaded), this is safe to bypass.
#![allow(unsafe_code)]

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// A future wrapper that unsafely implements Send on WASM.
///
/// # Safety
///
/// wasm32-unknown-unknown is single-threaded, so this is safe.
/// This wrapper should only be used on WASM targets.
pub(crate) struct SendFuture<F>(pub F);

// SAFETY: wasm32-unknown-unknown is single-threaded
unsafe impl<F> Send for SendFuture<F> {}

impl<F: Future> Future for SendFuture<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: We're not moving F out of the Pin, just projecting to it
        unsafe { self.map_unchecked_mut(|s| &mut s.0).poll(cx) }
    }
}

/// Wrapper for values that need Send+Sync on WASM.
///
/// # Safety
///
/// wasm32-unknown-unknown is single-threaded, so this is safe.
/// This wrapper should only be used on WASM targets.
#[derive(Clone)]
pub(crate) struct SendWrapper<T>(pub T);

// SAFETY: wasm32-unknown-unknown is single-threaded
unsafe impl<T> Send for SendWrapper<T> {}
unsafe impl<T> Sync for SendWrapper<T> {}

impl<T> SendWrapper<T> {
    /// Create a new `SendWrapper`.
    pub(crate) fn new(value: T) -> Self {
        Self(value)
    }

    /// Get a reference to the inner value.
    pub(crate) fn inner(&self) -> &T {
        self
    }

    /// Take the inner value (for use when the wrapper is stored in a container).
    pub(crate) fn into_inner(self) -> T {
        self.0
    }
}

impl<T> std::ops::Deref for SendWrapper<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
