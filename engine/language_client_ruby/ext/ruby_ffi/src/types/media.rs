use crate::Result;
use baml_types::BamlValue;
use magnus::{
    class, function, method, scan_args::scan_args, try_convert::TryConvertOwned, typed_data::Obj,
    Module, Object, RModule, TryConvert, Value,
};

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
            inner: baml_types::BamlMedia::Url(
                baml_types::BamlMediaType::Image,
                baml_types::MediaUrl::new(url, None),
            ),
        }
    }

    pub fn from_base64(base64: String, media_type: String) -> Self {
        Self {
            inner: baml_types::BamlMedia::Base64(
                baml_types::BamlMediaType::Image,
                baml_types::MediaBase64::new(base64, media_type),
            ),
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
    pub(crate) inner: baml_types::BamlMedia,
}

impl Audio {
    pub fn from_url(url: String) -> Self {
        Self {
            inner: baml_types::BamlMedia::Url(
                baml_types::BamlMediaType::Audio,
                baml_types::MediaUrl::new(url, None),
            ),
        }
    }
    pub fn from_base64(base64: String, media_type: String) -> Self {
        Self {
            inner: baml_types::BamlMedia::Base64(
                baml_types::BamlMediaType::Audio,
                baml_types::MediaBase64::new(base64, media_type),
            ),
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
