use napi::{bindgen_prelude::Env, JsArrayBuffer, JsObject, JsUnknown, NapiValue};
use napi_derive::napi;

use super::log_collector::serde_value_to_js;
use crate::errors::from_anyhow_error;

crate::lang_wrapper!(
    HTTPRequest,
    baml_types::tracing::events::HTTPRequest,
    clone_safe
);

crate::lang_wrapper!(HTTPBody, baml_types::tracing::events::HTTPBody, clone_safe);

#[napi]
impl HTTPRequest {
    #[napi(getter)]
    pub fn id(&self) -> String {
        self.inner.id().to_string()
    }

    #[napi(getter)]
    pub fn body(&self) -> HTTPBody {
        // TODO: Avoid clone.
        HTTPBody::from(self.inner.body().clone())
    }

    #[napi]
    pub fn to_string(&self) -> String {
        format!(
            "HTTPRequest(url={}, method={}, headers={}, body={})",
            self.inner.url(),
            self.inner.method(),
            serde_json::to_string_pretty(&self.inner.headers()).unwrap(),
            serde_json::to_string_pretty(&self.inner.body().as_serde_value()).unwrap()
        )
    }

    #[napi(getter)]
    pub fn url(&self) -> String {
        self.inner.url().to_string()
    }

    #[napi(getter)]
    pub fn method(&self) -> String {
        self.inner.method().to_string()
    }

    #[napi(getter)]
    pub fn headers(&self, env: Env) -> napi::Result<JsObject> {
        let mut obj = env.create_object()?;
        for (k, v) in self.inner.headers() {
            obj.set_named_property(k, v)?;
        }
        Ok(obj)
    }
}

#[napi]
impl HTTPBody {
    #[napi]
    pub fn raw(&self, env: Env) -> napi::Result<JsArrayBuffer> {
        // TODO: Avoid clone by using unsafe `env.create_arraybuffer_with_borrowed_data`
        // (documentation says the borrowed data can be mutated so it doesn't
        // look trivial to implement).
        env.create_arraybuffer_with_data(self.inner.raw().to_vec())
            .map(napi::JsArrayBufferValue::into_raw)
    }

    #[napi]
    pub fn text(&self) -> napi::Result<String> {
        self.inner
            .text()
            .map(String::from)
            .map_err(from_anyhow_error)
    }

    #[napi(ts_return_type = "any")]
    pub fn json(&self, env: Env) -> napi::Result<JsUnknown> {
        serde_value_to_js(env, &self.inner.json().map_err(from_anyhow_error)?)
    }
}
