use crate::Result;
use baml_types::{BamlMedia, BamlMediaContent, BamlMediaType, BamlValue};
use magnus::{class, function, Module, Object, RModule};

pub(crate) trait CloneAsBamlValue {
    fn clone_as_baml_value(&self) -> BamlValue;
}

#[magnus::wrap(class = "Baml::Ffi::Image", free_immediately, size)]
pub(crate) struct Image {
    pub(crate) inner: baml_types::BamlMedia,
}

impl Image {
    pub fn from_url(url: String) -> Self {
        Self {
            inner: BamlMedia::url(BamlMediaType::Image, url, None),
        }
    }

    pub fn from_base64(media_type: String, base64: String) -> Self {
        Self {
            inner: BamlMedia::base64(BamlMediaType::Image, base64, Some(media_type)),
        }
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("Image", class::object())?;
        cls.define_singleton_method("from_url", function!(Image::from_url, 1))?;
        cls.define_singleton_method("from_base64", function!(Image::from_base64, 2))?;

        Ok(())
    }
}

impl CloneAsBamlValue for Image {
    fn clone_as_baml_value(&self) -> BamlValue {
        BamlValue::Media(self.inner.clone())
    }
}

#[magnus::wrap(class = "Baml::Ffi::Audio", free_immediately, size)]
pub(crate) struct Audio {
    pub(crate) inner: BamlMedia,
}

impl Audio {
    pub fn from_url(url: String) -> Self {
        Self {
            inner: BamlMedia::url(BamlMediaType::Audio, url, None),
        }
    }
    pub fn from_base64(media_type: String, base64: String) -> Self {
        Self {
            inner: BamlMedia::base64(BamlMediaType::Image, base64, Some(media_type)),
        }
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("Audio", class::object())?;
        cls.define_singleton_method("from_url", function!(Audio::from_url, 1))?;
        cls.define_singleton_method("from_base64", function!(Audio::from_base64, 2))?;

        Ok(())
    }
}

impl CloneAsBamlValue for Audio {
    fn clone_as_baml_value(&self) -> BamlValue {
        BamlValue::Media(self.inner.clone())
    }
}
