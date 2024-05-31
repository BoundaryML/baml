use pyo3::prelude::{pymethods, PyAnyMethods, PyModule, PyResult};
use pyo3::types::PyType;
use pyo3::{Bound, Py, PyAny, PyObject, Python, ToPyObject};

crate::lang_wrapper!(BamlImagePy, baml_types::BamlImage);

#[pymethods]
impl BamlImagePy {
    #[staticmethod]
    fn from_url(url: String) -> Self {
        BamlImagePy {
            inner: baml_types::BamlImage::Url(baml_types::ImageUrl::new(url)),
        }
    }

    #[staticmethod]
    fn from_base64(media_type: String, base64: String) -> Self {
        BamlImagePy {
            inner: baml_types::BamlImage::Base64(baml_types::ImageBase64::new(base64, media_type)),
        }
    }

    #[getter]
    pub fn get_url(&self) -> PyResult<Option<String>> {
        Ok(match &self.inner {
            baml_types::BamlImage::Url(url) => Some(url.url.clone()),
            _ => None,
        })
    }

    #[getter]
    pub fn get_base64(&self) -> PyResult<Option<(String, String)>> {
        Ok(match &self.inner {
            baml_types::BamlImage::Base64(base64) => {
                Some((base64.base64.clone(), base64.media_type.clone()))
            }
            _ => None,
        })
    }

    #[setter]
    pub fn set_url(&mut self, url: String) {
        self.inner = baml_types::BamlImage::Url(baml_types::ImageUrl::new(url));
    }

    #[setter]
    pub fn set_base64(&mut self, base64: (String, String)) {
        self.inner =
            baml_types::BamlImage::Base64(baml_types::ImageBase64::new(base64.0, base64.1));
    }

    pub fn __repr__(&self) -> String {
        match &self.inner {
            baml_types::BamlImage::Url(url) => format!("BamlImagePy(url={})", url.url),
            baml_types::BamlImage::Base64(base64) => {
                format!(
                    "BamlImagePy(base64={}, media_type={})",
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
