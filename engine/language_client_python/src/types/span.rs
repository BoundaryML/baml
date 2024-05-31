use baml_runtime::runtime_interface::ExperimentalTracingInterface;
use baml_types::BamlValue;
use pyo3::prelude::{pymethods, PyResult};
use pyo3::{PyObject, Python, ToPyObject};

use crate::parse_py_type::parse_py_type;
use crate::BamlError;

use super::runtime::BamlRuntime;
use super::runtime_ctx_manager::RuntimeContextManager;

crate::lang_wrapper!(BamlSpan,
  Option<Option<baml_runtime::tracing::TracingSpan>>,
  no_from,
  rt: std::sync::Arc<baml_runtime::BamlRuntime>
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
    ) -> PyResult<Self> {
        let args = parse_py_type(args.into_bound(py).to_object(py), true)?
            .unwrap_or(BamlValue::Map(Default::default()));
        let Some(args_map) = args.as_map() else {
            return Err(BamlError::new_err("Failed to parse args"));
        };

        let (span, _) = runtime
            .inner
            .start_span(function_name, &args_map, &ctx.inner);

        log::trace!("Starting span: {:#?} for {:?}\n", span, function_name);
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
    ) -> PyResult<Option<String>> {
        log::info!("Finishing span: {:?}", self.inner);
        let result = parse_py_type(result.into_bound(py).to_object(py), true)?;

        let span = self
            .inner
            .take()
            .ok_or_else(|| BamlError::new_err("Span already finished"))?;

        self.rt
            .finish_span(span, result, &ctx.inner)
            .map_err(BamlError::from_anyhow)
            .map(|u| u.map(|id| id.to_string()))
    }
}
