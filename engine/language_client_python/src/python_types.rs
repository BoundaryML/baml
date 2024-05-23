use baml_runtime::tracing::TracingSpan;
use baml_runtime::{BamlRuntime, RuntimeContext};
use baml_types::BamlValue;
use pyo3::prelude::{pyclass, pymethods, PyAnyMethods, PyModule, PyResult};
use pyo3::types::{PyString, PyType};
use pyo3::{Bound, Py, PyAny, PyObject, PyRefMut, Python, ToPyObject};
use pythonize::{depythonize_bound, pythonize};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::parse_py_type::parse_py_type;
use crate::{BamlError, BamlRuntimeFfi, RuntimeContextManagerPy};
use baml_runtime::runtime_interface::ExperimentalTracingInterface;

#[pyclass]
pub struct FunctionResult {
    inner: baml_runtime::FunctionResult,
}

impl FunctionResult {
    pub fn new(inner: baml_runtime::FunctionResult) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl FunctionResult {
    fn __str__(&self) -> String {
        format!("{:#}", self.inner)
    }

    fn parsed(&self, py: Python<'_>) -> PyResult<PyObject> {
        let parsed = self
            .inner
            .parsed_content()
            .map_err(crate::BamlError::from_anyhow)?;

        Ok(pythonize(py, &BamlValue::from(parsed))?)
    }
}

#[pyclass]
pub struct FunctionResultStream {
    inner: Arc<Mutex<baml_runtime::FunctionResultStream>>,
    on_event: Option<PyObject>,
}

impl FunctionResultStream {
    pub fn new(inner: baml_runtime::FunctionResultStream, on_event: Option<PyObject>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(inner)),
            on_event,
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

    fn done(&self, py: Python<'_>, ctx: &RuntimeContextManagerPy) -> PyResult<PyObject> {
        let inner = self.inner.clone();

        let on_event = self.on_event.as_ref().map(|cb| {
            let cb = cb.clone_ref(py);
            move |event| {
                let partial = FunctionResult::new(event);
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
            res.map(FunctionResult::new).map_err(BamlError::from_anyhow)
        })
        .map(|f| f.into())
    }
}

// Use this once we update pyo3. Current version doesn't support this struct enum.
// pub enum BamlImagePy {
//     // struct
//     Url { url: String },
//     Base64 { base64: String },
// }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename = "Image")]
#[pyclass(name = "Image")]
pub struct BamlImagePy {
    pub(crate) url: Option<String>,
    pub(crate) base64: Option<String>,
    pub(crate) media_type: Option<String>,
}

// Implement constructor for BamlImage
#[pymethods]
impl BamlImagePy {
    #[new]
    fn new(url: Option<String>, base64: Option<String>, media_type: Option<String>) -> Self {
        BamlImagePy {
            url,
            base64,
            media_type,
        }
    }

    #[getter]
    pub fn get_url(&self) -> PyResult<Option<String>> {
        Ok(self.url.clone())
    }

    #[getter]
    pub fn get_base64(&self) -> PyResult<Option<String>> {
        Ok(self.base64.clone())
    }

    #[setter]
    pub fn set_url(&mut self, url: Option<String>) {
        self.url = url;
    }

    #[setter]
    pub fn set_base64(&mut self, base64: Option<String>) {
        self.base64 = base64;
    }

    pub fn __repr__(&self) -> String {
        let url_repr = match &self.url {
            Some(url) => format!("Optional(\"{}\")", url),
            None => "None".to_string(),
        };
        let base64_repr = match &self.base64 {
            Some(base64) => format!("Optional(\"{}\")", base64),
            None => "None".to_string(),
        };
        format!("Image(url={}, base64={})", url_repr, base64_repr)
    }

    // Makes it work with Pydantic
    #[classmethod]
    pub fn __get_pydantic_core_schema__(
        _cls: Bound<'_, PyType>,
        _source_type: Bound<'_, PyAny>,
        _handler: Bound<'_, PyAny>,
    ) -> PyResult<PyObject> {
        Python::with_gil(|py| {
            let code = r#"
from pydantic_core import core_schema

def get_schema():
    # No validation
    return core_schema.any_schema()

ret = get_schema()
    "#;
            // py.run(code, None, Some(ret_dict));
            let fun: Py<PyAny> = PyModule::from_code_bound(py, code, "", "")?
                .getattr("ret")?
                .into();
            Ok(fun.to_object(py)) // Return the PyObject
        })
    }

    pub fn __eq__(&self, other: &Self) -> bool {
        self == other
    }
}

#[pyclass(name = "BamlSpan")]
pub struct BamlSpanPy {
    span: Option<TracingSpan>,
    ctx: RuntimeContext,
    rt: Arc<BamlRuntime>,
}

#[pymethods]
impl BamlSpanPy {
    #[staticmethod]
    fn new(
        py: Python<'_>,
        runtime: &BamlRuntimeFfi,
        function_name: &str,
        args: PyObject,
        ctx: &RuntimeContextManagerPy,
    ) -> PyResult<Self> {
        let locals = pyo3_asyncio::tokio::get_current_locals(py)?;
        locals.context(py);

        let args = parse_py_type(args.into_bound(py).to_object(py))?;
        let Some(args_map) = args.as_map() else {
            return Err(BamlError::new_err("Failed to parse args"));
        };

        let (span, ctx) = runtime
            .internal
            .start_span(function_name, &args_map, &ctx.inner);
        Ok(Self {
            span,
            ctx,
            rt: runtime.internal.clone(),
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
            .span
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
