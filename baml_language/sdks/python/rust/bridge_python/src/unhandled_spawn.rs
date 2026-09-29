use std::{sync::OnceLock, time::Duration};

use pyo3::{
    Py, PyAny, Python,
    exceptions::PyValueError,
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

/// Shut down the BAML runtime: wait for in-flight calls and spawned work,
/// report errors nothing observed, and release the runtime.
///
/// `timeout` (seconds) bounds the wait: once it passes, work still running is
/// cancelled and then abandoned. Without one the wait lasts as long as the
/// work does, as Python's own exit waits for non-daemon threads. Either way,
/// Ctrl+C ends it with `KeyboardInterrupt`.
#[gen_stub_pyfunction]
#[pyfunction]
#[pyo3(signature = (timeout=None))]
pub fn shutdown_runtime(py: Python<'_>, timeout: Option<f64>) -> PyResult<()> {
    /// How often the wait gives Python a chance to handle signals.
    const SIGNAL_POLL: Duration = Duration::from_millis(100);
    let grace = timeout
        .map(Duration::try_from_secs_f64)
        .transpose()
        .map_err(|_| {
            PyValueError::new_err("timeout must be a finite, non-negative number of seconds")
        })?;
    let runtime =
        bridge_cffi::get_tokio_runtime().map_err(crate::errors::bridge_error_to_sdk_panic)?;
    let mut shutdown = runtime.spawn(bridge_cffi::shutdown_runtime(grace));
    loop {
        // The wait holds no GIL; between slices Python runs its signal
        // handlers, so an interrupt raises here and the shutdown is left to
        // finish, or not, with the process.
        let slice = py.detach(|| {
            runtime.block_on(async { tokio::time::timeout(SIGNAL_POLL, &mut shutdown).await })
        });
        match slice {
            Ok(Ok(result)) => return result.map_err(crate::errors::bridge_error_to_sdk_panic),
            Ok(Err(join_error)) => {
                return Err(crate::errors::bridge_error_to_sdk_panic(
                    bridge_cffi::error::BridgeError::Internal(format!(
                        "BAML runtime shutdown failed: {join_error}"
                    )),
                ));
            }
            Err(_) => py.check_signals()?,
        }
    }
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
