//! Provider-aware SSE event accumulation and content extraction.
//!
//! Extracts text content deltas from provider-specific SSE event formats
//! (`OpenAI` `choices[0].delta.content`, Anthropic `content_block_delta`).

use std::{
    collections::HashMap,
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
};

use bex_resource_types::{ResourceHandle, ResourceRegistryRef, ResourceType};

use crate::{LlmProvider, types::LlmOpError};

/// State for a single stream accumulator.
pub(crate) struct AccumulatorState {
    provider: LlmProvider,
    content: String,
    model: Option<String>,
    finish_reason: Option<String>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    is_done: bool,
}

/// Global registry for stream accumulator state.
///
/// Separate from `sys_native`'s registry because accumulator state is pure data
/// (no Tokio resources) and needs to be accessible from the blanket `SysOpLlm` impl.
struct AccumulatorRegistry {
    next_key: AtomicUsize,
    entries: RwLock<HashMap<usize, AccumulatorState>>,
}

impl AccumulatorRegistry {
    fn new() -> Self {
        Self {
            next_key: AtomicUsize::new(1),
            entries: RwLock::new(HashMap::new()),
        }
    }
}

impl ResourceRegistryRef for AccumulatorRegistry {
    fn remove(&self, key: usize) {
        self.entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&key);
    }
}

static ACCUM_REGISTRY: std::sync::LazyLock<Arc<AccumulatorRegistry>> =
    std::sync::LazyLock::new(|| Arc::new(AccumulatorRegistry::new()));

/// Create a new stream accumulator for the given provider.
///
/// Returns an error if the provider doesn't support streaming
/// (e.g. `google-ai`, `aws-bedrock`).
pub fn new_accumulator(provider_str: &str) -> Result<ResourceHandle, LlmOpError> {
    let provider = provider_str
        .parse::<LlmProvider>()
        .map_err(|_| LlmOpError::Other(format!("Unknown provider: {provider_str}")))?;

    // Reject providers that don't have streaming support in extract_delta.
    match provider {
        LlmProvider::OpenAi
        | LlmProvider::OpenAiGeneric
        | LlmProvider::AzureOpenAi
        | LlmProvider::Ollama
        | LlmProvider::OpenRouter
        | LlmProvider::OpenAiResponses
        | LlmProvider::Anthropic => {}
        _ => {
            return Err(LlmOpError::NotImplemented {
                message: format!(
                    "Streaming is not yet implemented for provider '{provider_str}'. \
                     Supported providers: openai, openai-generic, azure-openai, ollama, \
                     openrouter, openai-responses, anthropic."
                ),
            });
        }
    }

    let key = ACCUM_REGISTRY.next_key.fetch_add(1, Ordering::SeqCst);
    let state = AccumulatorState {
        provider,
        content: String::new(),
        model: None,
        finish_reason: None,
        input_tokens: None,
        output_tokens: None,
        is_done: false,
    };

    ACCUM_REGISTRY
        .entries
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, state);

    Ok(ResourceHandle::new(
        key,
        ResourceType::StreamAccumulator,
        format!("accumulator:{provider_str}"),
        Arc::clone(&ACCUM_REGISTRY) as Arc<dyn ResourceRegistryRef>,
    ))
}

/// Add SSE events to an accumulator. Events is a JSON array string.
pub fn add_events(handle: &ResourceHandle, events_json: &str) -> Result<(), LlmOpError> {
    let events: Vec<serde_json::Value> = serde_json::from_str(events_json)
        .map_err(|e| LlmOpError::Other(format!("Failed to parse events JSON: {e}")))?;

    let mut entries = ACCUM_REGISTRY
        .entries
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let state = entries
        .get_mut(&handle.key())
        .ok_or_else(|| LlmOpError::Other("Accumulator handle is invalid".into()))?;

    for event in &events {
        let data = event.get("data").and_then(|d| d.as_str()).unwrap_or("");

        if data == "[DONE]" {
            state.is_done = true;
            continue;
        }

        // Try to parse the data as JSON
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) else {
            continue;
        };

        extract_delta(state, &parsed);
    }

    Ok(())
}

/// Extract content delta from a parsed SSE event data payload.
fn extract_delta(state: &mut AccumulatorState, data: &serde_json::Value) {
    match state.provider {
        // OpenAI format: choices[0].delta.content
        LlmProvider::OpenAi
        | LlmProvider::OpenAiGeneric
        | LlmProvider::AzureOpenAi
        | LlmProvider::Ollama
        | LlmProvider::OpenRouter
        | LlmProvider::OpenAiResponses => {
            if let Some(model) = data.get("model").and_then(|m| m.as_str()) {
                state.model = Some(model.to_string());
            }
            if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
                if let Some(choice) = choices.first() {
                    if let Some(content) = choice
                        .get("delta")
                        .and_then(|d| d.get("content"))
                        .and_then(|c| c.as_str())
                    {
                        state.content.push_str(content);
                    }
                    if let Some(reason) = choice.get("finish_reason").and_then(|r| r.as_str()) {
                        state.finish_reason = Some(reason.to_string());
                        if reason == "stop" || reason == "length" {
                            state.is_done = true;
                        }
                    }
                }
            }
            // OpenAI includes usage in the final chunk when stream_options.include_usage is set.
            if let Some(usage) = data.get("usage") {
                if let Some(pt) = usage
                    .get("prompt_tokens")
                    .and_then(serde_json::Value::as_u64)
                {
                    state.input_tokens = Some(pt);
                }
                if let Some(ct) = usage
                    .get("completion_tokens")
                    .and_then(serde_json::Value::as_u64)
                {
                    state.output_tokens = Some(ct);
                }
            }
        }
        // Anthropic format: content_block_delta -> delta.text
        LlmProvider::Anthropic => {
            if let Some(event_type) = data.get("type").and_then(|t| t.as_str()) {
                match event_type {
                    "message_start" => {
                        if let Some(msg) = data.get("message") {
                            if let Some(model) = msg.get("model").and_then(|m| m.as_str()) {
                                state.model = Some(model.to_string());
                            }
                            if let Some(usage) = msg.get("usage") {
                                if let Some(it) = usage
                                    .get("input_tokens")
                                    .and_then(serde_json::Value::as_u64)
                                {
                                    state.input_tokens = Some(it);
                                }
                            }
                        }
                    }
                    "content_block_delta" => {
                        if let Some(text) = data
                            .get("delta")
                            .and_then(|d| d.get("text"))
                            .and_then(|t| t.as_str())
                        {
                            state.content.push_str(text);
                        }
                    }
                    "message_delta" => {
                        if let Some(reason) = data
                            .get("delta")
                            .and_then(|d| d.get("stop_reason"))
                            .and_then(|r| r.as_str())
                        {
                            state.finish_reason = Some(reason.to_string());
                        }
                        if let Some(usage) = data.get("usage") {
                            if let Some(ot) = usage
                                .get("output_tokens")
                                .and_then(serde_json::Value::as_u64)
                            {
                                state.output_tokens = Some(ot);
                            }
                        }
                    }
                    "message_stop" => {
                        state.is_done = true;
                    }
                    _ => {}
                }
            }
        }
        // Unsupported providers
        _ => {}
    }
}

/// Get the accumulated content.
pub fn get_content(handle: &ResourceHandle) -> Result<String, LlmOpError> {
    let entries = ACCUM_REGISTRY
        .entries
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let state = entries
        .get(&handle.key())
        .ok_or_else(|| LlmOpError::Other("Accumulator handle is invalid".into()))?;
    Ok(state.content.clone())
}

/// Check if the stream is done.
pub fn is_done(handle: &ResourceHandle) -> Result<bool, LlmOpError> {
    let entries = ACCUM_REGISTRY
        .entries
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let state = entries
        .get(&handle.key())
        .ok_or_else(|| LlmOpError::Other("Accumulator handle is invalid".into()))?;
    Ok(state.is_done)
}

/// Get the model name.
pub fn get_model(handle: &ResourceHandle) -> Result<Option<String>, LlmOpError> {
    let entries = ACCUM_REGISTRY
        .entries
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let state = entries
        .get(&handle.key())
        .ok_or_else(|| LlmOpError::Other("Accumulator handle is invalid".into()))?;
    Ok(state.model.clone())
}

/// Get the finish reason.
pub fn get_finish_reason(handle: &ResourceHandle) -> Result<Option<String>, LlmOpError> {
    let entries = ACCUM_REGISTRY
        .entries
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let state = entries
        .get(&handle.key())
        .ok_or_else(|| LlmOpError::Other("Accumulator handle is invalid".into()))?;
    Ok(state.finish_reason.clone())
}

/// Get the input/prompt token count.
pub fn get_input_tokens(handle: &ResourceHandle) -> Result<Option<u64>, LlmOpError> {
    let entries = ACCUM_REGISTRY
        .entries
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let state = entries
        .get(&handle.key())
        .ok_or_else(|| LlmOpError::Other("Accumulator handle is invalid".into()))?;
    Ok(state.input_tokens)
}

/// Get the output/completion token count.
pub fn get_output_tokens(handle: &ResourceHandle) -> Result<Option<u64>, LlmOpError> {
    let entries = ACCUM_REGISTRY
        .entries
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let state = entries
        .get(&handle.key())
        .ok_or_else(|| LlmOpError::Other("Accumulator handle is invalid".into()))?;
    Ok(state.output_tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_accumulation() {
        let handle = new_accumulator("openai").unwrap();
        let events = serde_json::json!([
            {
                "event": "message",
                "data": "{\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}",
                "id": null
            },
            {
                "event": "message",
                "data": "{\"choices\":[{\"delta\":{\"content\":\" world\"}}]}",
                "id": null
            }
        ]);
        add_events(&handle, &events.to_string()).unwrap();
        assert_eq!(get_content(&handle).unwrap(), "Hello world");
        assert!(!is_done(&handle).unwrap());
    }

    #[test]
    fn test_openai_done() {
        let handle = new_accumulator("openai").unwrap();
        let events = serde_json::json!([
            {
                "event": "message",
                "data": "[DONE]",
                "id": null
            }
        ]);
        add_events(&handle, &events.to_string()).unwrap();
        assert!(is_done(&handle).unwrap());
    }

    #[test]
    fn test_anthropic_accumulation() {
        let handle = new_accumulator("anthropic").unwrap();
        let events = serde_json::json!([
            {
                "event": "message_start",
                "data": "{\"type\":\"message_start\",\"message\":{\"model\":\"claude-3\",\"usage\":{\"input_tokens\":25}}}",
                "id": null
            },
            {
                "event": "content_block_delta",
                "data": "{\"type\":\"content_block_delta\",\"delta\":{\"text\":\"Hi\"}}",
                "id": null
            },
            {
                "event": "content_block_delta",
                "data": "{\"type\":\"content_block_delta\",\"delta\":{\"text\":\" there\"}}",
                "id": null
            },
            {
                "event": "message_delta",
                "data": "{\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":10}}",
                "id": null
            },
            {
                "event": "message_stop",
                "data": "{\"type\":\"message_stop\"}",
                "id": null
            }
        ]);
        add_events(&handle, &events.to_string()).unwrap();
        assert_eq!(get_content(&handle).unwrap(), "Hi there");
        assert!(is_done(&handle).unwrap());

        let entries = ACCUM_REGISTRY
            .entries
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let state = entries.get(&handle.key()).unwrap();
        assert_eq!(state.input_tokens, Some(25));
        assert_eq!(state.output_tokens, Some(10));
    }

    #[test]
    fn test_unsupported_provider_rejected() {
        let err = new_accumulator("google-ai").unwrap_err();
        assert!(matches!(err, LlmOpError::NotImplemented { .. }));
    }
}
