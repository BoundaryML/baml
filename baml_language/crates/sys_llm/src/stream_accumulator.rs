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
    completion_emitted: bool,
    observer: Option<sys_types::LlmCallObserver>,
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
    new_accumulator_with_observer(provider_str, None)
}

/// Create a stream accumulator that retains the structurally owned LLM
/// observer even after the function which created the stream has returned.
pub fn new_accumulator_with_observer(
    provider_str: &str,
    observer: Option<sys_types::LlmCallObserver>,
) -> Result<ResourceHandle, LlmOpError> {
    let provider = provider_str
        .parse::<LlmProvider>()
        .map_err(|_| LlmOpError::Other(format!("Unknown provider: {provider_str}")))?;

    // Reject providers that don't have streaming support in extract_delta.
    // Note: OpenAiResponses is intentionally excluded — its streaming format
    // uses `output: [{ type: "message", content: [{ text: "..." }] }]` and
    // `status` instead of chat-completions' `choices[0].delta.content` and
    // `finish_reason`, and extract_delta() does not yet handle that shape.
    match provider {
        LlmProvider::OpenAi
        | LlmProvider::OpenAiGeneric
        | LlmProvider::AzureOpenAi
        | LlmProvider::Ollama
        | LlmProvider::OpenRouter
        | LlmProvider::Anthropic => {}
        _ => {
            return Err(LlmOpError::NotImplemented {
                message: format!(
                    "Streaming is not yet implemented for provider '{provider_str}'. \
                     Supported providers: openai, openai-generic, azure-openai, ollama, \
                     openrouter, anthropic."
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
        completion_emitted: false,
        observer,
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
        // OpenAI Chat Completions format: choices[0].delta.content
        // (OpenAiResponses uses a different shape and is rejected upstream
        // by new_accumulator until this function learns to parse it.)
        LlmProvider::OpenAi
        | LlmProvider::OpenAiGeneric
        | LlmProvider::AzureOpenAi
        | LlmProvider::Ollama
        | LlmProvider::OpenRouter => {
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

/// Emit the final normalized observation once, after the caller has
/// successfully parsed the accumulated stream into its final BAML value.
///
/// The callback runs after releasing the registry lock: the engine observer
/// may intern model metadata and emit into the profiling ring.
pub fn complete_observation(handle: &ResourceHandle) -> Result<(), LlmOpError> {
    let pending = {
        let mut entries = ACCUM_REGISTRY
            .entries
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let state = entries
            .get_mut(&handle.key())
            .ok_or_else(|| LlmOpError::Other("Accumulator handle is invalid".into()))?;
        if state.completion_emitted {
            return Ok(());
        }
        state.completion_emitted = true;
        state.observer.clone().map(|observer| {
            let tokens_in =
                u32::try_from(state.input_tokens.unwrap_or_default()).unwrap_or(u32::MAX);
            let tokens_out =
                u32::try_from(state.output_tokens.unwrap_or_default()).unwrap_or(u32::MAX);
            (
                observer,
                sys_types::LlmCallObservation::Completed {
                    model: state.model.clone().unwrap_or_default(),
                    tokens_in,
                    tokens_out,
                    parse_error: false,
                },
            )
        })
    };
    if let Some((observer, observation)) = pending {
        observer(observation);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_observation_uses_final_metadata_and_emits_once() {
        let observations = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = Arc::clone(&observations);
        let observer: sys_types::LlmCallObserver = Arc::new(move |observation| {
            captured
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(observation);
        });
        let handle = new_accumulator_with_observer("openai", Some(observer)).unwrap();
        let events = serde_json::json!([
            {
                "event": "message",
                "data": "{\"model\":\"gpt-stream-final\",\"choices\":[{\"delta\":{\"content\":\"done\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":13,\"completion_tokens\":5}}",
                "id": null
            }
        ]);
        add_events(&handle, &events.to_string()).unwrap();
        assert!(
            observations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty(),
            "transport completion alone must not report a successful final parse"
        );

        complete_observation(&handle).unwrap();
        complete_observation(&handle).unwrap();

        assert_eq!(
            *observations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            vec![sys_types::LlmCallObservation::Completed {
                model: "gpt-stream-final".into(),
                tokens_in: 13,
                tokens_out: 5,
                parse_error: false,
            }]
        );
    }

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

    /// Helper: wrap data strings into the event array format expected by `add_events`.
    fn events_json(data_strings: &[&str]) -> String {
        let events: Vec<serde_json::Value> = data_strings
            .iter()
            .map(|d| serde_json::json!({"event": "message", "data": *d, "id": null}))
            .collect();
        serde_json::to_string(&events).unwrap()
    }

    // ========================================================================
    // Provider creation
    // ========================================================================

    #[test]
    fn test_all_supported_providers_accepted() {
        let providers = [
            "openai",
            "openai-generic",
            "azure-openai",
            "ollama",
            "openrouter",
            "anthropic",
        ];
        for p in providers {
            let handle =
                new_accumulator(p).unwrap_or_else(|e| panic!("{p} should be accepted: {e}"));
            assert_eq!(get_content(&handle).unwrap(), "");
            assert!(!is_done(&handle).unwrap());
            assert_eq!(get_model(&handle).unwrap(), None);
            assert_eq!(get_finish_reason(&handle).unwrap(), None);
            assert_eq!(get_input_tokens(&handle).unwrap(), None);
            assert_eq!(get_output_tokens(&handle).unwrap(), None);
        }
    }

    #[test]
    fn test_all_unsupported_providers_rejected() {
        // openai-responses uses a different streaming shape (output[].content[].text
        // + status) that extract_delta does not yet parse; treat it as unsupported
        // until that handling lands.
        for p in [
            "google-ai",
            "aws-bedrock",
            "vertex-ai",
            "openai-responses",
            "ai-gateway-images",
        ] {
            let err = new_accumulator(p).unwrap_err();
            assert!(
                matches!(err, LlmOpError::NotImplemented { .. }),
                "{p} should be rejected as NotImplemented, got: {err:?}"
            );
        }
    }

    #[test]
    fn test_invalid_provider_string_rejected() {
        let err = new_accumulator("not-a-provider").unwrap_err();
        assert!(
            matches!(err, LlmOpError::Other(_)),
            "invalid provider should be Other error, got: {err:?}"
        );
    }

    // ========================================================================
    // OpenAI format edge cases
    // ========================================================================

    #[test]
    fn test_openai_finish_reason_stop() {
        let handle = new_accumulator("openai").unwrap();
        let json =
            events_json(&[r#"{"choices":[{"delta":{"content":"hi"},"finish_reason":"stop"}]}"#]);
        add_events(&handle, &json).unwrap();
        assert!(is_done(&handle).unwrap());
        assert_eq!(get_finish_reason(&handle).unwrap(), Some("stop".into()));
    }

    #[test]
    fn test_openai_finish_reason_length() {
        let handle = new_accumulator("openai").unwrap();
        let json =
            events_json(&[r#"{"choices":[{"delta":{"content":"hi"},"finish_reason":"length"}]}"#]);
        add_events(&handle, &json).unwrap();
        assert!(is_done(&handle).unwrap());
        assert_eq!(get_finish_reason(&handle).unwrap(), Some("length".into()));
    }

    #[test]
    fn test_openai_finish_reason_other_does_not_set_done() {
        let handle = new_accumulator("openai").unwrap();
        let json = events_json(&[
            r#"{"choices":[{"delta":{"content":""},"finish_reason":"content_filter"}]}"#,
        ]);
        add_events(&handle, &json).unwrap();
        // finish_reason is stored but is_done is NOT set for non-stop/length reasons.
        assert!(!is_done(&handle).unwrap());
        assert_eq!(
            get_finish_reason(&handle).unwrap(),
            Some("content_filter".into())
        );
    }

    #[test]
    fn test_openai_model_extraction() {
        let handle = new_accumulator("openai").unwrap();
        let json = events_json(&[
            r#"{"model":"gpt-4o-2024-05-13","choices":[{"delta":{"content":"x"}}]}"#,
        ]);
        add_events(&handle, &json).unwrap();
        assert_eq!(
            get_model(&handle).unwrap(),
            Some("gpt-4o-2024-05-13".into())
        );
    }

    #[test]
    fn test_openai_usage_tokens() {
        let handle = new_accumulator("openai").unwrap();
        let json = events_json(&[
            r#"{"choices":[{"delta":{}}],"usage":{"prompt_tokens":42,"completion_tokens":17}}"#,
        ]);
        add_events(&handle, &json).unwrap();
        assert_eq!(get_input_tokens(&handle).unwrap(), Some(42));
        assert_eq!(get_output_tokens(&handle).unwrap(), Some(17));
    }

    #[test]
    fn test_openai_empty_delta() {
        let handle = new_accumulator("openai").unwrap();
        // First chunk adds content.
        add_events(
            &handle,
            &events_json(&[r#"{"choices":[{"delta":{"content":"hello"}}]}"#]),
        )
        .unwrap();
        // Second chunk has empty delta (no content key) — common for role-only chunks.
        add_events(&handle, &events_json(&[r#"{"choices":[{"delta":{}}]}"#])).unwrap();
        assert_eq!(get_content(&handle).unwrap(), "hello");
    }

    #[test]
    fn test_openai_null_content_in_delta() {
        let handle = new_accumulator("openai").unwrap();
        add_events(
            &handle,
            &events_json(&[r#"{"choices":[{"delta":{"content":"hi"}}]}"#]),
        )
        .unwrap();
        // null content should not change accumulated content.
        add_events(
            &handle,
            &events_json(&[r#"{"choices":[{"delta":{"content":null}}]}"#]),
        )
        .unwrap();
        assert_eq!(get_content(&handle).unwrap(), "hi");
    }

    #[test]
    fn test_openai_multiple_chunks_accumulate() {
        let handle = new_accumulator("openai").unwrap();
        let chunks = ["Hello", ", ", "world", "! ", "How ", "are ", "you?"];
        for chunk in chunks {
            let json = events_json(&[&format!(
                r#"{{"choices":[{{"delta":{{"content":"{chunk}"}}}}]}}"#
            )]);
            add_events(&handle, &json).unwrap();
        }
        assert_eq!(get_content(&handle).unwrap(), "Hello, world! How are you?");
    }

    #[test]
    fn test_openai_invalid_json_in_data_skipped() {
        let handle = new_accumulator("openai").unwrap();
        add_events(
            &handle,
            &events_json(&[r#"{"choices":[{"delta":{"content":"before"}}]}"#]),
        )
        .unwrap();
        // Invalid JSON in data field is silently skipped.
        add_events(&handle, &events_json(&["not valid json"])).unwrap();
        assert_eq!(get_content(&handle).unwrap(), "before");
    }

    #[test]
    fn test_openai_done_after_finish_reason() {
        let handle = new_accumulator("openai").unwrap();
        // finish_reason="stop" sets done.
        add_events(
            &handle,
            &events_json(&[r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#]),
        )
        .unwrap();
        assert!(is_done(&handle).unwrap());

        // [DONE] also sets done — no issues from double-set.
        add_events(&handle, &events_json(&["[DONE]"])).unwrap();
        assert!(is_done(&handle).unwrap());
    }

    // ========================================================================
    // Anthropic format edge cases
    // ========================================================================

    #[test]
    fn test_anthropic_content_block_delta_accumulates() {
        let handle = new_accumulator("anthropic").unwrap();
        let chunks = ["Hello", " ", "world"];
        for chunk in chunks {
            let json = events_json(&[&format!(
                r#"{{"type":"content_block_delta","delta":{{"text":"{chunk}"}}}}"#
            )]);
            add_events(&handle, &json).unwrap();
        }
        assert_eq!(get_content(&handle).unwrap(), "Hello world");
    }

    #[test]
    fn test_anthropic_message_start_extracts_model_and_input_tokens() {
        let handle = new_accumulator("anthropic").unwrap();
        let json = events_json(&[
            r#"{"type":"message_start","message":{"model":"claude-3-5-sonnet","usage":{"input_tokens":100}}}"#,
        ]);
        add_events(&handle, &json).unwrap();
        assert_eq!(
            get_model(&handle).unwrap(),
            Some("claude-3-5-sonnet".into())
        );
        assert_eq!(get_input_tokens(&handle).unwrap(), Some(100));
    }

    #[test]
    fn test_anthropic_message_delta_extracts_stop_reason_and_output_tokens() {
        let handle = new_accumulator("anthropic").unwrap();
        let json = events_json(&[
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":55}}"#,
        ]);
        add_events(&handle, &json).unwrap();
        assert_eq!(get_finish_reason(&handle).unwrap(), Some("end_turn".into()));
        assert_eq!(get_output_tokens(&handle).unwrap(), Some(55));
    }

    #[test]
    fn test_anthropic_message_stop_sets_done() {
        let handle = new_accumulator("anthropic").unwrap();
        // message_stop alone (without prior message_delta) sets is_done.
        let json = events_json(&[r#"{"type":"message_stop"}"#]);
        add_events(&handle, &json).unwrap();
        assert!(is_done(&handle).unwrap());
    }

    #[test]
    fn test_anthropic_unknown_event_type_ignored() {
        let handle = new_accumulator("anthropic").unwrap();
        add_events(
            &handle,
            &events_json(&[r#"{"type":"content_block_delta","delta":{"text":"hi"}}"#]),
        )
        .unwrap();
        // "ping" event type is silently ignored.
        add_events(&handle, &events_json(&[r#"{"type":"ping"}"#])).unwrap();
        assert_eq!(get_content(&handle).unwrap(), "hi");
        assert!(!is_done(&handle).unwrap());
    }

    // ========================================================================
    // Cross-provider
    // ========================================================================

    #[test]
    fn test_done_signal_is_idempotent() {
        let handle = new_accumulator("openai").unwrap();
        add_events(&handle, &events_json(&["[DONE]"])).unwrap();
        assert!(is_done(&handle).unwrap());
        add_events(&handle, &events_json(&["[DONE]"])).unwrap();
        assert!(is_done(&handle).unwrap());
    }

    #[test]
    fn test_events_after_done_still_processed() {
        // The accumulator doesn't gate on is_done — content chunks after [DONE]
        // are still appended. This pins current behavior.
        let handle = new_accumulator("openai").unwrap();
        add_events(&handle, &events_json(&["[DONE]"])).unwrap();
        assert!(is_done(&handle).unwrap());

        add_events(
            &handle,
            &events_json(&[r#"{"choices":[{"delta":{"content":"late"}}]}"#]),
        )
        .unwrap();
        assert_eq!(get_content(&handle).unwrap(), "late");
    }

    #[test]
    fn test_empty_events_array() {
        let handle = new_accumulator("openai").unwrap();
        add_events(&handle, "[]").unwrap();
        assert_eq!(get_content(&handle).unwrap(), "");
        assert!(!is_done(&handle).unwrap());
    }

    #[test]
    fn test_invalid_events_json() {
        let handle = new_accumulator("openai").unwrap();
        let err = add_events(&handle, "not-json").unwrap_err();
        assert!(
            matches!(err, LlmOpError::Other(_)),
            "invalid JSON should be Other error, got: {err:?}"
        );
    }

    // ========================================================================
    // Resource lifecycle
    // ========================================================================

    #[test]
    fn test_handle_drop_cleans_up_registry() {
        let handle = new_accumulator("openai").unwrap();
        let key = handle.key();

        // Entry exists while handle is alive.
        assert!(ACCUM_REGISTRY.entries.read().unwrap().contains_key(&key));

        drop(handle);

        // Entry removed after handle is dropped.
        assert!(!ACCUM_REGISTRY.entries.read().unwrap().contains_key(&key));
    }

    #[test]
    fn test_concurrent_accumulators_independent() {
        let h1 = new_accumulator("openai").unwrap();
        let h2 = new_accumulator("anthropic").unwrap();

        add_events(
            &h1,
            &events_json(&[r#"{"choices":[{"delta":{"content":"openai-text"}}]}"#]),
        )
        .unwrap();
        add_events(
            &h2,
            &events_json(&[r#"{"type":"content_block_delta","delta":{"text":"anthropic-text"}}"#]),
        )
        .unwrap();

        assert_eq!(get_content(&h1).unwrap(), "openai-text");
        assert_eq!(get_content(&h2).unwrap(), "anthropic-text");
        assert!(!is_done(&h1).unwrap());
        assert!(!is_done(&h2).unwrap());
    }
}
