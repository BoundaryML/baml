//! OpenAI provider request transformation.
//!
//! Converts a `PromptAst` into an `HttpRequest` for the OpenAI Chat Completions API.
//!
//! # API Reference
//! - Endpoint: `{base_url}/chat/completions` (default: `https://api.openai.com/v1/chat/completions`)
//! - Auth: `Authorization: Bearer {api_key}`
//! - Body: `{ "model": "...", "messages": [...], ... }`

use bex_llm_types::{HttpRequest, PromptAst, PromptAstNode, ResolvedClient};

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
    match &prompt.node {
        PromptAstNode::Message {
            role,
            content,
            metadata,
        } => {
            let msg = build_message(role, content, metadata)?;
            Ok(vec![msg])
        }
        PromptAstNode::Vec(nodes) => {
            let mut messages = Vec::new();
            for node in nodes {
                messages.extend(prompt_to_messages(node)?);
            }
            Ok(messages)
        }
        PromptAstNode::Str(s) => {
            // A bare string becomes a user message
            Ok(vec![serde_json::json!({
                "role": "user",
                "content": s
            })])
        }
        PromptAstNode::Media(_) => Err(ProviderError::InvalidPrompt(
            "bare media not allowed at top level; wrap in a message".to_string(),
        )),
    }
}

/// Build a single OpenAI message from a PromptAst Message node.
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

/// Build the content field for an OpenAI message.
///
/// Returns either a string (for simple text) or an array of content parts
/// (for mixed text/media content).
fn build_content(content: &PromptAst) -> Result<serde_json::Value, ProviderError> {
    match &content.node {
        PromptAstNode::Str(s) => Ok(serde_json::Value::String(s.clone())),
        PromptAstNode::Media(media) => {
            // Single media becomes a content array with one item
            let part = media_to_content_part(media)?;
            Ok(serde_json::Value::Array(vec![part]))
        }
        PromptAstNode::Vec(nodes) => {
            // Multiple content parts
            let mut parts = Vec::new();
            for node in nodes {
                parts.extend(flatten_content_parts(node)?);
            }
            Ok(serde_json::Value::Array(parts))
        }
        PromptAstNode::Message { .. } => Err(ProviderError::InvalidPrompt(
            "nested messages not allowed in content".to_string(),
        )),
    }
}

/// Flatten content nodes into content parts.
fn flatten_content_parts(content: &PromptAst) -> Result<Vec<serde_json::Value>, ProviderError> {
    match &content.node {
        PromptAstNode::Str(s) => Ok(vec![serde_json::json!({
            "type": "text",
            "text": s
        })]),
        PromptAstNode::Media(media) => {
            let part = media_to_content_part(media)?;
            Ok(vec![part])
        }
        PromptAstNode::Vec(nodes) => {
            let mut parts = Vec::new();
            for node in nodes {
                parts.extend(flatten_content_parts(node)?);
            }
            Ok(parts)
        }
        PromptAstNode::Message { .. } => Err(ProviderError::InvalidPrompt(
            "nested messages not allowed in content".to_string(),
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
    use baml_base::MediaKind;
    use bex_vm_types::{MediaContent, MediaValue};
    use indexmap::IndexMap;

    fn make_client(model: &str) -> ResolvedClient {
        let mut options = IndexMap::new();
        options.insert("model".to_string(), serde_json::json!(model));
        options.insert("api_key".to_string(), serde_json::json!("sk-test-key"));

        ResolvedClient {
            name: "test-client".to_string(),
            provider: "openai".to_string(),
            roles: RoleConfig::default(),
            features: ModelFeatures::default(),
            options,
            request_config: Default::default(),
        }
    }

    fn make_client_with_options(model: &str, extra_options: Vec<(&str, serde_json::Value)>) -> ResolvedClient {
        let mut options = IndexMap::new();
        options.insert("model".to_string(), serde_json::json!(model));
        options.insert("api_key".to_string(), serde_json::json!("sk-test-key"));
        for (key, value) in extra_options {
            options.insert(key.to_string(), value);
        }

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
    fn test_simple_message() {
        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Str(
                "Hello, world!".to_string(),
            ))),
            metadata: serde_json::Map::new(),
        });

        let client = make_client("gpt-4");
        let request = build_request(&prompt, &client).unwrap();

        assert_eq!(request.url, "https://api.openai.com/v1/chat/completions");
        assert_eq!(
            request.headers.get("Authorization"),
            Some(&"Bearer sk-test-key".to_string())
        );
    }

    #[test]
    fn test_multiple_messages() {
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

        let client = make_client("gpt-4");
        let request = build_request(&prompt, &client).unwrap();

        // Check the body contains the messages
        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                assert_eq!(messages.len(), 2);
                assert_eq!(messages[0]["role"], "system");
                assert_eq!(messages[1]["role"], "user");
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[test]
    fn test_custom_base_url() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));

        let mut options = IndexMap::new();
        options.insert("model".to_string(), serde_json::json!("gpt-4"));
        options.insert(
            "base_url".to_string(),
            serde_json::json!("https://custom.api.com/v1"),
        );

        let client = ResolvedClient {
            name: "test".to_string(),
            provider: "openai".to_string(),
            roles: RoleConfig::default(),
            features: ModelFeatures::default(),
            options,
            request_config: Default::default(),
        };

        let request = build_request(&prompt, &client).unwrap();
        assert_eq!(request.url, "https://custom.api.com/v1/chat/completions");
    }

    #[test]
    fn test_missing_model() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));

        let client = ResolvedClient {
            name: "test".to_string(),
            provider: "openai".to_string(),
            roles: RoleConfig::default(),
            features: ModelFeatures::default(),
            options: IndexMap::new(),
            request_config: Default::default(),
        };

        let result = build_request(&prompt, &client);
        assert!(matches!(result, Err(ProviderError::MissingOption(_))));
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

        let client = make_client("gpt-4o");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                assert_eq!(messages.len(), 1);
                let content = messages[0].get("content").unwrap().as_array().unwrap();
                assert_eq!(content.len(), 1);
                assert_eq!(content[0]["type"], "image_url");
                assert_eq!(
                    content[0]["image_url"]["url"],
                    "https://example.com/image.png"
                );
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

        let client = make_client("gpt-4o");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                let content = messages[0].get("content").unwrap().as_array().unwrap();
                assert_eq!(content[0]["type"], "image_url");
                let url = content[0]["image_url"]["url"].as_str().unwrap();
                assert!(url.starts_with("data:image/png;base64,"));
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

        let client = make_client("gpt-4o");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                let content = messages[0].get("content").unwrap().as_array().unwrap();
                let url = content[0]["image_url"]["url"].as_str().unwrap();
                // Should default to image/png
                assert!(url.starts_with("data:image/png;base64,"));
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

        let client = make_client("gpt-4o-audio-preview");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                let content = messages[0].get("content").unwrap().as_array().unwrap();
                assert_eq!(content[0]["type"], "input_audio");
                assert_eq!(content[0]["input_audio"]["format"], "wav");
                assert!(content[0]["input_audio"]["data"].as_str().is_some());
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[test]
    fn test_audio_mp3_format() {
        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Media(MediaValue {
                kind: MediaKind::Audio,
                content: MediaContent::Base64 {
                    base64_data: "//uQxAAAAAANIAAAAAExBTUUzLjEwMFVVVVVVVVVVVVVVVVVV".to_string(),
                },
                mime_type: Some("audio/mp3".to_string()),
            }))),
            metadata: serde_json::Map::new(),
        });

        let client = make_client("gpt-4o-audio-preview");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                let content = messages[0].get("content").unwrap().as_array().unwrap();
                assert_eq!(content[0]["input_audio"]["format"], "mp3");
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[test]
    fn test_audio_url_not_supported() {
        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Media(MediaValue {
                kind: MediaKind::Audio,
                content: MediaContent::Url {
                    url: "https://example.com/audio.wav".to_string(),
                    base64_data: None,
                },
                mime_type: Some("audio/wav".to_string()),
            }))),
            metadata: serde_json::Map::new(),
        });

        let client = make_client("gpt-4o-audio-preview");
        let result = build_request(&prompt, &client);
        assert!(matches!(result, Err(ProviderError::InvalidPrompt(_))));
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

        let client = make_client("gpt-4o");
        let result = build_request(&prompt, &client);
        assert!(matches!(result, Err(ProviderError::InvalidPrompt(_))));
    }

    #[test]
    fn test_pdf_not_supported() {
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

        let client = make_client("gpt-4o");
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

        let client = make_client("gpt-4o");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                let content = messages[0].get("content").unwrap().as_array().unwrap();
                assert_eq!(content.len(), 2);
                assert_eq!(content[0]["type"], "text");
                assert_eq!(content[0]["text"], "What's in this image?");
                assert_eq!(content[1]["type"], "image_url");
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

        let client = make_client("gpt-4o");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                let content = messages[0].get("content").unwrap().as_array().unwrap();
                assert_eq!(content.len(), 3);
                assert_eq!(content[0]["type"], "text");
                assert_eq!(content[1]["type"], "image_url");
                assert_eq!(content[2]["type"], "image_url");
                assert_eq!(content[1]["image_url"]["url"], "https://example.com/image1.jpg");
                assert_eq!(content[2]["image_url"]["url"], "https://example.com/image2.jpg");
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

        let client = make_client("gpt-4");
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
    // Custom headers and metadata tests
    // =========================================================================

    #[test]
    fn test_custom_headers() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));

        let client = make_client_with_options("gpt-4", vec![
            ("headers", serde_json::json!({
                "X-Custom-Header": "custom-value",
                "X-Another-Header": "another-value"
            })),
        ]);

        let request = build_request(&prompt, &client).unwrap();

        assert_eq!(
            request.headers.get("X-Custom-Header"),
            Some(&"custom-value".to_string())
        );
        assert_eq!(
            request.headers.get("X-Another-Header"),
            Some(&"another-value".to_string())
        );
    }

    #[test]
    fn test_message_metadata() {
        // Test that metadata (like cache_control) is passed through to the message
        let mut metadata = serde_json::Map::new();
        metadata.insert(
            "cache_control".to_string(),
            serde_json::json!({"type": "ephemeral"}),
        );
        metadata.insert("custom_field".to_string(), serde_json::json!("custom_value"));

        let prompt = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Str(
                "Hello, world!".to_string(),
            ))),
            metadata,
        });

        let client = make_client("gpt-4");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                assert_eq!(messages[0]["cache_control"]["type"], "ephemeral");
                assert_eq!(messages[0]["custom_field"], "custom_value");
            }
            _ => panic!("expected JSON body"),
        }
    }

    // =========================================================================
    // Optional parameters tests
    // =========================================================================

    #[test]
    fn test_optional_parameters() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));

        let client = make_client_with_options("gpt-4", vec![
            ("temperature", serde_json::json!(0.7)),
            ("max_tokens", serde_json::json!(1000)),
            ("top_p", serde_json::json!(0.9)),
            ("presence_penalty", serde_json::json!(0.5)),
            ("frequency_penalty", serde_json::json!(0.5)),
        ]);

        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                assert_eq!(body.get("temperature").unwrap(), 0.7);
                assert_eq!(body.get("max_tokens").unwrap(), 1000);
                assert_eq!(body.get("top_p").unwrap(), 0.9);
                assert_eq!(body.get("presence_penalty").unwrap(), 0.5);
                assert_eq!(body.get("frequency_penalty").unwrap(), 0.5);
            }
            _ => panic!("expected JSON body"),
        }
    }

    // =========================================================================
    // Error handling tests
    // =========================================================================

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

        let client = make_client("gpt-4o");
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

        let client = make_client("gpt-4o");
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

        let client = make_client("gpt-4");
        let request = build_request(&prompt, &client).unwrap();

        match &request.body {
            bex_llm_types::HttpBody::Json(body) => {
                let messages = body.get("messages").unwrap().as_array().unwrap();
                assert_eq!(messages.len(), 4);
                assert_eq!(messages[0]["role"], "system");
                assert_eq!(messages[1]["role"], "user");
                assert_eq!(messages[2]["role"], "assistant");
                assert_eq!(messages[3]["role"], "user");
            }
            _ => panic!("expected JSON body"),
        }
    }

    #[test]
    fn test_base_url_trailing_slash_handling() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));

        // Test with trailing slash
        let client = make_client_with_options("gpt-4", vec![
            ("base_url", serde_json::json!("https://custom.api.com/v1/")),
        ]);

        let request = build_request(&prompt, &client).unwrap();
        // Should not have double slash
        assert_eq!(request.url, "https://custom.api.com/v1/chat/completions");
    }

    #[test]
    fn test_no_api_key() {
        let prompt = PromptAst::without_span(PromptAstNode::Str("Hello".to_string()));

        let mut options = IndexMap::new();
        options.insert("model".to_string(), serde_json::json!("gpt-4"));
        // No api_key

        let client = ResolvedClient {
            name: "test".to_string(),
            provider: "openai".to_string(),
            roles: RoleConfig::default(),
            features: ModelFeatures::default(),
            options,
            request_config: Default::default(),
        };

        let request = build_request(&prompt, &client).unwrap();
        // Should still build request, just without auth header
        assert!(request.headers.get("Authorization").is_none());
    }
}
