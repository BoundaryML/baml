//! Chat message parts for rendered prompts.

use std::collections::HashMap;

use baml_types::{BamlMedia, BamlMediaContent};
use serde::Serialize;

/// A part of a chat message.
#[derive(Debug, PartialEq, Serialize, Clone)]
pub enum ChatMessagePart {
    /// Raw text content.
    Text(String),
    /// Media content (image, audio, etc.).
    Media(BamlMedia),
    /// A part with additional metadata.
    WithMeta(Box<ChatMessagePart>, HashMap<String, serde_json::Value>),
}

impl ChatMessagePart {
    /// Wrap this part with additional metadata.
    pub fn with_meta(self, meta: HashMap<String, serde_json::Value>) -> ChatMessagePart {
        match self {
            ChatMessagePart::WithMeta(part, mut existing_meta) => {
                existing_meta.extend(meta);
                ChatMessagePart::WithMeta(part, existing_meta)
            }
            _ => ChatMessagePart::WithMeta(Box::new(self), meta),
        }
    }

    /// Get the text content if this is a text part.
    pub fn as_text(&self) -> Option<&String> {
        match self {
            ChatMessagePart::Text(t) => Some(t),
            ChatMessagePart::WithMeta(t, _) => t.as_text(),
            ChatMessagePart::Media(_) => None,
        }
    }

    /// Get the media content if this is a media part.
    pub fn as_media(&self) -> Option<&BamlMedia> {
        match self {
            ChatMessagePart::Media(m) => Some(m),
            ChatMessagePart::WithMeta(t, _) => t.as_media(),
            ChatMessagePart::Text(_) => None,
        }
    }

    /// Get the metadata if present.
    pub fn meta(&self) -> Option<&HashMap<String, serde_json::Value>> {
        match self {
            ChatMessagePart::WithMeta(_, meta) => Some(meta),
            _ => None,
        }
    }

    /// Convert to completion format (text only, media ignored).
    pub fn as_completion(self) -> String {
        match self {
            ChatMessagePart::Text(t) => t,
            ChatMessagePart::Media(_) => String::new(),
            ChatMessagePart::WithMeta(p, _) => p.as_completion(),
        }
    }
}

impl std::fmt::Display for ChatMessagePart {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ChatMessagePart::Text(t) => write!(f, "{t}"),
            ChatMessagePart::Media(media) => match &media.content {
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
            ChatMessagePart::WithMeta(part, meta) => write!(f, "{meta:?}::{part}"),
        }
    }
}
