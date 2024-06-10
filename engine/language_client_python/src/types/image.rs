use pyo3::prelude::{pymethods, PyAnyMethods, PyModule, PyResult};
use pyo3::types::PyType;
use pyo3::{Bound, Py, PyAny, PyObject, Python, ToPyObject};

crate::lang_wrapper!(BamlImagePy, baml_types::BamlImage);

#[pymethods]
impl BamlImagePy {
    #[staticmethod]
    fn from_url(url: String) -> Self {
        Self {
            inner: baml_types::BamlImage::from_url(url),
        }
    }

    #[staticmethod]
    fn from_base64(media_type: String, base64: String) -> Self {
        Self {
            inner: baml_types::BamlImage::from_base64(media_type, base64),
        }
    }

    pub fn is_url(&self) -> bool {
        self.inner.is_url()
    }

    pub fn is_base64(&self) -> bool {
        self.inner.is_base64()
    }

    pub fn as_url(&self) -> PyResult<String> {
        self.inner
            .as_url()
            .map_err(|e| crate::BamlError::new_err(format!("{:?}", e)))
    }

    pub fn as_base64(&self) -> PyResult<Vec<String>> {
        self.inner
            .as_base64()
            .map(|baml_types::ImageBase64 { media_type, base64 }| vec![base64, media_type])
            .map_err(|e| crate::BamlError::new_err(format!("{:?}", e)))
    }

    pub fn __repr__(&self) -> String {
        match &self.inner {
            baml_types::BamlImage::Url(url) => format!("BamlImage(url={})", url.url),
            baml_types::BamlImage::Base64(base64) => {
                format!(
                    "BamlImage(base64={}, media_type={})",
                    base64.base64, base64.media_type
                )
            }
        }
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
        self.inner == other.inner
    }
}
