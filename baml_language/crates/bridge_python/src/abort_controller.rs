//! Python `AbortController` class for cancelling in-flight BAML function calls.

use bex_project::CancellationToken;
use pyo3::{prelude::pymethods, pyclass};

/// An abort controller for cancelling BAML function calls.
///
/// Usage from Python:
/// ```python
/// controller = AbortController()
/// # Pass to call_function / call_function_sync:
/// result = await call_function(rt, "MyFunc", args, abort_controller=controller)
/// # Cancel from another task:
/// controller.abort()
/// ```
#[pyclass]
pub struct AbortController {
    token: CancellationToken,
}

#[pymethods]
impl AbortController {
    #[new]
    fn new() -> Self {
        Self {
            token: CancellationToken::new(),
        }
    }

    /// Cancel the associated function call.
    ///
    /// If the function is still running, it will be interrupted at the next
    /// cancellation check point (before HTTP calls, between retries, etc.).
    /// Calling `abort()` multiple times is harmless.
    fn abort(&self) {
        self.token.cancel();
    }

    /// Whether `abort()` has been called.
    #[getter]
    fn aborted(&self) -> bool {
        self.token.is_cancelled()
    }
}

impl AbortController {
    /// Get a clone of the underlying `CancellationToken`.
    pub(crate) fn token(&self) -> CancellationToken {
        self.token.clone()
    }
}
