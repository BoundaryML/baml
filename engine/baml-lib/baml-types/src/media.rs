use serde::{Deserialize, Serialize};

use std::fmt;

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub enum BamlMediaType {
    Image,
    Audio,
}

impl fmt::Display for BamlMediaType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            BamlMediaType::Image => write!(f, "Image"),
            BamlMediaType::Audio => write!(f, "Audio"),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BamlMedia {
    Url(BamlMediaType, MediaUrl),
    Base64(BamlMediaType, MediaBase64),
}

impl BamlMedia {
    pub fn url(t: BamlMediaType, url: String) -> BamlMedia {
        BamlMedia::Url(t, MediaUrl::new(url))
    }

    pub fn base64(t: BamlMediaType, base64: String, media_type: String) -> BamlMedia {
        BamlMedia::Base64(t, MediaBase64::new(base64, media_type))
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct MediaUrl {
    pub url: String,
}

impl MediaUrl {
    pub fn new(url: String) -> Self {
        Self { url }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct MediaBase64 {
    pub base64: String,
    pub media_type: String,
}

impl MediaBase64 {
    pub fn new(base64: String, media_type: String) -> Self {
        Self { base64, media_type }
    }
}
