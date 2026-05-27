//! Host-boundary (pre-call) error → Python `BamlError` mapping.
//!
//! The structured *call-result* path raises `BamlError` / `BamlPanic` from
//! Python in `decode_call_result` — the engine hands back a
//! `BamlOutboundResult` envelope (31f-phase5). These helpers cover only the
//! pre-call host-boundary failures that never enter the VM (runtime not
//! initialized, a malformed call-args proto, etc.); they raise the same
//! pure-Python `baml_core.BamlError`, with the failure text as its `.value`.
//!
//! The old hardcoded pyo3 exceptions (`BamlError` / `BamlInvalidArgumentError`
//! / `BamlClientError` / `BamlCancelledError`) are gone: the typed thrown
//! value now rides inside the envelope instead of being stringified into a
//! fixed exception class.

use pyo3::prelude::*;

/// Raise the pure-Python `baml_core.BamlError` carrying `message` as `.value`.
///
/// Falls back to whatever import/construction error pyo3 produces if
/// `baml_core.BamlError` can't be reached (should never happen once the
/// package is importable).
pub fn py_baml_error(message: impl Into<String>) -> PyErr {
    let message = message.into();
    Python::attach(|py| {
        let build = || -> pyo3::PyResult<PyErr> {
            let cls = py.import("baml_core")?.getattr("BamlError")?;
            let inst = cls.call1((message.as_str(),))?;
            Ok(PyErr::from_value(inst))
        };
        build().unwrap_or_else(|e| e)
    })
}

/// Map a `bridge_cffi::BridgeError` (pre-call host-boundary failure) to the
/// Python `BamlError` wrapper.
pub fn bridge_error_to_py(err: bridge_cffi::error::BridgeError) -> PyErr {
    py_baml_error(err.to_string())
}
