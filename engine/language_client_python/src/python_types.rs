use std::ffi::OsString;

use baml_types::BamlValue;
use pyo3::prelude::{pyclass, pymethods, PyModule, PyResult};
use pyo3::types::PyType;
use pyo3::{Py, PyAny, PyObject, Python, ToPyObject};
use pythonize::pythonize;
use serde::{Deserialize, Serialize};

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

        Ok(pythonize(py, &BamlValue::from(parsed))?)
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
        _cls: &PyType,
        _source_type: &PyAny,
        _handler: &PyAny,
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
            let fun: Py<PyAny> = PyModule::from_code(py, code, "", "")
                .unwrap()
                .getattr("ret")
                .unwrap()
                .into();
            Ok(fun.to_object(py)) // Return the PyObject
        })
    }

    pub fn __eq__(&self, other: &Self) -> bool {
        self == other
    }
}
