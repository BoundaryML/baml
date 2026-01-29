//! Anthropic-format HTTP request builder.

use bex_external_types::{BexExternalValue, PrimitiveClientValue, PromptAst};
use indexmap::IndexMap;

use super::{BuildRequestError, LlmRequestBuilder, get_string_option, prompt_to_content_parts};

/// Builder for the Anthropic provider.
pub(crate) struct AnthropicBuilder;

impl LlmRequestBuilder for AnthropicBuilder {
    fn provider_skip_keys(&self) -> &'static [&'static str] {
        &["anthropic_version"]
    }

    fn build_url(&self, client: &PrimitiveClientValue) -> Result<String, BuildRequestError> {
        let base_url = get_string_option(client, "base_url")
            .unwrap_or_else(|| "https://api.anthropic.com".to_string());
        Ok(format!("{base_url}/v1/messages"))
    }

    fn build_auth_headers(&self, client: &PrimitiveClientValue) -> IndexMap<String, String> {
        let mut headers = IndexMap::new();
        // Anthropic uses x-api-key header
        if let Some(api_key) = get_string_option(client, "api_key") {
            headers.insert("x-api-key".to_string(), api_key);
        }
        // Anthropic version header
        let version = get_string_option(client, "anthropic_version")
            .unwrap_or_else(|| "2023-06-01".to_string());
        headers.insert("anthropic-version".to_string(), version);
        headers
    }

    fn build_prompt_body(&self, prompt: PromptAst) -> serde_json::Map<String, serde_json::Value> {
        let mut map = serde_json::Map::new();
        let (system_parts, messages) = extract_system_and_messages(prompt);
        if !system_parts.is_empty() {
            map.insert("system".to_string(), serde_json::Value::Array(system_parts));
        }
        map.insert("messages".to_string(), serde_json::Value::Array(messages));
        map
    }
}

/// Extract system messages to a separate array and return non-system messages.
///
/// Anthropic format:
/// - System: top-level `"system": [{"type": "text", "text": "..."}]`
/// - Messages: `[{"role": "user", "content": [{"type": "text", "text": "..."}]}]`
fn extract_system_and_messages(
    prompt: PromptAst,
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let mut system_parts = Vec::new();
    let mut messages = Vec::new();

    let items = match prompt {
        PromptAst::Vec(items) => items,
        single => vec![single],
    };

    for item in items {
        match item {
            PromptAst::Message {
                role,
                content,
                metadata: _,
            } if role == "system" => {
                // System messages → top-level system field
                let parts = prompt_to_content_parts(*content);
                system_parts.extend(parts);
            }
            PromptAst::Message {
                role,
                content,
                metadata,
            } => {
                // Non-system messages → messages array
                let content_parts = prompt_to_content_parts(*content);
                let mut msg = serde_json::Map::new();
                msg.insert("role".to_string(), serde_json::Value::String(role));
                msg.insert(
                    "content".to_string(),
                    serde_json::Value::Array(content_parts),
                );

                // Add metadata (e.g., cache_control)
                if let BexExternalValue::Map { entries, .. } = *metadata {
                    for (key, value) in entries {
                        if let BexExternalValue::String(v) = value {
                            msg.insert(key, serde_json::Value::String(v));
                        } else if let BexExternalValue::Bool(v) = value {
                            msg.insert(key, serde_json::Value::Bool(v));
                        }
                    }
                }

                messages.push(serde_json::Value::Object(msg));
            }
            _ => {} // Skip non-message nodes
        }
    }

    (system_parts, messages)
}
