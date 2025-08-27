use std::collections::HashMap;

use pyo3::{
    prelude::{pymethods, PyResult},
    PyErr, PyObject, PyRefMut, Python,
};

use super::{function_results::FunctionResult, runtime_ctx_manager::RuntimeContextManager};
use crate::errors::BamlError;

crate::lang_wrapper!(
    FunctionResultStream,
    baml_runtime::FunctionResultStream, optional,
    on_event: Option<PyObject>,
    tb: Option<baml_runtime::type_builder::TypeBuilder>,
    cb: Option<baml_runtime::client_registry::ClientRegistry>,
    env_vars: HashMap<String, String>,
    on_tick: Option<PyObject>
);

crate::lang_wrapper!(
    SyncFunctionResultStream,
    baml_runtime::FunctionResultStream, optional,
    on_event: Option<PyObject>,
    tb: Option<baml_runtime::type_builder::TypeBuilder>,
    cb: Option<baml_runtime::client_registry::ClientRegistry>,
    env_vars: HashMap<String, String>,
    on_tick: Option<PyObject>
);

impl FunctionResultStream {
    pub(crate) fn new(
        inner: baml_runtime::FunctionResultStream,
        event: Option<PyObject>,
        tb: Option<baml_runtime::type_builder::TypeBuilder>,
        cb: Option<baml_runtime::client_registry::ClientRegistry>,
        env_vars: HashMap<String, String>,
        on_tick: Option<PyObject>,
    ) -> Self {
        Self {
            inner: Some(inner),
            on_event: event,
            tb,
            cb,
            env_vars,
            on_tick,
        }
    }
}

impl SyncFunctionResultStream {
    pub(crate) fn new(
        inner: baml_runtime::FunctionResultStream,
        event: Option<PyObject>,
        tb: Option<baml_runtime::type_builder::TypeBuilder>,
        cb: Option<baml_runtime::client_registry::ClientRegistry>,
        env_vars: HashMap<String, String>,
        on_tick: Option<PyObject>,
    ) -> Self {
        Self {
            inner: Some(inner),
            on_event: event,
            tb,
            cb,
            env_vars,
            on_tick,
        }
    }
}

#[pymethods]
impl FunctionResultStream {
    fn __str__(&self) -> String {
        "FunctionResultStream".to_string()
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

    fn done(&mut self, py: Python<'_>, ctx: &RuntimeContextManager) -> PyResult<PyObject> {
        let Some(inner) = self.inner.take() else {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Stream already finished",
            ));
        };

        let on_event = self.on_event.as_ref().map(|cb| {
            let cb = cb.clone_ref(py);
            move |event| {
                let partial = FunctionResult::from(event);
                let res = Python::with_gil(|py| cb.call1(py, (partial,))).map(|_| ());
                if let Err(e) = res {
                    log::error!("Error calling on_event callback: {e:?}");
                }
            }
        });

        let on_tick_callback = self.on_tick.as_ref().map(|tick_cb| {
            let tick_cb = tick_cb.clone_ref(py);
            move || {
                Python::with_gil(|py| {
                    let res = tick_cb.call0(py);
                    if let Err(e) = res {
                        e.display(py);
                    }
                });
            }
        });

        let ctx_mng = ctx.inner.clone();
        let tb = self.tb.clone();
        let cb = self.cb.clone();
        let env_vars = self.env_vars.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let ctx_mng = ctx_mng;
            let mut inner = inner;
            let (res, _) = inner
                .run(
                    on_tick_callback,
                    on_event,
                    &ctx_mng,
                    tb.as_ref(),
                    cb.as_ref(),
                    env_vars,
                )
                .await;
            res.map(FunctionResult::from)
                .map_err(BamlError::from_anyhow)
        })
        .map(pyo3::Bound::into)
    }
}

#[pymethods]
impl SyncFunctionResultStream {
    fn __str__(&self) -> String {
        "SyncFunctionResultStream".to_string()
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

    fn done(&mut self, ctx: &RuntimeContextManager) -> PyResult<FunctionResult> {
        let Some(inner) = self.inner.take() else {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Stream already finished",
            ));
        };

        let on_event = self.on_event.as_ref().map(|cb| {
            let cb = Python::with_gil(|py| cb.clone_ref(py));
            move |event| {
                let partial = FunctionResult::from(event);
                let res = Python::with_gil(|py| cb.call1(py, (partial,))).map(|_| ());
                if let Err(e) = res {
                    log::error!("Error calling on_event callback: {e:?}");
                }
            }
        });

        let on_tick_callback = self.on_tick.as_ref().map(|tick_cb| {
            let tick_cb = Python::with_gil(|py| tick_cb.clone_ref(py));
            move || {
                Python::with_gil(|py| {
                    // For now, we pass "Unknown" as the reason
                    // In a full implementation, we'd get the last event from the collector
                    tick_cb.call1(py, ("Unknown", py.None())).ok();
                });
            }
        });

        let ctx_mng = ctx.inner.clone();
        let tb = self.tb.clone();
        let cb = self.cb.clone();
        let env_vars = self.env_vars.clone();
        let ctx_mng = ctx_mng;
        let mut inner = inner;
        let (res, _) = inner.run_sync(
            on_tick_callback,
            on_event,
            &ctx_mng,
            tb.as_ref(),
            cb.as_ref(),
            env_vars,
        );
        res.map(FunctionResult::from)
            .map_err(BamlError::from_anyhow)
    }
}
