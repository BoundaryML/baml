use baml_types::{BamlMedia, BamlMediaType, BamlValue};
use magnus::{class, function, Module, Object, RModule};

use crate::Result;

pub(crate) trait CloneAsBamlValue {
    fn clone_as_baml_value(&self) -> BamlValue;
}

#[magnus::wrap(class = "Baml::Ffi::Image", free_immediately, size)]
pub(crate) struct Image {
    pub(crate) inner: baml_types::BamlMedia,
}

impl Image {
    pub fn from_url(url: String, media_type: Option<String>) -> Self {
        Self {
            inner: BamlMedia::url(BamlMediaType::Image, url, media_type),
        }
    }

    pub fn from_base64(media_type: String, base64: String) -> Self {
        Self {
            inner: BamlMedia::base64(BamlMediaType::Image, base64, Some(media_type)),
        }
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("Image", class::object())?;
        cls.define_singleton_method("from_url", function!(Image::from_url, 2))?;
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
    pub fn from_url(url: String, media_type: Option<String>) -> Self {
        Self {
            inner: BamlMedia::url(BamlMediaType::Audio, url, media_type),
        }
    }
    pub fn from_base64(media_type: String, base64: String) -> Self {
        Self {
            inner: BamlMedia::base64(BamlMediaType::Audio, base64, Some(media_type)),
        }
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("Audio", class::object())?;
        cls.define_singleton_method("from_url", function!(Audio::from_url, 2))?;
        cls.define_singleton_method("from_base64", function!(Audio::from_base64, 2))?;

        Ok(())
    }
}

impl CloneAsBamlValue for Audio {
    fn clone_as_baml_value(&self) -> BamlValue {
        BamlValue::Media(self.inner.clone())
    }
}

#[magnus::wrap(class = "Baml::Ffi::Pdf", free_immediately, size)]
pub(crate) struct Pdf {
    pub(crate) inner: BamlMedia,
}

impl Pdf {
    pub fn from_url(url: String) -> Self {
        Self {
            inner: BamlMedia::url(BamlMediaType::Pdf, url, Some("application/pdf".to_string())),
        }
    }

    pub fn from_base64(base64: String) -> Self {
        Self {
            inner: BamlMedia::base64(
                BamlMediaType::Pdf,
                base64,
                Some("application/pdf".to_string()),
            ),
        }
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("Pdf", class::object())?;
        cls.define_singleton_method("from_url", function!(Pdf::from_url, 1))?;
        cls.define_singleton_method("from_base64", function!(Pdf::from_base64, 1))?;

        Ok(())
    }
}

impl CloneAsBamlValue for Pdf {
    fn clone_as_baml_value(&self) -> BamlValue {
        BamlValue::Media(self.inner.clone())
    }
}

#[magnus::wrap(class = "Baml::Ffi::Video", free_immediately, size)]
pub(crate) struct Video {
    pub(crate) inner: BamlMedia,
}

impl Video {
    pub fn from_url(url: String, media_type: Option<String>) -> Self {
        Self {
            inner: BamlMedia::url(BamlMediaType::Video, url, media_type),
        }
    }
    pub fn from_base64(media_type: String, base64: String) -> Self {
        Self {
            inner: BamlMedia::base64(BamlMediaType::Video, base64, Some(media_type)),
        }
    }

    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("Video", class::object())?;
        cls.define_singleton_method("from_url", function!(Video::from_url, 2))?;
        cls.define_singleton_method("from_base64", function!(Video::from_base64, 2))?;

        Ok(())
    }
}

impl CloneAsBamlValue for Video {
    fn clone_as_baml_value(&self) -> BamlValue {
        BamlValue::Media(self.inner.clone())
    }
}
