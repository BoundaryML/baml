//! LlmProvider-specific HTTP request building.
//!
//! Converts a `crate::baml_std::PrimitiveClient` + `PromptAst` into a `baml.http.Request` instance.

mod anthropic;
mod bedrock;
mod openai;

use std::str::FromStr;

use bex_external_types::BexExternalValue;

use crate::LlmProvider;

/// Build a provider-specific HTTP request from a specialized prompt.
///
/// Returns an owned `HttpRequest` matching the `baml.http.Request` class:
/// `{ method: String, url: String, headers: Map<String, String>, body: String }`
pub(crate) async fn build_request(
    client: &crate::baml_std::PrimitiveClient,
    prompt: bex_vm_types::PromptAst,
    callbacks: Option<&crate::BuildRequestCallbacks>,
) -> Result<crate::baml_std::HttpRequest, BuildRequestError> {
    let provider = LlmProvider::from_str(&client.provider)
        .map_err(|_| BuildRequestError::UnsupportedLlmProvider(client.provider.clone()))?;

    let mut request = match provider {
        LlmProvider::OpenAi
        | LlmProvider::OpenAiGeneric
        | LlmProvider::AzureOpenAi
        | LlmProvider::Ollama
        | LlmProvider::OpenRouter => openai::chat_completions::build_request(client, &prompt),
        LlmProvider::OpenAiResponses => openai::responses::build_request(client, &prompt),
        LlmProvider::Anthropic => anthropic::build_request(client, &prompt),
        LlmProvider::AwsBedrock => bedrock::build_request(client, &prompt, callbacks).await,
        LlmProvider::GoogleAi
        | LlmProvider::VertexAi
        | LlmProvider::BamlFallback
        | LlmProvider::BamlRoundRobin => Err(BuildRequestError::UnsupportedLlmProvider(
            client.provider.clone(),
        )),
    }?;

    // Auth is applied after body construction. Eventually this can be promoted
    // to a standalone step in the LLM function pipeline (llm.baml) so that
    // auth can be resolved, cached, or refreshed independently of request
    // building.
    crate::auth_request::auth_request(provider, &mut request, client, callbacks).await?;

    Ok(request)
}

/// Extract a MIME type from a `MediaValue`, returning an error if none is set.
pub(super) fn mime_type_as_ok(
    media: &baml_builtins::MediaValue,
) -> Result<&str, BuildRequestError> {
    media
        .mime_type
        .as_deref()
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
        _ => None, // Skip non-serializable types (Resource, PromptAst, etc.)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_builtins::PromptAst;
    use bex_external_types::{AsBexExternalValue, Ty};
    use indexmap::IndexMap;

    use super::*;

    fn make_client(
        provider: &str,
        mut options: crate::baml_std::PrimitiveClientOptions,
    ) -> crate::baml_std::PrimitiveClient {
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

        let result = build_request(&client, prompt, None).await.unwrap();

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

        let result = build_request(&client, prompt, None).await.unwrap();

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
        let result = build_request(&client, prompt, None).await.unwrap();
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
    async fn test_openai_custom_base_url() {
        let client = make_client(
            "openai",
            crate::baml_std::PrimitiveClientOptions {
                base_url: Some("https://custom.api.com".to_string()),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );
        let prompt = msg("user", "hello");
        let result = build_request(&client, prompt, None).await.unwrap();
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
        let result = build_request(&client, prompt, None).await.unwrap();
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
        let result = build_request(&client, prompt, None).await.unwrap();
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

        let result = build_request(&client, prompt, None).await.unwrap();

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
        let result = build_request(&client, prompt, None).await.unwrap();
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
        let result = build_request(&client, prompt, None).await.unwrap();

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
        let result = build_request(&client, prompt, None).await.unwrap();
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
        let result = build_request(&client, prompt, None).await.unwrap();
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
}
