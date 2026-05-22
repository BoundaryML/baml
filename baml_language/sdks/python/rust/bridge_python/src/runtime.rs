//! BamlRuntime PyO3 class - wraps `Arc<dyn Bex>`.

use std::sync::Arc;

use bex_project::Bex;
use bridge_ctypes::{HANDLE_TABLE, external_to_outbound, kwargs_to_bex_values};
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
    errors::{BamlError, BamlInvalidArgumentError, bridge_error_to_py, runtime_error_to_py},
    types::collector::Collector,
};

/// The main BAML runtime, wrapping a `dyn Bex` instance.
#[gen_stub_pyclass]
#[pyclass]
pub struct BamlRuntime {
    bex: Arc<dyn Bex>,
}

#[gen_stub_pymethods]
#[pymethods]
impl BamlRuntime {
    /// Initialize the process-global runtime from in-memory BAML source files.
    ///
    /// Mirrors `bridge_cffi::engine::initialize_runtime`: the same
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
        match bridge_cffi::engine::initialize_runtime(&root_path, files) {
            Ok(bex) => Ok(BamlRuntime { bex }),
            Err(e) => Err(bridge_error_to_py(e)),
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
        let bex = self.bex.clone();
        let kwargs = decode_args(&args_proto, &function_name)?;
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

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result = bex
                .call_function(&function_name, kwargs, call_ctx.build())
                .await
                .map_err(runtime_error_to_py)?;

            let handle_options = bridge_ctypes::CffiHandleTableOptions::for_in_process();
            let baml_value = external_to_outbound(&result, &handle_options).map_err(|e| {
                pyo3::PyErr::new::<BamlInvalidArgumentError, _>(format!(
                    "Failed to encode result: {e}"
                ))
            })?;

            Ok(baml_value.encode_to_vec())
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
        let bex = self.bex.clone();
        let kwargs = decode_args(&args_proto, &function_name)?;
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

        let rt = bridge_cffi::engine::get_tokio_runtime().map_err(bridge_error_to_py)?;

        let result = py
            .detach(|| rt.block_on(bex.call_function(&function_name, kwargs, call_ctx.build())))
            .map_err(runtime_error_to_py)?;

        let handle_options = bridge_ctypes::CffiHandleTableOptions::for_in_process();
        let baml_value = external_to_outbound(&result, &handle_options).map_err(|e| {
            pyo3::PyErr::new::<BamlInvalidArgumentError, _>(format!("Failed to encode result: {e}"))
        })?;

        Ok(baml_value.encode_to_vec())
    }
}

/// Decode protobuf-encoded function arguments into `BexArgs`.
fn decode_args(args_proto: &[u8], function_name: &str) -> PyResult<bex_project::BexArgs> {
    let args =
        bridge_ctypes::baml_core::cffi::CallFunctionArgs::decode(args_proto).map_err(|e| {
            pyo3::PyErr::new::<BamlInvalidArgumentError, _>(format!(
                "Failed to decode arguments for function '{function_name}': {e}"
            ))
        })?;

    let kwargs = kwargs_to_bex_values(args.kwargs, &HANDLE_TABLE).map_err(|e| {
        pyo3::PyErr::new::<BamlInvalidArgumentError, _>(format!(
            "Failed to convert arguments for function '{function_name}': {e}"
        ))
    })?;

    Ok(kwargs.into())
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
    let bex = bridge_cffi::engine::get_runtime().map_err(|e| match e {
        bridge_cffi::BridgeError::NotInitialized => pyo3::PyErr::new::<BamlError, _>(
            "BAML runtime has not been initialized — did baml_sdk/__init__.py fail to import?",
        ),
        other => bridge_error_to_py(other),
    })?;
    Ok(BamlRuntime { bex })
}
