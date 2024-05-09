use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde::Deserialize;
use std::collections::HashMap;

#[napi(object)]
//#[derive(Deserialize)]
//#[serde(deny_unknown_fields)]
pub struct RuntimeContext {
    //#[serde(default = "HashMap::new")]
    pub env: HashMap<String, String>,
    //#[serde(default = "HashMap::new")]
    pub tags: HashMap<String, serde_json::Value>,
}

impl Into<baml_runtime::RuntimeContext> for RuntimeContext {
    fn into(self) -> baml_runtime::RuntimeContext {
        baml_runtime::RuntimeContext {
            env: self.env,
            tags: self.tags,
        }
    }
}

#[napi]
pub struct FunctionResult {
    inner: baml_runtime::FunctionResult,
}

impl FunctionResult {
    pub fn new(inner: baml_runtime::FunctionResult) -> Self {
        Self { inner }
    }
}
