use pyo3::{
    prelude::pymethods,
    types::{PyByteArray, PyDict, PyDictMethods},
    Py, PyObject, PyResult, Python,
};

use super::log_collector::serde_value_to_py;
use crate::errors::BamlError;

crate::lang_wrapper!(
    HTTPRequest,
    baml_types::tracing::events::HTTPRequest,
    clone_safe
);

crate::lang_wrapper!(HTTPBody, baml_types::tracing::events::HTTPBody, clone_safe);

#[pymethods]
impl HTTPRequest {
    #[getter]
    pub fn id(&self) -> String {
        self.inner.id.to_string()
    }

    #[getter]
    pub fn body(&self) -> HTTPBody {
        // TODO: Avoid clone.
        HTTPBody::from(self.inner.body.clone())
    }

    pub fn __repr__(&self) -> String {
        format!(
            "HTTPRequest(url={}, method={}, headers={}, body={})",
            self.inner.url,
            self.inner.method,
            serde_json::to_string_pretty(&self.inner.headers()).unwrap(),
            serde_json::to_string_pretty(&self.inner.body.as_serde_value()).unwrap()
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
        for (k, v) in self.inner.headers() {
            // serde_json::Value::to_string includes quotes around the
            // string, we only want the string content not the quotes.
            dict.set_item(k, v)?;
        }
        Ok(dict.into())
    }
}

#[pymethods]
impl HTTPBody {
    pub fn raw<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, PyByteArray> {
        PyByteArray::new(py, self.inner.raw())
    }

    pub fn text(&self) -> PyResult<String> {
        self.inner
            .text()
            .map(String::from)
            .map_err(BamlError::from_anyhow)
    }

    pub fn json<'py>(&self, py: Python<'py>) -> PyResult<PyObject> {
        serde_value_to_py(py, &self.inner.json().map_err(BamlError::from_anyhow)?)
    }
}
