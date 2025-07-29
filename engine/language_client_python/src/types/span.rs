use std::collections::HashMap;

use baml_runtime::runtime_interface::ExperimentalTracingInterface;
use baml_types::BamlValue;
use pyo3::{
    prelude::{pymethods, PyResult},
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
        let result = parse_py_type(result.into_bound(py).into_py_any(py)?, true)?;

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
