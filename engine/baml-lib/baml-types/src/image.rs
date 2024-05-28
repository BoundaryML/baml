use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BamlImage {
    Url(ImageUrl),
    Base64(ImageBase64),
}

impl BamlImage {
    pub fn url(url: String) -> BamlImage {
        BamlImage::Url(ImageUrl::new(url))
    }

    pub fn base64(base64: String, media_type: String) -> BamlImage {
        BamlImage::Base64(ImageBase64::new(base64, media_type))
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
}

impl ImageUrl {
    pub fn new(url: String) -> ImageUrl {
        ImageUrl { url }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ImageBase64 {
    pub base64: String,
    pub media_type: String,
}

impl ImageBase64 {
    pub fn new(base64: String, media_type: String) -> ImageBase64 {
        ImageBase64 { base64, media_type }
    }
}
