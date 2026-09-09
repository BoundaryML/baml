use std::sync::OnceLock;

use crate::encoded_result::BamlEncodedResult;
use bridge_cffi::OwnedUnhandledSpawnError;
use bridge_ctypes::{HANDLE_TABLE, TransferSession};

use pyo3::{
    Py, PyAny, Python,
    prelude::{PyAnyMethods, PyResult, pyfunction},
};
use pyo3_stub_gen::derive::gen_stub_pyfunction;

static CALLBACK: OnceLock<Py<PyAny>> = OnceLock::new();

#[gen_stub_pyfunction]
#[pyfunction]
pub fn register_unhandled_spawn_error_callback(callback: Py<PyAny>) {
    if CALLBACK.set(callback).is_ok() {
        bridge_cffi::register_owned_unhandled_spawn_error_callback(deliver);
    }
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn shutdown_runtime(py: Python<'_>) -> PyResult<()> {
    py.detach(|| bridge_cffi::get_tokio_runtime()?.block_on(bridge_cffi::shutdown_runtime()))
        .map_err(crate::errors::bridge_error_to_sdk_panic)
}

fn deliver(error: OwnedUnhandledSpawnError) {
    let Some(callback) = CALLBACK.get() else {
        return;
    };
    // The aggregate owns its exported values independently of a replacement
    // global runtime. Decoding adopts before reporting the error to user code.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Python::attach(|py| -> PyResult<()> {
            let result = Py::new(
                py,
                BamlEncodedResult::with_invocation_session(
                    error.content,
                    TransferSession::new(&HANDLE_TABLE),
                    error.runtime,
                    error.transfers,
                )?,
            )?;
            if let Err(error) = callback.call1(py, (result.clone_ref(py), error.cancelled)) {
                // A broken handler may have retained the native envelope.
                // Discard provisional ownership; adopted refs are unaffected.
                result.bind(py).call_method0("_discard")?;
                error.write_unraisable(py, Some(callback.bind(py)));
            }
            Ok(())
        })
    }));
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => log::error!("Python unhandled-spawn delivery failed: {error}"),
        Err(_) => log::error!("Python unhandled-spawn delivery panicked"),
    }
}
