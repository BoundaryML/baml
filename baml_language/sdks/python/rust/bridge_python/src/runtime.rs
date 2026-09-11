//! BamlRuntime PyO3 class - wraps `Arc<dyn Bex>`.

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
        // `prepare_call` pins a handle target before we yield to the event
        // loop, so a Python-side release of that handle cannot race the call.
        let prepared = (|| -> Result<_, bridge_cffi::BridgeError> {
            let runtime = bridge_cffi::get_runtime()?;
            let prepared = bridge_cffi::prepare_call(&args_proto)?;
            Ok((runtime, prepared))
        })();

        // Tracing is a no-op: `ctx`/`collectors` are accepted for ABI
        // stability but no longer wired into the call context.
        let _ = (&ctx, &collectors);

        // The whole Result -> BamlOutboundResult translation (incl. the
        // catch_unwind -> SdkPanic boundary) lives in bridge_cffi; we just
        // return the encoded envelope bytes for Python to decode + raise.
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let bytes = match prepared {
                Ok((runtime, prepared)) => bridge_cffi::invoke_prepared(runtime, prepared).await,
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
            let prepared = bridge_cffi::prepare_call(&args_proto)?;
            let rt = bridge_cffi::get_tokio_runtime()?;
            Ok((runtime, prepared, rt))
        })();

        let (runtime, prepared, rt) = match prepared {
            Ok(v) => v,
            Err(e) => return Ok(bridge_cffi::error_to_outbound(e)),
        };

        // Tracing is a no-op: `ctx`/`collectors` are accepted for ABI
        // stability but no longer wired into the call context.
        let _ = (&ctx, &collectors);

        // Same shared invoke_prepared as the async + C-ABI paths — returns the
        // encoded BamlOutboundResult envelope bytes.
        let bytes = py.detach(|| rt.block_on(bridge_cffi::invoke_prepared(runtime, prepared)));
        Ok(bytes)
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
