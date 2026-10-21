//! BamlRuntime PyO3 class - wraps `Arc<dyn Bex>`.

use pyo3::{
    IntoPyObjectExt, Py, Python,
    prelude::{PyResult, pyfunction, pymethods},
    pyclass,
    types::PyAny,
};
use pyo3_stub_gen::{
    derive::{gen_methods_from_python, gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods},
    inventory::submit,
};

use crate::{
    callback_dispatch::{DispatchScope, SyncMessage},
    errors::{bridge_error_to_sdk_panic, py_sdk_panic},
};

/// The main BAML runtime. A zero-sized handle: the single source of truth for
/// the `Arc<dyn Bex>` singleton is `bridge_cffi`, fetched via
/// `bridge_cffi::get_runtime()` at each call site (31e-phase4), so this
/// no longer caches its own clone.
#[gen_stub_pyclass]
#[pyclass]
pub struct BamlRuntime;

#[gen_stub_pymethods]
#[pymethods]
impl BamlRuntime {
    /// Initialize the process-global runtime from in-memory BAML source files.
    ///
    /// Mirrors `bridge_cffi::initialize_runtime`: the same
    /// single-slot singleton is used, so a second call replaces the prior
    /// runtime.
    ///
    /// # Arguments
    /// * `root_path` - Root path for BAML files
    /// * `files` - Map of filename to file content
    #[staticmethod]
    fn initialize_runtime(
        root_path: String,
        files: std::collections::HashMap<String, String>,
    ) -> PyResult<Self> {
        // `initialize_runtime` stores the `Arc<dyn Bex>` in bridge_cffi's
        // singleton; we don't keep our own copy.
        match bridge_cffi::initialize_runtime(&root_path, files) {
            Ok(_bex) => Ok(BamlRuntime),
            // Handle-returning site: can't hand back envelope bytes, so an
            // SDK setup failure surfaces as BamlPanic(SdkPanic) (32c).
            Err(e) => Err(bridge_error_to_sdk_panic(e)),
        }
    }

    /// Initialize the process-global runtime from serialized BAML bytecode.
    ///
    /// Generated SDKs use this path so importing `baml_sdk` can skip parsing
    /// and compiling the inlined BAML source files.
    ///
    /// # Arguments
    /// * `bytecode` - borsh-encoded BAML bytecode program
    #[staticmethod]
    #[pyo3(signature = (bytecode, embedded_baml_toml=None))]
    fn initialize_runtime_from_blob(
        bytecode: Vec<u8>,
        embedded_baml_toml: Option<String>,
    ) -> PyResult<Self> {
        match bridge_cffi::initialize_runtime_from_blob(&bytecode, embedded_baml_toml.as_deref()) {
            Ok(_bex) => Ok(BamlRuntime),
            Err(e) => Err(crate::errors::bridge_error_to_initialization_error(e)),
        }
    }
}

// Manual stub declarations for methods with complex parameter types
// that pyo3-stub-gen cannot process (reference params, PyRef, etc.).
submit! {
    gen_methods_from_python! {
        r#"
        import typing

        class BamlRuntime:
            def call_function(self, args_proto: bytes, *, stream: bool = False) -> typing.Any:
                """Call a BAML function asynchronously."""

            def call_function_sync(self, args_proto: bytes, *, stream: bool = False) -> typing.Any:
                """Call a BAML function synchronously (blocking)."""
        "#
    }
}

#[pymethods]
impl BamlRuntime {
    /// Call a BAML function asynchronously.
    ///
    /// # Arguments
    /// * `args_proto` - Protobuf-encoded `CallFunctionArgs` including its target
    #[pyo3(signature = (args_proto, *, stream=false))]
    fn call_function<'py>(
        &self,
        py: Python<'py>,
        args_proto: Vec<u8>,
        stream: bool,
    ) -> PyResult<Py<PyAny>> {
        // Byte-returning site (32c): pre-call host-boundary failures don't
        // raise — they become a structured BamlOutboundResult envelope so the
        // future yields bytes that decode_call_result raises uniformly (same
        // BamlError(baml.errors.*) as an engine failure).
        // `decode_invocation_request` pins a handle target before we yield to the event
        // loop, so a Python-side release of that handle cannot race the call.
        let request = bridge_cffi::decode_invocation_request(&args_proto);

        let environment = match &request {
            Ok(call) => Some(DispatchScope::asynchronous(py, call.host_call_id())?),
            Err(_) => None,
        };

        // The whole Result -> BamlOutboundResult translation (incl. the
        // catch_unwind -> SdkPanic boundary) lives in bridge_cffi; we just
        // return the encoded envelope bytes for Python to decode + raise.
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let _environment = environment;
            let (bytes, retained) = execute_python_invocation(request, stream).await;
            Python::attach(|py| result_with_stream(py, bytes, retained, stream))
        })
        .map(pyo3::Bound::into)
    }

    /// Call a BAML function synchronously (blocking).
    ///
    /// # Arguments
    /// * `args_proto` - Protobuf-encoded `CallFunctionArgs` including its target
    #[pyo3(signature = (args_proto, *, stream=false))]
    fn call_function_sync(
        &self,
        py: Python<'_>,
        args_proto: Vec<u8>,
        stream: bool,
    ) -> PyResult<Py<PyAny>> {
        // Byte-returning site (32c): pre-call host-boundary failures
        // (uninitialized runtime, malformed call-args, no tokio runtime) don't
        // raise — they become a structured BamlOutboundResult envelope so the
        // returned bytes decode + raise uniformly via decode_call_result.
        let request = (|| -> Result<_, bridge_cffi::BridgeError> {
            let request = bridge_cffi::decode_invocation_request(&args_proto)?;
            let rt = bridge_cffi::get_tokio_runtime()?;
            Ok((request, rt))
        })();

        let (request, rt) = match request {
            Ok(v) => v,
            Err(e) => {
                return result_with_stream(py, bridge_cffi::error_to_outbound(e), None, stream);
            }
        };

        // Same shared execute_invocation as the async + C-ABI paths — returns the
        // encoded BamlOutboundResult envelope bytes.
        let call_id = request.host_call_id();
        let (sender, mut receiver) = std::sync::mpsc::channel();
        let _environment = DispatchScope::synchronous(call_id, sender.clone())?;
        let (retained_sender, retained_receiver) = std::sync::mpsc::channel();
        let task = rt.spawn(async move {
            let (bytes, retained) = execute_python_invocation(Ok(request), stream).await;
            let _ = retained_sender.send(retained);
            bytes
        });
        // Observe panics/cancellation of the engine task as well as normal
        // results; a producer disappearing must never strand the caller queue.
        rt.spawn(async move {
            let bytes = match task.await {
                Ok(bytes) => bytes,
                Err(error) => bridge_cffi::error_to_outbound(bridge_cffi::BridgeError::Internal(
                    format!("BAML execution task failed: {error}"),
                )),
            };
            let _ = sender.send(SyncMessage::Finished(bytes));
        });
        loop {
            let message = py.detach({
                let receiver = &mut receiver;
                move || receiver.recv_timeout(std::time::Duration::from_millis(100))
            });
            if let Err(error) = py.check_signals() {
                bridge_cffi::cancel_function_call_by_id(call_id);
                return Err(error);
            }
            match message {
                Ok(SyncMessage::Dispatch(dispatch)) => crate::host_value::dispatch_sync(dispatch),
                Ok(SyncMessage::Finished(bytes)) => {
                    return result_with_stream(
                        py,
                        bytes,
                        retained_receiver.try_recv().ok().flatten(),
                        stream,
                    );
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(py_sdk_panic(
                        "BAML callback queue disconnected before completion",
                    ));
                }
            }
        }
    }
}

/// Return the process-global `BamlRuntime`, or raise `BamlError` if
/// `BamlRuntime.initialize_runtime(...)` has not been called yet.
///
/// Used by the pure-Python factories in `baml_bridge` so generated
/// leaves don't have to thread a runtime reference through every call
/// site.
#[gen_stub_pyfunction]
#[pyfunction]
pub fn get_runtime() -> PyResult<BamlRuntime> {
    // Validate the singleton is initialized so callers get a helpful error
    // here rather than a confusing one deep in a later call; the handle itself
    // is zero-sized (the Arc lives in bridge_cffi).
    // Handle-returning site: an uninitialized/failed runtime is an SDK setup
    // failure, surfaced as BamlPanic(SdkPanic) (32c).
    bridge_cffi::get_runtime().map_err(|e| match e {
        bridge_cffi::BridgeError::NotInitialized => py_sdk_panic(
            "BAML runtime has not been initialized — did baml_sdk/__init__.py fail to import?",
        ),
        other => bridge_error_to_sdk_panic(other),
    })?;
    Ok(BamlRuntime)
}

type RetainedStream = (bex_project::HostInvocation, crate::py_handle::BamlPyHandle);

async fn execute_python_invocation(
    request: Result<bridge_cffi::InvocationRequest, bridge_cffi::BridgeError>,
    stream: bool,
) -> (Vec<u8>, Option<RetainedStream>) {
    let mut request = match request {
        Ok(request) => request,
        Err(error) => return (bridge_cffi::error_to_outbound(error), None),
    };
    let retained = if stream {
        match request.retain_stream().await {
            Ok((execution, key)) => Some((
                execution,
                crate::py_handle::BamlPyHandle::new(
                    key,
                    bridge_ctypes::baml_bridge::cffi::BamlHandleType::InvocationState as u64,
                ),
            )),
            Err(error) => return (bridge_cffi::error_to_outbound(error), None),
        }
    } else {
        None
    };
    (bridge_cffi::execute_invocation(request).await, retained)
}

fn result_with_stream(
    py: Python<'_>,
    bytes: Vec<u8>,
    retained: Option<RetainedStream>,
    stream: bool,
) -> PyResult<Py<PyAny>> {
    if stream {
        let frame = retained
            .map(|(execution, key)| crate::invocation::stream_frame(execution, key))
            .transpose()?;
        (bytes, frame).into_py_any(py)
    } else {
        bytes.into_py_any(py)
    }
}
