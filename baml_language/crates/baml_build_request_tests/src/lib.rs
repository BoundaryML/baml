//! Test utilities for BAML request building pipeline.
//!
//! This crate provides snapshot tests for:
//! - `render_prompt`: Prompt rendering with Jinja templates
//! - `render_raw_curl`: Raw curl command generation
//! - `build_request`: HTTP request construction

use baml_db::{RootDatabase, SourceFile};
use baml_jinja_runtime::{
    render_prompt, ChatMessagePart, RenderContext, RenderContext_Client, RenderedPrompt,
};
use baml_runtime::function_lookup::{
    get_first_function, get_first_function_name, get_function_client, get_function_prompt,
};
use baml_types::{BamlMap, BamlValue};
use serde::Serialize;

/// Load a BAML file and create a database.
pub fn load_baml_file(content: &str) -> (RootDatabase, SourceFile) {
    let mut db = RootDatabase::default();
    let source = db.add_file("test.baml", content);
    (db, source)
}

// Re-export function lookup utilities for convenience
pub use baml_runtime::function_lookup;

/// Snapshot of a rendered prompt.
#[derive(Debug, Serialize)]
pub struct PromptSnapshot {
    pub fixture: String,
    pub function: String,
    pub prompt: RenderedPromptSnapshot,
}

/// Serializable version of RenderedPrompt.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum RenderedPromptSnapshot {
    Completion { text: String },
    Chat { messages: Vec<ChatMessageSnapshot> },
}

/// Serializable chat message.
#[derive(Debug, Serialize)]
pub struct ChatMessageSnapshot {
    pub role: String,
    pub content: Vec<ChatMessagePartSnapshot>,
}

/// Serializable chat message part.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ChatMessagePartSnapshot {
    Text { text: String },
    Media { media_type: String },
    WithMeta { inner: Box<ChatMessagePartSnapshot>, meta: serde_json::Value },
}

impl From<&RenderedPrompt> for RenderedPromptSnapshot {
    fn from(prompt: &RenderedPrompt) -> Self {
        match prompt {
            RenderedPrompt::Completion(text) => RenderedPromptSnapshot::Completion {
                text: text.clone(),
            },
            RenderedPrompt::Chat(messages) => RenderedPromptSnapshot::Chat {
                messages: messages
                    .iter()
                    .map(|msg| ChatMessageSnapshot {
                        role: msg.role.clone(),
                        content: msg.parts.iter().map(ChatMessagePartSnapshot::from).collect(),
                    })
                    .collect(),
            },
        }
    }
}

impl From<&ChatMessagePart> for ChatMessagePartSnapshot {
    fn from(part: &ChatMessagePart) -> Self {
        match part {
            ChatMessagePart::Text(text) => ChatMessagePartSnapshot::Text { text: text.clone() },
            ChatMessagePart::Media(media) => ChatMessagePartSnapshot::Media {
                media_type: format!("{:?}", media.media_type),
            },
            ChatMessagePart::WithMeta(inner, meta) => ChatMessagePartSnapshot::WithMeta {
                inner: Box::new(ChatMessagePartSnapshot::from(inner.as_ref())),
                meta: serde_json::to_value(meta).unwrap_or_default(),
            },
        }
    }
}

/// Render a prompt for a function with default test args.
pub fn render_prompt_for_fixture(
    baml_content: &str,
    func_name: &str,
) -> anyhow::Result<RenderedPrompt> {
    let (db, source) = load_baml_file(baml_content);

    let func_loc = get_first_function(&db, source)
        .ok_or_else(|| anyhow::anyhow!("No function found in fixture"))?;

    let prompt_template = get_function_prompt(&db, func_loc)
        .ok_or_else(|| anyhow::anyhow!("Function '{}' not found or has no prompt", func_name))?;

    let client_name = get_function_client(&db, func_loc).unwrap_or_else(|| "default".to_string());

    // For now, use empty args - in the future we could parse test args from the BAML file
    let args = BamlValue::Map(BamlMap::new());

    let ctx = RenderContext {
        client: RenderContext_Client {
            name: client_name,
            provider: "openai".to_string(),
            default_role: "system".to_string(),
            allowed_roles: vec![
                "system".to_string(),
                "user".to_string(),
                "assistant".to_string(),
            ],
            ..Default::default()
        },
        ..Default::default()
    };

    render_prompt(&prompt_template, &args, ctx).map_err(|e| anyhow::anyhow!("{}", e))
}

// Re-export get_first_function_name for tests
pub fn get_first_function_name_from_file(db: &RootDatabase, source: SourceFile) -> Option<String> {
    get_first_function_name(db, source)
}
