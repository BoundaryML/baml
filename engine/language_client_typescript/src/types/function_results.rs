use baml_types::BamlValue;
use napi_derive::napi;

crate::lang_wrapper!(FunctionResultPy, baml_runtime::FunctionResult);

#[napi]
impl FunctionResultPy {
    fn __str__(&self) -> String {
        format!("{:#}", self.inner)
    }

    #[napi]
    pub fn parsed(&self) -> napi::Result<serde_json::Value> {
        let parsed = self
            .inner
            .parsed_content()
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?;

        Ok(serde_json::json!(BamlValue::from(parsed)))
    }
}
