use serde::{Deserialize, Serialize};

use std::{fmt, path::PathBuf};

#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub enum BamlMediaType {
    Image,
    Audio,
}

impl fmt::Display for BamlMediaType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            BamlMediaType::Image => write!(f, "image"),
            BamlMediaType::Audio => write!(f, "audio"),
        }
    }
}

// We rely on the serialization and deserialization of this struct for:
// - prompt rendering (going into minijinja rendering and coming out)
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BamlMedia {
    File(BamlMediaType, MediaFile),
    Url(BamlMediaType, MediaUrl),
    Base64(BamlMediaType, MediaBase64),
}

impl BamlMedia {
    pub fn media_type(&self) -> &BamlMediaType {
        match self {
            BamlMedia::File(t, _) => t,
            BamlMedia::Url(t, _) => t,
            BamlMedia::Base64(t, _) => t,
        }
    }

    pub fn file(
        t: BamlMediaType,
        baml_path: PathBuf,
        relpath: String,
        media_type: Option<String>,
    ) -> BamlMedia {
        BamlMedia::File(
            t,
            MediaFile {
                baml_path,
                relpath,
                media_type: Some(media_type.unwrap_or_else(|| "".to_string())),
            },
        )
    }

    pub fn url(t: BamlMediaType, url: String, media_type: Option<String>) -> BamlMedia {
        BamlMedia::Url(
            t,
            MediaUrl::new(url, Some(media_type.unwrap_or_else(|| "".to_string()))),
        )
    }

    pub fn base64(t: BamlMediaType, base64: String, media_type: String) -> BamlMedia {
        BamlMedia::Base64(t, MediaBase64::new(base64, media_type))
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct MediaFile {
    /// Path of the BAML file containing the media file
    pub baml_path: PathBuf,
    /// The path provided as the "file" value, e.g. in
    /// "image { file path/to/image.png }", this would be "path/to/image.png"
    /// and should be interpreted relative to the parent dir of baml_path
    pub relpath: String,
    pub media_type: Option<String>,
}

impl fmt::Display for MediaFile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.relpath)
    }
}
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct MediaUrl {
    pub url: String,
    pub media_type: Option<String>,
}

impl MediaUrl {
    pub fn new(url: String, media_type: Option<String>) -> Self {
        Self { url, media_type }
    }
}

impl fmt::Display for MediaUrl {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.url)
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct MediaBase64 {
    pub base64: String,
    /// Explicitly specified by the 'type' field on img and audio structs in BAML files.
    /// example: "image/png", "image/jpeg", "audio/mp3"
    pub media_type: String,
}

impl MediaBase64 {
    pub fn new(base64: String, media_type: String) -> Self {
        Self { base64, media_type }
    }
}
