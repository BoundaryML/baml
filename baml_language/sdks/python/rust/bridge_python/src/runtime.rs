//! BamlRuntime PyO3 class - binds one engine weakly and retains its session.

use pyo3::{
    Py, Python,
    prelude::{PyResult, pyfunction, pymethods},
    pyclass,
    types::PyAny,
};
use pyo3_stub_gen::{
    derive::{gen_methods_from_python, gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods},
    inventory::submit,
};

use crate::encoded_result::BamlEncodedResult;

use crate::{
    errors::{bridge_error_to_sdk_panic, py_sdk_panic},
    types::collector::Collector,
};

type DecodedCallArgs = bridge_cffi::PreparedCall;

#[gen_stub_pyfunction]
#[pyfunction]
pub fn _pending_transfer_count() -> PyResult<usize> {
    bridge_cffi::get_runtime_with_transfers()
        .map(|(_, session)| session.pending_count())
        .map_err(bridge_error_to_sdk_panic)
}

/// A binding to one installed runtime/session. SDK modules may retain this
/// indefinitely without retaining a closed engine; admitted calls upgrade
/// the weak reference and hold the engine until their outcome is delivered.
#[gen_stub_pyclass]
#[pyclass]
pub struct BamlRuntime {
    runtime: std::sync::Weak<dyn bex_project::Bex>,
    transfers: bridge_ctypes::TransferSession<'static>,
}

impl BamlRuntime {
    fn current() -> Result<Self, bridge_cffi::BridgeError> {
        let (runtime, transfers) = bridge_cffi::get_runtime_with_transfers()?;
        Ok(Self {
            runtime: std::sync::Arc::downgrade(&runtime),
            transfers,
        })
    }

    fn initialized(runtime: std::sync::Arc<dyn bex_project::Bex>) -> PyResult<Self> {
        let (current, transfers) =
            bridge_cffi::get_runtime_with_transfers().map_err(bridge_error_to_sdk_panic)?;
        // Initialization and binding must never silently select a runtime
        // installed concurrently by another SDK/thread.
        if !std::sync::Arc::ptr_eq(&runtime, &current) {
            return Err(py_sdk_panic(
                "BAML runtime was replaced during initialization",
            ));
        }
        Ok(Self {
            runtime: std::sync::Arc::downgrade(&runtime),
            transfers,
        })
    }

    fn prepare(&self, bytes: &[u8]) -> Result<OwnedCall, bridge_cffi::BridgeError> {
        // Consume incoming transfer leases even if this binding was closed.
        let decoded = decode_args(bytes)?;
        let (runtime, transfers) = self.authority()?;
        Ok((runtime, transfers, decoded))
    }

    fn authority(
        &self,
    ) -> Result<
        (
            std::sync::Arc<dyn bex_project::Bex>,
            bridge_ctypes::TransferSession<'static>,
        ),
        bridge_cffi::BridgeError,
    > {
        if self.transfers.is_closed() {
            return Err(bridge_cffi::BridgeError::InvalidInvocation(
                "SDK runtime was closed or replaced".into(),
            ));
        }
        let runtime = self.runtime.upgrade().ok_or_else(|| {
            bridge_cffi::BridgeError::InvalidInvocation("SDK runtime is no longer available".into())
        })?;
        Ok((runtime, self.transfers.clone()))
    }

    fn prepare_host_operation(
        &self,
        bytes: &[u8],
    ) -> Result<OwnedHostOperation, bridge_cffi::BridgeError> {
        let prepared = bridge_cffi::host_registration::prepare_operation(bytes)?;
        let (runtime, transfers) = self.authority()?;
        Ok((runtime, transfers, prepared))
    }
}

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
        // singleton; the SDK binding keeps a weak reference, not another owner.
        match bridge_cffi::initialize_runtime(&root_path, files) {
            Ok(bex) => Self::initialized(bex),
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
    fn initialize_runtime_from_bytecode(
        bytecode: Vec<u8>,
        embedded_baml_toml: Option<String>,
    ) -> PyResult<Self> {
        match bridge_cffi::initialize_runtime_from_bytecode(
            &bytecode,
            embedded_baml_toml.as_deref(),
        ) {
            Ok(bex) => Self::initialized(bex),
            Err(e) => Err(crate::errors::bridge_error_to_initialization_error(e)),
        }
    }

    /// Cancel work on this binding's issuer, including already-admitted work
    /// during shutdown. A dropped engine has no work left to cancel.
    fn cancel_function_call(&self, call_id: u64) -> bool {
        self.runtime.upgrade().is_some_and(|runtime| {
            runtime
                .cancel_function_call(bex_project::CallId(call_id))
                .is_ok()
        })
    }
}

// Manual stub declarations for methods with complex parameter types
// that pyo3-stub-gen cannot process (reference params, PyRef, etc.).
submit! {
    gen_methods_from_python! {
        r#"
        import typing

        class BamlRuntime:
            def call_function(self, args_proto: bytes, ctx: typing.Optional["HostSpanManager"] = None, collectors: typing.Optional[typing.Sequence["Collector"]] = None) -> typing.Awaitable["BamlEncodedResult"]:
                """Call a BAML function asynchronously."""

            def call_function_sync(self, args_proto: bytes, ctx: typing.Optional["HostSpanManager"] = None, collectors: typing.Optional[typing.Sequence["Collector"]] = None) -> "BamlEncodedResult":
                """Call a BAML function synchronously (blocking)."""

            def _host_operation(self, args_proto: bytes) -> typing.Awaitable["BamlEncodedResult"]:
                """Execute a private SDK registration/projection request."""

            def _host_operation_sync(self, args_proto: bytes) -> "BamlEncodedResult":
                """Execute a private SDK registration/projection request, blocking."""
        "#
    }
}

#[pymethods]
impl BamlRuntime {
    fn _host_operation<'py>(&self, py: Python<'py>, args_proto: Vec<u8>) -> PyResult<Py<PyAny>> {
        let _drain = DrainHostReleases;
        let prepared = self.prepare_host_operation(&args_proto);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            finish_host_operation(prepared).await
        })
        .map(pyo3::Bound::into)
    }

    fn _host_operation_sync(
        &self,
        py: Python<'_>,
        args_proto: Vec<u8>,
    ) -> PyResult<BamlEncodedResult> {
        let _drain = DrainHostReleases;
        let prepared = self.prepare_host_operation(&args_proto);
        match bridge_cffi::get_tokio_runtime() {
            Ok(executor) => py.detach(|| executor.block_on(finish_host_operation(prepared))),
            Err(error) => host_operation_error(error),
        }
    }

    /// Call a BAML function asynchronously.
    ///
    /// # Arguments
    /// * `args_proto` - Protobuf-encoded `CallFunctionArgs` including its target
    /// * `ctx` - Accepted for ABI compatibility; currently ignored
    /// * `collectors` - Accepted for ABI compatibility; currently ignored
    #[pyo3(signature = (args_proto, ctx=None, collectors=None))]
    fn call_function<'py>(
        &self,
        py: Python<'py>,
        args_proto: Vec<u8>,
        ctx: Option<&crate::types::HostSpanManager>,
        collectors: Option<Vec<pyo3::PyRef<'py, Collector>>>,
    ) -> PyResult<Py<PyAny>> {
        // Preparation failures use the same owned result envelope as engine
        // outcomes. The Python decoder adopts before returning or raising.
        let _drain = DrainHostReleases;
        let prepared = self.prepare(&args_proto);
        bex_project::host_release_dispatch::drain();

        // Tracing is a no-op: `ctx`/`collectors` are accepted for ABI
        // stability but no longer wired into the call context.
        let _ = (&ctx, &collectors);

        // The result object owns delivery cleanup even when conversion to a
        // Python future result fails, or nobody observes the completed future.
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            match prepared {
                Ok((runtime, transfers, decoded)) => {
                    let encoded =
                        bridge_cffi::invoke_prepared_encoded(runtime.clone(), decoded).await;
                    BamlEncodedResult::new(encoded, transfers, Some(runtime))
                }
                Err(error) => boundary_error_result(error),
            }
        })
        .map(pyo3::Bound::into)
    }

    /// Call a BAML function synchronously (blocking).
    ///
    /// # Arguments
    /// * `args_proto` - Protobuf-encoded `CallFunctionArgs` including its target
    /// * `ctx` - Accepted for ABI compatibility; currently ignored
    /// * `collectors` - Accepted for ABI compatibility; currently ignored
    #[pyo3(signature = (args_proto, ctx=None, collectors=None))]
    fn call_function_sync(
        &self,
        py: Python<'_>,
        args_proto: Vec<u8>,
        ctx: Option<&crate::types::HostSpanManager>,
        collectors: Option<Vec<pyo3::PyRef<'_, Collector>>>,
    ) -> PyResult<BamlEncodedResult> {
        // Keep pre-call failures in the same owned envelope and error decoder
        // as engine outcomes, including malformed arguments and no runtime.
        let _drain = DrainHostReleases;
        let prepared = (|| -> Result<_, bridge_cffi::BridgeError> {
            let (runtime, transfers, decoded) = self.prepare(&args_proto)?;
            let rt = bridge_cffi::get_tokio_runtime()?;
            Ok((runtime, transfers, decoded, rt))
        })();

        let (runtime, transfers, decoded, rt) = match prepared {
            Ok(v) => v,
            Err(e) => return boundary_error_result(e),
        };

        // Tracing is a no-op: `ctx`/`collectors` are accepted for ABI
        // stability but no longer wired into the call context.
        let _ = (&ctx, &collectors);
        let encoded = py.detach(|| {
            rt.block_on(bridge_cffi::invoke_prepared_encoded(
                runtime.clone(),
                decoded,
            ))
        });
        BamlEncodedResult::new(encoded, transfers, Some(runtime))
    }
}

fn boundary_error_result(error: bridge_cffi::BridgeError) -> PyResult<BamlEncodedResult> {
    BamlEncodedResult::new(
        bridge_cffi::baml_to_host::error_to_outbound_encoded(error),
        bridge_ctypes::TransferSession::new(&bridge_ctypes::HANDLE_TABLE),
        None,
    )
}

type OwnedHostOperation = (
    std::sync::Arc<dyn bex_project::Bex>,
    bridge_ctypes::TransferSession<'static>,
    bridge_cffi::host_registration::PreparedHostOperation,
);

async fn finish_host_operation(
    prepared: Result<OwnedHostOperation, bridge_cffi::BridgeError>,
) -> PyResult<BamlEncodedResult> {
    let _drain = DrainHostReleases;
    match prepared {
        Ok((runtime, session, operation)) => {
            let encoded =
                bridge_cffi::host_registration::execute_operation(runtime.clone(), operation).await;
            BamlEncodedResult::new(encoded, session, Some(runtime))
        }
        Err(error) => host_operation_error(error),
    }
}

fn host_operation_error(error: bridge_cffi::BridgeError) -> PyResult<BamlEncodedResult> {
    BamlEncodedResult::new(
        bridge_cffi::host_registration::operation_error(error),
        bridge_ctypes::TransferSession::new(&bridge_ctypes::HANDLE_TABLE),
        None,
    )
}

struct DrainHostReleases;
impl Drop for DrainHostReleases {
    fn drop(&mut self) {
        bex_project::host_release_dispatch::drain();
    }
}

type OwnedCall = (
    std::sync::Arc<dyn bex_project::Bex>,
    bridge_ctypes::TransferSession<'static>,
    bridge_cffi::PreparedCall,
);

fn prepare_owned_call(
    handle: &crate::py_handle::BamlPyHandle,
    bytes: &[u8],
) -> Result<OwnedCall, bridge_cffi::BridgeError> {
    // Consume argument transfers even if the receiver/issuer has closed.
    let call = decode_args(bytes)?;
    let (runtime, transfers) = handle
        .invocation_owner()
        .map_err(|error| bridge_cffi::BridgeError::InvalidInvocation(error.to_string()))?;
    Ok((runtime, transfers, call))
}

pub(crate) fn call_owned_function(
    py: Python<'_>,
    handle: &crate::py_handle::BamlPyHandle,
    bytes: Vec<u8>,
) -> PyResult<Py<PyAny>> {
    let _drain = DrainHostReleases;
    let prepared = prepare_owned_call(handle, &bytes);
    bex_project::host_release_dispatch::drain();
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        match prepared {
            Ok((runtime, transfers, call)) => {
                let encoded = bridge_cffi::invoke_prepared_encoded(runtime.clone(), call).await;
                BamlEncodedResult::new(encoded, transfers, Some(runtime))
            }
            Err(error) => boundary_error_result(error),
        }
    })
    .map(pyo3::Bound::into)
}

pub(crate) fn call_owned_function_sync(
    py: Python<'_>,
    handle: &crate::py_handle::BamlPyHandle,
    bytes: Vec<u8>,
) -> PyResult<BamlEncodedResult> {
    let _drain = DrainHostReleases;
    let prepared = prepare_owned_call(handle, &bytes)
        .and_then(|call| bridge_cffi::get_tokio_runtime().map(|executor| (call, executor)));
    let ((runtime, transfers, call), executor) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => return boundary_error_result(error),
    };
    let encoded = py
        .detach(|| executor.block_on(bridge_cffi::invoke_prepared_encoded(runtime.clone(), call)));
    BamlEncodedResult::new(encoded, transfers, Some(runtime))
}

/// Prepare and retain both receiver and issuer before handing work to Tokio.
/// Argument decoding happens even when the local reference has expired: it
/// consumes the encoder's transferred argument leases on every rejection path.
pub(crate) fn call_interface_method(
    py: Python<'_>,
    handle: &crate::py_handle::BamlPyHandle,
    member: String,
    bytes: Vec<u8>,
) -> PyResult<Py<PyAny>> {
    use bridge_ctypes::baml_bridge::cffi::{
        BamlHandleType, CallFunctionArgs, InterfaceMethodTarget, call_function_args::CallTarget,
    };
    use prost::Message;

    // Future conversion can fail before Tokio takes ownership (for example,
    // when there is no Python event loop). Drop transferred arguments before
    // this guard flushes their host releases.
    let _drain = DrainHostReleases;

    let prepared = (|| -> Result<_, bridge_cffi::BridgeError> {
        let mut args =
            CallFunctionArgs::decode(bytes.as_slice()).map_err(bridge_ctypes::CtypesError::from)?;
        args.call_target = Some(CallTarget::InterfaceMethod(InterfaceMethodTarget {
            view: handle.handle_key,
            member,
            type_args: std::mem::take(&mut args.type_args),
        }));
        let call = bridge_cffi::prepare_call(&args.encode_to_vec())?;
        if handle.handle_type != BamlHandleType::AdtInterface as u64 {
            return Err(bridge_cffi::BridgeError::InvalidInvocation(
                "reference is not a BAML interface".into(),
            ));
        }
        let (runtime, transfers) = handle
            .invocation_owner()
            .map_err(|error| bridge_cffi::BridgeError::InvalidInvocation(error.to_string()))?;
        Ok((runtime, transfers, call))
    })();
    bex_project::host_release_dispatch::drain();
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        match prepared {
            Ok((runtime, transfers, call)) => {
                let encoded = bridge_cffi::invoke_prepared_encoded(runtime.clone(), call).await;
                BamlEncodedResult::new(encoded, transfers, Some(runtime))
            }
            Err(error) => boundary_error_result(error),
        }
    })
    .map(pyo3::Bound::into)
}

/// Decode protobuf-encoded function arguments into `BexArgs`.
///
/// Returns a `BridgeError` (not a `PyErr`) so the byte-returning call sites can
/// route the failure through `bridge_cffi::error_to_outbound` into the
/// structured `BamlOutboundResult` envelope (32c) rather than raising.
fn decode_args(args_proto: &[u8]) -> Result<DecodedCallArgs, bridge_cffi::BridgeError> {
    bridge_cffi::prepare_call(args_proto)
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
    BamlRuntime::current().map_err(|e| match e {
        bridge_cffi::BridgeError::NotInitialized => py_sdk_panic(
            "BAML runtime has not been initialized — did baml_sdk/__init__.py fail to import?",
        ),
        other => bridge_error_to_sdk_panic(other),
    })
}
