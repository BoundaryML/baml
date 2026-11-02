//! The outcome of an operation on a resource: a value now, or a future that
//! yields one.

use std::{future::Future, pin::Pin};

use crate::errors::VmRustFnError;

/// What a resource operation returns: settled on the spot, or a future the
/// engine awaits with the heap permit released.
///
/// A resource only fails as the VM does, with a [`VmRustFnError`]; the sys-op
/// glue attaches the operation it belongs to.
pub enum OpOutput<T> {
    Ready(Result<T, VmRustFnError>),
    Async(Pin<Box<dyn Future<Output = Result<T, VmRustFnError>> + Send>>),
}

impl<T> OpOutput<T> {
    /// A settled success.
    pub fn ok(value: T) -> Self {
        Self::Ready(Ok(value))
    }

    /// A settled failure.
    pub fn err(error: impl Into<VmRustFnError>) -> Self {
        Self::Ready(Err(error.into()))
    }
}

impl<T: Send + 'static> OpOutput<T> {
    /// A future the engine awaits. `?` inside it converts every VM error
    /// shape into a [`VmRustFnError`].
    pub fn async_op(
        future: impl Future<Output = Result<T, VmRustFnError>> + Send + 'static,
    ) -> Self {
        Self::Async(Box::pin(future))
    }
}
