use pyo3::{prelude::*, types::PyDict};

use super::log_collector::serde_value_to_py;

crate::lang_wrapper!(
    HTTPRequest,
    baml_types::tracing::events::HTTPRequest,
    clone_safe
);

// TODO: Each time the body and headers are accessed we're running the
// conversion to Python dicts. Needs caching.
#[pymethods]
impl HTTPRequest {
    /// Return the raw JSON string, as originally stored.
    #[getter]
    pub fn body_raw(&self) -> String {
        serde_json::to_string(&self.inner.body).unwrap_or("None".to_string())
    }

    /// Parse `body` as JSON (serde_json::Value) and recursively
    /// convert it into a Python dict / list / etc.
    #[getter]
    pub fn body(&self, py: Python<'_>) -> PyResult<PyObject> {
        // Recursively convert to Python objects:
        serde_value_to_py(py, &self.inner.body)
    }

    pub fn __repr__(&self) -> String {
        format!(
            "HTTPRequest(url={}, method={}, headers={}, body={})",
            self.inner.url,
            self.inner.method,
            serde_json::to_string_pretty(&self.inner.headers).unwrap(),
            serde_json::to_string_pretty(&self.inner.body).unwrap()
        )
    }

    #[getter]
    pub fn url(&self) -> String {
        self.inner.url.clone()
    }

    #[getter]
    pub fn method(&self) -> String {
        self.inner.method.clone()
    }

    #[getter]
    pub fn headers<'py>(&self, py: Python<'py>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        if let Some(obj) = self.inner.headers.as_object() {
            for (k, v) in obj {
                dict.set_item(k, v.to_string())?;
            }
        }
        Ok(dict.into())
    }
}
