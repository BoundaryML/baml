use std::collections::HashMap;

use baml_runtime::runtime_interface::ExperimentalTracingInterface;
use baml_types::BamlValue;
use pyo3::{
    prelude::{pymethods, PyResult},
    types::{PyAnyMethods, PyTypeMethods},
    IntoPyObjectExt, PyObject, Python,
};

use super::runtime_ctx_manager::RuntimeContextManager;
use crate::{
    errors::{BamlError, BamlInvalidArgumentError},
    parse_py_type::parse_py_type,
    runtime::BamlRuntime,
};

crate::lang_wrapper!(
  BamlSpan,
  Option<baml_runtime::tracing::TracingCall>,
  no_from,
  rt: std::sync::Arc<crate::runtime::CoreBamlRuntime>
);

#[pymethods]
impl BamlSpan {
    #[staticmethod]
    fn new(
        py: Python<'_>,
        runtime: &BamlRuntime,
        function_name: &str,
        args: PyObject,
        ctx: &RuntimeContextManager,
        env_vars: HashMap<String, String>,
    ) -> PyResult<Self> {
        let args = parse_py_type(args.into_bound(py).into_py_any(py)?, true)?
            .unwrap_or(BamlValue::Map(Default::default()));
        let Some(args_map) = args.as_map() else {
            return Err(BamlInvalidArgumentError::new_err("Failed to parse args"));
        };

        let span = runtime
            .inner
            .start_call(function_name, args_map, &ctx.inner, &env_vars);

        log::trace!("Starting span: {span:#?} for {function_name:?}\n");
        Ok(Self {
            inner: Some(span),
            rt: runtime.inner.clone(),
        })
    }

    // method to finish
    fn finish(
        &mut self,
        py: Python<'_>,
        result: PyObject,
        ctx: &RuntimeContextManager,
        env_vars: HashMap<String, String>,
    ) -> PyResult<Option<String>> {
        log::trace!("Finishing span: {:?}", self.inner);

        // Check if the result is a Python exception
        let result = if let Ok(exception_type) = py
            .import("builtins")
            .and_then(|m| m.getattr("BaseException"))
        {
            let bound_result = result.bind(py);
            if bound_result.is_instance(&exception_type).unwrap_or(false) {
                // It's an exception - create a special marker class to signal error to tracer
                let exc_message = if let Ok(traceback_module) = py.import("traceback") {
                    // Try to format the exception with traceback
                    if let Ok(formatted) = traceback_module.call_method1(
                        "format_exception",
                        (
                            bound_result.get_type(),
                            bound_result,
                            bound_result.getattr("__traceback__").ok(),
                        ),
                    ) {
                        if let Ok(lines) = formatted.extract::<Vec<String>>() {
                            lines.join("")
                        } else {
                            format!("{bound_result:?}")
                        }
                    } else {
                        format!("{bound_result:?}")
                    }
                } else {
                    format!("{bound_result:?}")
                };

                // Get exception type name
                let type_name = bound_result
                    .get_type()
                    .name()
                    .ok()
                    .and_then(|n| n.extract::<String>().ok())
                    .unwrap_or_else(|| "Exception".to_string());

                // Create a special marker class that finish_call can recognize as an error
                let mut error_map = baml_types::BamlMap::new();
                error_map.insert("message".to_string(), BamlValue::String(exc_message));
                error_map.insert("type".to_string(), BamlValue::String(type_name));
                Some(BamlValue::Class(
                    "__PythonException__".to_string(),
                    error_map,
                ))
            } else {
                // Not an exception, parse normally
                parse_py_type(result.into_bound(py).into_py_any(py)?, true)
                    .ok()
                    .flatten()
            }
        } else {
            // Couldn't import builtins, parse normally
            parse_py_type(result.into_bound(py).into_py_any(py)?, true)
                .ok()
                .flatten()
        };

        let call = self
            .inner
            .take()
            .ok_or_else(|| BamlError::new_err("Span already finished"))?;

        self.rt
            .finish_call(call, result, &ctx.inner, &env_vars)
            .map_err(BamlError::from_anyhow)
            .map(|u| Some(u.to_string()))
    }
}
