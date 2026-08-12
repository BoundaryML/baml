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

/// Truthy test for a `GOOGLE_GENAI_*` boolean env var: `"true"` / `"1"`,
/// trimmed and case-insensitive (matching the google-genai SDK).
async fn env_truthy(io: &dyn ::sys_types::runtime_io::RuntimeIo, key: &str) -> bool {
    matches!(
        io.env_get(key.to_string()).await.ok().flatten(),
        Some(v) if matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1")
    )
}

/// Whether the client opts into the Gemini **Enterprise** backend, via
/// `options.enterprise` or `GOOGLE_GENAI_USE_ENTERPRISE`.
///
/// An explicitly-set `options.enterprise` wins over the env var (so
/// `enterprise false` disables it even when `GOOGLE_GENAI_USE_ENTERPRISE=true`);
/// only an unset option falls back to the env var.
///
/// In google-genai, enterprise is an alias for the Vertex backend that
/// additionally defaults the location to `"global"` when it is otherwise
/// unresolved (applied in `auth_vertex`).
pub(crate) async fn google_use_enterprise(
    client: &crate::baml_std::PrimitiveClient,
    io: &dyn ::sys_types::runtime_io::RuntimeIo,
) -> bool {
    if client.provider != "google-ai" {
        return false;
    }

    let configured = match &client.provider_options {
        Some(crate::baml_std::ProviderOptions::GoogleAi(options)) => options.enterprise,
        _ => None,
    };
    match configured {
        Some(explicit) => explicit,
        None => env_truthy(io, "GOOGLE_GENAI_USE_ENTERPRISE").await,
    }
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
    let mut provider = LlmProvider::from_str(&client.provider)
        .map_err(|_| BuildRequestError::UnsupportedLlmProvider(client.provider.clone()))?;

    // google-genai parity: route a google-ai client through the Vertex backend
    // (URL, Google Cloud credentials, and auth) when GOOGLE_GENAI_USE_VERTEXAI
    // or GOOGLE_GENAI_USE_ENTERPRISE / `options.enterprise` is set. The SDK
    // treats enterprise as an alias for `vertexai=True` (enterprise additionally
    // defaults location to "global"; see auth_vertex). Location and project then
    // come from GOOGLE_CLOUD_LOCATION / GOOGLE_CLOUD_PROJECT or the credential
    // chain.
    if provider == LlmProvider::GoogleAi
        && (google_use_enterprise(client, &*io).await
            || env_truthy(&*io, "GOOGLE_GENAI_USE_VERTEXAI").await)
    {
        provider = LlmProvider::VertexAi;
    }

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

    // In the browser (WASM playground), requests go directly to LLM providers,
    // which CORS blocks. When BOUNDARY_PROXY_URL is set we route through the
    // playground proxy: it forwards to the original target (carried in the
    // `baml-original-url` header) and injects server-side API keys. Read the
    // env var before `auth_request` consumes `io`; apply the rewrite after, so
    // any auth headers are already present (the proxy overrides them for
    // allowed origins). Bedrock is excluded — its SigV4 signature is bound to
    // the host and would not survive the rewrite (matching legacy behavior).
    #[cfg(target_arch = "wasm32")]
    let proxy_url = if provider == LlmProvider::AwsBedrock {
        None
    } else {
        io.env_get("BOUNDARY_PROXY_URL".to_string())
            .await
            .ok()
            .flatten()
    };

    crate::auth_request::auth_request(provider, &mut request, client, io).await?;

    #[cfg(target_arch = "wasm32")]
    if let Some(proxy_url) = proxy_url {
        let proxy_url = proxy_url.trim();
        if !proxy_url.is_empty() {
            apply_proxy_rewrite(&mut request, proxy_url)?;
        }
    }

    Ok(request)
}

/// Route a request through the playground proxy (WASM only).
///
/// Sends to `{proxy}/<path+query>` and puts the original origin in the
/// `baml-original-url` header. The proxy reconstructs the real target by
/// appending the forwarded path to that origin, then injects the API key for
/// allowed model-provider origins.
#[cfg(target_arch = "wasm32")]
fn apply_proxy_rewrite(
    request: &mut crate::baml_std::HttpRequest,
    proxy_url: &str,
) -> Result<(), BuildRequestError> {
    let parsed = url::Url::parse(&request.url)
        .map_err(|e| BuildRequestError::Other(format!("invalid URL '{}': {e}", request.url)))?;
    let origin = parsed.origin().ascii_serialization();

    // Everything after the origin — appended to the original origin by the proxy.
    let mut suffix = parsed.path().to_string();
    if let Some(query) = parsed.query() {
        suffix.push('?');
        suffix.push_str(query);
    }

    request.url = format!("{}{}", proxy_url.trim_end_matches('/'), suffix);
    request
        .headers
        .insert("baml-original-url".to_string(), origin);
    Ok(())
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

    let url = google::resolve_vertex_raw_predict_url(client);

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
        // Bigints can exceed JSON number precision; emit as a decimal string.
        BexExternalValue::Bigint(b) => Some(serde_json::json!(b.to_string())),
        BexExternalValue::Float(f) => Some(serde_json::json!(f)),
        BexExternalValue::Bool(b) => Some(serde_json::json!(b)),
        BexExternalValue::String(s) => Some(serde_json::json!(s.as_str())),
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
        | BexExternalValue::HostValue(_)
        | BexExternalValue::Instance { .. }
        | BexExternalValue::Variant { .. }
        | BexExternalValue::Union { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_builtins2::{MediaContent, MediaValue, PromptAst, PromptAstSimple};
    use bex_external_types::{AsBexExternalValue, RuntimeTy};
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
                    element_type: RuntimeTy::String {
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
                api_key: Some("test-google-key".to_string()),
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
                api_key: Some("test-google-key".to_string()),
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
                api_key: Some("test-google-key".to_string()),
                base_url: Some("https://generativelanguage.googleapis.com/v1beta".to_string()),
                request_body: IndexMap::from([(
                    "generationConfig".to_string(),
                    BexExternalValue::Map {
                        key_type: baml_type::RuntimeTy::string(),
                        value_type: baml_type::RuntimeTy::unknown(),
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
                api_key: Some("test-google-key".to_string()),
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
                        key_type: baml_type::RuntimeTy::string(),
                        value_type: baml_type::RuntimeTy::unknown(),
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

    /// Minimal `RuntimeIo` serving env vars, credential files, and an OAuth
    /// token response — enough to drive the `GOOGLE_GENAI_USE_VERTEXAI` flip
    /// end to end.
    struct GcpEnvIo {
        env_vars: std::collections::HashMap<String, String>,
        files: std::collections::HashMap<String, String>,
        token_body: String,
    }

    impl ::sys_types::runtime_io::RuntimeIo for GcpEnvIo {
        fn http__send(
            &self,
            _request: sys_types::generated::owned::http::Request,
            _timeout_nanos: std::sync::Arc<num_bigint::BigInt>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<
                            sys_types::runtime_io::HttpResponseHandle,
                            ::sys_types::runtime_io::RuntimeIoError,
                        >,
                    > + Send
                    + '_,
            >,
        > {
            Box::pin(async {
                Ok(sys_types::runtime_io::HttpResponseHandle {
                    raw: bex_external_types::BexExternalValue::Null,
                    status_code: 200,
                    headers: IndexMap::new(),
                    url: String::new(),
                })
            })
        }

        fn http_response_text(
            &self,
            _: &sys_types::runtime_io::HttpResponseHandle,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<String, ::sys_types::runtime_io::RuntimeIoError>,
                    > + Send
                    + '_,
            >,
        > {
            let body = self.token_body.clone();
            Box::pin(async move { Ok(body) })
        }

        fn env_get(
            &self,
            key: String,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<Option<String>, ::sys_types::runtime_io::RuntimeIoError>,
                    > + Send
                    + '_,
            >,
        > {
            let val = self.env_vars.get(&key).cloned();
            Box::pin(async move { Ok(val) })
        }

        fn fs_open(
            &self,
            path: String,
            _mode: bex_external_types::BexExternalValue,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<
                            sys_types::runtime_io::FsFileHandle,
                            ::sys_types::runtime_io::RuntimeIoError,
                        >,
                    > + Send
                    + '_,
            >,
        > {
            let exists = self.files.contains_key(&path);
            Box::pin(async move {
                if exists {
                    Ok(sys_types::runtime_io::FsFileHandle {
                        raw: bex_external_types::BexExternalValue::String(path.into()),
                    })
                } else {
                    Err(::sys_types::runtime_io::RuntimeIoError::Other(
                        "not found".into(),
                    ))
                }
            })
        }

        fn fs_file_text(
            &self,
            handle: &sys_types::runtime_io::FsFileHandle,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<String, ::sys_types::runtime_io::RuntimeIoError>,
                    > + Send
                    + '_,
            >,
        > {
            let path = match &handle.raw {
                bex_external_types::BexExternalValue::String(s) => s.to_string(),
                _ => {
                    return Box::pin(async {
                        Err(::sys_types::runtime_io::RuntimeIoError::Other(
                            "bad handle".into(),
                        ))
                    });
                }
            };
            let contents = self.files.get(&path).cloned();
            Box::pin(async move {
                contents.ok_or_else(|| {
                    ::sys_types::runtime_io::RuntimeIoError::Other("not found".into())
                })
            })
        }

        fn sys_shell(
            &self,
            _: String,
            _options: Option<sys_types::generated::owned::sys::ProcessOptions>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<
                            sys_types::generated::owned::sys::ShellOutput,
                            ::sys_types::runtime_io::RuntimeIoError,
                        >,
                    > + Send
                    + '_,
            >,
        > {
            Box::pin(async {
                Err(::sys_types::runtime_io::RuntimeIoError::Other(
                    "unsupported".into(),
                ))
            })
        }
    }

    /// `GOOGLE_GENAI_USE_VERTEXAI=true` routes a google-ai client through the
    /// Vertex backend: aiplatform URL from `GOOGLE_CLOUD_PROJECT` +
    /// `GOOGLE_CLOUD_LOCATION`, bearer auth from ADC — no `api_key` needed, and
    /// the google-ai default `base_url` is ignored.
    #[tokio::test]
    async fn test_google_ai_use_vertexai_env_routes_through_vertex() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );

        let adc_json = serde_json::json!({
            "client_id": "flip-cid",
            "client_secret": "flip-secret",
            "refresh_token": "google-ai-flip-refresh-token",
            "type": "authorized_user",
            "token_uri": "https://fake-oauth.example.com/token",
        })
        .to_string();
        let io = GcpEnvIo {
            env_vars: std::collections::HashMap::from([
                ("GOOGLE_GENAI_USE_VERTEXAI".to_string(), "true".to_string()),
                (
                    "GOOGLE_CLOUD_PROJECT".to_string(),
                    "env-project".to_string(),
                ),
                (
                    "GOOGLE_CLOUD_LOCATION".to_string(),
                    "us-central1".to_string(),
                ),
                (
                    "GOOGLE_APPLICATION_CREDENTIALS".to_string(),
                    "/fake/flip-adc.json".to_string(),
                ),
            ]),
            files: std::collections::HashMap::from([("/fake/flip-adc.json".to_string(), adc_json)]),
            token_body: serde_json::json!({
                "access_token": "ya29.flip-token",
                "token_type": "Bearer",
                "expires_in": 3600,
            })
            .to_string(),
        };

        let prompt = msg("user", "Hello");
        let result = build_request(&client, prompt, Arc::new(io)).await.unwrap();

        assert_eq!(
            result.url,
            "https://us-central1-aiplatform.googleapis.com/v1/projects/env-project/locations/us-central1/publishers/google/models/gemini-2.0-flash:generateContent",
        );
        assert_eq!(
            result.headers.get("authorization").unwrap(),
            "Bearer ya29.flip-token",
        );
        assert!(!result.headers.contains_key("x-goog-api-key"));
    }

    /// A `GcpEnvIo` wired for ADC bearer auth (fake authorized-user creds + a
    /// fake token endpoint), with `extra_env` merged in. Mints `ya29.ent-token`.
    fn gcp_adc_io(extra_env: &[(&str, &str)]) -> GcpEnvIo {
        let adc_json = serde_json::json!({
            "client_id": "ent-cid",
            "client_secret": "ent-secret",
            "refresh_token": "google-ai-ent-refresh-token",
            "type": "authorized_user",
            "token_uri": "https://fake-oauth.example.com/token",
        })
        .to_string();
        let mut env_vars = std::collections::HashMap::from([(
            "GOOGLE_APPLICATION_CREDENTIALS".to_string(),
            "/fake/ent-adc.json".to_string(),
        )]);
        for (k, v) in extra_env {
            env_vars.insert((*k).to_string(), (*v).to_string());
        }
        GcpEnvIo {
            env_vars,
            files: std::collections::HashMap::from([("/fake/ent-adc.json".to_string(), adc_json)]),
            token_body: serde_json::json!({
                "access_token": "ya29.ent-token",
                "token_type": "Bearer",
                "expires_in": 3600,
            })
            .to_string(),
        }
    }

    /// Environment-driven Vertex routing must discard the Google AI Studio
    /// default URL for Anthropic partner models as well as Gemini models.
    #[tokio::test]
    async fn test_google_ai_use_vertexai_env_routes_claude_through_vertex() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("claude-sonnet-4-20250514".to_string()),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );
        let io = gcp_adc_io(&[
            ("GOOGLE_GENAI_USE_VERTEXAI", "true"),
            ("GOOGLE_CLOUD_PROJECT", "env-project"),
            ("GOOGLE_CLOUD_LOCATION", "us-central1"),
        ]);

        let result = build_request(&client, msg("user", "Hello"), Arc::new(io))
            .await
            .unwrap();

        assert_eq!(
            result.url,
            "https://us-central1-aiplatform.googleapis.com/v1/projects/env-project/locations/us-central1/publishers/anthropic/models/claude-sonnet-4-20250514:rawPredict",
        );
        assert_eq!(
            result.headers.get("authorization").unwrap(),
            "Bearer ya29.ent-token",
        );
    }

    /// `GOOGLE_GENAI_USE_ENTERPRISE=true` is an alias for the Vertex backend:
    /// same aiplatform URL + ADC bearer auth as `GOOGLE_GENAI_USE_VERTEXAI`.
    #[tokio::test]
    async fn test_google_ai_use_enterprise_env_routes_through_vertex() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );
        let io = gcp_adc_io(&[
            ("GOOGLE_GENAI_USE_ENTERPRISE", "true"),
            ("GOOGLE_CLOUD_PROJECT", "env-project"),
            ("GOOGLE_CLOUD_LOCATION", "us-central1"),
        ]);
        let prompt = msg("user", "Hello");
        let result = build_request(&client, prompt, Arc::new(io)).await.unwrap();
        assert_eq!(
            result.url,
            "https://us-central1-aiplatform.googleapis.com/v1/projects/env-project/locations/us-central1/publishers/google/models/gemini-2.0-flash:generateContent",
        );
        assert_eq!(
            result.headers.get("authorization").unwrap(),
            "Bearer ya29.ent-token",
        );
        assert!(!result.headers.contains_key("x-goog-api-key"));
    }

    /// `options.enterprise = true` routes through Vertex with no env flag set.
    #[tokio::test]
    async fn test_google_ai_enterprise_option_routes_through_vertex() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                provider_options: crate::baml_std::GoogleAiOptions {
                    enterprise: Some(true),
                    credentials: Some("/fake/google-options.json".to_string()),
                    location: Some("us-central1".to_string()),
                    project_id: Some("options-project".to_string()),
                    ..Default::default()
                }
                .into_bex_external_value(),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );
        let mut io = gcp_adc_io(&[]);
        let credentials = io.files.remove("/fake/ent-adc.json").unwrap();
        io.files
            .insert("/fake/google-options.json".to_string(), credentials);
        io.env_vars.remove("GOOGLE_APPLICATION_CREDENTIALS");
        let prompt = msg("user", "Hello");
        let result = build_request(&client, prompt, Arc::new(io)).await.unwrap();
        assert_eq!(
            result.url,
            "https://us-central1-aiplatform.googleapis.com/v1/projects/options-project/locations/us-central1/publishers/google/models/gemini-2.0-flash:generateContent",
        );
        assert_eq!(
            result.headers.get("authorization").unwrap(),
            "Bearer ya29.ent-token",
        );
        assert!(!result.headers.contains_key("x-goog-api-key"));
    }

    /// Enterprise mode defaults an unset location to the global endpoint
    /// (host `aiplatform.googleapis.com`, `locations/global`), matching
    /// google-genai — no `GOOGLE_CLOUD_LOCATION` needed.
    #[tokio::test]
    async fn test_google_ai_enterprise_defaults_unset_location_to_global() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                provider_options: crate::baml_std::GoogleAiOptions {
                    enterprise: Some(true),
                    ..Default::default()
                }
                .into_bex_external_value(),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );
        // GOOGLE_CLOUD_LOCATION intentionally unset.
        let io = gcp_adc_io(&[("GOOGLE_CLOUD_PROJECT", "env-project")]);
        let prompt = msg("user", "Hello");
        let result = build_request(&client, prompt, Arc::new(io)).await.unwrap();
        assert_eq!(
            result.url,
            "https://aiplatform.googleapis.com/v1/projects/env-project/locations/global/publishers/google/models/gemini-2.0-flash:generateContent",
        );
    }

    /// Plain vertexai routing (no enterprise signal) still requires a location:
    /// unlike enterprise, it does NOT silently default to global.
    #[tokio::test]
    async fn test_google_ai_vertexai_without_location_does_not_default_global() {
        let client = make_client(
            "google-ai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gemini-2.0-flash".to_string()),
                ..crate::baml_std::PrimitiveClientOptions::default()
            },
        );
        let io = gcp_adc_io(&[
            ("GOOGLE_GENAI_USE_VERTEXAI", "true"),
            ("GOOGLE_CLOUD_PROJECT", "env-project"),
        ]);
        let prompt = msg("user", "Hello");
        let err = build_request(&client, prompt, Arc::new(io))
            .await
            .unwrap_err();
        let err_msg = format!("{err:?}");
        assert!(
            err_msg.contains("Could not resolve location"),
            "vertexai must not silently default to global: {err_msg}"
        );
    }
}
