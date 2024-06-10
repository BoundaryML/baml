use baml_types::BamlValue;
use magnus::{
    class, exception::runtime_error, function, method, prelude::*, value::Value, Error, RModule,
    Ruby,
};

use crate::Result;

#[magnus::wrap(class = "Baml::Image", free_immediately, size)]
pub struct BamlImage {
    inner: baml_types::BamlImage,
}

impl Into<BamlValue> for &BamlImage {
    fn into(self) -> BamlValue {
        BamlValue::Image(self.inner.clone())
    }
}

impl BamlImage {
    fn from_url(url: String) -> Self {
        Self {
            inner: baml_types::BamlImage::from_url(url),
        }
    }

    fn from_base64(media_type: String, base64: String) -> Self {
        Self {
            inner: baml_types::BamlImage::from_base64(media_type, base64),
        }
    }

    pub fn is_url(&self) -> bool {
        self.inner.is_url()
    }

    pub fn is_base64(&self) -> bool {
        self.inner.is_base64()
    }

    pub fn as_url(ruby: &Ruby, rb_self: &BamlImage) -> Result<String> {
        rb_self
            .inner
            .as_url()
            .map_err(|e| crate::baml_error(ruby, e, "Failed to convert image to URL"))
    }

    pub fn as_base64(ruby: &Ruby, rb_self: &BamlImage) -> Result<(String, String)> {
        rb_self
            .inner
            .as_base64()
            .map(|baml_types::ImageBase64 { media_type, base64 }| (media_type, base64))
            .map_err(|e| crate::baml_error(ruby, e, "Failed to convert image to base64"))
    }

    pub fn inspect(&self) -> String {
        format!("{:?}", self.inner)
    }

    pub fn eql(&self, other: &Self) -> bool {
        self.inner == other.inner
    }

    pub fn hash(&self) -> u64 {
        use std::hash::{DefaultHasher, Hash, Hasher};

        let mut s = DefaultHasher::new();
        self.inner.hash(&mut s);
        s.finish()
    }
}

impl crate::DefineInRuby for BamlImage {
    fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("Image", class::object())?;

        cls.define_singleton_method("from_url", function!(BamlImage::from_url, 1))?;
        cls.define_singleton_method("from_base64", function!(BamlImage::from_base64, 2))?;

        cls.define_method("is_url", method!(BamlImage::is_url, 0))?;
        cls.define_method("is_base64", method!(BamlImage::is_url, 0))?;
        cls.define_method("as_url", method!(BamlImage::as_url, 0))?;
        cls.define_method("as_base64", method!(BamlImage::as_base64, 0))?;
        cls.define_method("inspect", method!(BamlImage::as_base64, 0))?;

        cls.define_method("==", method!(BamlImage::eql, 1))?;
        cls.define_method("eql?", method!(BamlImage::eql, 1))?;
        cls.define_method("hash", method!(BamlImage::hash, 0))?;

        Ok(())
    }
}
