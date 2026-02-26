//! BamlRuntime PyO3 class - wraps `Arc<dyn Bex>`.

use std::sync::Arc;

use bex_project::Bex;
use bridge_ctypes::{HANDLE_TABLE, external_to_baml_value, kwargs_to_bex_values};
use prost::Message;
use pyo3::{
    PyObject, Python,
    prelude::{PyResult, pymethods},
    pyclass,
};

use crate::{
    abort_controller::AbortController,
    errors::{BamlInvalidArgumentError, bridge_error_to_py, runtime_error_to_py},
    types::collector::Collector,
};

/// The main BAML runtime, wrapping a `dyn Bex` instance.
#[pyclass]
pub struct BamlRuntime {
    bex: Arc<dyn Bex>,
}

#[pymethods]
impl BamlRuntime {
    /// Create a runtime from in-memory BAML source files.
    ///
    /// # Arguments
    /// * `root_path` - Root path for BAML files
    /// * `files` - Map of filename to file content
    #[staticmethod]
    fn initialize(
        root_path: String,
        files: std::collections::HashMap<String, String>,
    ) -> PyResult<Self> {
        match bridge_cffi::engine::initialize_runtime(&root_path, files) {
            Ok(bex) => Ok(BamlRuntime { bex }),
            Err(e) => Err(bridge_error_to_py(e)),
        }
    }

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
    ) -> PyResult<PyObject> {
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

            let handle_options = bridge_ctypes::HandleTableOptions::for_in_process();
            let baml_value = external_to_baml_value(&result, &handle_options).map_err(|e| {
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
            .allow_threads(|| {
                rt.block_on(bex.call_function(&function_name, kwargs, call_ctx.build()))
            })
            .map_err(runtime_error_to_py)?;

        let handle_options = bridge_ctypes::HandleTableOptions::for_in_process();
        let baml_value = external_to_baml_value(&result, &handle_options).map_err(|e| {
            pyo3::PyErr::new::<BamlInvalidArgumentError, _>(format!("Failed to encode result: {e}"))
        })?;

        Ok(baml_value.encode_to_vec())
    }
}

/// Decode protobuf-encoded function arguments into `BexArgs`.
fn decode_args(args_proto: &[u8], function_name: &str) -> PyResult<bex_project::BexArgs> {
    let args = bridge_ctypes::baml::cffi::CallFunctionArgs::decode(args_proto).map_err(|e| {
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
