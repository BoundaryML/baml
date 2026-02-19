//! bridge_python - PyO3 Python bindings for BAML using bex_engine.
//!
//! This crate provides the same Python API as `language_client_python`
//! but powered by `bex_engine` (via `bridge_cffi`) instead of `baml-runtime`.

mod abort_controller;
mod errors;
mod runtime;
mod types;

use pyo3::{
    Bound,
    prelude::{PyModule, PyResult, pyfunction, pymodule},
    types::PyModuleMethods,
    wrap_pyfunction,
};

#[pyfunction]
fn get_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Flush all buffered trace events to the JSONL file (if BAML_TRACE_FILE is set).
///
/// Delegates to `bex_events::event_store::flush()`.
#[pyfunction]
fn flush_events() {
    bex_events::event_store::flush();
}

#[pymodule]
fn baml_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<abort_controller::AbortController>()?;
    m.add_class::<runtime::BamlRuntime>()?;
    m.add_class::<types::FunctionResult>()?;
    m.add_class::<types::HostSpanManager>()?;
    m.add_class::<types::collector::Collector>()?;
    m.add_class::<types::collector::FunctionLog>()?;
    m.add_class::<types::collector::Timing>()?;
    m.add_class::<types::collector::Usage>()?;
    m.add_class::<types::collector::LLMCall>()?;
    m.add_wrapped(wrap_pyfunction!(get_version))?;
    m.add_wrapped(wrap_pyfunction!(flush_events))?;
    errors::register_errors(m)?;
    Ok(())
}
