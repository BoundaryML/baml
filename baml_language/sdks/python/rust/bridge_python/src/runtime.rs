//! BamlRuntime PyO3 class - wraps `Arc<dyn Bex>`.

use std::sync::Arc;

use futures::FutureExt;

use bridge_ctypes::baml_bridge::cffi::CallFunctionArgs;
use prost::Message;
use pyo3::{
    Bound, IntoPyObjectExt, Py, Python,
    prelude::{PyResult, pyfunction, pymethods},
    pyclass,
    types::{PyAny, PyDict},
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
            def call_function(self, args_proto: bytes, kwargs: typing.Optional[dict] = None, cancel: typing.Any = None, *, stream: bool = False) -> typing.Any:
                """Call a BAML function asynchronously."""

            def call_function_sync(self, args_proto: bytes, kwargs: typing.Optional[dict] = None, cancel: typing.Any = None, *, stream: bool = False) -> typing.Any:
                """Call a BAML function synchronously (blocking)."""
        "#
    }
}

#[pymethods]
impl BamlRuntime {
    /// Call a BAML function asynchronously.
    ///
    /// # Arguments
    /// * `args_proto` - Protobuf-encoded `CallFunctionArgs` with its target and
    ///   controls but no `kwargs`
    /// * `kwargs` - The arguments by declared parameter name, as Python values;
    ///   encoded here against the target's declared types
    /// * `cancel` - The `_baml.cancel` token, if any
    /// * `stream` - Whether the call pulls a stream step
    #[pyo3(signature = (args_proto, kwargs=None, cancel=None, *, stream=false))]
    fn call_function<'py>(
        &self,
        py: Python<'py>,
        args_proto: Vec<u8>,
        kwargs: Option<&Bound<'py, PyDict>>,
        cancel: Option<&Bound<'py, PyAny>>,
        stream: bool,
    ) -> PyResult<Py<PyAny>> {
        // A value that does not fit its declared type is the caller's error
        // and raises here, before anything is sent. Byte-returning site (32c)
        // otherwise: pre-call host-boundary failures don't raise — they become
        // a structured BamlOutboundResult envelope so the future yields bytes
        // that decode_call_result raises uniformly (same BamlError(baml.errors.*)
        // as an engine failure).
        // `decode_invocation_request` pins a handle target before we yield to the event
        // loop, so a Python-side release of that handle cannot race the call.
        let args_proto = encode_request(py, args_proto, kwargs, cancel)?;
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
    /// * `args_proto` - Protobuf-encoded `CallFunctionArgs` with its target and
    ///   controls but no `kwargs`
    /// * `kwargs` - The arguments by declared parameter name, as Python values;
    ///   encoded here against the target's declared types
    /// * `cancel` - The `_baml.cancel` token, if any
    /// * `stream` - Whether the call pulls a stream step
    #[pyo3(signature = (args_proto, kwargs=None, cancel=None, *, stream=false))]
    fn call_function_sync(
        &self,
        py: Python<'_>,
        args_proto: Vec<u8>,
        kwargs: Option<&Bound<'_, PyDict>>,
        cancel: Option<&Bound<'_, PyAny>>,
        stream: bool,
    ) -> PyResult<Py<PyAny>> {
        // A value that does not fit its declared type raises here. Otherwise
        // byte-returning site (32c): pre-call host-boundary failures
        // (uninitialized runtime, malformed call-args, no tokio runtime) don't
        // raise — they become a structured BamlOutboundResult envelope so the
        // returned bytes decode + raise uniformly via decode_call_result.
        let args_proto = encode_request(py, args_proto, kwargs, cancel)?;
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
        let (sender, receiver) = std::sync::mpsc::channel();
        let wake = Arc::new(tokio::sync::Notify::new());
        let _environment = DispatchScope::synchronous(call_id, sender.clone(), Arc::clone(&wake))?;
        let mut invocation: Invocation = Box::pin(
            std::panic::AssertUnwindSafe(execute_python_invocation(Ok(request), stream))
                .catch_unwind(),
        );
        // A thread inside a runtime cannot block on one; it waits for a worker.
        if tokio::runtime::Handle::try_current().is_err() {
            match run_until_callback(py, &rt, &mut invocation, &wake) {
                Ok(Some(outcome)) => return finish(py, outcome, stream),
                Ok(None) => {}
                Err(error) => {
                    bridge_cffi::cancel_function_call_by_id(call_id);
                    // The cancelled call finishes unwinding on the runtime.
                    rt.spawn(invocation);
                    return Err(error);
                }
            }
        }
        wait_on_runtime(py, &rt, call_id, invocation, sender, receiver, stream)
    }
}

/// A sync call's invocation, with a panic caught as its outcome.
type Invocation = std::pin::Pin<Box<dyn Future<Output = Outcome> + Send>>;
type Outcome = std::thread::Result<(Vec<u8>, Option<RetainedStream>)>;

/// How often a blocked sync caller checks for Python signals (Ctrl-C).
const SIGNAL_POLL: std::time::Duration = std::time::Duration::from_millis(100);

/// Run a sync call on the calling thread, with the GIL released, until it
/// finishes (`Some`) or first needs a Python callback (`None`). `Err` is a
/// Python signal, such as Ctrl-C, raised while it ran.
///
/// Handing a call to a runtime worker and waiting for the answer costs about
/// half of a trivial call in thread wake-ups, so a call starts here. It
/// leaves at its first callback: the callback runs on this thread, and while
/// it does nothing would drive the call, which could be what the callback is
/// waiting for. Tasks the call spawns run on the workers either way.
fn run_until_callback(
    py: Python<'_>,
    rt: &tokio::runtime::Runtime,
    invocation: &mut Invocation,
    wake: &tokio::sync::Notify,
) -> PyResult<Option<Outcome>> {
    enum Step {
        Finished(Box<Outcome>),
        Callback,
        Tick,
    }
    loop {
        let step = py.detach(|| {
            rt.block_on(async {
                tokio::select! {
                    biased;
                    outcome = &mut *invocation => Step::Finished(Box::new(outcome)),
                    () = wake.notified() => Step::Callback,
                    () = tokio::time::sleep(SIGNAL_POLL) => Step::Tick,
                }
            })
        });
        match step {
            Step::Finished(outcome) => return Ok(Some(*outcome)),
            Step::Callback => return Ok(None),
            Step::Tick => py.check_signals()?,
        }
    }
}

/// Run a sync call on a runtime worker and wait for it here, running its
/// Python callbacks as they arrive.
fn wait_on_runtime(
    py: Python<'_>,
    rt: &tokio::runtime::Runtime,
    call_id: u64,
    invocation: Invocation,
    sender: std::sync::mpsc::Sender<SyncMessage>,
    mut receiver: std::sync::mpsc::Receiver<SyncMessage>,
    stream: bool,
) -> PyResult<Py<PyAny>> {
    let (retained_sender, retained_receiver) = std::sync::mpsc::channel();
    let task = rt.spawn(async move {
        let (bytes, retained) = match invocation.await {
            Ok(finished) => finished,
            Err(panic) => (panic_outbound(panic.as_ref()), None),
        };
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
            move || receiver.recv_timeout(SIGNAL_POLL)
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

/// The result of a sync call that finished on the calling thread.
fn finish(py: Python<'_>, outcome: Outcome, stream: bool) -> PyResult<Py<PyAny>> {
    match outcome {
        Ok((bytes, retained)) => result_with_stream(py, bytes, retained, stream),
        Err(panic) => result_with_stream(py, panic_outbound(panic.as_ref()), None, stream),
    }
}

/// The envelope for a sync call whose invocation panicked, as the runtime's
/// join error reported it before calls ran on the calling thread.
fn panic_outbound(panic: &(dyn std::any::Any + Send)) -> Vec<u8> {
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("panic");
    bridge_cffi::error_to_outbound(bridge_cffi::BridgeError::Internal(format!(
        "BAML execution task failed: {message}"
    )))
}

/// Fill the request's `kwargs` (and cancel token) from Python values encoded
/// against the target's declared types, and return the complete request.
///
/// A failed encode has already released every owner it created; the
/// identified call allocation is retired here so the failure does not strand
/// it.
fn encode_request(
    py: Python<'_>,
    args_proto: Vec<u8>,
    kwargs: Option<&Bound<'_, PyDict>>,
    cancel: Option<&Bound<'_, PyAny>>,
) -> PyResult<Vec<u8>> {
    // Always re-encoded, kwargs or not: the call's type arguments are
    // restated with references of the call's own, so the `BamlType` wrappers'
    // keys are never drained out from under them.
    let mut call = CallFunctionArgs::decode(args_proto.as_slice())
        .map_err(|error| py_sdk_panic(format!("malformed CallFunctionArgs: {error}")))?;
    if let Err(error) = crate::encode::encode_call_kwargs(py, &mut call, kwargs, cancel) {
        bridge_cffi::release_function_call_by_id(call.call_id);
        return Err(error);
    }
    Ok(call.encode_to_vec())
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
