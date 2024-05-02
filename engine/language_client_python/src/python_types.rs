use pyo3::prelude::{pyclass, pymethods, pymodule, PyModule, PyResult};
use pyo3::{create_exception, PyErr, PyObject, Python};
use pythonize::pythonize;

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

        Ok(pythonize(py, parsed)?)
    }
}
