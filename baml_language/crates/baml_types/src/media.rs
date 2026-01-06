//! Media types for images, audio, video, and files.

use serde::{Deserialize, Serialize};

/// The type of media content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BamlMediaType {
    Image,
    Audio,
    Video,
    Pdf,
}

impl std::fmt::Display for BamlMediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BamlMediaType::Image => write!(f, "image"),
            BamlMediaType::Audio => write!(f, "audio"),
            BamlMediaType::Video => write!(f, "video"),
            BamlMediaType::Pdf => write!(f, "pdf"),
        }
    }
}

/// The content of a media object - either a URL, base64 data, or file path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BamlMediaContent {
    Url(MediaUrl),
    Base64(MediaBase64),
    File(MediaFile),
}

/// Media content referenced by URL.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaUrl {
    pub url: String,
}

/// Media content as base64-encoded data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaBase64 {
    pub base64: String,
    pub media_type: String, // MIME type
}

/// Media content referenced by file path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaFile {
    pub path: String,
}

/// A complete media object with type and content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BamlMedia {
    pub media_type: BamlMediaType,
    pub content: BamlMediaContent,
    pub mime_type: Option<String>,
}

impl BamlMedia {
    /// Create a media object from a URL.
    pub fn url(media_type: BamlMediaType, url: impl Into<String>) -> Self {
        Self {
            media_type,
            content: BamlMediaContent::Url(MediaUrl { url: url.into() }),
            mime_type: None,
        }
    }

    /// Create a media object from base64 data.
    pub fn base64(
        media_type: BamlMediaType,
        base64: impl Into<String>,
        mime_type: impl Into<String>,
    ) -> Self {
        let mime = mime_type.into();
        Self {
            media_type,
            content: BamlMediaContent::Base64(MediaBase64 {
                base64: base64.into(),
                media_type: mime.clone(),
            }),
            mime_type: Some(mime),
        }
    }

    /// Create a media object from a file path.
    pub fn file(media_type: BamlMediaType, path: impl Into<String>) -> Self {
        Self {
            media_type,
            content: BamlMediaContent::File(MediaFile { path: path.into() }),
            mime_type: None,
        }
    }

    /// Check if this media is a URL reference.
    pub fn is_url(&self) -> bool {
        matches!(self.content, BamlMediaContent::Url(_))
    }

    /// Check if this media is base64 encoded.
    pub fn is_base64(&self) -> bool {
        matches!(self.content, BamlMediaContent::Base64(_))
    }

    /// Check if this media is a file reference.
    pub fn is_file(&self) -> bool {
        matches!(self.content, BamlMediaContent::File(_))
    }
}

impl Serialize for BamlMedia {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("BamlMedia", 3)?;
        state.serialize_field("media_type", &self.media_type)?;
        state.serialize_field("content", &self.content)?;
        if let Some(mime) = &self.mime_type {
            state.serialize_field("mime_type", mime)?;
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for BamlMedia {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct MediaHelper {
            media_type: BamlMediaType,
            content: BamlMediaContent,
            mime_type: Option<String>,
        }

        let helper = MediaHelper::deserialize(deserializer)?;
        Ok(BamlMedia {
            media_type: helper.media_type,
            content: helper.content,
            mime_type: helper.mime_type,
        })
    }
}
