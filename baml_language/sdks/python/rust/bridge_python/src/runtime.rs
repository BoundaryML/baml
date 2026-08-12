//! BamlRuntime PyO3 class - wraps `Arc<dyn Bex>`.

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
    errors::{bridge_error_to_sdk_panic, py_sdk_panic},
    types::collector::Collector,
};

struct DecodedCallArgs {
    kwargs: bex_project::BexArgs,
    call_id: bex_project::CallId,
    target: bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget,
    /// Explicit, named `TypeVar` bindings for a generic call (`_types=` + a
    /// generic receiver's class type args): `TypeVar name -> concrete type`,
    /// insertion order is De Bruijn order. Empty for non-generic calls. The
    /// engine maps each name onto the entry-frame `type_args` slot by matching
    /// the callee's generic params.
    type_args: indexmap::IndexMap<String, bex_project::RuntimeTy>,
}

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
    fn initialize_runtime_from_bytecode(
        bytecode: Vec<u8>,
        embedded_baml_toml: Option<String>,
    ) -> PyResult<Self> {
        match bridge_cffi::initialize_runtime_from_bytecode(
            &bytecode,
            embedded_baml_toml.as_deref(),
        ) {
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
            def call_function(self, args_proto: bytes, ctx: typing.Optional["HostSpanManager"] = None, collectors: typing.Optional[typing.Sequence["Collector"]] = None) -> typing.Any:
                """Call a BAML function asynchronously."""

            def call_function_sync(self, args_proto: bytes, ctx: typing.Optional["HostSpanManager"] = None, collectors: typing.Optional[typing.Sequence["Collector"]] = None) -> bytes:
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
        // Byte-returning site (32c): pre-call host-boundary failures don't
        // raise — they become a structured BamlOutboundResult envelope so the
        // future yields bytes that decode_call_result raises uniformly (same
        // BamlError(baml.errors.*) as an engine failure).
        let prepared = (|| -> Result<_, bridge_cffi::BridgeError> {
            let runtime = bridge_cffi::get_runtime()?;
            let decoded = decode_args(&args_proto)?;
            Ok((runtime, decoded))
        })();

        // Tracing is a no-op: `ctx`/`collectors` are accepted for ABI
        // stability but no longer wired into the call context.
        let _ = (&ctx, &collectors);

        // The whole Result -> BamlOutboundResult translation (incl. the
        // catch_unwind -> SdkPanic boundary) lives in bridge_cffi; we just
        // return the encoded envelope bytes for Python to decode + raise.
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let bytes = match prepared {
                Ok((runtime, decoded)) => {
                    let call_ctx = bridge_cffi::function_call_context_builder(decoded.call_id)
                        .with_type_args(decoded.type_args)
                        .build();
                    match decoded.target {
                        bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget::FunctionName(function_name) => {
                            bridge_cffi::call_and_encode(runtime, function_name, decoded.kwargs, call_ctx).await
                        }
                        bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget::FunctionHandle(handle_key) => {
                            bridge_cffi::call_handle_and_encode(runtime, handle_key, decoded.kwargs, call_ctx).await
                        }
                    }
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
    ) -> PyResult<Vec<u8>> {
        // Byte-returning site (32c): pre-call host-boundary failures
        // (uninitialized runtime, malformed call-args, no tokio runtime) don't
        // raise — they become a structured BamlOutboundResult envelope so the
        // returned bytes decode + raise uniformly via decode_call_result.
        let prepared = (|| -> Result<_, bridge_cffi::BridgeError> {
            let runtime = bridge_cffi::get_runtime()?;
            let decoded = decode_args(&args_proto)?;
            let rt = bridge_cffi::get_tokio_runtime()?;
            Ok((runtime, decoded, rt))
        })();

        let (runtime, decoded, rt) = match prepared {
            Ok(v) => v,
            Err(e) => return Ok(bridge_cffi::error_to_outbound(e)),
        };

        // Tracing is a no-op: `ctx`/`collectors` are accepted for ABI
        // stability but no longer wired into the call context.
        let _ = (&ctx, &collectors);
        let call_ctx = bridge_cffi::function_call_context_builder(decoded.call_id)
            .with_type_args(decoded.type_args)
            .build();

        // Same shared call_and_encode as the async + C-ABI paths — returns the
        // encoded BamlOutboundResult envelope bytes.
        let bytes = py.detach(|| match decoded.target {
            bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget::FunctionName(
                function_name,
            ) => rt.block_on(bridge_cffi::call_and_encode(
                runtime,
                function_name,
                decoded.kwargs,
                call_ctx,
            )),
            bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget::FunctionHandle(
                handle_key,
            ) => rt.block_on(bridge_cffi::call_handle_and_encode(
                runtime,
                handle_key,
                decoded.kwargs,
                call_ctx,
            )),
        });
        Ok(bytes)
    }
}

/// Decode protobuf-encoded function arguments into `BexArgs`.
///
/// Returns a `BridgeError` (not a `PyErr`) so the byte-returning call sites can
/// route the failure through `bridge_cffi::error_to_outbound` into the
/// structured `BamlOutboundResult` envelope (32c) rather than raising.
fn decode_args(args_proto: &[u8]) -> Result<DecodedCallArgs, bridge_cffi::BridgeError> {
    use bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget;

    let args = bridge_ctypes::baml_bridge::cffi::CallFunctionArgs::decode(args_proto)
        .map_err(bridge_ctypes::CtypesError::from)?;

    if args.call_id == 0 {
        return Err(bridge_cffi::BridgeError::InvalidCallId);
    }

    let call_id = bex_project::CallId(args.call_id);
    let target = args
        .call_target
        .ok_or(bridge_cffi::BridgeError::MissingCallTarget)?;
    if matches!(target, CallTarget::FunctionHandle(_)) && !args.type_args.is_empty() {
        return Err(bridge_cffi::BridgeError::FunctionHandleTypeArgs);
    }
    let type_args = bridge_ctypes::proto_ty_args_to_named(&args.type_args)?;
    let kwargs = kwargs_to_bex_values(args.kwargs, &HANDLE_TABLE)?;

    Ok(DecodedCallArgs {
        kwargs: kwargs.into(),
        call_id,
        target,
        type_args,
    })
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
