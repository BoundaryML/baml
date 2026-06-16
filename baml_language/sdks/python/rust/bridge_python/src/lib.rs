//! bridge_python - PyO3 Python bindings for BAML using bex_engine.
//!
//! This crate provides the same Python API as `language_client_python`
//! but powered by `bex_engine` (via `bridge_cffi`) instead of `baml-runtime`.

mod baml_call_context;
mod errors;
pub mod host_value;
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
    baml_version::CANONICAL_VERSION
}

/// No-op: tracing has been removed. Kept as a live symbol for ABI stability
/// (SDK `atexit` + `__all__` reference it).
#[gen_stub_pyfunction]
#[pyfunction]
fn flush_events() {}

#[gen_stub_pyfunction]
#[pyfunction]
fn new_function_call() -> u64 {
    bridge_cffi::new_function_call_id()
}

#[gen_stub_pyfunction]
#[pyfunction]
fn cancel_function_call(call_id: u64) -> bool {
    bridge_cffi::cancel_function_call_by_id(call_id)
}

#[pymodule]
fn baml_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<baml_call_context::BamlCallContext>()?;
    m.add_class::<py_handle::BamlPyHandle>()?;
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
    m.add_wrapped(wrap_pyfunction!(new_function_call))?;
    m.add_wrapped(wrap_pyfunction!(cancel_function_call))?;
    m.add_wrapped(wrap_pyfunction!(runtime::get_runtime))?;
    m.add_wrapped(wrap_pyfunction!(host_value::register_host_callable))?;
    m.add_wrapped(wrap_pyfunction!(host_value::release_host_callable))?;
    m.add_wrapped(wrap_pyfunction!(host_value::lookup_host_value))?;

    // Wire the bridge_cffi C entry points to this bridge's per-process
    // Python host-value registry. First-call-wins inside bridge_cffi, so
    // repeated module loads (e.g. via importlib.reload) are harmless.
    bridge_cffi::register_host_dispatch_callback(host_value::host_dispatch_callback);
    bridge_cffi::register_host_release_callback(host_value::host_release_callback);

    Ok(())
}

define_stub_info_gatherer!(stub_info);
