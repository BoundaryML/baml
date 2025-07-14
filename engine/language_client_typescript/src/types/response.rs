use napi::{bindgen_prelude::Env, JsObject};
use napi_derive::napi;

use super::request::HTTPBody;

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
            serde_json::to_string_pretty(&self.inner.headers()).unwrap(),
            serde_json::to_string_pretty(&self.inner.body.as_serde_value()).unwrap()
        )
    }

    #[napi(getter)]
    pub fn status(&self) -> u16 {
        self.inner.status
    }

    #[napi(getter)]
    pub fn headers(&self, env: Env) -> napi::Result<JsObject> {
        let mut obj = env.create_object()?;
        if let Some(headers) = self.inner.headers() {
            for (k, v) in headers {
                obj.set_named_property(k, v)?;
            }
        }
        Ok(obj)
    }

    #[napi(getter)]
    pub fn body(&self) -> HTTPBody {
        // TODO: Avoid clone.
        HTTPBody::from(self.inner.body.clone())
    }
}

crate::lang_wrapper!(
    SSEResponse,
    baml_types::tracing::events::SSEEvent,
    clone_safe
);

#[napi]
impl SSEResponse {
    #[napi(getter)]
    pub fn text(&self) -> String {
        self.inner.data.clone()
    }

    #[napi]
    pub fn json(&self) -> Option<serde_json::Value> {
        serde_json::from_str(&self.inner.data).ok()
    }
}
