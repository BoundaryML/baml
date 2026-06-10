//! BamlRuntime PyO3 class - wraps `Arc<dyn Bex>`.

use std::sync::Arc;

use bridge_ctypes::{HANDLE_TABLE, kwargs_to_bex_values};
use prost::Message;
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

use crate::{
    abort_controller::AbortController,
    errors::{bridge_error_to_sdk_panic, py_sdk_panic},
    types::collector::Collector,
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
    fn initialize_runtime_from_bytecode(bytecode: Vec<u8>) -> PyResult<Self> {
        match bridge_cffi::initialize_runtime_from_bytecode(&bytecode) {
            Ok(_bex) => Ok(BamlRuntime),
            // Handle-returning site: can't hand back envelope bytes, so an
            // SDK setup failure surfaces as BamlPanic(SdkPanic) (32c).
            Err(e) => Err(bridge_error_to_sdk_panic(e)),
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
            def call_function(self, function_name: str, args_proto: bytes, ctx: typing.Optional["HostSpanManager"] = None, collectors: typing.Optional[typing.Sequence["Collector"]] = None, abort_controller: typing.Optional["AbortController"] = None) -> typing.Any:
                """Call a BAML function asynchronously."""

            def call_function_sync(self, function_name: str, args_proto: bytes, ctx: typing.Optional["HostSpanManager"] = None, collectors: typing.Optional[typing.Sequence["Collector"]] = None, abort_controller: typing.Optional["AbortController"] = None) -> bytes:
                """Call a BAML function synchronously (blocking)."""
        "#
    }
}

#[pymethods]
impl BamlRuntime {
    /// Call a BAML function asynchronously.
    ///
    /// # Arguments
    /// * `function_name` - Name of the BAML function to call
    /// * `args_proto` - Protobuf-encoded HostFunctionArguments bytes
    /// * `ctx` - Host span manager; if active spans exist, nests under host trace
    /// * `collectors` - Optional list of Collector objects to track this call
    /// * `abort_controller` - Optional AbortController to cancel the call
    #[pyo3(signature = (function_name, args_proto, ctx=None, collectors=None, abort_controller=None))]
    fn call_function<'py>(
        &self,
        py: Python<'py>,
        function_name: String,
        args_proto: Vec<u8>,
        ctx: Option<&crate::types::HostSpanManager>,
        collectors: Option<Vec<pyo3::PyRef<'py, Collector>>>,
        abort_controller: Option<&AbortController>,
    ) -> PyResult<Py<PyAny>> {
        // Byte-returning site (32c): pre-call host-boundary failures don't
        // raise — they become a structured BamlOutboundResult envelope so the
        // future yields bytes that decode_call_result raises uniformly (same
        // BamlError(baml.errors.*) as an engine failure).
        let prepared = (|| -> Result<_, bridge_cffi::BridgeError> {
            let runtime = bridge_cffi::get_runtime()?;
            let (kwargs, type_refs) = decode_args(&args_proto, &function_name)?;
            let type_args = bridge_cffi::resolve_type_args(&runtime, type_refs)?;
            Ok((runtime, kwargs, type_args))
        })();

        let host_ctx = ctx.and_then(|c| c.host_span_context());
        let cancel = abort_controller
            .map(AbortController::token)
            .unwrap_or_default();

        let collector_arcs: Vec<Arc<bex_events::Collector>> = collectors
            .as_ref()
            .map(|colls| colls.iter().map(|c| c.inner_arc()).collect())
            .unwrap_or_default();

        let call_id = bex_project::CallId::next();
        let mut call_ctx = bex_project::FunctionCallContextBuilder::new(call_id)
            .with_collectors(collector_arcs)
            .with_cancel_token(cancel);

        if let Some(host_ctx) = host_ctx {
            call_ctx = call_ctx.with_host_ctx(host_ctx);
        }

        // The whole Result -> BamlOutboundResult translation (incl. the
        // catch_unwind -> SdkPanic boundary) lives in bridge_cffi; we just
        // return the encoded envelope bytes for Python to decode + raise.
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let bytes = match prepared {
                Ok((runtime, kwargs, type_args)) => {
                    let call_ctx = call_ctx.with_type_args(type_args);
                    bridge_cffi::call_and_encode(runtime, function_name, kwargs, call_ctx.build())
                        .await
                }
                Err(e) => bridge_cffi::error_to_outbound(e),
            };
            Ok(bytes)
        })
        .map(pyo3::Bound::into)
    }

    /// Call a BAML function synchronously (blocking).
    ///
    /// # Arguments
    /// * `function_name` - Name of the BAML function to call
    /// * `args_proto` - Protobuf-encoded HostFunctionArguments bytes
    /// * `ctx` - Host span manager; if active spans exist, nests under host trace
    /// * `collectors` - Optional list of Collector objects to track this call
    /// * `abort_controller` - Optional AbortController to cancel the call
    #[pyo3(signature = (function_name, args_proto, ctx=None, collectors=None, abort_controller=None))]
    fn call_function_sync(
        &self,
        py: Python<'_>,
        function_name: String,
        args_proto: Vec<u8>,
        ctx: Option<&crate::types::HostSpanManager>,
        collectors: Option<Vec<pyo3::PyRef<'_, Collector>>>,
        abort_controller: Option<&AbortController>,
    ) -> PyResult<Vec<u8>> {
        // Byte-returning site (32c): pre-call host-boundary failures
        // (uninitialized runtime, malformed call-args, no tokio runtime) don't
        // raise — they become a structured BamlOutboundResult envelope so the
        // returned bytes decode + raise uniformly via decode_call_result.
        let prepared = (|| -> Result<_, bridge_cffi::BridgeError> {
            let runtime = bridge_cffi::get_runtime()?;
            let (kwargs, type_refs) = decode_args(&args_proto, &function_name)?;
            let type_args = bridge_cffi::resolve_type_args(&runtime, type_refs)?;
            let rt = bridge_cffi::get_tokio_runtime()?;
            Ok((runtime, kwargs, type_args, rt))
        })();

        let (runtime, kwargs, type_args, rt) = match prepared {
            Ok(v) => v,
            Err(e) => return Ok(bridge_cffi::error_to_outbound(e)),
        };

        let host_ctx = ctx.and_then(|c| c.host_span_context());
        let cancel = abort_controller
            .map(AbortController::token)
            .unwrap_or_default();

        let collector_arcs: Vec<Arc<bex_events::Collector>> = collectors
            .as_ref()
            .map(|colls| colls.iter().map(|c| c.inner_arc()).collect())
            .unwrap_or_default();

        let call_id = bex_project::CallId::next();
        let mut call_ctx = bex_project::FunctionCallContextBuilder::new(call_id)
            .with_collectors(collector_arcs)
            .with_cancel_token(cancel)
            .with_type_args(type_args);

        if let Some(host_ctx) = host_ctx {
            call_ctx = call_ctx.with_host_ctx(host_ctx);
        }

        // Same shared call_and_encode as the async + C-ABI paths — returns the
        // encoded BamlOutboundResult envelope bytes.
        let bytes = py.detach(|| {
            rt.block_on(bridge_cffi::call_and_encode(
                runtime,
                function_name,
                kwargs,
                call_ctx.build(),
            ))
        });

        Ok(bytes)
    }
}

/// Decode protobuf-encoded function arguments into `BexArgs`.
///
/// Returns a `BridgeError` (not a `PyErr`) so the byte-returning call sites can
/// route the failure through `bridge_cffi::error_to_outbound` into the
/// structured `BamlOutboundResult` envelope (32c) rather than raising.
fn decode_args(
    args_proto: &[u8],
    function_name: &str,
) -> Result<
    (
        bex_project::BexArgs,
        Vec<bridge_ctypes::baml_core::cffi::InboundTypeRef>,
    ),
    bridge_cffi::BridgeError,
> {
    use bridge_ctypes::CtypesError;

    let args =
        bridge_ctypes::baml_core::cffi::CallFunctionArgs::decode(args_proto).map_err(|e| {
            CtypesError::InternalError(format!(
                "Failed to decode arguments for function '{function_name}': {e}"
            ))
        })?;

    let kwargs = kwargs_to_bex_values(args.kwargs, &HANDLE_TABLE).map_err(|e| {
        CtypesError::InternalError(format!(
            "Failed to convert arguments for function '{function_name}': {e}"
        ))
    })?;

    Ok((kwargs.into(), args.type_args))
}

/// Return the process-global `BamlRuntime`, or raise `BamlError` if
/// `BamlRuntime.initialize_runtime(...)` has not been called yet.
///
/// Used by the pure-Python factories in `baml_core` so generated
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
