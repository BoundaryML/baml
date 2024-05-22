use baml_types::BamlValue;
use napi::bindgen_prelude::*;
use napi_derive::napi;

use std::path::PathBuf;

#[napi]
pub struct FunctionResult {
    inner: baml_runtime::FunctionResult,
}

#[napi]
impl FunctionResult {
    pub fn new(inner: baml_runtime::FunctionResult) -> Self {
        Self { inner }
    }

    #[napi(getter)]
    pub fn parsed(&self) -> Result<serde_json::Value> {
        log::debug!("parsed content");
        Ok(serde_json::json!(BamlValue::from(
            self.inner.parsed_content()?
        )))
    }
}

//#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
//#[serde(rename = "Image")]
//#[pyclass(name = "Image")]
#[derive(Debug)]
enum ImageRepr {
    Url(String),
    Base64(String),
}

#[napi]
struct Image {
    repr: ImageRepr,
}

#[napi]
impl Image {
    #[napi(factory)]
    pub fn from_base64(base64: String) -> Self {
        Self {
            repr: ImageRepr::Base64(base64),
        }
    }

    #[napi(factory)]
    pub fn from_url(url: String) -> Self {
        Self {
            repr: ImageRepr::Url(url),
        }
    }

    /// Returns the debug representation of the image.
    fn to_string(&self) -> String {
        format!("{:?}", self.repr)
    }
}
