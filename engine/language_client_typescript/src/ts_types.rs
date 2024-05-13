use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[napi(object)]
//#[derive(Deserialize)]
//#[serde(deny_unknown_fields)]
pub struct RuntimeContext {
    //#[serde(default = "HashMap::new")]
    pub env: Option<HashMap<String, String>>,
    //#[serde(default = "HashMap::new")]
    pub tags: Option<HashMap<String, serde_json::Value>>,
}

impl Into<baml_runtime::RuntimeContext> for RuntimeContext {
    fn into(self) -> baml_runtime::RuntimeContext {
        baml_runtime::RuntimeContext {
            env: self.env.unwrap_or(HashMap::new()),
            tags: self.tags.unwrap_or(HashMap::new()),
        }
    }
}

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
        Ok(self.inner.parsed_content()?.clone())
    }
}

#[napi(string_enum)]
pub enum LanguageClientType {
    PythonPydantic,
    Ruby,
    Typescript,
}

#[napi(object)]
pub struct GenerateArgs {
    pub client_type: LanguageClientType,
    pub output_path: String,
}

impl GenerateArgs {
    pub fn client_type(&self) -> internal_baml_codegen::LanguageClientType {
        match self.client_type {
            LanguageClientType::Ruby => internal_baml_codegen::LanguageClientType::Ruby,
            LanguageClientType::PythonPydantic => {
                internal_baml_codegen::LanguageClientType::PythonPydantic
            }
            LanguageClientType::Typescript => internal_baml_codegen::LanguageClientType::Typescript,
        }
    }

    pub fn as_codegen_args(&self) -> internal_baml_codegen::GeneratorArgs {
        internal_baml_codegen::GeneratorArgs {
            output_root: PathBuf::from(self.output_path.clone()),
            encoded_baml_files: None,
        }
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
