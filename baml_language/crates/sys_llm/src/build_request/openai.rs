//! OpenAI-format HTTP request builder.
//!
//! Supports: `OpenAi`, `OpenAiGeneric`, `AzureOpenAi`, Ollama, `OpenRouter`.

use bex_external_types::{BexExternalValue, PrimitiveClientValue, PromptAst};
use indexmap::IndexMap;

use super::{BuildRequestError, LlmRequestBuilder, get_string_option, prompt_to_content_parts};
use crate::LlmProvider;

/// Builder for OpenAI-compatible providers.
pub(crate) struct OpenAiBuilder<'a> {
    provider: &'a LlmProvider,
}

impl<'a> OpenAiBuilder<'a> {
    pub(crate) fn new(provider: &'a LlmProvider) -> Self {
        Self { provider }
    }
}

impl LlmRequestBuilder for OpenAiBuilder<'_> {
    fn provider_skip_keys(&self) -> &'static [&'static str] {
        &["resource_name", "api_version"]
    }

    fn build_url(&self, client: &PrimitiveClientValue) -> Result<String, BuildRequestError> {
        let base_url = get_string_option(client, "base_url")
            .unwrap_or_else(|| "https://api.openai.com".to_string());

        // Azure uses a different URL pattern
        if *self.provider == LlmProvider::AzureOpenAi {
            let deployment = get_string_option(client, "resource_name")
                .ok_or_else(|| BuildRequestError::MissingOption("resource_name".into()))?;
            let model = get_string_option(client, "model")
                .ok_or_else(|| BuildRequestError::MissingOption("model".into()))?;
            let api_version = get_string_option(client, "api_version")
                .unwrap_or_else(|| "2024-02-15-preview".to_string());
            return Ok(format!(
                "https://{deployment}.openai.azure.com/openai/deployments/{model}/chat/completions?api-version={api_version}"
            ));
        }

        Ok(format!("{base_url}/v1/chat/completions"))
    }

    fn build_auth_headers(&self, client: &PrimitiveClientValue) -> IndexMap<String, String> {
        let mut headers = IndexMap::new();
        if let Some(api_key) = get_string_option(client, "api_key") {
            if *self.provider == LlmProvider::AzureOpenAi {
                headers.insert("api-key".to_string(), api_key);
            } else {
                headers.insert("authorization".to_string(), format!("Bearer {api_key}"));
            }
        }
        headers
    }

    fn build_prompt_body(&self, prompt: PromptAst) -> serde_json::Map<String, serde_json::Value> {
        let mut map = serde_json::Map::new();
        let messages = prompt_to_openai_messages(prompt);
        map.insert("messages".to_string(), serde_json::Value::Array(messages));
        map
    }
}

/// Convert `PromptAst` to `OpenAI` message format.
///
/// `OpenAI` format:
/// ```json
/// [{"role": "system", "content": "..."}, {"role": "user", "content": [{"type": "text", "text": "..."}]}]
/// ```
fn prompt_to_openai_messages(prompt: PromptAst) -> Vec<serde_json::Value> {
    match prompt {
        PromptAst::Vec(items) => items
            .into_iter()
            .filter_map(prompt_node_to_message)
            .collect(),
        single => prompt_node_to_message(single).into_iter().collect(),
    }
}

fn prompt_node_to_message(node: PromptAst) -> Option<serde_json::Value> {
    match node {
        PromptAst::Message {
            role,
            content,
            metadata,
        } => {
            let content_parts = prompt_to_content_parts(*content);
            let mut msg = serde_json::Map::new();
            msg.insert("role".to_string(), serde_json::Value::String(role));

            // Always use array format for content parts.
            msg.insert(
                "content".to_string(),
                serde_json::Value::Array(content_parts),
            );

            // Add metadata (e.g., cache_control) if present
            if let BexExternalValue::Map { entries, .. } = *metadata {
                for (key, value) in entries {
                    if let BexExternalValue::String(v) = value {
                        msg.insert(key, serde_json::Value::String(v));
                    } else if let BexExternalValue::Bool(v) = value {
                        msg.insert(key, serde_json::Value::Bool(v));
                    }
                }
            }

            Some(serde_json::Value::Object(msg))
        }
        _ => None, // Skip non-message nodes at top level
    }
}
