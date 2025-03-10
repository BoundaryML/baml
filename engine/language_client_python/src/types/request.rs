use pyo3::{
    prelude::pymethods,
    types::{PyByteArray, PyDict, PyDictMethods},
    Py, PyObject, PyResult, Python,
};

use crate::errors::BamlError;

use super::log_collector::serde_value_to_py;

crate::lang_wrapper!(
    HTTPRequest,
    baml_types::tracing::events::HTTPRequest,
    clone_safe
);

crate::lang_wrapper!(HTTPBody, baml_types::tracing::events::HTTPBody, clone_safe);

#[pymethods]
impl HTTPRequest {
    #[getter]
    pub fn body(&self) -> HTTPBody {
        HTTPBody::from(self.inner.body.clone())
    }

    pub fn __repr__(&self) -> String {
        // Try to print as JSON, it that fails try to print string, otherwise
        // just array of bytes.
        let body = self
            .inner
            .body
            .json()
            .or_else(|_e| {
                self.inner
                    .body
                    .text()
                    .map(|s| serde_json::Value::String(s.into()))
            })
            .unwrap_or_else(|_e| {
                serde_json::Value::Array(
                    self.inner
                        .body
                        .raw()
                        .iter()
                        .map(|byte| serde_json::Value::from(*byte))
                        .collect(),
                )
            });

        format!(
            "HTTPRequest(url={}, method={}, headers={}, body={})",
            self.inner.url,
            self.inner.method,
            serde_json::to_string_pretty(&self.inner.headers).unwrap(),
            serde_json::to_string_pretty(&body).unwrap()
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
