use baml_types::BamlValue;
use pyo3::prelude::{pymethods, PyResult};
use pyo3::{PyObject, Python};
use pythonize::pythonize;

crate::lang_wrapper!(FunctionResult, baml_runtime::FunctionResult);

#[pymethods]
impl FunctionResult {
    fn __str__(&self) -> String {
        format!("{:#}", self.inner)
    }

    fn is_ok(&self) -> bool {
        self.inner.parsed_content().is_ok()
    }

    fn parsed(&self, py: Python<'_>) -> PyResult<PyObject> {
        let parsed = self
            .inner
            .parsed_content()
            .map_err(crate::BamlError::from_anyhow)?;

        Ok(pythonize(py, &BamlValue::from(parsed))?)
    }
}
