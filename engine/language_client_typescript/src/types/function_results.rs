use baml_types::BamlValue;
use napi_derive::napi;

crate::lang_wrapper!(FunctionResult, baml_runtime::FunctionResult);

#[napi]
impl FunctionResult {
    fn __str__(&self) -> String {
        format!("{:#}", self.inner)
    }

    #[napi]
    pub fn is_ok(&self) -> bool {
        self.inner.parsed_content().is_ok()
    }

    #[napi]
    pub fn parsed(&self) -> napi::Result<serde_json::Value> {
        let parsed = self
            .inner
            .parsed_content()
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))?;

        Ok(serde_json::json!(BamlValue::from(parsed)))
    }
}
