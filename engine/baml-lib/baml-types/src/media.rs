use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use std::{borrow::Cow, fmt, path::PathBuf};

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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BamlMedia {
    pub media_type: BamlMediaType,
    pub content: BamlMediaContent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BamlMediaContent {
    File(MediaFile),
    Url(MediaUrl),
    Base64(MediaBase64),
}

impl BamlMedia {
    pub fn file(
        media_type: BamlMediaType,
        baml_path: PathBuf,
        relpath: String,
        mime_type: Option<String>,
    ) -> BamlMedia {
        Self {
            media_type,
            content: BamlMediaContent::File(MediaFile {
                span_path: baml_path,
                relpath: relpath.into(),
                mime_type: Some(mime_type.unwrap_or_else(|| "".to_string())),
            }),
        }
    }

    pub fn url(media_type: BamlMediaType, url: String, mime_type: Option<String>) -> BamlMedia {
        Self {
            media_type,
            content: BamlMediaContent::Url(MediaUrl::new(
                url,
                Some(mime_type.unwrap_or_else(|| "".to_string())),
            )),
        }
    }

    pub fn base64(media_type: BamlMediaType, base64: String, mime_type: String) -> BamlMedia {
        Self {
            media_type,
            content: BamlMediaContent::Base64(MediaBase64::new(base64, mime_type)),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
/// NB: baml_path and relpath are Path objects to simplify path manipulation (joining,
/// extension parsing), and can both be safely converted to `String` using
/// `.to_lossy_string()` because file refs can only be instantiated using BAML code,
/// which must be UTF-8 and cannot reference non-UTF-8 paths
pub struct MediaFile {
    /// Path of the BAML file containing the media file
    pub span_path: PathBuf,
    /// The path provided as the "file" value, e.g. in
    /// "image { file path/to/image.png }", this would be "path/to/image.png"
    /// and should be interpreted relative to the parent dir of baml_path
    pub relpath: PathBuf,
    pub mime_type: Option<String>,
}

impl MediaFile {
    pub fn path(&self) -> Result<PathBuf> {
        Ok(self
            .span_path
            .parent()
            .context("Internal error: no path to resolve against")?
            .join(&self.relpath))
    }
    pub fn extension(&self) -> Option<Cow<str>> {
        self.relpath.extension().map(|ext| ext.to_string_lossy())
    }
}

impl fmt::Display for MediaFile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.relpath.display())
    }
}
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct MediaUrl {
    pub url: String,
    pub mime_type: Option<String>,
}

impl MediaUrl {
    pub fn new(url: String, mime_type: Option<String>) -> Self {
        Self { url, mime_type }
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
    pub mime_type: String,
}

impl MediaBase64 {
    pub fn new(base64: String, mime_type: String) -> Self {
        Self { base64, mime_type }
    }
}
