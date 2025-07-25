use std::collections::HashMap;

use pyo3::{
    prelude::{pymethods, PyResult},
    PyObject, PyRefMut, Python,
};
use tokio_util::sync::CancellationToken;

use super::{function_results::FunctionResult, runtime_ctx_manager::RuntimeContextManager};
use crate::errors::BamlError;

crate::lang_wrapper!(
    FunctionResultStream,
    baml_runtime::FunctionResultStream, thread_safe,
    on_event: Option<PyObject>,
    tb: Option<baml_runtime::type_builder::TypeBuilder>,
    cb: Option<baml_runtime::client_registry::ClientRegistry>,
    env_vars: HashMap<String, String>,
    cancellation_token: CancellationToken
);

crate::lang_wrapper!(
    SyncFunctionResultStream,
    baml_runtime::FunctionResultStream, sync_thread_safe,
    on_event: Option<PyObject>,
    tb: Option<baml_runtime::type_builder::TypeBuilder>,
    cb: Option<baml_runtime::client_registry::ClientRegistry>,
    env_vars: HashMap<String, String>,
    cancellation_token: CancellationToken
);

impl FunctionResultStream {
    pub(crate) fn new(
        inner: baml_runtime::FunctionResultStream,
        event: Option<PyObject>,
        tb: Option<baml_runtime::type_builder::TypeBuilder>,
        cb: Option<baml_runtime::client_registry::ClientRegistry>,
        env_vars: HashMap<String, String>,
    ) -> Self {
        Self {
            inner: std::sync::Arc::new(tokio::sync::Mutex::new(inner)),
            on_event: event,
            tb,
            cb,
            env_vars,
            cancellation_token: CancellationToken::new(),
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
    ) -> Self {
        Self {
            inner: std::sync::Arc::new(std::sync::Mutex::new(inner)),
            on_event: event,
            tb,
            cb,
            env_vars,
            cancellation_token: CancellationToken::new(),
        }
    }
}

#[pymethods]
impl FunctionResultStream {
    /// Cancel the stream processing.
    /// This will stop any ongoing stream processing and clean up resources.
    /// Also cancels the underlying Rust HTTP requests.
    fn cancel(&self) -> PyResult<()> {
        self.cancellation_token.cancel();
        
        // Also cancel the underlying Rust stream
        let inner = self.inner.clone();
        let token = self.cancellation_token.clone();
        tokio::spawn(async move {
            if let Ok(mut stream) = inner.try_lock() {
                stream.set_cancellation_token(token);
                stream.cancel();
            }
        });
        
        Ok(())
    }

    /// Check if the stream has been cancelled
    fn is_cancelled(&self) -> bool {
        self.cancellation_token.is_cancelled()
    }

    fn on_event(&mut self, py: Python, func: Option<PyObject>) -> PyResult<()> {
        self.on_event = func;
        Ok(())
    }

    fn done(&self, py: Python, ctx_manager: &RuntimeContextManager) -> PyResult<PyObject> {
        let inner = self.inner.clone();
        let cancellation_token = self.cancellation_token.clone();

        let on_event = self.on_event.clone();
        let tb = self.tb.clone();
        let cb = self.cb.clone();
        let env_vars = self.env_vars.clone();
        let ctx_manager = ctx_manager.inner.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            // Set the cancellation token on the stream
            if let Ok(mut stream) = inner.try_lock() {
                stream.set_cancellation_token(cancellation_token);
            }

            let on_event_fn = on_event.map(|callback| {
                move |result: baml_runtime::FunctionResult| {
                    Python::with_gil(|py| {
                        let py_result = FunctionResult::from(result);
                        if let Err(e) = callback.call1(py, (py_result,)) {
                            eprintln!("Error calling Python callback: {:?}", e);
                        }
                    });
                }
            });

            let result = inner
                .lock()
                .await
                .run(
                    None::<fn()>,
                    on_event_fn,
                    &ctx_manager,
                    tb.as_ref(),
                    cb.as_ref(),
                    env_vars,
                )
                .await;

            Python::with_gil(|py| {
                result
                    .0
                    .map(FunctionResult::from)
                    .map_err(BamlError::from_anyhow)
                    .map(|r| r.into_py(py))
            })
        })
    }
}

#[pymethods]
impl SyncFunctionResultStream {
    /// Cancel the stream processing (sync version).
    fn cancel(&self) -> PyResult<()> {
        self.cancellation_token.cancel();
        
        // Also cancel the underlying Rust stream
        if let Ok(mut stream) = self.inner.try_lock() {
            stream.set_cancellation_token(self.cancellation_token.clone());
            stream.cancel();
        }
        
        Ok(())
    }

    /// Check if the stream has been cancelled
    fn is_cancelled(&self) -> bool {
        self.cancellation_token.is_cancelled()
    }

    fn on_event(&mut self, py: Python, func: Option<PyObject>) -> PyResult<()> {
        self.on_event = func;
        Ok(())
    }

    fn done(&self, py: Python, ctx_manager: &RuntimeContextManager) -> PyResult<FunctionResult> {
        let inner = self.inner.clone();
        let cancellation_token = self.cancellation_token.clone();

        let on_event = self.on_event.clone();
        let tb = self.tb.clone();
        let cb = self.cb.clone();
        let env_vars = self.env_vars.clone();
        let ctx_manager = ctx_manager.inner.clone();

        // Set the cancellation token on the stream
        if let Ok(mut stream) = inner.try_lock() {
            stream.set_cancellation_token(cancellation_token);
        }

        let on_event_fn = on_event.map(|callback| {
            move |result: baml_runtime::FunctionResult| {
                Python::with_gil(|py| {
                    let py_result = FunctionResult::from(result);
                    if let Err(e) = callback.call1(py, (py_result,)) {
                        eprintln!("Error calling Python callback: {:?}", e);
                    }
                });
            }
        });

        let result = inner
            .lock()
            .unwrap()
            .run_sync(
                None::<fn()>,
                on_event_fn,
                &ctx_manager,
                tb.as_ref(),
                cb.as_ref(),
                env_vars,
            );

        result
            .0
            .map(FunctionResult::from)
            .map_err(BamlError::from_anyhow)
    }
}
        tb: Option<baml_runtime::type_builder::TypeBuilder>,
        cb: Option<baml_runtime::client_registry::ClientRegistry>,
        env_vars: HashMap<String, String>,
    ) -> Self {
        Self {
            inner: std::sync::Arc::new(std::sync::Mutex::new(inner)),
            on_event: event,
            tb,
            cb,
            env_vars,
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

    fn done(&self, py: Python<'_>, ctx: &RuntimeContextManager) -> PyResult<PyObject> {
        let inner = self.inner.clone();

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

        let ctx_mng = ctx.inner.clone();
        let tb = self.tb.clone();
        let cb = self.cb.clone();
        let env_vars = self.env_vars.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let ctx_mng = ctx_mng;
            let mut locked = inner.lock().await;
            let (res, _) = locked
                .run(
                    None::<fn()>,
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

    fn done(&self, ctx: &RuntimeContextManager) -> PyResult<FunctionResult> {
        let inner = self.inner.clone();

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

        let ctx_mng = ctx.inner.clone();
        let tb = self.tb.clone();
        let cb = self.cb.clone();
        let env_vars = self.env_vars.clone();
        let ctx_mng = ctx_mng;
        let mut locked = inner.lock().unwrap();
        let (res, _) = locked.run_sync(
            None::<fn()>,
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
