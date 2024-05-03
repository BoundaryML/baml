use std::ffi::OsString;

use pyo3::prelude::{pyclass, pymethods, pymodule, PyModule, PyResult};
use pyo3::{create_exception, PyErr, PyObject, Python};
use pythonize::pythonize;

use crate::BamlError;

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

#[derive(Clone)]
#[pyclass]
pub struct GenerateArgs {
    pub client_type: internal_baml_codegen::LanguageClientType,
    pub output_path: OsString,
}

#[pymethods]
impl GenerateArgs {
    #[staticmethod]
    #[pyo3(signature = (*, client_type, output_path))]
    fn new(client_type: String, output_path: OsString) -> PyResult<Self> {
        let client_type = serde_json::from_str(&format!("\"{client_type}\"")).map_err(|e| {
            BamlError::from_anyhow(
                Into::<anyhow::Error>::into(e).context("Failed to parse client_type"),
            )
        })?;
        Ok(Self {
            client_type,
            output_path,
        })
    }
}

impl Into<internal_baml_codegen::GeneratorArgs> for &GenerateArgs {
    fn into(self) -> internal_baml_codegen::GeneratorArgs {
        internal_baml_codegen::GeneratorArgs {
            output_root: self.output_path.clone().into(),
            encoded_baml_files: None,
        }
    }
}
