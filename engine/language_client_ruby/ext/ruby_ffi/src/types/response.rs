use magnus::{class, method, Error, Module, RModule, Ruby};

use super::request::HTTPBody;
use crate::Result;

crate::lang_wrapper!(
    HTTPResponse,
    "Baml::Ffi::HTTPResponse",
    baml_types::tracing::events::HTTPResponse,
    clone_safe
);

impl HTTPResponse {
    pub fn to_s(&self) -> String {
        format!(
            "HTTPResponse(status={}, headers={}, body={})",
            self.inner.status,
            serde_json::to_string_pretty(&self.inner.headers()).unwrap_or_default(),
            serde_json::to_string_pretty(&self.inner.body.as_serde_value()).unwrap_or_default()
        )
    }

    pub fn status(&self) -> u16 {
        self.inner.status
    }

    pub fn headers(ruby: &Ruby, rb_self: &Self) -> Result<magnus::Value> {
        // Convert headers to Ruby hash
        serde_magnus::serialize(&rb_self.inner.headers())
            .map_err(|e| Error::new(ruby.exception_runtime_error(), format!("{:?}", e)))
    }

    pub fn body(&self) -> HTTPBody {
        // TODO: Avoid clone.
        HTTPBody::from(self.inner.body.clone())
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("HTTPResponse", class::object())?;

        cls.define_method("to_s", method!(HTTPResponse::to_s, 0))?;
        cls.define_method("status", method!(HTTPResponse::status, 0))?;
        cls.define_method("headers", method!(HTTPResponse::headers, 0))?;
        cls.define_method("body", method!(HTTPResponse::body, 0))?;

        Ok(())
    }
}
