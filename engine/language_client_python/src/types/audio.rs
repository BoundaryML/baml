use baml_types::BamlMediaContent;
use pyo3::prelude::{pymethods, PyResult};
use pyo3::types::PyType;
use pyo3::{Bound, PyAny, PyObject, Python};
use pythonize::{depythonize_bound, pythonize};

use crate::errors::BamlError;

use super::media_repr::{self, UserFacingBamlMedia};
crate::lang_wrapper!(BamlAudioPy, baml_types::BamlMedia);

#[pymethods]
impl BamlAudioPy {
    #[staticmethod]
    fn from_url(url: String) -> Self {
        BamlAudioPy {
            inner: baml_types::BamlMedia::url(baml_types::BamlMediaType::Audio, url, None),
        }
    }

    #[staticmethod]
    fn from_base64(media_type: String, base64: String) -> Self {
        BamlAudioPy {
            inner: baml_types::BamlMedia::base64(
                baml_types::BamlMediaType::Image,
                base64,
                Some(media_type),
            ),
        }
    }

    pub fn is_url(&self) -> bool {
        matches!(&self.inner.content, BamlMediaContent::Url(_))
    }

    pub fn as_url(&self) -> PyResult<String> {
        match &self.inner.content {
            BamlMediaContent::Url(url) => Ok(url.url.clone()),
            _ => Err(BamlError::new_err("Audio is not a URL")),
        }
    }

    pub fn as_base64(&self) -> PyResult<Vec<String>> {
        match &self.inner.content {
            BamlMediaContent::Base64(base64) => Ok(vec![
                base64.base64.clone(),
                self.inner.mime_type.clone().unwrap_or("".to_string()),
            ]),
            _ => Err(BamlError::new_err("Audio is not base64")),
        }
    }

    pub fn __repr__(&self) -> String {
        match &self.inner.content {
            BamlMediaContent::Url(url) => {
                format!("BamlAudioPy(url={})", url.url)
            }
            BamlMediaContent::Base64(base64) => {
                format!(
                    "BamlAudioPy(base64={}, media_type={})",
                    base64.base64,
                    self.inner.mime_type.clone().unwrap_or("".to_string())
                )
            }
            _ => format!("Unknown BamlAudioPy variant"),
        }
    }

    #[classmethod]
    pub fn __get_pydantic_core_schema__(
        _cls: Bound<'_, PyType>,
        _source_type: Bound<'_, PyAny>,
        _handler: Bound<'_, PyAny>,
    ) -> PyResult<PyObject> {
        media_repr::__get_pydantic_core_schema__(_cls, _source_type, _handler)
    }

    #[staticmethod]
    fn baml_deserialize(data: PyObject, py: Python<'_>) -> PyResult<Self> {
        let data: UserFacingBamlMedia = depythonize_bound(data.into_bound(py))?;
        Ok(BamlAudioPy {
            inner: data.to_baml_media(baml_types::BamlMediaType::Audio),
        })
    }

    pub fn baml_serialize(&self, py: Python<'_>) -> PyResult<PyObject> {
        let s: UserFacingBamlMedia = (&self.inner).try_into().map_err(BamlError::from_anyhow)?;
        let s = serde_json::to_value(&s).map_err(|e| BamlError::from_anyhow(e.into()))?;
        Ok(pythonize(py, &s)?)
    }

    pub fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
