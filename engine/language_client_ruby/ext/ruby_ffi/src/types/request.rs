use magnus::{class, method, r_array::TypedArray, Error, Module, RModule, Ruby};

use crate::Result;

crate::lang_wrapper!(
    HTTPRequest,
    "Baml::Ffi::HTTPRequest",
    baml_types::tracing::events::HTTPRequest,
    clone_safe
);

crate::lang_wrapper!(
    HTTPBody,
    "Baml::Ffi::HTTPBody",
    baml_types::tracing::events::HTTPBody,
    clone_safe
);

impl HTTPRequest {
    pub fn to_s(&self) -> String {
        format!(
            "HTTPRequest(url={}, method={}, headers={}, body={})",
            self.inner.url(),
            self.inner.method(),
            serde_json::to_string_pretty(&self.inner.headers()).unwrap_or_default(),
            serde_json::to_string_pretty(&self.inner.body().as_serde_value()).unwrap_or_default()
        )
    }

    pub fn id(&self) -> String {
        self.inner.id().to_string()
    }

    pub fn url(&self) -> String {
        self.inner.url().to_string()
    }

    pub fn method(&self) -> String {
        self.inner.method().to_string()
    }

    pub fn headers(ruby: &Ruby, rb_self: &Self) -> Result<magnus::Value> {
        // Convert headers to Ruby hash
        serde_magnus::serialize(&rb_self.inner.headers())
            .map_err(|e| Error::new(ruby.exception_runtime_error(), format!("{e:?}")))
    }

    pub fn body(&self) -> HTTPBody {
        // TODO: Avoid clone.
        HTTPBody::from(self.inner.body().clone())
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("HTTPRequest", class::object())?;

        cls.define_method("to_s", method!(HTTPRequest::to_s, 0))?;
        cls.define_method("id", method!(HTTPRequest::id, 0))?;
        cls.define_method("url", method!(HTTPRequest::url, 0))?;
        cls.define_method("method", method!(HTTPRequest::method, 0))?;
        cls.define_method("headers", method!(HTTPRequest::headers, 0))?;
        cls.define_method("body", method!(HTTPRequest::body, 0))?;

        Ok(())
    }
}

impl HTTPBody {
    pub fn raw(ruby: &Ruby, rb_self: &Self) -> Result<TypedArray<u8>> {
        let array = ruby.typed_ary_new();

        // TODO: Can we avoid cloning or at least do this faster than byte by
        // byte?
        for byte in rb_self.inner.raw() {
            array.push(*byte)?;
        }

        Ok(array)
    }

    pub fn text(ruby: &Ruby, rb_self: &Self) -> Result<String> {
        rb_self.inner.text().map(String::from).map_err(|e| {
            Error::new(
                ruby.exception_runtime_error(),
                format!("Failed to get text from HTTP body:\n{e:?}"),
            )
        })
    }

    pub fn json(ruby: &Ruby, rb_self: &Self) -> Result<magnus::Value> {
        serde_magnus::serialize(&rb_self.inner.json().map_err(|e| {
            Error::new(
                ruby.exception_runtime_error(),
                format!("Failed deserializing HTTP body as JSON:\n{e:?}"),
            )
        })?)
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("HTTPBody", class::object())?;

        cls.define_method("raw", method!(HTTPBody::raw, 0))?;
        cls.define_method("text", method!(HTTPBody::text, 0))?;
        cls.define_method("json", method!(HTTPBody::json, 0))?;

        Ok(())
    }
}
