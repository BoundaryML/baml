//! Rendered prompt types.

use serde::Serialize;

use crate::ChatMessagePart;

/// A rendered chat message.
#[derive(Debug, PartialEq, Serialize, Clone)]
pub struct RenderedChatMessage {
    /// The role of this message.
    pub role: String,
    /// Whether duplicate roles are allowed.
    #[serde(skip_serializing)]
    pub allow_duplicate_role: bool,
    /// The message parts.
    #[serde(rename = "content")]
    pub parts: Vec<ChatMessagePart>,
}

/// The result of rendering a prompt template.
#[derive(Debug, PartialEq, Clone, Serialize)]
#[serde(tag = "type")]
pub enum RenderedPrompt {
    /// A completion prompt (single string).
    Completion {
        /// The completion text.
        text: String,
    },
    /// A chat prompt (multiple messages with roles).
    Chat {
        /// The chat messages.
        messages: Vec<RenderedChatMessage>,
    },
}

impl std::fmt::Display for RenderedPrompt {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RenderedPrompt::Completion { text } => write!(f, "{text}"),
            RenderedPrompt::Chat { messages } => {
                for message in messages {
                    writeln!(
                        f,
                        "{}: {}",
                        message.role,
                        message
                            .parts
                            .iter()
                            .map(ChatMessagePart::to_string)
                            .collect::<Vec<String>>()
                            .join("")
                    )?;
                }
                Ok(())
            }
        }
    }
}
