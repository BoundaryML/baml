use napi_derive::napi;

use crate::errors::from_anyhow_error;

crate::lang_wrapper!(FunctionResult, baml_runtime::FunctionResult);

#[napi]
impl FunctionResult {
    fn __str__(&self) -> String {
        format!("{:#}", self.inner)
    }

    #[napi]
    pub fn is_ok(&self) -> bool {
        self.inner.result_with_constraints_content().is_ok()
    }

    #[napi]
    pub fn parsed(&self, allow_partials: bool) -> napi::Result<serde_json::Value> {
        let parsed = self
            .inner
            .result_with_constraints_content()
            .map_err(from_anyhow_error)?;

        let response = serde_json::to_value( if allow_partials {
            parsed.serialize_partial()
        } else {
            parsed.serialize_final()
        }
        )?;
        Ok(response)
    }
}
