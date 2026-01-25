//! Anthropic provider request transformation.
//!
//! Converts a `PromptAst` into an `HttpRequest` for the Anthropic Messages API.
//!
//! # API Reference
//! - Endpoint: `{base_url}/v1/messages` (default: `https://api.anthropic.com/v1/messages`)
//! - Auth: `x-api-key: {api_key}`
//! - Body: `{ "model": "...", "messages": [...], "max_tokens": ..., ... }`

use bex_llm_types::{HttpRequest, PromptAst, PromptAstNode, ResolvedClient};

use super::ProviderError;

/// Default base URL for Anthropic API.
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// Default API version header value.
pub const DEFAULT_API_VERSION: &str = "2023-06-01";

/// Default max_tokens if not specified.
pub const DEFAULT_MAX_TOKENS: u64 = 4096;

/// Build an HTTP request for the Anthropic Messages API.
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

    let endpoint = format!("{}/v1/messages", base_url.trim_end_matches('/'));

    // Build the request
    let mut request = HttpRequest::post(&endpoint);

    // Add API key header (Anthropic uses x-api-key, not Bearer auth)
    if let Some(api_key) = client.options.get("api_key").and_then(|v| v.as_str()) {
        request = request.header("x-api-key", api_key);
    }

    // Add anthropic-version header
    let api_version = client
        .options
        .get("anthropic_version")
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_API_VERSION);
    request = request.header("anthropic-version", api_version);

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

/// Build the JSON body for the Anthropic request.
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

    // Convert prompt to messages and extract system message
    let (system_content, messages) = prompt_to_messages(prompt)?;

    // Add system message at top level if present
    if let Some(system) = system_content {
        body.insert("system".to_string(), system);
    }

    body.insert("messages".to_string(), serde_json::Value::Array(messages));

    // Add max_tokens (required by Anthropic)
    if !client.options.contains_key("max_tokens") {
        body.insert("max_tokens".to_string(), serde_json::json!(DEFAULT_MAX_TOKENS));
    }

    // Add optional parameters from client options
    for (key, value) in &client.options {
        // Skip keys that are handled separately or not part of the API body
        if matches!(
            key.as_str(),
            "model" | "base_url" | "api_key" | "headers" | "http" | "anthropic_version"
        ) {
            continue;
        }
        body.insert(key.clone(), value.clone());
    }

    Ok(serde_json::Value::Object(body))
}

/// Convert a `PromptAst` to Anthropic message format.
///
/// Returns (system_content, messages) where system_content is the extracted
/// system message (Anthropic requires system to be at the top level) and
/// messages is the list of user/assistant messages.
fn prompt_to_messages(
    prompt: &PromptAst,
) -> Result<(Option<serde_json::Value>, Vec<serde_json::Value>), ProviderError> {
    let mut system_content: Option<serde_json::Value> = None;
    let mut messages = Vec::new();

    collect_messages(prompt, &mut system_content, &mut messages)?;

    Ok((system_content, messages))
}

/// Recursively collect messages from a PromptAst.
fn collect_messages(
    prompt: &PromptAst,
    system_content: &mut Option<serde_json::Value>,
    messages: &mut Vec<serde_json::Value>,
) -> Result<(), ProviderError> {
    match &prompt.node {
        PromptAstNode::Message {
            role,
            content,
            metadata,
        } => {
            if role == "system" {
                // Anthropic puts system content at top level
                let content_value = build_content(content)?;
                *system_content = Some(content_value);
            } else {
                let msg = build_message(role, content, metadata)?;
                messages.push(msg);
            }
        }
        PromptAstNode::Vec(nodes) => {
            for node in nodes {
                collect_messages(node, system_content, messages)?;
            }
        }
        PromptAstNode::Str(s) => {
            // A bare string becomes a user message
            messages.push(serde_json::json!({
                "role": "user",
                "content": s
            }));
        }
        PromptAstNode::Media(_) => {
            return Err(ProviderError::InvalidPrompt(
                "bare media not allowed at top level; wrap in a message".to_string(),
            ));
        }
    }
    Ok(())
}

/// Build a single Anthropic message from a PromptAst Message node.
fn build_message(
    role: &str,
    content: &PromptAst,
    metadata: &serde_json::Map<String, serde_json::Value>,
) -> Result<serde_json::Value, ProviderError> {
    let content_value = build_content(content)?;

    let mut msg = serde_json::Map::new();
    msg.insert("role".to_string(), serde_json::Value::String(role.to_string()));
    msg.insert("content".to_string(), content_value);

    // Add metadata fields to the message (e.g., cache_control)
    for (key, value) in metadata {
        msg.insert(key.clone(), value.clone());
    }

    Ok(serde_json::Value::Object(msg))
}

/// Build the content field for an Anthropic message.
///
/// Anthropic always uses content blocks format: [{"type": "text", "text": "..."}]
fn build_content(content: &PromptAst) -> Result<serde_json::Value, ProviderError> {
    match &content.node {
        PromptAstNode::Str(s) => {
            // Anthropic prefers content blocks even for simple text
            Ok(serde_json::json!([{
                "type": "text",
                "text": s
            }]))
        }
        PromptAstNode::Media(media) => {
            let part = media_to_content_block(media)?;
            Ok(serde_json::Value::Array(vec![part]))
        }
        PromptAstNode::Vec(nodes) => {
            let mut parts = Vec::new();
            for node in nodes {
                parts.extend(flatten_content_blocks(node)?);
            }
            Ok(serde_json::Value::Array(parts))
        }
        PromptAstNode::Message { .. } => Err(ProviderError::InvalidPrompt(
            "nested messages not allowed in content".to_string(),
        )),
    }
}

/// Flatten content nodes into Anthropic content blocks.
fn flatten_content_blocks(content: &PromptAst) -> Result<Vec<serde_json::Value>, ProviderError> {
    match &content.node {
        PromptAstNode::Str(s) => Ok(vec![serde_json::json!({
            "type": "text",
            "text": s
        })]),
        PromptAstNode::Media(media) => {
            let block = media_to_content_block(media)?;
            Ok(vec![block])
        }
        PromptAstNode::Vec(nodes) => {
            let mut blocks = Vec::new();
            for node in nodes {
                blocks.extend(flatten_content_blocks(node)?);
            }
            Ok(blocks)
        }
        PromptAstNode::Message { .. } => Err(ProviderError::InvalidPrompt(
            "nested messages not allowed in content".to_string(),
        )),
    }
}

/// Convert a MediaValue to an Anthropic content block.
fn media_to_content_block(
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
            match &media.content {
                MediaContent::Url { url, .. } => {
                    Ok(serde_json::json!({
                        "type": "image",
                        "source": {
                            "type": "url",
                            "url": url
                        }
                    }))
                }
                MediaContent::Base64 { base64_data } => {
                    Ok(serde_json::json!({
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": mime_type,
                            "data": base64_data
                        }
                    }))
                }
                MediaContent::File { .. } => Err(ProviderError::InvalidPrompt(
                    "file references should be resolved before building request".to_string(),
                )),
            }
        }
        MediaKind::Audio => {
            // Anthropic audio format
            match &media.content {
                MediaContent::Base64 { base64_data } => {
                    Ok(serde_json::json!({
                        "type": "audio",
                        "source": {
                            "type": "base64",
                            "media_type": mime_type,
                            "data": base64_data
                        }
                    }))
                }
                MediaContent::Url { url, .. } => {
                    Ok(serde_json::json!({
                        "type": "audio",
                        "source": {
                            "type": "url",
                            "url": url
                        }
                    }))
                }
                MediaContent::File { .. } => Err(ProviderError::InvalidPrompt(
                    "file references should be resolved before building request".to_string(),
                )),
            }
        }
        MediaKind::Pdf => {
            // Anthropic PDF support
            match &media.content {
                MediaContent::Base64 { base64_data } => {
                    Ok(serde_json::json!({
                        "type": "document",
                        "source": {
                            "type": "base64",
                            "media_type": mime_type,
                            "data": base64_data
                        }
                    }))
                }
                MediaContent::Url { url, .. } => {
                    Ok(serde_json::json!({
                        "type": "document",
                        "source": {
                            "type": "url",
                            "url": url
                        }
                    }))
                }
                MediaContent::File { .. } => Err(ProviderError::InvalidPrompt(
                    "file references should be resolved before building request".to_string(),
                )),
            }
        }
        MediaKind::Video => Err(ProviderError::InvalidPrompt(
            "Anthropic does not support video input".to_string(),
        )),
        MediaKind::Generic => Err(ProviderError::InvalidPrompt(
            "Anthropic does not support generic media type".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bex_llm_types::{ModelFeatures, RoleConfig};
    use baml_base::MediaKind;
    use bex_vm_types::{MediaContent, MediaValue};
    use indexmap::IndexMap;

    fn make_client(model: &str) -> ResolvedClient {
        let mut options = IndexMap::new();
        options.insert("model".to_string(), serde_json::json!(model));
        options.insert("api_key".to_string(), serde_json::json!("sk-ant-test-key"));

        ResolvedClient {
            name: "test-client".to_string(),
            provider: "anthropic".to_string(),
            roles: RoleConfig::default(),
            features: ModelFeatures::default(),
            options,
            request_config: Default::default(),
        }
    }

    fn make_client_with_options(model: &str, extra_options: Vec<(&str, serde_json::Value)>) -> ResolvedClient {
        let mut options = IndexMap::new();
        options.insert("model".to_string(), serde_json::json!(model));
        options.insert("api_key".to_string(), serde_json::json!("sk-ant-test-key"));
        for (key, value) in extra_options {
            options.insert(key.to_string(), value);
        }

        ResolvedClient {
            name: "test-client".to_string(),
            provider: "anthropic".to_string(),
            roles: RoleConfig::default(),
            features: ModelFeatures::default(),
            options,
            request_config: Default::default(),
        }
    }

    #[test]
    fn test_simple_message() {
        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Str(
                "Hello, world!".to_string(),
            ))),
            metadata: serde_json::Map::new(),
        });

        let client = make_client("claude-3-opus-20240229");
        let request = build_request(&prompt, &client).unwrap();

        assert_eq!(request.url, "https://api.anthropic.com/v1/messages");
        assert_eq!(
            request.headers.get("x-api-key"),
            Some(&"sk-ant-test-key".to_string())
        );
        assert_eq!(
            request.headers.get("anthropic-version"),
            Some(&"2023-06-01".to_string())
        );
    }

    #[test]
    fn test_system_message_extraction() {
        let prompt = PromptAst::without_span(PromptAstNode::Vec(vec![
            PromptAst::without_span(PromptAstNode::Message {
                role: "system".to_string(),
                content: Box::new(PromptAst::without_span(PromptAstNode::Str(
                    "You are helpful.".to_string(),
                ))),
                metadata: serde_json::Map::new(),
            }),
            PromptAst::without_span(PromptAstNode::Message {
                role: "user".to_string(),
                content: Box::new(PromptAst::without_span(PromptAstNode::Str(
                    "Hi!".to_string(),
                ))),
                metadata: serde_json::Map::new(),
            }),
        ]));

        let client = make_client("claude-3-opus-20240229");
        let request = build_request(&prompt, &client).unwrap();

        // Check the body has system at top level and messages without system
        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                // System should be at top level
                assert!(body.get("system").is_some());

                // Messages should only have user message
                let messages = body.get("messages").unwrap().as_array().unwrap();
                assert_eq!(messages.len(), 1);
                assert_eq!(messages[0]["role"], "user");
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[test]
    fn test_max_tokens_default() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));
        let client = make_client("claude-3-opus-20240229");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                assert_eq!(body.get("max_tokens").unwrap(), DEFAULT_MAX_TOKENS);
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[test]
    fn test_custom_base_url() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));

        let mut options = IndexMap::new();
        options.insert("model".to_string(), serde_json::json!("claude-3-opus"));
        options.insert(
            "base_url".to_string(),
            serde_json::json!("https://custom.api.com"),
        );

        let client = ResolvedClient {
            name: "test".to_string(),
            provider: "anthropic".to_string(),
            roles: RoleConfig::default(),
            features: ModelFeatures::default(),
            options,
            request_config: Default::default(),
        };

        let request = build_request(&prompt, &client).unwrap();
        assert_eq!(request.url, "https://custom.api.com/v1/messages");
    }

    // =========================================================================
    // Image handling tests
    // =========================================================================

    #[test]
    fn test_image_url() {
        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Media(MediaValue {
                kind: MediaKind::Image,
                content: MediaContent::Url {
                    url: "https://example.com/image.png".to_string(),
                    base64_data: None,
                },
                mime_type: Some("image/png".to_string()),
            }))),
            metadata: serde_json::Map::new(),
        });

        let client = make_client("claude-3-opus-20240229");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                assert_eq!(messages.len(), 1);
                let content = messages[0].get("content").unwrap().as_array().unwrap();
                assert_eq!(content.len(), 1);
                assert_eq!(content[0]["type"], "image");
                assert_eq!(content[0]["source"]["type"], "url");
                assert_eq!(content[0]["source"]["url"], "https://example.com/image.png");
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[test]
    fn test_image_base64() {
        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Media(MediaValue {
                kind: MediaKind::Image,
                content: MediaContent::Base64 {
                    base64_data: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==".to_string(),
                },
                mime_type: Some("image/png".to_string()),
            }))),
            metadata: serde_json::Map::new(),
        });

        let client = make_client("claude-3-opus-20240229");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                let content = messages[0].get("content").unwrap().as_array().unwrap();
                assert_eq!(content[0]["type"], "image");
                assert_eq!(content[0]["source"]["type"], "base64");
                assert_eq!(content[0]["source"]["media_type"], "image/png");
                assert!(content[0]["source"]["data"].as_str().is_some());
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[test]
    fn test_image_default_mime_type() {
        // Test that default mime type is used when not specified
        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Media(MediaValue {
                kind: MediaKind::Image,
                content: MediaContent::Base64 {
                    base64_data: "abc123".to_string(),
                },
                mime_type: None, // No mime type specified
            }))),
            metadata: serde_json::Map::new(),
        });

        let client = make_client("claude-3-opus-20240229");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                let content = messages[0].get("content").unwrap().as_array().unwrap();
                // Should default to image/png
                assert_eq!(content[0]["source"]["media_type"], "image/png");
            }
            _ => panic!("expected JSON body"),
        }
    }

    // =========================================================================
    // Audio handling tests
    // =========================================================================

    #[test]
    fn test_audio_base64() {
        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Media(MediaValue {
                kind: MediaKind::Audio,
                content: MediaContent::Base64 {
                    base64_data: "UklGRiQAAABXQVZFZm10IBAAAAABAAEARKwAAIhYAQACABAAZGF0YQAAAAA=".to_string(),
                },
                mime_type: Some("audio/wav".to_string()),
            }))),
            metadata: serde_json::Map::new(),
        });

        let client = make_client("claude-3-5-sonnet-20241022");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                let content = messages[0].get("content").unwrap().as_array().unwrap();
                assert_eq!(content[0]["type"], "audio");
                assert_eq!(content[0]["source"]["type"], "base64");
                assert_eq!(content[0]["source"]["media_type"], "audio/wav");
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[test]
    fn test_audio_url() {
        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Media(MediaValue {
                kind: MediaKind::Audio,
                content: MediaContent::Url {
                    url: "https://example.com/audio.mp3".to_string(),
                    base64_data: None,
                },
                mime_type: Some("audio/mp3".to_string()),
            }))),
            metadata: serde_json::Map::new(),
        });

        let client = make_client("claude-3-5-sonnet-20241022");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                let content = messages[0].get("content").unwrap().as_array().unwrap();
                assert_eq!(content[0]["type"], "audio");
                assert_eq!(content[0]["source"]["type"], "url");
                assert_eq!(content[0]["source"]["url"], "https://example.com/audio.mp3");
            }
            _ => panic!("expected JSON body"),
        }
    }

    // =========================================================================
    // PDF/Document handling tests
    // =========================================================================

    #[test]
    fn test_pdf_base64() {
        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Media(MediaValue {
                kind: MediaKind::Pdf,
                content: MediaContent::Base64 {
                    base64_data: "JVBERi0xLjQKJeLjz9MK".to_string(),
                },
                mime_type: Some("application/pdf".to_string()),
            }))),
            metadata: serde_json::Map::new(),
        });

        let client = make_client("claude-3-5-sonnet-20241022");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                let content = messages[0].get("content").unwrap().as_array().unwrap();
                assert_eq!(content[0]["type"], "document");
                assert_eq!(content[0]["source"]["type"], "base64");
                assert_eq!(content[0]["source"]["media_type"], "application/pdf");
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[test]
    fn test_pdf_url() {
        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Media(MediaValue {
                kind: MediaKind::Pdf,
                content: MediaContent::Url {
                    url: "https://example.com/document.pdf".to_string(),
                    base64_data: None,
                },
                mime_type: Some("application/pdf".to_string()),
            }))),
            metadata: serde_json::Map::new(),
        });

        let client = make_client("claude-3-5-sonnet-20241022");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                let content = messages[0].get("content").unwrap().as_array().unwrap();
                assert_eq!(content[0]["type"], "document");
                assert_eq!(content[0]["source"]["type"], "url");
                assert_eq!(content[0]["source"]["url"], "https://example.com/document.pdf");
            }
            _ => panic!("expected JSON body"),
        }
    }

    // =========================================================================
    // Unsupported media type tests
    // =========================================================================

    #[test]
    fn test_video_not_supported() {
        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Media(MediaValue {
                kind: MediaKind::Video,
                content: MediaContent::Url {
                    url: "https://example.com/video.mp4".to_string(),
                    base64_data: None,
                },
                mime_type: Some("video/mp4".to_string()),
            }))),
            metadata: serde_json::Map::new(),
        });

        let client = make_client("claude-3-5-sonnet-20241022");
        let result = build_request(&prompt, &client);
        assert!(matches!(result, Err(ProviderError::InvalidPrompt(_))));
    }

    #[test]
    fn test_generic_media_not_supported() {
        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Media(MediaValue {
                kind: MediaKind::Generic,
                content: MediaContent::Base64 {
                    base64_data: "somedata".to_string(),
                },
                mime_type: Some("application/octet-stream".to_string()),
            }))),
            metadata: serde_json::Map::new(),
        });

        let client = make_client("claude-3-5-sonnet-20241022");
        let result = build_request(&prompt, &client);
        assert!(matches!(result, Err(ProviderError::InvalidPrompt(_))));
    }

    // =========================================================================
    // Multi-part content tests
    // =========================================================================

    #[test]
    fn test_mixed_text_and_image() {
        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Vec(vec![
                PromptAst::without_span(PromptAstNode::Str("What's in this image?".to_string())),
                PromptAst::without_span(PromptAstNode::Media(MediaValue {
                    kind: MediaKind::Image,
                    content: MediaContent::Url {
                        url: "https://example.com/image.jpg".to_string(),
                        base64_data: None,
                    },
                    mime_type: Some("image/jpeg".to_string()),
                })),
            ]))),
            metadata: serde_json::Map::new(),
        });

        let client = make_client("claude-3-opus-20240229");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                let content = messages[0].get("content").unwrap().as_array().unwrap();
                assert_eq!(content.len(), 2);
                assert_eq!(content[0]["type"], "text");
                assert_eq!(content[0]["text"], "What's in this image?");
                assert_eq!(content[1]["type"], "image");
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[test]
    fn test_multiple_images() {
        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Vec(vec![
                PromptAst::without_span(PromptAstNode::Str("Compare these images:".to_string())),
                PromptAst::without_span(PromptAstNode::Media(MediaValue {
                    kind: MediaKind::Image,
                    content: MediaContent::Url {
                        url: "https://example.com/image1.jpg".to_string(),
                        base64_data: None,
                    },
                    mime_type: Some("image/jpeg".to_string()),
                })),
                PromptAst::without_span(PromptAstNode::Media(MediaValue {
                    kind: MediaKind::Image,
                    content: MediaContent::Url {
                        url: "https://example.com/image2.jpg".to_string(),
                        base64_data: None,
                    },
                    mime_type: Some("image/jpeg".to_string()),
                })),
            ]))),
            metadata: serde_json::Map::new(),
        });

        let client = make_client("claude-3-opus-20240229");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                let content = messages[0].get("content").unwrap().as_array().unwrap();
                assert_eq!(content.len(), 3);
                assert_eq!(content[0]["type"], "text");
                assert_eq!(content[1]["type"], "image");
                assert_eq!(content[2]["type"], "image");
                assert_eq!(content[1]["source"]["url"], "https://example.com/image1.jpg");
                assert_eq!(content[2]["source"]["url"], "https://example.com/image2.jpg");
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[test]
    fn test_nested_content_vectors() {
        // Test deeply nested content (Vec inside Vec)
        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Vec(vec![
                PromptAst::without_span(PromptAstNode::Vec(vec![
                    PromptAst::without_span(PromptAstNode::Str("First part".to_string())),
                    PromptAst::without_span(PromptAstNode::Str("Second part".to_string())),
                ])),
                PromptAst::without_span(PromptAstNode::Str("Third part".to_string())),
            ]))),
            metadata: serde_json::Map::new(),
        });

        let client = make_client("claude-3-opus-20240229");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                let content = messages[0].get("content").unwrap().as_array().unwrap();
                // Should be flattened to 3 text parts
                assert_eq!(content.len(), 3);
                assert_eq!(content[0]["text"], "First part");
                assert_eq!(content[1]["text"], "Second part");
                assert_eq!(content[2]["text"], "Third part");
            }
            _ => panic!("expected JSON body"),
        }
    }

    // =========================================================================
    // Cache control and metadata tests (Anthropic-specific)
    // =========================================================================

    #[test]
    fn test_cache_control_metadata() {
        // Anthropic supports cache_control for prompt caching
        let mut metadata = serde_json::Map::new();
        metadata.insert(
            "cache_control".to_string(),
            serde_json::json!({"type": "ephemeral"}),
        );

        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Str(
                "This is a cacheable message.".to_string(),
            ))),
            metadata,
        });

        let client = make_client("claude-3-5-sonnet-20241022");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                assert_eq!(messages[0]["cache_control"]["type"], "ephemeral");
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[test]
    fn test_system_message_content_format() {
        // System message content should also be in content blocks format
        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "system".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Str(
                "You are a helpful assistant.".to_string(),
            ))),
            metadata: serde_json::Map::new(),
        });

        let client = make_client("claude-3-opus-20240229");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                // System should be in content blocks format
                let system = body.get("system").unwrap().as_array().unwrap();
                assert_eq!(system.len(), 1);
                assert_eq!(system[0]["type"], "text");
                assert_eq!(system[0]["text"], "You are a helpful assistant.");
            }
            _ => panic!("expected JSON body"),
        }
    }

    // =========================================================================
    // Custom headers and API version tests
    // =========================================================================

    #[test]
    fn test_custom_anthropic_version() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));

        let client = make_client_with_options("claude-3-opus-20240229", vec![
            ("anthropic_version", serde_json::json!("2024-01-01")),
        ]);

        let request = build_request(&prompt, &client).unwrap();

        assert_eq!(
            request.headers.get("anthropic-version"),
            Some(&"2024-01-01".to_string())
        );
    }

    #[test]
    fn test_custom_headers() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));

        let client = make_client_with_options("claude-3-opus-20240229", vec![
            ("headers", serde_json::json!({
                "anthropic-beta": "messages-2023-12-15",
                "X-Custom-Header": "custom-value"
            })),
        ]);

        let request = build_request(&prompt, &client).unwrap();

        assert_eq!(
            request.headers.get("anthropic-beta"),
            Some(&"messages-2023-12-15".to_string())
        );
        assert_eq!(
            request.headers.get("X-Custom-Header"),
            Some(&"custom-value".to_string())
        );
    }

    // =========================================================================
    // Optional parameters tests
    // =========================================================================

    #[test]
    fn test_custom_max_tokens() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));

        let client = make_client_with_options("claude-3-opus-20240229", vec![
            ("max_tokens", serde_json::json!(1000)),
        ]);

        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                assert_eq!(body.get("max_tokens").unwrap(), 1000);
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[test]
    fn test_optional_parameters() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));

        let client = make_client_with_options("claude-3-opus-20240229", vec![
            ("temperature", serde_json::json!(0.7)),
            ("top_p", serde_json::json!(0.9)),
            ("top_k", serde_json::json!(40)),
            ("stop_sequences", serde_json::json!(["END", "STOP"])),
        ]);

        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                assert_eq!(body.get("temperature").unwrap(), 0.7);
                assert_eq!(body.get("top_p").unwrap(), 0.9);
                assert_eq!(body.get("top_k").unwrap(), 40);
                let stop_sequences = body.get("stop_sequences").unwrap().as_array().unwrap();
                assert_eq!(stop_sequences.len(), 2);
            }
            _ => panic!("expected JSON body"),
        }
    }

    // =========================================================================
    // Error handling tests
    // =========================================================================

    #[test]
    fn test_missing_model() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));

        let client = ResolvedClient {
            name: "test".to_string(),
            provider: "anthropic".to_string(),
            roles: RoleConfig::default(),
            features: ModelFeatures::default(),
            options: IndexMap::new(),
            request_config: Default::default(),
        };

        let result = build_request(&prompt, &client);
        assert!(matches!(result, Err(ProviderError::MissingOption(_))));
    }

    #[test]
    fn test_bare_media_at_top_level_error() {
        // Bare media without wrapping in a message should error
        let prompt = PromptAst::without_span(PromptAstNode::Media(MediaValue {
            kind: MediaKind::Image,
            content: MediaContent::Url {
                url: "https://example.com/image.png".to_string(),
                base64_data: None,
            },
            mime_type: Some("image/png".to_string()),
        }));

        let client = make_client("claude-3-opus-20240229");
        let result = build_request(&prompt, &client);
        assert!(matches!(result, Err(ProviderError::InvalidPrompt(_))));
    }

    #[test]
    fn test_file_reference_not_resolved_error() {
        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Media(MediaValue {
                kind: MediaKind::Image,
                content: MediaContent::File {
                    file: "/path/to/image.png".to_string(),
                    base64_data: None,
                },
                mime_type: Some("image/png".to_string()),
            }))),
            metadata: serde_json::Map::new(),
        });

        let client = make_client("claude-3-opus-20240229");
        let result = build_request(&prompt, &client);
        assert!(matches!(result, Err(ProviderError::InvalidPrompt(_))));
    }

    // =========================================================================
    // Conversation format tests
    // =========================================================================

    #[test]
    fn test_conversation_with_assistant_messages() {
        let prompt = PromptAst::without_span(PromptAstNode::Vec(vec![
            PromptAst::without_span(PromptAstNode::Message {
                role: "system".to_string(),
                content: Box::new(PromptAst::without_span(PromptAstNode::Str(
                    "You are a helpful assistant.".to_string(),
                ))),
                metadata: serde_json::Map::new(),
            }),
            PromptAst::without_span(PromptAstNode::Message {
                role: "user".to_string(),
                content: Box::new(PromptAst::without_span(PromptAstNode::Str(
                    "Hello".to_string(),
                ))),
                metadata: serde_json::Map::new(),
            }),
            PromptAst::without_span(PromptAstNode::Message {
                role: "assistant".to_string(),
                content: Box::new(PromptAst::without_span(PromptAstNode::Str(
                    "Hi there! How can I help you?".to_string(),
                ))),
                metadata: serde_json::Map::new(),
            }),
            PromptAst::without_span(PromptAstNode::Message {
                role: "user".to_string(),
                content: Box::new(PromptAst::without_span(PromptAstNode::Str(
                    "What's the weather like?".to_string(),
                ))),
                metadata: serde_json::Map::new(),
            }),
        ]));

        let client = make_client("claude-3-opus-20240229");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                // System should be extracted to top level
                assert!(body.get("system").is_some());

                // Messages should only have user/assistant
                let messages = body.get("messages").unwrap().as_array().unwrap();
                assert_eq!(messages.len(), 3);
                assert_eq!(messages[0]["role"], "user");
                assert_eq!(messages[1]["role"], "assistant");
                assert_eq!(messages[2]["role"], "user");
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[test]
    fn test_multiple_system_messages_last_wins() {
        // When there are multiple system messages, the last one should be used
        let prompt = PromptAst::without_span(PromptAstNode::Vec(vec![
            PromptAst::without_span(PromptAstNode::Message {
                role: "system".to_string(),
                content: Box::new(PromptAst::without_span(PromptAstNode::Str(
                    "First system message.".to_string(),
                ))),
                metadata: serde_json::Map::new(),
            }),
            PromptAst::without_span(PromptAstNode::Message {
                role: "system".to_string(),
                content: Box::new(PromptAst::without_span(PromptAstNode::Str(
                    "Second system message.".to_string(),
                ))),
                metadata: serde_json::Map::new(),
            }),
            PromptAst::without_span(PromptAstNode::Message {
                role: "user".to_string(),
                content: Box::new(PromptAst::without_span(PromptAstNode::Str(
                    "Hi!".to_string(),
                ))),
                metadata: serde_json::Map::new(),
            }),
        ]));

        let client = make_client("claude-3-opus-20240229");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let system = body.get("system").unwrap().as_array().unwrap();
                // Last system message should be used
                assert_eq!(system[0]["text"], "Second system message.");
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[test]
    fn test_no_system_message() {
        // Test that requests without system messages work correctly
        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Str(
                "Hello!".to_string(),
            ))),
            metadata: serde_json::Map::new(),
        });

        let client = make_client("claude-3-opus-20240229");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                // No system field should be present
                assert!(body.get("system").is_none());

                let messages = body.get("messages").unwrap().as_array().unwrap();
                assert_eq!(messages.len(), 1);
                assert_eq!(messages[0]["role"], "user");
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[test]
    fn test_base_url_trailing_slash_handling() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));

        // Test with trailing slash
        let client = make_client_with_options("claude-3-opus-20240229", vec![
            ("base_url", serde_json::json!("https://custom.api.com/")),
        ]);

        let request = build_request(&prompt, &client).unwrap();
        // Should not have double slash
        assert_eq!(request.url, "https://custom.api.com/v1/messages");
    }

    #[test]
    fn test_no_api_key() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));

        let mut options = IndexMap::new();
        options.insert("model".to_string(), serde_json::json!("claude-3-opus-20240229"));
        // No api_key

        let client = ResolvedClient {
            name: "test".to_string(),
            provider: "anthropic".to_string(),
            roles: RoleConfig::default(),
            features: ModelFeatures::default(),
            options,
            request_config: Default::default(),
        };

        let request = build_request(&prompt, &client).unwrap();
        // Should still build request, just without api key header
        assert!(request.headers.get("x-api-key").is_none());
        // But should still have anthropic-version
        assert!(request.headers.get("anthropic-version").is_some());
    }
}
