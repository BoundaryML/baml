use pyo3::prelude::{pymethods, PyResult};
use pyo3::{PyObject, PyRefMut, Python};

use crate::BamlError;

use super::function_results::FunctionResultPy;
use super::runtime_ctx_manager::RuntimeContextManagerPy;

crate::lang_wrapper!(
    FunctionResultStreamPy,
    baml_runtime::FunctionResultStream, thread_safe,
    on_event: Option<PyObject>
);

impl FunctionResultStreamPy {
    pub(super) fn new(inner: baml_runtime::FunctionResultStream, event: Option<PyObject>) -> Self {
        Self {
            inner: std::sync::Arc::new(tokio::sync::Mutex::new(inner)),
            on_event: event,
        }
    }
}

#[pymethods]
impl FunctionResultStreamPy {
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

    fn done(&self, py: Python<'_>, ctx: &RuntimeContextManagerPy) -> PyResult<PyObject> {
        let inner = self.inner.clone();

        let on_event = self.on_event.as_ref().map(|cb| {
            let cb = cb.clone_ref(py);
            move |event| {
                let partial = FunctionResultPy::from(event);
                let res = Python::with_gil(|py| cb.call1(py, (partial,))).map(|_| ());
                if let Err(e) = res {
                    log::error!("Error calling on_event callback: {:?}", e);
                }
            }
        });

        let ctx_mng = ctx.inner.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            let ctx_mng = ctx_mng;
            let mut locked = inner.lock().await;
            let (res, _) = locked.run(on_event, &ctx_mng).await;
            res.map(FunctionResultPy::from)
                .map_err(BamlError::from_anyhow)
        })
        .map(|f| f.into())
    }
}
