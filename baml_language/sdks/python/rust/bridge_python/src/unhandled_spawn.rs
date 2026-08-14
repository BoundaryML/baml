use std::sync::OnceLock;

use pyo3::{
    Py, PyAny, Python,
    prelude::{PyResult, pyfunction},
};
use pyo3_stub_gen::derive::gen_stub_pyfunction;

static CALLBACK: OnceLock<Py<PyAny>> = OnceLock::new();

#[gen_stub_pyfunction]
#[pyfunction]
pub fn register_unhandled_spawn_error_callback(callback: Py<PyAny>) {
    if CALLBACK.set(callback).is_ok() {
        bridge_cffi::register_unhandled_spawn_error_callback(deliver);
    }
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn shutdown_runtime(py: Python<'_>) -> PyResult<()> {
    py.detach(|| bridge_cffi::get_tokio_runtime()?.block_on(bridge_cffi::shutdown_runtime()))
        .map_err(crate::errors::bridge_error_to_sdk_panic)
}

extern "C" fn deliver(content: *const i8, length: usize, cancelled: i32) {
    let Some(callback) = CALLBACK.get() else {
        return;
    };
    let bytes = if content.is_null() || length == 0 {
        Vec::<u8>::new()
    } else {
        // SAFETY: bridge_cffi keeps the borrowed callback buffer valid until return.
        unsafe { std::slice::from_raw_parts(content.cast::<u8>(), length) }.to_vec()
    };

    Python::attach(|py| {
        if let Err(error) = callback.call1(py, (bytes, cancelled != 0)) {
            error.write_unraisable(py, Some(callback.bind(py)));
        }
    });
}
