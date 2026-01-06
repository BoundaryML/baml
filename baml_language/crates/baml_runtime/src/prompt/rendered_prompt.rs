//! Rendered prompt types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::types::BamlValue;

/// Universal rendered prompt - provider agnostic.
#[derive(Debug, Clone)]
pub struct RenderedPrompt {
    /// The rendered messages.
    pub messages: Vec<RenderedMessage>,
    /// Output format specification (for the LLM to understand expected output).
    pub output_format: OutputFormatContent,
    /// Metadata about rendering.
    pub metadata: RenderMetadata,
}

impl RenderedPrompt {
    /// Create a new rendered prompt with messages.
    pub fn new(messages: Vec<RenderedMessage>) -> Self {
        Self {
            messages,
            output_format: OutputFormatContent::default(),
            metadata: RenderMetadata::default(),
        }
    }

    /// Create a simple prompt with a single user message.
    pub fn simple(content: impl Into<String>) -> Self {
        Self::new(vec![RenderedMessage::user(content)])
    }
}

/// A single rendered message.
#[derive(Debug, Clone)]
pub struct RenderedMessage {
    pub role: Role,
    pub parts: Vec<MessagePart>,
    /// Whether consecutive messages with same role should be merged.
    pub allow_duplicate_role: bool,
}

impl RenderedMessage {
    /// Create a new message with a role and parts.
    pub fn new(role: Role, parts: Vec<MessagePart>) -> Self {
        Self {
            role,
            parts,
            allow_duplicate_role: false,
        }
    }

    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(Role::System, vec![MessagePart::Text(content.into())])
    }

    /// Create a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(Role::User, vec![MessagePart::Text(content.into())])
    }

    /// Create an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(Role::Assistant, vec![MessagePart::Text(content.into())])
    }

    /// Get the text content of this message.
    pub fn text_content(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                MessagePart::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

/// Message role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    /// Provider-specific role (e.g., "tool" for OpenAI).
    #[serde(untagged)]
    Custom(String),
}

impl Role {
    pub fn as_str(&self) -> &str {
        match self {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Custom(s) => s.as_str(),
        }
    }
}

/// Part of a message.
#[derive(Debug, Clone)]
pub enum MessagePart {
    Text(String),
    Media(MediaContent),
    /// Part with additional metadata.
    WithMeta {
        part: Box<MessagePart>,
        meta: HashMap<String, serde_json::Value>,
    },
}

/// Media content - stays unresolved until media rewrite pass.
#[derive(Debug, Clone)]
pub enum MediaContent {
    /// URL to fetch (will be resolved during media rewrite if needed).
    Url { url: String, media_type: MediaType },
    /// Already resolved to base64.
    Base64 {
        mime_type: String,
        data: String,
        media_type: MediaType,
    },
    /// File path (for local files, will be resolved during media rewrite).
    FilePath { path: PathBuf, media_type: MediaType },
}

impl MediaContent {
    /// Create a URL media reference.
    pub fn url(url: impl Into<String>, media_type: MediaType) -> Self {
        MediaContent::Url {
            url: url.into(),
            media_type,
        }
    }

    /// Create a base64 media reference.
    pub fn base64(mime_type: impl Into<String>, data: impl Into<String>, media_type: MediaType) -> Self {
        MediaContent::Base64 {
            mime_type: mime_type.into(),
            data: data.into(),
            media_type,
        }
    }

    /// Check if this media needs resolution.
    pub fn needs_resolution(&self) -> bool {
        matches!(self, MediaContent::Url { .. } | MediaContent::FilePath { .. })
    }
}

/// Type of media content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    Image,
    Audio,
    Video,
    File,
}

/// Output format specification.
#[derive(Debug, Clone, Default)]
pub struct OutputFormatContent {
    /// Expected output type description.
    pub type_description: Option<String>,
    /// JSON schema for structured output.
    pub json_schema: Option<serde_json::Value>,
}

/// Metadata about the rendering process.
#[derive(Debug, Clone, Default)]
pub struct RenderMetadata {
    /// Template variables that were used.
    pub template_vars: HashMap<String, BamlValue>,
    /// Time taken to render.
    pub render_duration: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_prompt() {
        let prompt = RenderedPrompt::simple("Hello, world!");
        assert_eq!(prompt.messages.len(), 1);
        assert_eq!(prompt.messages[0].role, Role::User);
        assert_eq!(prompt.messages[0].text_content(), "Hello, world!");
    }

    #[test]
    fn test_multi_message_prompt() {
        let prompt = RenderedPrompt::new(vec![
            RenderedMessage::system("You are a helpful assistant."),
            RenderedMessage::user("What is 2+2?"),
        ]);
        assert_eq!(prompt.messages.len(), 2);
        assert_eq!(prompt.messages[0].role, Role::System);
        assert_eq!(prompt.messages[1].role, Role::User);
    }

    #[test]
    fn test_role_serialization() {
        assert_eq!(Role::System.as_str(), "system");
        assert_eq!(Role::User.as_str(), "user");
        assert_eq!(Role::Assistant.as_str(), "assistant");
        assert_eq!(Role::Custom("tool".to_string()).as_str(), "tool");
    }

    #[test]
    fn test_media_needs_resolution() {
        let url_media = MediaContent::url("https://example.com/image.png", MediaType::Image);
        assert!(url_media.needs_resolution());

        let base64_media = MediaContent::base64("image/png", "abc123", MediaType::Image);
        assert!(!base64_media.needs_resolution());
    }
}
