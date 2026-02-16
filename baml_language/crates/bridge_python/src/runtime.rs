//! BamlRuntime PyO3 class - wraps Arc<BexEngine>.

use std::sync::Arc;

use bex_engine::BexEngine;
use bridge_ctypes::{external_to_cffi_value, kwargs_to_bex_values};
use prost::Message;
use pyo3::{
    PyObject, Python,
    prelude::{PyResult, pymethods},
    pyclass,
};
use sys_native::SysOpsExt;

use crate::{
    errors::{BamlInvalidArgumentError, engine_error_to_py},
    types::collector::Collector,
};

/// The main BAML runtime, wrapping a `BexEngine` instance.
#[pyclass]
pub struct BamlRuntime {
    engine: Arc<BexEngine>,
}

#[pymethods]
impl BamlRuntime {
    /// Create a runtime from in-memory BAML source files.
    ///
    /// # Arguments
    /// * `root_path` - Root path for BAML files
    /// * `files` - Map of filename to file content
    #[staticmethod]
    fn from_files(
        root_path: String,
        files: std::collections::HashMap<String, String>,
    ) -> PyResult<Self> {
        let engine = bex_factory::new_engine(&root_path, &files, bex_factory::SysOps::native())
            .map_err(|e| pyo3::PyErr::new::<BamlInvalidArgumentError, _>(e.to_string()))?;

        Ok(BamlRuntime { engine })
    }

    /// Call a BAML function asynchronously.
    ///
    /// # Arguments
    /// * `function_name` - Name of the BAML function to call
    /// * `args_proto` - Protobuf-encoded HostFunctionArguments bytes
    /// * `ctx` - Host span manager; if active spans exist, nests under host trace
    /// * `collectors` - Optional list of Collector objects to track this call
    #[pyo3(signature = (function_name, args_proto, ctx=None, collectors=None))]
    fn call_function<'py>(
        &self,
        py: Python<'py>,
        function_name: String,
        args_proto: Vec<u8>,
        ctx: Option<&crate::types::HostSpanManager>,
        collectors: Option<Vec<pyo3::PyRef<'py, Collector>>>,
    ) -> PyResult<PyObject> {
        let engine = self.engine.clone();

        // Decode protobuf arguments
        let args = bridge_ctypes::baml::cffi::HostFunctionArguments::decode(args_proto.as_slice())
            .map_err(|e| {
                pyo3::PyErr::new::<BamlInvalidArgumentError, _>(format!(
                    "Failed to decode arguments: {e}"
                ))
            })?;

        // Convert kwargs to BexExternalValue HashMap
        let kwargs = kwargs_to_bex_values(args.kwargs).map_err(|e| {
            pyo3::PyErr::new::<BamlInvalidArgumentError, _>(format!(
                "Failed to convert arguments: {e}"
            ))
        })?;

        // Look up function params to get the correct argument order
        let params = engine
            .function_params(&function_name)
            .map_err(engine_error_to_py)?;

        // Order args by parameter names
        let mut ordered_args = Vec::with_capacity(params.len());
        for (param_name, _param_ty) in &params {
            let param_name_str = *param_name;
            match kwargs.get(param_name_str) {
                Some(val) => {
                    ordered_args.push(val.clone());
                }
                None => {
                    return Err(pyo3::PyErr::new::<BamlInvalidArgumentError, _>(format!(
                        "Missing argument '{param_name_str}' for function '{function_name}'"
                    )));
                }
            }
        }

        // Extract host span context before releasing the GIL
        let host_ctx = ctx.and_then(|c| c.host_span_context());

        // Clone Arc refs to collectors so they can cross the async boundary.
        let collector_arcs: Vec<Arc<bex_events::Collector>> = collectors
            .as_ref()
            .map(|colls| colls.iter().map(|c| c.inner_arc()).collect())
            .unwrap_or_default();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result = engine
                .call_function(&function_name, ordered_args, host_ctx, &collector_arcs)
                .await
                .map_err(engine_error_to_py)?;

            // Encode result as protobuf
            let cffi_value = external_to_cffi_value(&result).map_err(|e| {
                pyo3::PyErr::new::<BamlInvalidArgumentError, _>(format!(
                    "Failed to encode result: {e}"
                ))
            })?;

            Ok(cffi_value.encode_to_vec())
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
    #[pyo3(signature = (function_name, args_proto, ctx=None, collectors=None))]
    fn call_function_sync(
        &self,
        py: Python<'_>,
        function_name: String,
        args_proto: Vec<u8>,
        ctx: Option<&crate::types::HostSpanManager>,
        collectors: Option<Vec<pyo3::PyRef<'_, Collector>>>,
    ) -> PyResult<Vec<u8>> {
        let engine = self.engine.clone();

        // Decode protobuf arguments
        let args = bridge_ctypes::baml::cffi::HostFunctionArguments::decode(args_proto.as_slice())
            .map_err(|e| {
                pyo3::PyErr::new::<BamlInvalidArgumentError, _>(format!(
                    "Failed to decode arguments: {e}"
                ))
            })?;

        // Convert kwargs to BexExternalValue HashMap
        let kwargs = kwargs_to_bex_values(args.kwargs).map_err(|e| {
            pyo3::PyErr::new::<BamlInvalidArgumentError, _>(format!(
                "Failed to convert arguments: {e}"
            ))
        })?;

        // Look up function params to get the correct argument order
        let params = engine
            .function_params(&function_name)
            .map_err(engine_error_to_py)?;

        // Order args by parameter names
        let mut ordered_args = Vec::with_capacity(params.len());
        for (param_name, _param_ty) in &params {
            let param_name_str = *param_name;
            match kwargs.get(param_name_str) {
                Some(val) => {
                    ordered_args.push(val.clone());
                }
                None => {
                    return Err(pyo3::PyErr::new::<BamlInvalidArgumentError, _>(format!(
                        "Missing argument '{param_name_str}' for function '{function_name}'"
                    )));
                }
            }
        }

        // Extract host span context before releasing the GIL
        let host_ctx = ctx.and_then(|c| c.host_span_context());

        let collector_arcs: Vec<Arc<bex_events::Collector>> = collectors
            .as_ref()
            .map(|colls| colls.iter().map(|c| c.inner_arc()).collect())
            .unwrap_or_default();

        let rt = baml_cffi::engine::get_tokio_runtime();

        let result = py
            .allow_threads(|| {
                rt.block_on(engine.call_function(
                    &function_name,
                    ordered_args,
                    host_ctx,
                    &collector_arcs,
                ))
            })
            .map_err(engine_error_to_py)?;

        // Encode result as protobuf
        let cffi_value = external_to_cffi_value(&result).map_err(|e| {
            pyo3::PyErr::new::<BamlInvalidArgumentError, _>(format!("Failed to encode result: {e}"))
        })?;

        Ok(cffi_value.encode_to_vec())
    }
}
