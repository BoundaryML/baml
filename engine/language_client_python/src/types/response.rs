use pyo3::{
    prelude::{pymethods, Py},
    types::{PyDict, PyDictMethods},
    PyObject, PyResult, Python,
};

use super::request::HTTPBody;
use crate::{errors::BamlError, types::log_collector::serde_value_to_py};

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
            serde_json::to_string_pretty(&self.inner.headers()).unwrap(),
            serde_json::to_string_pretty(&self.inner.body.as_serde_value()).unwrap()
        )
    }

    #[getter]
    pub fn status(&self) -> u16 {
        self.inner.status
    }

    #[getter]
    pub fn headers<'py>(&self, py: Python<'py>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        if let Some(obj) = self.inner.headers() {
            for (k, v) in obj {
                dict.set_item(k, v)?;
            }
        }
        Ok(dict.into())
    }

    #[getter]
    pub fn body(&self) -> HTTPBody {
        // TODO: Avoid clone.
        HTTPBody::from(self.inner.body.clone())
    }
}

crate::lang_wrapper!(
    SSEResponse,
    baml_types::tracing::events::SSEEvent,
    clone_safe
);

#[pymethods]
impl SSEResponse {
    #[getter]
    pub fn text(&self) -> String {
        self.inner.data.clone()
    }

    /// Attempt to parse the data as JSON if it is a valid JSON string.
    /// otherwise return None.
    pub fn json<'py>(&self, py: Python<'py>) -> PyResult<Option<PyObject>> {
        Ok(
            match serde_json::from_str(&self.inner.data).map_err(BamlError::from_anyhow) {
                Ok(v) => Some(serde_value_to_py(py, &v)?),
                Err(_) => None,
            },
        )
    }
}
