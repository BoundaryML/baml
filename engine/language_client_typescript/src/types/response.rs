use napi::{bindgen_prelude::Env, JsObject};
use napi_derive::napi;

crate::lang_wrapper!(
    HTTPResponse,
    baml_types::tracing::events::HTTPResponse,
    clone_safe
);

#[napi]
impl HTTPResponse {
    #[napi]
    pub fn to_string(&self) -> String {
        format!(
            "HTTPResponse(status={}, headers={}, body={})",
            self.inner.status,
            serde_json::to_string_pretty(&self.inner.headers).unwrap(),
            serde_json::to_string_pretty(&self.inner.body).unwrap()
        )
    }

    #[napi(getter)]
    pub fn status(&self) -> u16 {
        self.inner.status
    }

    #[napi(getter)]
    pub fn headers(&self, env: Env) -> napi::Result<JsObject> {
        let obj = env.create_object()?;
        if let Some(headers) = self.inner.headers.as_object() {
            for (k, v) in headers {
                // let js_value = serde_value_to_js(env, v)?;
                // obj.set_named_property(k, js_value)?;
            }
        }
        Ok(obj)
    }

    #[napi(getter)]
    pub fn body(&self) -> napi::Result<serde_json::Value> {
        Ok(self.inner.body.clone())
    }
}
