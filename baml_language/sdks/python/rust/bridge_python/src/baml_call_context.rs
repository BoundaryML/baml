//! Python `BamlCallContext` class for cancelling in-flight BAML function calls.

use std::sync::Mutex;

use pyo3::{prelude::pymethods, pyclass};
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// A call context for cancelling BAML function calls.
///
/// Usage from Python:
/// ```python
/// ctx = BamlCallContext()
/// # Pass to call_function / call_function_sync:
/// result = await call_function(rt, "MyFunc", args, _ctx=ctx)
/// # Cancel from another task:
/// ctx.abort()
/// ```
#[gen_stub_pyclass]
#[pyclass]
pub struct BamlCallContext {
    aborted: Mutex<bool>,
    active_call_ids: Mutex<Vec<u64>>,
}

#[gen_stub_pymethods]
#[pymethods]
impl BamlCallContext {
    #[new]
    fn new() -> Self {
        Self {
            aborted: Mutex::new(false),
            active_call_ids: Mutex::new(Vec::new()),
        }
    }

    /// Cancel the associated function call.
    ///
    /// If the function is still running, it will be interrupted at the next
    /// cancellation check point (before HTTP calls, between retries, etc.).
    /// Calling `abort()` multiple times is harmless.
    fn abort(&self) {
        if let Ok(mut aborted) = self.aborted.lock() {
            *aborted = true;
        }
        let call_ids = self
            .active_call_ids
            .lock()
            .map(|ids| ids.clone())
            .unwrap_or_default();
        for call_id in call_ids {
            bridge_cffi::cancel_function_call_by_id(call_id);
        }
    }

    /// Whether `abort()` has been called.
    #[getter]
    fn aborted(&self) -> bool {
        self.aborted.lock().map(|aborted| *aborted).unwrap_or(true)
    }

    /// Bind this controller to an in-flight CFFI call id for the duration of
    /// one host call. Private runtime hook used by `baml_bridge`.
    fn _attach_call_id(&self, call_id: u64) {
        if let Ok(mut ids) = self.active_call_ids.lock()
            && !ids.contains(&call_id)
        {
            ids.push(call_id);
        }
        if self.aborted() {
            bridge_cffi::cancel_function_call_by_id(call_id);
        }
    }

    /// Remove a call-id binding installed by `_attach_call_id`.
    fn _detach_call_id(&self, call_id: u64) {
        if let Ok(mut ids) = self.active_call_ids.lock() {
            ids.retain(|id| *id != call_id);
        }
    }
}
