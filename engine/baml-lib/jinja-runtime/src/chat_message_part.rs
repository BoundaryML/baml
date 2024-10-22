use std::collections::HashMap;

use baml_types::{BamlMedia, BamlMediaContent};
use serde::Serialize;

#[derive(Debug, PartialEq, Serialize, Clone)]
pub enum ChatMessagePart {
    // raw user-provided text
    Text(String),
    Media(BamlMedia),
    WithMeta(Box<ChatMessagePart>, HashMap<String, serde_json::Value>),
}

impl ChatMessagePart {
    pub fn with_meta(self, meta: HashMap<String, serde_json::Value>) -> ChatMessagePart {
        match self {
            ChatMessagePart::WithMeta(part, mut existing_meta) => {
                existing_meta.extend(meta);
                ChatMessagePart::WithMeta(part, existing_meta)
            }
            _ => ChatMessagePart::WithMeta(Box::new(self), meta),
        }
    }

    pub fn as_text(&self) -> Option<&String> {
        match self {
            ChatMessagePart::Text(t) => Some(t),
            ChatMessagePart::WithMeta(t, _) => t.as_text(),
            ChatMessagePart::Media(_) => None,
        }
    }

    pub fn as_media(&self) -> Option<&BamlMedia> {
        match self {
            ChatMessagePart::Media(m) => Some(m),
            ChatMessagePart::WithMeta(t, _) => t.as_media(),
            ChatMessagePart::Text(_) => None,
        }
    }

    pub fn meta(&self) -> Option<&HashMap<String, serde_json::Value>> {
        match self {
            ChatMessagePart::WithMeta(_, meta) => Some(meta),
            _ => None,
        }
    }

    pub fn as_completion(self) -> String {
        match self {
            ChatMessagePart::Text(t) => t,
            ChatMessagePart::Media(_) => "".to_string(), // we are choosing to ignore the image for now
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
                BamlMediaContent::File(file) => write!(
                    f,
                    "<{}_placeholder: {}>",
                    media.media_type,
                    file.relpath.to_string_lossy()
                ),
            },
            ChatMessagePart::WithMeta(part, meta) => write!(f, "{meta:?}::{part}"),
        }
    }
}
