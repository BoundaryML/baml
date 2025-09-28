use napi::{
    bindgen_prelude::{ArrayBuffer, Env, JsObjectValue, Object},
    Unknown,
};
use napi_derive::napi;

use crate::{errors::from_anyhow_error, types::log_collector::serde_value_to_js};

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
    pub fn headers(&self, env: &Env) -> napi::Result<Object<'_>> {
        let mut obj = Object::new(env)?;
        for (k, v) in self.inner.headers() {
            obj.set_named_property(k, v)?;
        }
        Ok(obj)
    }
}

#[napi]
impl HTTPBody {
    #[napi]
    pub fn raw<'e>(&self, env: &'e Env) -> napi::Result<ArrayBuffer<'e>> {
        // TODO: Avoid clone by using unsafe `env.create_arraybuffer_with_borrowed_data`
        // (documentation says the borrowed data can be mutated so it doesn't
        // look trivial to implement).
        ArrayBuffer::from_data(env, self.inner.raw().to_vec())
    }

    #[napi]
    pub fn text(&self) -> napi::Result<String> {
        self.inner
            .text()
            .map(String::from)
            .map_err(from_anyhow_error)
    }

    #[napi(ts_return_type = "any")]
    pub fn json<'e>(&self, env: &'e Env) -> napi::Result<Unknown<'e>> {
        serde_value_to_js(env, &self.inner.json().map_err(from_anyhow_error)?)
    }
}
