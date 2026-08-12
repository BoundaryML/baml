//! Pre-call host-boundary error helpers for the *handle-returning* sites.
//!
//! After 32c the structured call-result path raises `BamlError` / `BamlPanic`
//! from Python in `decode_call_result` (the engine hands back a
//! `BamlOutboundResult` envelope), and the byte-returning pre-call sites ride
//! that same envelope via `bridge_cffi::error_to_outbound`.
//!
//! What's left here is only the *handle-returning* pre-call sites
//! (`get_runtime` / `initialize_runtime`), which can't hand back envelope
//! bytes — they `raise`. By the 32c decision these SDK-internal *setup*
//! failures (e.g. runtime not initialized) are panic-shaped, so they surface
//! as a `baml.panics.SdkPanic` wrapped in `BamlPanic`, built by the pure-Python
//! `baml_bridge.make_sdk_panic`.

use pyo3::prelude::*;

/// Raise a `BamlPanic` wrapping a `baml.panics.SdkPanic { message }`, built via
/// `baml_bridge.make_sdk_panic`.
///
/// Falls back to whatever import/construction error pyo3 produces if
/// `baml_bridge.make_sdk_panic` can't be reached (should never happen once the
/// package is importable).
pub fn py_sdk_panic(message: impl Into<String>) -> PyErr {
    let message = message.into();
    Python::attach(|py| {
        let build = || -> PyResult<PyErr> {
            let func = py.import("baml_bridge")?.getattr("make_sdk_panic")?;
            let inst = func.call1((message.as_str(),))?;
            Ok(PyErr::from_value(inst))
        };
        build().unwrap_or_else(|e| e)
    })
}

/// Map a `bridge_cffi::BridgeError` from a handle-returning pre-call site to a
/// `BamlPanic(SdkPanic)`.
pub fn bridge_error_to_sdk_panic(err: bridge_cffi::error::BridgeError) -> PyErr {
    py_sdk_panic(err.to_string())
}

/// Preserve the complete generated-bytecode startup diagnostic as the
/// exception message without adding Python-side classification text.
pub fn bridge_error_to_initialization_error(err: bridge_cffi::error::BridgeError) -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(err.to_string())
}
