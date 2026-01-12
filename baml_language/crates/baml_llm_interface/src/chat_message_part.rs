//! Chat message parts for rendered prompts.

use std::collections::HashMap;

use ir_stub::{BamlMedia, BamlMediaContent};
use serde::Serialize;

/// A part of a chat message.
#[derive(Debug, PartialEq, Serialize, Clone)]
#[serde(tag = "type")]
pub enum ChatMessagePart {
    /// Raw text content.
    Text {
        /// The text content.
        text: String,
    },
    /// Media content (image, audio, etc.).
    Media {
        /// The media content.
        #[serde(flatten)]
        media: BamlMedia,
    },
    /// A part with additional metadata.
    WithMeta {
        /// The inner part.
        inner: Box<ChatMessagePart>,
        /// The metadata.
        meta: HashMap<String, serde_json::Value>,
    },
}

impl ChatMessagePart {
    /// Wrap this part with additional metadata.
    pub fn with_meta(self, new_meta: HashMap<String, serde_json::Value>) -> ChatMessagePart {
        match self {
            ChatMessagePart::WithMeta { inner, mut meta } => {
                meta.extend(new_meta);
                ChatMessagePart::WithMeta { inner, meta }
            }
            _ => ChatMessagePart::WithMeta { inner: Box::new(self), meta: new_meta },
        }
    }

    /// Get the text content if this is a text part.
    pub fn as_text(&self) -> Option<&String> {
        match self {
            ChatMessagePart::Text { text } => Some(text),
            ChatMessagePart::WithMeta { inner, .. } => inner.as_text(),
            ChatMessagePart::Media { .. } => None,
        }
    }

    /// Get the media content if this is a media part.
    pub fn as_media(&self) -> Option<&BamlMedia> {
        match self {
            ChatMessagePart::Media { media } => Some(media),
            ChatMessagePart::WithMeta { inner, .. } => inner.as_media(),
            ChatMessagePart::Text { .. } => None,
        }
    }

    /// Get the metadata if present.
    pub fn meta(&self) -> Option<&HashMap<String, serde_json::Value>> {
        match self {
            ChatMessagePart::WithMeta { meta, .. } => Some(meta),
            _ => None,
        }
    }

    /// Convert to completion format (text only, media ignored).
    pub fn as_completion(self) -> String {
        match self {
            ChatMessagePart::Text { text } => text,
            ChatMessagePart::Media { .. } => String::new(),
            ChatMessagePart::WithMeta { inner, .. } => inner.as_completion(),
        }
    }
}

impl std::fmt::Display for ChatMessagePart {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ChatMessagePart::Text { text } => write!(f, "{text}"),
            ChatMessagePart::Media { media } => match &media.content {
                BamlMediaContent::Url(url) => {
                    write!(f, "<{}_placeholder: {}>", media.media_type, url.url)
                }
                BamlMediaContent::Base64(_) => {
                    write!(f, "<{}_placeholder base64>", media.media_type)
                }
                BamlMediaContent::File(file) => {
                    write!(f, "<{}_placeholder: {}>", media.media_type, file.path)
                }
            },
            ChatMessagePart::WithMeta { inner, meta } => write!(f, "{meta:?}::{inner}"),
        }
    }
}
