use pyo3::{
    prelude::{pymethods, Py},
    types::{PyDict, PyDictMethods},
    PyObject, PyResult, Python,
};

use super::log_collector::serde_value_to_py;

crate::lang_wrapper!(
    HTTPResponse,
    baml_types::tracing::events::HTTPResponse,
    clone_safe
);

// TODO: print each of these as actual json pretty strings or python dicts
#[pymethods]
impl HTTPResponse {
    pub fn __repr__(&self) -> String {
        format!(
            "HTTPResponse(status={}, headers={}, body={})",
            self.inner.status,
            serde_json::to_string_pretty(&self.inner.headers).unwrap(),
            serde_json::to_string_pretty(&self.inner.body).unwrap()
        )
    }

    #[getter]
    pub fn status(&self) -> u16 {
        self.inner.status
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

    // note the body may be an error string, not a dict
    #[getter]
    pub fn body(&self, py: Python<'_>) -> PyResult<PyObject> {
        serde_value_to_py(py, &self.inner.body)
    }
}
