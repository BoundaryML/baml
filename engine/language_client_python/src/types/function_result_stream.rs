use pyo3::prelude::{pymethods, PyResult};
use pyo3::{PyObject, PyRefMut, Python};

use crate::BamlError;

use super::function_results::FunctionResult;
use super::runtime_ctx_manager::RuntimeContextManager;

crate::lang_wrapper!(
    FunctionResultStream,
    baml_runtime::FunctionResultStream, thread_safe,
    on_event: Option<PyObject>,
    tb: Option<baml_runtime::type_builder::TypeBuilder>
);

impl FunctionResultStream {
    pub(crate) fn new(
        inner: baml_runtime::FunctionResultStream,
        event: Option<PyObject>,
        tb: Option<baml_runtime::type_builder::TypeBuilder>,
    ) -> Self {
        Self {
            inner: std::sync::Arc::new(tokio::sync::Mutex::new(inner)),
            on_event: event,
            tb,
        }
    }
}

#[pymethods]
impl FunctionResultStream {
    fn __str__(&self) -> String {
        format!("FunctionResultStream")
    }

    /// Set the callback to be called when an event is received
    ///
    /// Callback will take an instance of FunctionResult
    fn on_event<'p>(
        mut slf: PyRefMut<'p, Self>,
        py: Python<'p>,
        on_event_cb: PyObject,
    ) -> PyRefMut<'p, Self> {
        slf.on_event = Some(on_event_cb.clone_ref(py));

        slf
    }

    fn done(&self, py: Python<'_>, ctx: &RuntimeContextManager) -> PyResult<PyObject> {
        let inner = self.inner.clone();

        let on_event = self.on_event.as_ref().map(|cb| {
            let cb = cb.clone_ref(py);
            move |event| {
                let partial = FunctionResult::from(event);
                let res = Python::with_gil(|py| cb.call1(py, (partial,))).map(|_| ());
                if let Err(e) = res {
                    log::error!("Error calling on_event callback: {:?}", e);
                }
            }
        });

        let ctx_mng = ctx.inner.clone();
        let tb = self.tb.as_ref().map(|tb| tb.clone());
        pyo3_asyncio::tokio::future_into_py(py, async move {
            let ctx_mng = ctx_mng;
            let mut locked = inner.lock().await;
            let (res, _) = locked.run(on_event, &ctx_mng, tb.as_ref()).await;
            res.map(FunctionResult::from)
                .map_err(BamlError::from_anyhow)
        })
        .map(|f| f.into())
    }
}
