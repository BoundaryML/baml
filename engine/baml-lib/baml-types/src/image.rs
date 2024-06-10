use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Hash, PartialEq, Serialize)]
#[serde(untagged)]
pub enum BamlImage {
    Url(ImageUrl),
    Base64(ImageBase64),
}

impl BamlImage {
    pub fn from_url(url: String) -> BamlImage {
        BamlImage::Url(ImageUrl::new(url))
    }

    pub fn from_base64(media_type: String, base64: String) -> BamlImage {
        BamlImage::Base64(ImageBase64::new(media_type, base64))
    }

    pub fn is_url(&self) -> bool {
        matches!(&self, BamlImage::Url(_))
    }

    pub fn is_base64(&self) -> bool {
        matches!(&self, BamlImage::Base64(_))
    }

    pub fn as_url(&self) -> Result<String> {
        match &self {
            BamlImage::Url(url) => Ok(url.url.clone()),
            _ => anyhow::bail!("Conversion to URL is only supported for URL images"),
        }
    }

    pub fn as_base64(&self) -> Result<ImageBase64> {
        match &self {
            BamlImage::Base64(base64) => Ok(base64.clone()),
            _ => anyhow::bail!("Conversion to base64 is only supported for base64 images"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Hash, PartialEq, Serialize)]
pub struct ImageUrl {
    pub url: String,
}

impl ImageUrl {
    pub fn new(url: String) -> ImageUrl {
        ImageUrl { url }
    }
}

#[derive(Clone, Debug, Deserialize, Hash, PartialEq, Serialize)]
pub struct ImageBase64 {
    pub media_type: String,
    pub base64: String,
}

impl ImageBase64 {
    pub fn new(media_type: String, base64: String) -> ImageBase64 {
        ImageBase64 { media_type, base64 }
    }
}
