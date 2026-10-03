//! bridge_python - PyO3 Python bindings for BAML using bex_engine.
//!
//! This crate provides the same Python API as `language_client_python`
//! but powered by `bex_engine` (via `bridge_cffi`) instead of `baml-runtime`.

mod callback_dispatch;
mod errors;
mod host_capture;
pub mod host_value;
mod invocation;
mod media;
mod py_handle;
pub mod runtime;
pub mod types;
mod unhandled_spawn;

use pyo3::{
    Bound,
    exceptions::PyImportError,
    prelude::{PyModule, PyResult, pyfunction, pymodule},
    types::PyModuleMethods,
    wrap_pyfunction,
};
use pyo3_stub_gen::{define_stub_info_gatherer, derive::gen_stub_pyfunction};

const BRIDGE_RUNTIME_NAME: &str = "baml-bridge";

#[gen_stub_pyfunction]
#[pyfunction]
fn get_version() -> &'static str {
    get_toolchain_version()
}

#[gen_stub_pyfunction]
#[pyfunction]
fn get_toolchain_version() -> &'static str {
    baml_version::CANONICAL_VERSION
}

#[gen_stub_pyfunction]
#[pyfunction]
fn get_bridge_runtime_version() -> &'static str {
    baml_version::PYPI_VERSION
}

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

#[gen_stub_pyfunction]
#[pyfunction]
fn release_function_call(call_id: u64) -> bool {
    bridge_cffi::release_function_call_by_id(call_id)
}

#[gen_stub_pyfunction]
#[pyfunction]
fn invocation_clock_ns(call_id: u64) -> PyResult<u64> {
    bridge_cffi::invocation_clock_by_id(call_id).map_err(crate::errors::bridge_error_to_sdk_panic)
}

#[pymodule]
fn baml_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    bridge_cffi::register_bridge(bridge_cffi::BridgeInfo {
        language: bridge_cffi::BridgeLanguage::Python,
        bridge_runtime_name: BRIDGE_RUNTIME_NAME.to_string(),
        bridge_runtime_version: get_bridge_runtime_version().to_string(),
        toolchain_version: get_toolchain_version().to_string(),
    })
    .map_err(PyImportError::new_err)?;

    m.add_class::<py_handle::BamlPyHandle>()?;
    m.add_wrapped(wrap_pyfunction!(invocation::_trace_selection))?;
    m.add_wrapped(wrap_pyfunction!(invocation::_invocation_context))?;
    m.add_class::<invocation::_HostExecution>()?;
    m.add_wrapped(wrap_pyfunction!(invocation::_validate_host_options))?;
    m.add_wrapped(wrap_pyfunction!(invocation::_begin_host_invocation))?;
    m.add_wrapped(wrap_pyfunction!(invocation::_define_host_marker))?;
    m.add_wrapped(wrap_pyfunction!(py_handle::_seed_function_ref_handle))?;
    m.add_wrapped(wrap_pyfunction!(py_handle::_seed_generic_media_handle))?;
    m.add_wrapped(wrap_pyfunction!(py_handle::_seed_heap_handle))?;
    m.add_wrapped(wrap_pyfunction!(py_handle::_release_wire_handle))?;
    m.add_wrapped(wrap_pyfunction!(py_handle::_live_handle_count))?;
    m.add_wrapped(wrap_pyfunction!(py_handle::_handle_refcount))?;
    m.add_class::<runtime::BamlRuntime>()?;
    media::register(m)?;
    m.add_class::<types::FunctionResult>()?;
    m.add_wrapped(wrap_pyfunction!(get_version))?;
    m.add_wrapped(wrap_pyfunction!(get_toolchain_version))?;
    m.add_wrapped(wrap_pyfunction!(get_bridge_runtime_version))?;
    m.add_wrapped(wrap_pyfunction!(new_function_call))?;
    m.add_wrapped(wrap_pyfunction!(cancel_function_call))?;
    m.add_wrapped(wrap_pyfunction!(release_function_call))?;
    m.add_wrapped(wrap_pyfunction!(invocation_clock_ns))?;
    m.add_wrapped(wrap_pyfunction!(
        unhandled_spawn::register_unhandled_spawn_error_callback
    ))?;
    m.add_wrapped(wrap_pyfunction!(unhandled_spawn::shutdown_runtime))?;
    m.add_wrapped(wrap_pyfunction!(runtime::get_or_init_runtime))?;
    m.add_wrapped(wrap_pyfunction!(host_value::register_host_callable))?;
    m.add_wrapped(wrap_pyfunction!(host_value::release_host_callable))?;
    m.add_wrapped(wrap_pyfunction!(host_value::lookup_host_value))?;
    m.add_wrapped(wrap_pyfunction!(host_value::_invoke_host_callable))?;
    m.add_wrapped(wrap_pyfunction!(host_value::_complete_host_call_success))?;
    m.add_wrapped(wrap_pyfunction!(host_value::_complete_host_call_error))?;
    m.add_wrapped(wrap_pyfunction!(host_value::_register_host_call_execution))?;
    m.add_wrapped(wrap_pyfunction!(host_value::_discard_host_call_args))?;

    // Wire the bridge_cffi C entry points to this bridge's per-process
    // Python host-value registry. First-call-wins inside bridge_cffi, so
    // repeated module loads (e.g. via importlib.reload) are harmless.
    bridge_cffi::register_host_dispatch_v2(host_value::host_dispatch_callback);
    bridge_cffi::register_host_release_callback(host_value::host_release_callback);

    Ok(())
}

define_stub_info_gatherer!(stub_info);
