//! LlmProvider-specific HTTP request building.
//!
//! Converts a `crate::baml_std::PrimitiveClient` + `PromptAst` into a `baml.http.Request` instance.

mod anthropic;
mod bedrock;
// pub(crate) for VERTEX_PROJECT_ID_PLACEHOLDER, used by auth_request::vertex.
pub(crate) mod google;
mod openai;

use std::{str::FromStr, sync::Arc};

use bex_external_types::BexExternalValue;

use crate::LlmProvider;

/// Returns true if the model name indicates an Anthropic model (e.g. Claude on Vertex AI).
pub(crate) fn is_anthropic_model(model: &str) -> bool {
    model.starts_with("claude")
}

/// Build a provider-specific HTTP request from a specialized prompt.
///
/// Returns an owned `HttpRequest` matching the `baml.http.Request` class:
/// `{ method: String, url: String, headers: Map<String, String>, body: String }`
pub(crate) async fn build_request(
    client: &crate::baml_std::PrimitiveClient,
    prompt: bex_vm_types::PromptAst,
    io: Arc<dyn ::sys_types::runtime_io::RuntimeIo>,
) -> Result<crate::baml_std::HttpRequest, BuildRequestError> {
    let provider = LlmProvider::from_str(&client.provider)
        .map_err(|_| BuildRequestError::UnsupportedLlmProvider(client.provider.clone()))?;

    // Resolve media (fetch URLs, read files) before building the provider-specific request.
    let handler = crate::resolve_media::MediaUrlHandler::from_client(client);
    crate::resolve_media::resolve_media(&prompt, &handler, &*io).await?;

    let mut request = match provider {
        LlmProvider::OpenAi
        | LlmProvider::OpenAiGeneric
        | LlmProvider::AzureOpenAi
        | LlmProvider::Ollama
        | LlmProvider::OpenRouter => openai::chat_completions::build_request(client, &prompt),
        LlmProvider::OpenAiResponses => openai::responses::build_request(client, &prompt),
        LlmProvider::AiGatewayImages => openai::images::build_request(client, &prompt),
        LlmProvider::Anthropic => anthropic::build_request(client, &prompt),
        LlmProvider::AwsBedrock => bedrock::build_request(client, &prompt, io.clone()).await,
        LlmProvider::GoogleAi => google::build_request(client, &prompt, provider),
        LlmProvider::VertexAi => {
            if is_anthropic_model(&client.model) {
                build_vertex_anthropic_request(client, &prompt)
            } else {
                google::build_request(client, &prompt, provider)
            }
        }
        LlmProvider::BamlFallback | LlmProvider::BamlRoundRobin => Err(
            BuildRequestError::UnsupportedLlmProvider(client.provider.clone()),
        ),
    }?;

    // Apply user-configured headers on top of provider defaults.
    for (k, v) in &client.options.headers {
        request.headers.insert(k.to_ascii_lowercase(), v.clone());
    }

    // Append query params to the URL (percent-encoded via the `url` crate).
    if !client.options.query_params.is_empty() {
        let mut parsed = url::Url::parse(&request.url)
            .map_err(|e| BuildRequestError::Other(format!("invalid URL '{}': {e}", request.url)))?;
        for (k, v) in &client.options.query_params {
            parsed.query_pairs_mut().append_pair(k, v);
        }
        request.url = parsed.to_string();
    }

    crate::auth_request::auth_request(provider, &mut request, client, io).await?;

    Ok(request)
}

// ---------------------------------------------------------------------------
// Vertex AI + Anthropic model (rawPredict)
// ---------------------------------------------------------------------------

/// Build an Anthropic-format request for Vertex AI's `rawPredict` endpoint.
///
/// Vertex AI proxies Anthropic models via `rawPredict`, which passes the body
/// through to the Anthropic API. The body format is identical to the Anthropic
/// Messages API, with `anthropic_version` in the body (not as a header).
fn build_vertex_anthropic_request(
    client: &crate::baml_std::PrimitiveClient,
    prompt: &bex_vm_types::PromptAst,
) -> Result<crate::baml_std::HttpRequest, BuildRequestError> {
    let mut headers = indexmap::IndexMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());

    let mut extra = client.extra_body.clone();
    extra
        .entry("anthropic_version")
        .or_insert(serde_json::json!("vertex-2023-10-16"));

    let max_tokens = extra
        .remove("max_tokens")
        .and_then(|v| v.as_i64())
        .or(Some(anthropic::DEFAULT_MAX_TOKENS));

    let body_str = anthropic::build_anthropic_body_str(&client.model, prompt, max_tokens, &extra)?;

    let url = google::resolve_vertex_raw_predict_url(client)?;

    Ok(crate::baml_std::HttpRequest {
        method: "POST".to_string(),
        url,
        headers,
        body: body_str,
    })
}

/// Extract a MIME type from a `MediaValue`, returning an error if none is set.
pub(super) fn mime_type_as_ok(
    media: &baml_builtins2::MediaValue,
) -> Result<String, BuildRequestError> {
    media
        .mime_type()
        .ok_or_else(|| BuildRequestError::UnsupportedMedia("missing MIME type on media".into()))
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum BuildRequestError {
    #[error("Unsupported provider: {0}")]
    UnsupportedLlmProvider(String),
    #[error("Invalid option value for '{key}': {reason}")]
    InvalidOption { key: String, reason: String },
    #[error("Unsupported media: {0}")]
    UnsupportedMedia(String),
    #[error("File not resolved: {0}")]
    FileNotResolved(String),
    #[error("Failed to serialize request body: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Authorization failed: {0}")]
    AuthorizationFailed(String),
    #[error("{0}")]
    Other(String),
}

/// Convert a `BexExternalValue` to a `serde_json::Value`.
pub(crate) fn bex_value_to_json(value: &BexExternalValue) -> Option<serde_json::Value> {
    match value {
        BexExternalValue::Null => Some(serde_json::Value::Null),
        BexExternalValue::Int(i) => Some(serde_json::json!(i)),
        BexExternalValue::Float(f) => Some(serde_json::json!(f)),
        BexExternalValue::Bool(b) => Some(serde_json::json!(b)),
        BexExternalValue::String(s) => Some(serde_json::json!(s)),
        BexExternalValue::Array { items, .. } => {
            let arr: Vec<serde_json::Value> = items.iter().filter_map(bex_value_to_json).collect();
            Some(serde_json::Value::Array(arr))
        }
        BexExternalValue::Map { entries, .. } => {
            let map: serde_json::Map<String, serde_json::Value> = entries
                .iter()
                .filter_map(|(k, v)| bex_value_to_json(v).map(|jv| (k.clone(), jv)))
                .collect();
            Some(serde_json::Value::Object(map))
        }
        // Binary data, opaque handles, and internal types have no JSON representation.
        BexExternalValue::Uint8Array(_)
        | BexExternalValue::RustData(_)
        | BexExternalValue::Handle(_)
        | BexExternalValue::FunctionRef { .. }
        | BexExternalValue::Adt(_)
        | BexExternalValue::Instance { .. }
        | BexExternalValue::Variant { .. }
        | BexExternalValue::Union { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_builtins2::{MediaContent, MediaValue, PromptAst, PromptAstSimple};
    use bex_external_types::{AsBexExternalValue, Ty};
    use indexmap::IndexMap;

    use super::*;

    fn make_client(
        provider: &str,
        mut options: crate::baml_std::PrimitiveClientOptions,
    ) -> crate::baml_std::PrimitiveClient {
        if options.model.is_none() {
            options.model = Some("test-model".to_string());
        }
        options.default_role = Some("user".to_string());
        options.allowed_roles = Some(vec![
            "system".to_string(),
            "user".to_string(),
            "assistant".to_string(),
        ]);
        crate::baml_std::PrimitiveClient::new(
            "test-client".to_string(),
            provider.to_string(),
            options,
        )
        .unwrap()
    }

    fn msg(role: &str, text: &str) -> Arc<PromptAst> {
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(text.to_string().into()),
            metadata: serde_json::Value::Null,
        })
    }

    fn image_media() -> Arc<MediaValue> {
        Arc::new(MediaValue::new(
            baml_base::MediaKind::Image,
            MediaContent::Url {
                url: "https://example.com/image.png".to_string(),
                base64_data: None,
            },
            Some("image/png".to_string()),
        ))
    }

    fn simple_image_prompt() -> Arc<PromptAst> {
        Arc::new(PromptAst::Simple(Arc::new(PromptAstSimple::Multiple(
            vec![
                Arc::new(PromptAstSimple::String("Describe this image:".to_string())),
                Arc::new(PromptAstSimple::Media(image_media())),
            ],
        ))))
    }

    /// Parse the body JSON from an `HttpRequest`.
    fn parse_body(req: &crate::baml_std::HttpRequest) -> serde_json::Value {
        serde_json::from_str(&req.body).unwrap()
    }

    #[test]
    fn test_unknown_provider_rejected_at_construction() {
        let mut options = crate::baml_std::PrimitiveClientOptions {
            base_url: Some("https://example.com".to_string()),
            ..crate::baml_std::PrimitiveClientOptions::default()
        };
        options.default_role = Some("user".to_string());
        let result = crate::baml_std::PrimitiveClient::new(
            "test-client".to_string(),
            "unknown-provider".to_string(),
            options,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown provider"));
    }

    // ========================================================================
    // OpenAI tests
    // ========================================================================

    /// Matches `test_expose_request_gpt4` from `test_request.py`.
    #[tokio::test]
    async fn test_openai_gpt4o_system_only() {
        let client = make_client(
            "openai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gpt-4o".to_string()),
                api_key: Some("sk-test-key".to_string()),
                base_url: Some("https://api.openai.com/v1".to_string()),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );

        let system_text = "Given the receipt below:\n\n```\ntest@email.com\n```\n\nAnswer in JSON using this schema:\n{\n  items: [\n    {\n      name: string,\n      description: string or null,\n      quantity: int,\n      price: float,\n    }\n  ],\n  total_cost: float or null,\n  venue: \"barisa\" or \"ox_burger\",\n}";
        let prompt = Arc::new(PromptAst::Vec(vec![msg("system", system_text)]));

        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();

        // Verify envelope
        assert_eq!(result.method, "POST");
        assert_eq!(result.url, "https://api.openai.com/v1/chat/completions");
        assert_eq!(
            result.headers.get("content-type").unwrap(),
            "application/json"
        );

        // Verify body
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "model": "gpt-4o",
                "messages": [
                    {
                        "role": "system",
                        "content": [
                            {
                                "type": "text",
                                "text": system_text,
                            }
                        ]
                    }
                ]
            })
        );
    }

    /// Matches `test_expose_request_fallback` from `test_request.py`.
    #[tokio::test]
    async fn test_openai_gpt4_turbo_system_and_user() {
        let client = make_client(
            "openai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gpt-4-turbo".to_string()),
                api_key: Some("sk-test-key".to_string()),
                base_url: Some("https://api.openai.com/v1".to_string()),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );

        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are a helpful assistant."),
            msg("user", "Write a nice short story about Dr. Pepper"),
        ]));

        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();

        assert_eq!(result.url, "https://api.openai.com/v1/chat/completions");

        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "model": "gpt-4-turbo",
                "messages": [
                    {
                        "role": "system",
                        "content": [{"type": "text", "text": "You are a helpful assistant."}],
                    },
                    {
                        "role": "user",
                        "content": [
                            {
                                "type": "text",
                                "text": "Write a nice short story about Dr. Pepper",
                            }
                        ],
                    },
                ]
            })
        );
    }

    #[tokio::test]
    async fn test_openai_content_always_array() {
        let client = make_client(
            "openai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gpt-4o".to_string()),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );
        let prompt = msg("user", "Hello world");
        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "model": "gpt-4o",
                "messages": [
                    {"role": "user", "content": [{"type": "text", "text": "Hello world"}]}
                ]
            })
        );
    }

    #[tokio::test]
    async fn test_openai_responses_unrole_tagged_image_prompt_becomes_user_request() {
        let client = crate::baml_std::PrimitiveClient::new(
            "test-client".to_string(),
            "openai-responses".to_string(),
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gpt-5".to_string()),
                api_key: Some("sk-test-key".to_string()),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        )
        .unwrap();
        assert_eq!(client.default_role, "system");

        let specialized =
            crate::execute_specialize_prompt_from_owned(&client, simple_image_prompt()).unwrap();
        let result = build_request(
            &client,
            specialized,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();

        let body = parse_body(&result);
        assert_eq!(body["input"][0]["role"], "user");
        assert_eq!(
            body["input"][0]["content"],
            serde_json::json!([
                {"type": "input_text", "text": "Describe this image:"},
                {"type": "input_image", "image_url": "https://example.com/image.png"}
            ])
        );
    }

    #[tokio::test]
    async fn test_openai_chat_unrole_tagged_image_prompt_becomes_user_request() {
        let client = crate::baml_std::PrimitiveClient::new(
            "test-client".to_string(),
            "openai".to_string(),
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gpt-4o".to_string()),
                api_key: Some("sk-test-key".to_string()),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        )
        .unwrap();
        assert_eq!(client.default_role, "system");

        let specialized =
            crate::execute_specialize_prompt_from_owned(&client, simple_image_prompt()).unwrap();
        let result = build_request(
            &client,
            specialized,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();

        let body = parse_body(&result);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(
            body["messages"][0]["content"],
            serde_json::json!([
                {"type": "text", "text": "Describe this image:"},
                {"type": "image_url", "image_url": {"url": "https://example.com/image.png"}}
            ])
        );
    }

    #[tokio::test]
    async fn test_openai_custom_base_url() {
        let client = make_client(
            "openai",
            crate::baml_std::PrimitiveClientOptions {
                base_url: Some("https://custom.api.com".to_string()),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );
        let prompt = msg("user", "hello");
        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert_eq!(result.url, "https://custom.api.com/chat/completions");
    }

    #[tokio::test]
    async fn test_openai_forwards_options_to_body() {
        let client = make_client(
            "openai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gpt-4o".to_string()),
                request_body: IndexMap::from([(
                    "temperature".to_string(),
                    BexExternalValue::Float(0.7),
                )]),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );
        let prompt = msg("user", "hello");
        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "model": "gpt-4o",
                "temperature": 0.7,
                "messages": [
                    {"role": "user", "content": [{"type": "text", "text": "hello"}]}
                ]
            })
        );
    }

    #[tokio::test]
    async fn test_openai_skips_internal_options_in_body() {
        let client = make_client(
            "openai",
            crate::baml_std::PrimitiveClientOptions {
                api_key: Some("sk-secret".to_string()),
                base_url: Some("https://api.openai.com".to_string()),
                model: Some("gpt-4o".to_string()),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );
        let prompt = msg("user", "hello");
        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "model": "gpt-4o",
                "messages": [
                    {"role": "user", "content": [{"type": "text", "text": "hello"}]}
                ]
            })
        );
    }

    // ========================================================================
    // Anthropic tests
    // ========================================================================

    /// Matches `test_expose_request_round_robin` from `test_request.py`.
    #[tokio::test]
    async fn test_anthropic_claude_system_extracted() {
        let client = make_client(
            "anthropic",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("claude-3-haiku-20240307".to_string()),
                api_key: Some("sk-ant-test".to_string()),
                base_url: Some("https://api.anthropic.com".to_string()),
                request_body: IndexMap::from([(
                    "max_tokens".to_string(),
                    BexExternalValue::Int(1000),
                )]),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );

        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are a helpful assistant."),
            msg("user", "Write a nice short story about Dr. Pepper"),
        ]));

        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();

        // Verify envelope
        assert_eq!(result.method, "POST");
        assert_eq!(result.url, "https://api.anthropic.com/v1/messages");
        assert_eq!(
            result.headers.get("content-type").unwrap(),
            "application/json"
        );

        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 1000,
                "messages": [
                    {
                        "role": "user",
                        "content": [
                            {
                                "type": "text",
                                "text": "Write a nice short story about Dr. Pepper",
                            }
                        ]
                    }
                ],
                "system": [{"type": "text", "text": "You are a helpful assistant."}],
            })
        );
    }

    #[tokio::test]
    async fn test_anthropic_no_system_message() {
        let client = make_client(
            "anthropic",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("claude-3-haiku-20240307".to_string()),
                request_body: IndexMap::from([(
                    "max_tokens".to_string(),
                    BexExternalValue::Int(1000),
                )]),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );
        let prompt = msg("user", "Hello");
        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 1000,
                "messages": [
                    {"role": "user", "content": [{"type": "text", "text": "Hello"}]}
                ]
            })
        );
    }

    #[tokio::test]
    async fn test_anthropic_custom_headers() {
        let client = make_client(
            "anthropic",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("claude-3-haiku-20240307".to_string()),
                api_key: Some("sk-ant-test".to_string()),
                request_body: IndexMap::from([(
                    "max_tokens".to_string(),
                    BexExternalValue::Int(500),
                )]),
                allowed_role_metadata: Some(BexExternalValue::Array {
                    element_type: Ty::String {
                        attr: baml_type::TyAttr::default(),
                    },
                    items: vec![BexExternalValue::String("cache_control".into())],
                }),
                headers: IndexMap::from([(
                    "anthropic-beta".to_string(),
                    "prompt-caching-2024-07-31".into(),
                )]),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );

        let prompt = msg("user", "hello");
        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();

        assert_eq!(
            result.headers.get("anthropic-beta").unwrap(),
            "prompt-caching-2024-07-31"
        );

        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 500,
                "messages": [
                    {"role": "user", "content": [{"type": "text", "text": "hello"}]}
                ]
            })
        );
    }

    #[tokio::test]
    async fn test_anthropic_forwards_max_tokens() {
        let client = make_client(
            "anthropic",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("claude-3-haiku-20240307".to_string()),
                request_body: IndexMap::from([(
                    "max_tokens".to_string(),
                    BexExternalValue::Int(1000),
                )]),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );
        let prompt = msg("user", "hello");
        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 1000,
                "messages": [
                    {"role": "user", "content": [{"type": "text", "text": "hello"}]}
                ]
            })
        );
    }

    // ========================================================================
    // Query params tests
    // ========================================================================

    #[tokio::test]
    async fn test_query_params_appended_to_url() {
        let client = make_client(
            "openai",
            crate::baml_std::PrimitiveClientOptions {
                base_url: Some("https://api.openai.com/v1".to_string()),
                model: Some("gpt-4o".to_string()),
                query_params: IndexMap::from([
                    ("foo".to_string(), "bar".to_string()),
                    ("baz".to_string(), "qux".to_string()),
                ]),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );
        let prompt = msg("user", "hello");
        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert!(
            result.url.contains("foo=bar") && result.url.contains("baz=qux"),
            "URL should contain query params: {}",
            result.url
        );
    }

    #[tokio::test]
    async fn test_query_params_percent_encoded() {
        let client = make_client(
            "openai",
            crate::baml_std::PrimitiveClientOptions {
                base_url: Some("https://api.openai.com/v1".to_string()),
                model: Some("gpt-4o".to_string()),
                query_params: IndexMap::from([(
                    "key".to_string(),
                    "value with spaces & special=chars".to_string(),
                )]),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );
        let prompt = msg("user", "hello");
        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert!(
            result
                .url
                .contains("key=value+with+spaces+%26+special%3Dchars"),
            "Query param values should be percent-encoded: {}",
            result.url
        );
    }

    #[tokio::test]
    async fn test_no_query_params_no_question_mark() {
        let client = make_client(
            "openai",
            crate::baml_std::PrimitiveClientOptions {
                base_url: Some("https://api.openai.com/v1".to_string()),
                model: Some("gpt-4o".to_string()),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );
        let prompt = msg("user", "hello");
        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert!(
            !result.url.contains('?'),
            "URL should not contain '?' without query params: {}",
            result.url
        );
    }

    // ========================================================================
    // Anthropic tests (continued)
    // ========================================================================

    #[tokio::test]
    async fn test_anthropic_default_max_tokens_when_not_set() {
        let client = make_client(
            "anthropic",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("claude-3-haiku-20240307".to_string()),
                provider_options: crate::baml_std::AnthropicOptions {
                    max_tokens: Some(4096),
                }
                .into_bex_external_value(),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );
        let prompt = msg("user", "hello");
        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 4096,
                "messages": [
                    {"role": "user", "content": [{"type": "text", "text": "hello"}]}
                ]
            })
        );
    }

    // ========================================================================
    // Google AI tests
    // ========================================================================

    #[tokio::test]
    async fn test_google_ai_system_and_user() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                api_key: Some("gemini-key".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );

        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are a helpful assistant."),
            msg("user", "Write a nice short story about Dr. Pepper"),
        ]));

        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();

        assert_eq!(result.method, "POST");
        assert_eq!(
            result.url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent"
        );
        assert_eq!(
            result.headers.get("content-type").unwrap(),
            "application/json"
        );
        assert_eq!(result.headers.get("x-goog-api-key").unwrap(), "gemini-key");

        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {
                        "role": "user",
                        "parts": [{"text": "Write a nice short story about Dr. Pepper"}]
                    }
                ],
                "systemInstruction": {
                    "parts": [{"text": "You are a helpful assistant."}]
                }
            })
        );
    }

    #[tokio::test]
    async fn test_google_ai_user_only() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );

        let prompt = msg("user", "Hello");
        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {"role": "user", "parts": [{"text": "Hello"}]}
                ]
            })
        );
    }

    #[tokio::test]
    async fn test_google_ai_multi_turn() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );

        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful."),
            msg("user", "Hi"),
            msg("model", "Hello!"),
            msg("user", "How are you?"),
        ]));

        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {"role": "user", "parts": [{"text": "Hi"}]},
                    {"role": "model", "parts": [{"text": "Hello!"}]},
                    {"role": "user", "parts": [{"text": "How are you?"}]}
                ],
                "systemInstruction": {
                    "parts": [{"text": "You are helpful."}]
                }
            })
        );
    }

    #[tokio::test]
    async fn test_google_ai_forwards_generation_config() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                request_body: IndexMap::from([(
                    "generationConfig".to_string(),
                    BexExternalValue::Map {
                        key_type: baml_type::Ty::string(),
                        value_type: baml_type::Ty::unknown(),
                        entries: IndexMap::from([(
                            "temperature".to_string(),
                            BexExternalValue::Float(0.5),
                        )]),
                    },
                )]),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );

        let prompt = msg("user", "hello");
        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {"role": "user", "parts": [{"text": "hello"}]}
                ],
                "generationConfig": {
                    "temperature": 0.5
                }
            })
        );
    }

    #[tokio::test]
    async fn test_google_ai_query_params() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                query_params: IndexMap::from([("key".to_string(), "my-api-key".to_string())]),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );

        let prompt = msg("user", "hello");
        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        assert!(
            result.url.contains("key=my-api-key"),
            "URL should contain query param: {}",
            result.url
        );
    }

    // ========================================================================
    // Vertex AI tests
    // ========================================================================

    /// Vertex AI tests use `query_params: { "key": ... }` to skip ADC/OAuth
    /// token resolution, which would fail in CI/test environments.
    #[tokio::test]
    async fn test_vertex_ai_system_and_user() {
        let client = make_client(
            "vertex-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some(
                    "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/google/models"
                        .to_string(),
                ),
                query_params: IndexMap::from([("key".to_string(), "test-key".to_string())]),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );

        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are a helpful assistant."),
            msg("user", "Write a nice short story about Dr. Pepper"),
        ]));

        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();

        assert_eq!(result.method, "POST");
        assert!(
            result.url.starts_with(
                "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/google/models/gemini-2.0-flash:generateContent"
            ),
            "URL mismatch: {}",
            result.url
        );
        assert_eq!(
            result.headers.get("content-type").unwrap(),
            "application/json"
        );

        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {
                        "role": "user",
                        "parts": [{"text": "Write a nice short story about Dr. Pepper"}]
                    }
                ],
                "systemInstruction": {
                    "parts": [{"text": "You are a helpful assistant."}]
                }
            })
        );
    }

    #[tokio::test]
    async fn test_vertex_ai_user_only() {
        let client = make_client(
            "vertex-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some(
                    "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/google/models"
                        .to_string(),
                ),
                query_params: IndexMap::from([("key".to_string(), "test-key".to_string())]),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );

        let prompt = msg("user", "Hello");
        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {"role": "user", "parts": [{"text": "Hello"}]}
                ]
            })
        );
    }

    #[tokio::test]
    async fn test_vertex_ai_multi_turn() {
        let client = make_client(
            "vertex-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some(
                    "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/google/models"
                        .to_string(),
                ),
                query_params: IndexMap::from([("key".to_string(), "test-key".to_string())]),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );

        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful."),
            msg("user", "Hi"),
            msg("model", "Hello!"),
            msg("user", "How are you?"),
            msg("model", "I'm well."),
        ]));

        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {"role": "user", "parts": [{"text": "Hi"}]},
                    {"role": "model", "parts": [{"text": "Hello!"}]},
                    {"role": "user", "parts": [{"text": "How are you?"}]},
                    {"role": "model", "parts": [{"text": "I'm well."}]}
                ],
                "systemInstruction": {
                    "parts": [{"text": "You are helpful."}]
                }
            })
        );
    }

    #[tokio::test]
    async fn test_vertex_ai_forwards_generation_config() {
        let client = make_client(
            "vertex-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some(
                    "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/google/models"
                        .to_string(),
                ),
                query_params: IndexMap::from([("key".to_string(), "test-key".to_string())]),
                request_body: IndexMap::from([(
                    "generationConfig".to_string(),
                    BexExternalValue::Map {
                        key_type: baml_type::Ty::string(),
                        value_type: baml_type::Ty::unknown(),
                        entries: IndexMap::from([
                            (
                                "temperature".to_string(),
                                BexExternalValue::Float(0.7),
                            ),
                            (
                                "maxOutputTokens".to_string(),
                                BexExternalValue::Int(2048),
                            ),
                        ]),
                    },
                )]),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );

        let prompt = msg("user", "hello");
        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "contents": [
                    {"role": "user", "parts": [{"text": "hello"}]}
                ],
                "generationConfig": {
                    "temperature": 0.7,
                    "maxOutputTokens": 2048
                }
            })
        );
    }

    // ========================================================================
    // Vertex AI + Anthropic (rawPredict) tests
    // ========================================================================

    #[tokio::test]
    async fn test_vertex_anthropic_uses_raw_predict_url() {
        let client = make_client(
            "vertex-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("claude-sonnet-4-20250514".to_string()),
                base_url: Some(
                    "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/google/models"
                        .to_string(),
                ),
                query_params: IndexMap::from([("key".to_string(), "test-key".to_string())]),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );

        let prompt = msg("user", "Hello");
        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();

        assert!(
            result.url.contains(":rawPredict"),
            "URL should use rawPredict: {}",
            result.url
        );
        assert!(
            !result.url.contains(":generateContent"),
            "URL should NOT use generateContent: {}",
            result.url
        );
    }

    #[tokio::test]
    async fn test_vertex_anthropic_body_format() {
        let client = make_client(
            "vertex-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("claude-sonnet-4-20250514".to_string()),
                base_url: Some(
                    "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/google/models"
                        .to_string(),
                ),
                query_params: IndexMap::from([("key".to_string(), "test-key".to_string())]),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );

        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful."),
            msg("user", "Hi"),
            msg("assistant", "Hello!"),
            msg("user", "How are you?"),
        ]));

        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        let body = parse_body(&result);

        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-sonnet-4-20250514",
                "anthropic_version": "vertex-2023-10-16",
                "max_tokens": 8192,
                "system": [{"type": "text", "text": "You are helpful."}],
                "messages": [
                    {"role": "user", "content": [{"type": "text", "text": "Hi"}]},
                    {"role": "assistant", "content": [{"type": "text", "text": "Hello!"}]},
                    {"role": "user", "content": [{"type": "text", "text": "How are you?"}]}
                ]
            })
        );
    }

    /// End-to-end: specialization (with `remap_roles` as `lower_cst` sets it)
    /// followed by `build_request`. Proves Claude on Vertex keeps "assistant"
    /// through the full pipeline.
    #[tokio::test]
    async fn test_vertex_anthropic_e2e_roles_survive_specialization() {
        let client = make_client(
            "vertex-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("claude-sonnet-4-20250514".to_string()),
                base_url: Some(
                    "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/google/models"
                        .to_string(),
                ),
                query_params: IndexMap::from([("key".to_string(), "test-key".to_string())]),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );

        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful."),
            msg("user", "Hi"),
            msg("assistant", "Hello!"),
            msg("user", "How are you?"),
        ]));

        // Specialize first (remap_roles should be skipped for Claude).
        let specialized = crate::execute_specialize_prompt_from_owned(&client, prompt).unwrap();

        // Then build request.
        let result = build_request(
            &client,
            specialized,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        let body = parse_body(&result);

        assert!(result.url.contains(":rawPredict"));
        // "assistant" must survive, NOT be remapped to "model".
        assert_eq!(
            body["messages"][1]["role"], "assistant",
            "Claude on Vertex should keep 'assistant' role, got: {}",
            body["messages"][1]["role"]
        );
    }

    /// Mirror test: Gemini on Vertex DOES remap assistant -> model through
    /// the same pipeline.
    #[tokio::test]
    async fn test_vertex_gemini_e2e_remaps_assistant_to_model() {
        let client = make_client(
            "vertex-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some(
                    "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/google/models"
                        .to_string(),
                ),
                query_params: IndexMap::from([("key".to_string(), "test-key".to_string())]),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );

        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("user", "Hi"),
            msg("assistant", "Hello!"),
            msg("user", "How are you?"),
        ]));

        let specialized = crate::execute_specialize_prompt_from_owned(&client, prompt).unwrap();
        let result = build_request(
            &client,
            specialized,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();
        let body = parse_body(&result);

        assert!(result.url.contains(":generateContent"));
        // "assistant" must be remapped to "model" for Gemini.
        assert_eq!(
            body["contents"][1]["role"], "model",
            "Gemini on Vertex should remap to 'model', got: {}",
            body["contents"][1]["role"]
        );
    }

    #[tokio::test]
    async fn test_vertex_gemini_still_uses_generate_content() {
        let client = make_client(
            "vertex-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                base_url: Some(
                    "https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/google/models"
                        .to_string(),
                ),
                query_params: IndexMap::from([("key".to_string(), "test-key".to_string())]),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );

        let prompt = msg("user", "Hello");
        let result = build_request(
            &client,
            prompt,
            Arc::new(::sys_types::runtime_io::NoopRuntimeIo),
        )
        .await
        .unwrap();

        assert!(
            result.url.contains(":generateContent"),
            "Gemini should use generateContent: {}",
            result.url
        );
    }
}
