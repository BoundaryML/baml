use baml_runtime::runtime_interface::ExperimentalTracingInterface;
use pyo3::prelude::{pymethods, PyResult};
use pyo3::{PyObject, Python, ToPyObject};

use crate::parse_py_type::parse_py_type;
use crate::BamlError;

use super::runtime::BamlRuntimePy;
use super::runtime_ctx_manager::RuntimeContextManagerPy;

crate::lang_wrapper!(BamlSpanPy,
  Option<baml_runtime::tracing::TracingSpan>,
  no_from,
  rt: std::sync::Arc<baml_runtime::BamlRuntime>
);

#[pymethods]
impl BamlSpanPy {
    #[staticmethod]
    fn new(
        py: Python<'_>,
        runtime: &BamlRuntimePy,
        function_name: &str,
        args: PyObject,
        ctx: &RuntimeContextManagerPy,
    ) -> PyResult<Self> {
        let args = parse_py_type(args.into_bound(py).to_object(py))?;
        let Some(args_map) = args.as_map() else {
            return Err(BamlError::new_err("Failed to parse args"));
        };

        let (span, _) = runtime
            .inner
            .start_span(function_name, &args_map, &ctx.inner);
        Ok(Self {
            inner: span,
            rt: runtime.inner.clone(),
        })
    }

    // method to finish
    fn finish(
        &mut self,
        py: Python<'_>,
        result: PyObject,
        ctx: &RuntimeContextManagerPy,
    ) -> PyResult<PyObject> {
        let result = parse_py_type(result.into_bound(py).to_object(py))?;

        let span = self
            .inner
            .take()
            .ok_or_else(|| BamlError::new_err("Span already finished"))?;

        let runtime = self.rt.clone();
        let ctx = ctx.inner.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let result = runtime
                .finish_span(span, Some(result), &ctx)
                .await
                .map_err(BamlError::from_anyhow)
                .map(|u| u.map(|id| id.to_string()))?;
            Ok(result)
        })
        .map(|f| f.into())
    }
}
