//! bridge_python - PyO3 Python bindings for BAML using bex_engine.
//!
//! This crate provides the same Python API as `language_client_python`
//! but powered by `bex_engine` (via `bridge_cffi`) instead of `baml-runtime`.

mod abort_controller;
mod errors;
mod media;
mod py_handle;
pub mod runtime;
pub mod types;

use pyo3::{
    Bound,
    prelude::{PyModule, PyResult, pyfunction, pymodule},
    types::PyModuleMethods,
    wrap_pyfunction,
};
use pyo3_stub_gen::{define_stub_info_gatherer, derive::gen_stub_pyfunction};

#[gen_stub_pyfunction]
#[pyfunction]
fn get_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Flush all buffered trace events to the JSONL file (if BAML_TRACE_FILE is set).
#[gen_stub_pyfunction]
#[pyfunction]
fn flush_events() {
    bridge_cffi::flush_event_sink();
}

#[pymodule]
fn baml_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<abort_controller::AbortController>()?;
    m.add_class::<py_handle::BamlPyHandle>()?;
    m.add_wrapped(wrap_pyfunction!(py_handle::take_pyhandle_from_table))?;
    m.add_wrapped(wrap_pyfunction!(py_handle::put_pyhandle_into_table))?;
    m.add_wrapped(wrap_pyfunction!(py_handle::_seed_function_ref_handle))?;
    m.add_wrapped(wrap_pyfunction!(py_handle::_seed_generic_media_handle))?;
    m.add_class::<runtime::BamlRuntime>()?;
    media::register(m)?;
    m.add_class::<types::FunctionResult>()?;
    m.add_class::<types::HostSpanManager>()?;
    m.add_class::<types::collector::Collector>()?;
    m.add_class::<types::collector::FunctionLog>()?;
    m.add_class::<types::collector::Timing>()?;
    m.add_class::<types::collector::Usage>()?;
    m.add_class::<types::collector::LLMCall>()?;
    m.add_wrapped(wrap_pyfunction!(get_version))?;
    m.add_wrapped(wrap_pyfunction!(flush_events))?;
    m.add_wrapped(wrap_pyfunction!(runtime::get_runtime))?;
    errors::register_errors(m)?;
    Ok(())
}

define_stub_info_gatherer!(stub_info);
