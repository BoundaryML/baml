//! OpenAI provider request transformation.
//!
//! Converts a `PromptAst` into an `HttpRequest` for the OpenAI Chat Completions API.
//!
//! # API Reference
//! - Endpoint: `{base_url}/chat/completions` (default: `https://api.openai.com/v1/chat/completions`)
//! - Auth: `Authorization: Bearer {api_key}`
//! - Body: `{ "model": "...", "messages": [...], ... }`

use bex_llm_types::{HttpRequest, ResolvedClient};
use bex_vm_types::PromptAst;

use super::ProviderError;

/// Default base URL for OpenAI API.
pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// Build an HTTP request for the OpenAI Chat Completions API.
pub fn build_request(
    prompt: &PromptAst,
    client: &ResolvedClient,
) -> Result<HttpRequest, ProviderError> {
    // Get base URL from options or use default
    let base_url = client
        .options
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_BASE_URL);

    let endpoint = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    // Build the request
    let mut request = HttpRequest::post(&endpoint);

    // Add authorization if api_key is present
    if let Some(api_key) = client.options.get("api_key").and_then(|v| v.as_str()) {
        request = request.bearer_auth(api_key);
    }

    // Add any custom headers from options
    if let Some(headers) = client.options.get("headers").and_then(|v| v.as_object()) {
        for (key, value) in headers {
            if let Some(v) = value.as_str() {
                request = request.header(key, v);
            }
        }
    }

    // Build the request body
    let body = build_body(prompt, client)?;
    request = request.json(body);

    Ok(request)
}

/// Build the JSON body for the OpenAI request.
fn build_body(
    prompt: &PromptAst,
    client: &ResolvedClient,
) -> Result<serde_json::Value, ProviderError> {
    let mut body = serde_json::Map::new();

    // Add model (required)
    let model = client
        .options
        .get("model")
        .ok_or_else(|| ProviderError::MissingOption("model".to_string()))?;
    body.insert("model".to_string(), model.clone());

    // Convert prompt to messages
    let messages = prompt_to_messages(prompt)?;
    body.insert("messages".to_string(), serde_json::Value::Array(messages));

    // Add optional parameters from client options
    for (key, value) in &client.options {
        // Skip keys that are handled separately or not part of the API body
        if matches!(
            key.as_str(),
            "model" | "base_url" | "api_key" | "headers" | "http"
        ) {
            continue;
        }
        body.insert(key.clone(), value.clone());
    }

    Ok(serde_json::Value::Object(body))
}

/// Convert a `PromptAst` to OpenAI message format.
fn prompt_to_messages(prompt: &PromptAst) -> Result<Vec<serde_json::Value>, ProviderError> {
    match prompt {
        PromptAst::Message {
            role,
            content,
            metadata: _,
        } => {
            let msg = build_message(role, content)?;
            Ok(vec![msg])
        }
        PromptAst::Vec(nodes) => {
            let mut messages = Vec::new();
            for node in nodes {
                messages.extend(prompt_to_messages(node)?);
            }
            Ok(messages)
        }
        PromptAst::String(s) => {
            // A bare string becomes a user message
            Ok(vec![serde_json::json!({
                "role": "user",
                "content": s
            })])
        }
        PromptAst::Media(_) => Err(ProviderError::InvalidPrompt(
            "bare media not allowed at top level; wrap in a message".to_string(),
        )),
        PromptAst::PrintType { .. } => Err(ProviderError::InvalidPrompt(
            "PrintType not allowed in prompt; should be rendered first".to_string(),
        )),
    }
}

/// Build a single OpenAI message from a PromptAst Message node.
fn build_message(
    role: &str,
    content: &PromptAst,
) -> Result<serde_json::Value, ProviderError> {
    let content_value = build_content(content)?;

    let mut msg = serde_json::Map::new();
    msg.insert("role".to_string(), serde_json::Value::String(role.to_string()));
    msg.insert("content".to_string(), content_value);

    // Note: metadata is Value::Null in VM PromptAst, so we skip it for now
    // In the future, we could convert Value to JSON if metadata is populated

    Ok(serde_json::Value::Object(msg))
}

/// Build the content field for an OpenAI message.
///
/// Returns either a string (for simple text) or an array of content parts
/// (for mixed text/media content).
fn build_content(content: &PromptAst) -> Result<serde_json::Value, ProviderError> {
    match content {
        PromptAst::String(s) => Ok(serde_json::Value::String(s.clone())),
        PromptAst::Media(heap_ptr) => {
            // SAFETY: The HeapPtr is valid during the build_request call.
            // The PromptAst was just created and hasn't been garbage collected.
            let media = unsafe {
                match heap_ptr.get() {
                    bex_vm_types::Object::Media(m) => m,
                    other => {
                        return Err(ProviderError::InvalidPrompt(format!(
                            "expected Media object, got {:?}",
                            bex_vm_types::ObjectType::of(other)
                        )));
                    }
                }
            };
            // Single media becomes a content array with one item
            let part = media_to_content_part(media)?;
            Ok(serde_json::Value::Array(vec![part]))
        }
        PromptAst::Vec(nodes) => {
            // Multiple content parts
            let mut parts = Vec::new();
            for node in nodes {
                parts.extend(flatten_content_parts(node)?);
            }
            Ok(serde_json::Value::Array(parts))
        }
        PromptAst::Message { .. } => Err(ProviderError::InvalidPrompt(
            "nested messages not allowed in content".to_string(),
        )),
        PromptAst::PrintType { .. } => Err(ProviderError::InvalidPrompt(
            "PrintType not allowed in content".to_string(),
        )),
    }
}

/// Flatten content nodes into content parts.
fn flatten_content_parts(content: &PromptAst) -> Result<Vec<serde_json::Value>, ProviderError> {
    match content {
        PromptAst::String(s) => Ok(vec![serde_json::json!({
            "type": "text",
            "text": s
        })]),
        PromptAst::Media(heap_ptr) => {
            // SAFETY: The HeapPtr is valid during the build_request call.
            let media = unsafe {
                match heap_ptr.get() {
                    bex_vm_types::Object::Media(m) => m,
                    other => {
                        return Err(ProviderError::InvalidPrompt(format!(
                            "expected Media object, got {:?}",
                            bex_vm_types::ObjectType::of(other)
                        )));
                    }
                }
            };
            let part = media_to_content_part(media)?;
            Ok(vec![part])
        }
        PromptAst::Vec(nodes) => {
            let mut parts = Vec::new();
            for node in nodes {
                parts.extend(flatten_content_parts(node)?);
            }
            Ok(parts)
        }
        PromptAst::Message { .. } => Err(ProviderError::InvalidPrompt(
            "nested messages not allowed in content".to_string(),
        )),
        PromptAst::PrintType { .. } => Err(ProviderError::InvalidPrompt(
            "PrintType not allowed in content".to_string(),
        )),
    }
}

/// Convert a MediaValue to an OpenAI content part.
fn media_to_content_part(
    media: &bex_vm_types::MediaValue,
) -> Result<serde_json::Value, ProviderError> {
    use baml_base::MediaKind;
    use bex_vm_types::MediaContent;

    // Get mime type, defaulting based on media kind
    let mime_type = media.mime_type.as_deref().unwrap_or_else(|| match media.kind {
        MediaKind::Image => "image/png",
        MediaKind::Audio => "audio/wav",
        MediaKind::Video => "video/mp4",
        MediaKind::Pdf => "application/pdf",
        MediaKind::Generic => "application/octet-stream",
    });

    match media.kind {
        MediaKind::Image => {
            let image_url = match &media.content {
                MediaContent::Url { url, .. } => url.clone(),
                MediaContent::Base64 { base64_data } => {
                    format!("data:{};base64,{}", mime_type, base64_data)
                }
                MediaContent::File { .. } => {
                    return Err(ProviderError::InvalidPrompt(
                        "file references should be resolved before building request".to_string(),
                    ));
                }
            };

            Ok(serde_json::json!({
                "type": "image_url",
                "image_url": {
                    "url": image_url
                }
            }))
        }
        MediaKind::Audio => {
            // OpenAI audio input format
            match &media.content {
                MediaContent::Base64 { base64_data } => {
                    // Extract format from mime type (e.g., "audio/wav" -> "wav")
                    let format = mime_type
                        .strip_prefix("audio/")
                        .unwrap_or("wav");

                    Ok(serde_json::json!({
                        "type": "input_audio",
                        "input_audio": {
                            "data": base64_data,
                            "format": format
                        }
                    }))
                }
                _ => Err(ProviderError::InvalidPrompt(
                    "OpenAI requires audio to be base64 encoded".to_string(),
                )),
            }
        }
        MediaKind::Video => Err(ProviderError::InvalidPrompt(
            "OpenAI does not support video input".to_string(),
        )),
        MediaKind::Pdf | MediaKind::Generic => Err(ProviderError::InvalidPrompt(
            "OpenAI does not support document input in chat completions".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bex_llm_types::{ModelFeatures, RoleConfig};
    use bex_vm_types::Value;
    use indexmap::IndexMap;

    fn make_client(model: &str) -> ResolvedClient {
        let mut options = IndexMap::new();
        options.insert("model".to_string(), serde_json::json!(model));
        options.insert("api_key".to_string(), serde_json::json!("test-key"));

        ResolvedClient {
            name: "test-client".to_string(),
            provider: "openai".to_string(),
            roles: RoleConfig::default(),
            features: ModelFeatures::default(),
            options,
            request_config: Default::default(),
        }
    }

    #[test]
    fn test_basic_request() {
        let prompt = PromptAst::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::String("Hello".to_string())),
            metadata: Value::Null,
        };
        let client = make_client("gpt-4");

        let request = build_request(&prompt, &client).unwrap();

        assert_eq!(request.url, "https://api.openai.com/v1/chat/completions");
        assert_eq!(
            request.headers.get("Authorization"),
            Some(&"Bearer test-key".to_string())
        );
    }

    #[test]
    fn test_bare_string_becomes_user_message() {
        let prompt = PromptAst::String("Hello".to_string());
        let client = make_client("gpt-4");

        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                assert_eq!(messages.len(), 1);
                assert_eq!(messages[0].get("role").unwrap(), "user");
                assert_eq!(messages[0].get("content").unwrap(), "Hello");
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[test]
    fn test_custom_base_url() {
        let prompt = PromptAst::String("Hello".to_string());
        let mut client = make_client("custom-model");
        client.options.insert(
            "base_url".to_string(),
            serde_json::json!("https://custom.api.example.com/v1"),
        );

        let request = build_request(&prompt, &client).unwrap();

        assert_eq!(
            request.url,
            "https://custom.api.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_missing_model_error() {
        let prompt = PromptAst::String("Hello".to_string());
        let client = ResolvedClient {
            name: "test".to_string(),
            provider: "openai".to_string(),
            roles: RoleConfig::default(),
            features: ModelFeatures::default(),
            options: IndexMap::new(), // No model!
            request_config: Default::default(),
        };

        let result = build_request(&prompt, &client);
        assert!(matches!(result, Err(ProviderError::MissingOption(_))));
    }

    #[test]
    fn test_multiple_messages() {
        let prompt = PromptAst::Vec(vec![
            PromptAst::Message {
                role: "system".to_string(),
                content: Box::new(PromptAst::String("You are helpful.".to_string())),
                metadata: Value::Null,
            },
            PromptAst::Message {
                role: "user".to_string(),
                content: Box::new(PromptAst::String("Hello".to_string())),
                metadata: Value::Null,
            },
        ]);
        let client = make_client("gpt-4");

        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                assert_eq!(messages.len(), 2);
                assert_eq!(messages[0].get("role").unwrap(), "system");
                assert_eq!(messages[1].get("role").unwrap(), "user");
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[test]
    fn test_custom_headers() {
        let prompt = PromptAst::String("Hello".to_string());
        let mut client = make_client("gpt-4");
        client.options.insert(
            "headers".to_string(),
            serde_json::json!({
                "X-Custom-Header": "custom-value"
            }),
        );

        let request = build_request(&prompt, &client).unwrap();

        assert_eq!(
            request.headers.get("X-Custom-Header"),
            Some(&"custom-value".to_string())
        );
    }

    #[test]
    fn test_additional_options_passed_to_body() {
        let prompt = PromptAst::String("Hello".to_string());
        let mut client = make_client("gpt-4");
        client
            .options
            .insert("temperature".to_string(), serde_json::json!(0.7));
        client
            .options
            .insert("max_tokens".to_string(), serde_json::json!(100));

        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                assert_eq!(body.get("temperature").unwrap(), 0.7);
                assert_eq!(body.get("max_tokens").unwrap(), 100);
            }
            _ => panic!("expected JSON body"),
        }
    }
}
