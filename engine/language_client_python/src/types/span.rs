use baml_runtime::runtime_interface::ExperimentalTracingInterface;
use baml_types::BamlValue;
use pyo3::prelude::{pymethods, PyResult};
use pyo3::{PyObject, Python, ToPyObject};

use crate::parse_py_type::parse_py_type;
use crate::BamlError;

use super::runtime::BamlRuntime;
use super::runtime_ctx_manager::RuntimeContextManager;

crate::lang_wrapper!(BamlSpan,
  Option<baml_runtime::tracing::TracingSpan>,
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
        ctx: &RuntimeContextManager,
    ) -> PyResult<PyObject> {
        let result = parse_py_type(result.into_bound(py).to_object(py), true)?;

        let span = self
            .inner
            .take()
            .ok_or_else(|| BamlError::new_err("Span already finished"))?;

        let runtime = self.rt.clone();
        let ctx = ctx.inner.clone();
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let result = runtime
                .finish_span(span, result, &ctx)
                .await
                .map_err(BamlError::from_anyhow)
                .map(|u| u.map(|id| id.to_string()))?;
            Ok(result)
        })
        .map(|f| f.into())
    }

    fn finish_sync(
        &mut self,
        py: Python<'_>,
        result: PyObject,
        ctx: &RuntimeContextManager,
    ) -> PyResult<PyObject> {
        let result = parse_py_type(result, true)?;

        // Acquire the span from the internal storage
        let span = self
            .inner
            .take()
            .ok_or_else(|| BamlError::new_err("Span already finished"))?;

        // Using block_on to run the asynchronous function in a synchronous way
        // You need an instance of Runtime to call block_on
        let tokio_runtime = tokio::runtime::Runtime::new().unwrap();
        let runtime = self.rt.clone();
        let ctx = ctx.inner.clone();

        let finish_span_future = runtime.finish_span(span, result, &ctx);

        // Block the current thread until the asynchronous code completes
        let result = tokio_runtime
            .block_on(finish_span_future)
            .map_err(BamlError::from_anyhow)
            .and_then(|u| {
                u.map(|id| id.to_string())
                    .ok_or_else(|| BamlError::new_err("No ID returned from finish_span"))
            })?;

        Ok(result.to_object(py))
    }
}
