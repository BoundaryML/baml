//! LlmProvider-specific HTTP request building.
//!
//! Converts a `LlmPrimitiveClient` + `PromptAst` into a `baml.http.Request` instance.

mod anthropic;
mod bedrock;
mod google;
mod openai;

use std::str::FromStr;

use bex_external_types::BexExternalValue;
use bex_heap::{builtin_types, builtin_types::owned::LlmPrimitiveClient};

use crate::{LlmProvider, auth_request::LlmRequestAuthorizer};

/// Option keys consumed by `specialize_prompt` — never forwarded to the request body.
const SPECIALIZE_PROMPT_SKIP_KEYS: &[&str] = &[
    "max_one_system_prompt",
    "allowed_role_metadata",
    "default_role",
    "allowed_roles",
];

/// Option keys consumed by `build_request` itself (URL, auth, headers, model) —
/// never forwarded to the request body.
const BUILD_REQUEST_SKIP_KEYS: &[&str] = &["api_key", "base_url", "model", "headers"];

/// Callbacks available during request building for cross-op calls.
///
/// Bridges `SysOpHttp`, `SysOpEnv`, and `SysOpFs` into provider-specific
/// request builders without a direct dependency on `sys_types`.
pub(crate) struct BuildRequestCallbacks<'a> {
    pub http_send: &'a crate::HttpSendFn,
    pub env_read: &'a crate::EnvReadFn,
    pub fs_read: &'a crate::FsReadFn,
}

/// Trait for building provider-specific HTTP requests.
///
/// Each provider implements only `build_request`. Shared helpers
/// (`build_headers`, `forward_options`) are free functions.
pub(crate) trait LlmRequestBuilder {
    /// Build the full HTTP request for this provider.
    async fn build_request(
        &self,
        client: &LlmPrimitiveClient,
        prompt: bex_vm_types::PromptAst,
        stream: bool,
        callbacks: &BuildRequestCallbacks<'_>,
    ) -> Result<RawHttpRequest, BuildRequestError>;
}

// ---------------------------------------------------------------------------
// Shared default helpers - called by provider `build_request` implementations
// ---------------------------------------------------------------------------

/// Build headers: content-type + auth headers + custom headers from options.
pub(crate) fn build_headers(
    auth_headers: indexmap::IndexMap<String, String>,
    client: &LlmPrimitiveClient,
) -> indexmap::IndexMap<String, String> {
    let mut headers = indexmap::IndexMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());
    headers.extend(auth_headers);
    // Forward custom headers from client.options["headers"]
    if let Some(BexExternalValue::Map { entries, .. }) = client.options.get("headers") {
        for (key, value) in entries {
            if let BexExternalValue::String(v) = value {
                headers.insert(key.clone(), v.clone());
            }
        }
    }
    headers
}

/// Forward non-skipped options from `client.options` into the JSON body.
///
/// Keys already present in `body` (written by the provider builder) are never
/// overwritten. The builder's values for prompt-derived fields like `messages`,
/// `system`, `input`, `stream`, and `stream_options` take precedence.
pub(crate) fn forward_options(
    provider_skip_keys: &[&str],
    client: &LlmPrimitiveClient,
    body: &mut serde_json::Map<String, serde_json::Value>,
) {
    for (key, value) in &client.options {
        if SPECIALIZE_PROMPT_SKIP_KEYS.contains(&key.as_str())
            || BUILD_REQUEST_SKIP_KEYS.contains(&key.as_str())
            || provider_skip_keys.contains(&key.as_str())
        {
            continue;
        }
        // Never overwrite keys the builder already wrote (e.g. messages,
        // system, input, stream, stream_options).
        if body.contains_key(key) {
            continue;
        }
        if let Some(json_val) = bex_value_to_json(value) {
            body.insert(key.clone(), json_val);
        }
    }
}

/// Check if a Vertex AI client is configured for Anthropic (Claude) models.
///
/// The legacy engine detects this via `anthropic_version` in the options, or
/// model names starting with "claude".
fn is_anthropic_on_vertex(client: &LlmPrimitiveClient) -> bool {
    client.options.contains_key("anthropic_version")
        || get_string_option(client, "model").is_some_and(|m| m.starts_with("claude"))
}

/// Build a provider-specific HTTP request from a specialized prompt.
///
/// Returns an owned `HttpRequest` matching the `baml.http.Request` class:
/// `{ method: String, url: String, headers: Map<String, String>, body: String }`
pub(crate) async fn build_request(
    client: &LlmPrimitiveClient,
    prompt: bex_vm_types::PromptAst,
    stream: bool,
    http_send: &crate::HttpSendFn,
    env_read: &crate::EnvReadFn,
    fs_read: &crate::FsReadFn,
) -> Result<builtin_types::owned::HttpRequest, BuildRequestError> {
    let provider = LlmProvider::from_str(&client.provider)
        .map_err(|_| BuildRequestError::UnsupportedLlmProvider(client.provider.clone()))?;

    let callbacks = BuildRequestCallbacks {
        http_send,
        env_read,
        fs_read,
    };

    let raw = match provider {
        LlmProvider::OpenAi
        | LlmProvider::OpenAiGeneric
        | LlmProvider::AzureOpenAi
        | LlmProvider::Ollama
        | LlmProvider::OpenRouter => {
            openai::OpenAiBuilder::new(&provider)
                .build_request(client, prompt, stream, &callbacks)
                .await?
        }
        LlmProvider::OpenAiResponses => {
            openai::OpenAiResponsesBuilder::new(&provider)
                .build_request(client, prompt, stream, &callbacks)
                .await?
        }
        LlmProvider::Anthropic => {
            anthropic::AnthropicBuilder
                .build_request(client, prompt, stream, &callbacks)
                .await?
        }
        LlmProvider::AwsBedrock => {
            bedrock::BedrockBuilder
                .build_request(client, prompt, stream, &callbacks)
                .await?
        }
        LlmProvider::VertexAi if is_anthropic_on_vertex(client) => {
            let mut raw = anthropic::AnthropicBuilder
                .build_request(client, prompt, stream, &callbacks)
                .await?;
            // Anthropic-on-Vertex uses rawPredict/streamRawPredict endpoints
            // with the standard Anthropic body format (including stream: true).
            raw.url = google::vertex_anthropic_url(client, stream)?;
            raw
        }
        LlmProvider::VertexAi | LlmProvider::GoogleAi => {
            google::GoogleBuilder
                .build_request(client, prompt, stream, &callbacks)
                .await?
        }
        LlmProvider::BamlFallback | LlmProvider::BamlRoundRobin => {
            return Err(BuildRequestError::UnsupportedLlmProvider(
                client.provider.clone(),
            ));
        }
    };

    // Authorize the built request (API keys, bearer tokens, SigV4 signatures, etc.)
    // Eventually this will be moved to its own stage in llm.baml after request building.
    let authorized = match provider {
        LlmProvider::Anthropic => {
            crate::auth_request::AnthropicAuth
                .authorize(raw, client, &callbacks)
                .await?
        }
        LlmProvider::OpenAi
        | LlmProvider::OpenAiGeneric
        | LlmProvider::AzureOpenAi
        | LlmProvider::Ollama
        | LlmProvider::OpenRouter
        | LlmProvider::OpenAiResponses => {
            crate::auth_request::OpenAiAuth {
                provider: &provider,
            }
            .authorize(raw, client, &callbacks)
            .await?
        }
        LlmProvider::AwsBedrock => {
            crate::auth_request::BedrockAuth
                .authorize(raw, client, &callbacks)
                .await?
        }
        LlmProvider::VertexAi if is_anthropic_on_vertex(client) => {
            // Anthropic-on-Vertex needs the anthropic-version header.
            // Vertex OAuth2 auth is not yet implemented here.
            crate::auth_request::AnthropicAuth
                .authorize(raw, client, &callbacks)
                .await?
        }
        LlmProvider::VertexAi | LlmProvider::GoogleAi => {
            // Auth not yet implemented -- return the raw request as-is.
            raw
        }
        _ => unreachable!("unsupported providers rejected above"),
    };

    Ok(authorized.into_owned())
}

/// Intermediate struct before converting to an owned `HttpRequest`.
#[derive(Debug)]
pub(crate) struct RawHttpRequest {
    pub method: String,
    pub url: String,
    pub headers: indexmap::IndexMap<String, String>,
    pub body: String,
}

impl RawHttpRequest {
    /// Convert to an owned `builtin_types::owned::HttpRequest`.
    fn into_owned(self) -> builtin_types::owned::HttpRequest {
        builtin_types::owned::HttpRequest {
            method: self.method,
            url: self.url,
            headers: self.headers,
            body: self.body,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum BuildRequestError {
    #[error("Unsupported provider: {0}")]
    UnsupportedLlmProvider(String),
    #[error("Missing required option: {0}")]
    MissingOption(String),
    #[error("Invalid option value for '{key}': {reason}")]
    InvalidOption { key: String, reason: String },
    #[error("Unsupported media: {0}")]
    UnsupportedMedia(String),
    #[error("File not resolved: {0}")]
    FileNotResolved(String),
    #[error("Failed to serialize request body: {0}")]
    BodySerialization(String),
    /// Will move to a dedicated error type once request building and request
    /// authorization are fully separate.
    #[error("Authorization failed: {0}")]
    AuthorizationFailed(String),
}

/// Returns the MIME type of a media value, or an error if none is set.
pub(crate) fn mime_type_as_ok(
    media: &baml_builtins::MediaValue,
) -> Result<&str, BuildRequestError> {
    media.mime_type.as_deref().ok_or_else(|| {
        BuildRequestError::UnsupportedMedia(format!(
            "missing MIME type for {} media; please specify one explicitly",
            media.kind
        ))
    })
}

/// Helper to extract a string option from client.options.
pub(crate) fn get_string_option(client: &LlmPrimitiveClient, key: &str) -> Option<String> {
    match client.options.get(key) {
        Some(BexExternalValue::String(s)) => Some(s.clone()),
        _ => None,
    }
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

/// Test-only stub callbacks that panic if called. Most providers don't invoke
/// these during request building, so the stubs are safe for unit tests.
#[cfg(test)]
pub(crate) fn stub_callbacks() -> (crate::HttpSendFn, crate::EnvReadFn, crate::FsReadFn) {
    use std::sync::Arc;
    let http: crate::HttpSendFn =
        Arc::new(|_| Box::pin(async { panic!("unexpected http_send call in test") }));
    let env: crate::EnvReadFn =
        Arc::new(|_| Box::pin(async { panic!("unexpected env_read call in test") }));
    let fs: crate::FsReadFn =
        Arc::new(|_| Box::pin(async { panic!("unexpected fs_read call in test") }));
    (http, env, fs)
}

/// Test-only no-op env/fs callbacks that return "not found" instead of panicking.
/// Use these in tests that exercise credential resolution (e.g., bedrock without
/// explicit credentials) where the AWS SDK will read env vars and files.
#[cfg(test)]
pub(crate) fn noop_env_fs_callbacks() -> (crate::EnvReadFn, crate::FsReadFn) {
    use std::sync::Arc;
    let env: crate::EnvReadFn = Arc::new(|_| Box::pin(async { Ok(None) }));
    let fs: crate::FsReadFn =
        Arc::new(|_| Box::pin(async { Err(crate::LlmOpError::Other("not found".into())) }));
    (env, fs)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_builtins::PromptAst;
    use bex_external_types::Ty;
    use indexmap::IndexMap;

    use super::*;

    fn make_client(provider: &str, options: Vec<(&str, BexExternalValue)>) -> LlmPrimitiveClient {
        let mut opts = IndexMap::new();
        for (k, v) in options {
            opts.insert(k.to_string(), v);
        }
        LlmPrimitiveClient {
            name: "test-client".to_string(),
            provider: provider.to_string(),
            default_role: "user".to_string(),
            allowed_roles: vec![
                "system".to_string(),
                "user".to_string(),
                "assistant".to_string(),
            ],
            options: opts,
        }
    }

    fn msg(role: &str, text: &str) -> Arc<PromptAst> {
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(text.to_string().into()),
            metadata: serde_json::Value::Null,
        })
    }

    /// Parse the body JSON from an `HttpRequest`.
    fn parse_body(req: &builtin_types::owned::HttpRequest) -> serde_json::Value {
        serde_json::from_str(&req.body).unwrap()
    }

    #[tokio::test]
    async fn test_unsupported_provider() {
        let client = make_client("unknown-provider", vec![]);
        let prompt = msg("user", "hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        };
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unsupported provider")
        );
    }

    // ========================================================================
    // OpenAI tests — modeled after integ-tests/python/tests/test_request.py
    // ========================================================================

    /// Matches `test_expose_request_gpt4` from `test_request.py`.
    #[tokio::test]
    async fn test_openai_gpt4o_system_only() {
        let client = make_client(
            "openai",
            vec![
                ("model", BexExternalValue::String("gpt-4o".into())),
                ("api_key", BexExternalValue::String("sk-test-key".into())),
            ],
        );

        let system_text = "Given the receipt below:\n\n```\ntest@email.com\n```\n\nAnswer in JSON using this schema:\n{\n  items: [\n    {\n      name: string,\n      description: string or null,\n      quantity: int,\n      price: float,\n    }\n  ],\n  total_cost: float or null,\n  venue: \"barisa\" or \"ox_burger\",\n}";
        let prompt = Arc::new(PromptAst::Vec(vec![msg("system", system_text)]));

        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();

        // Verify envelope
        assert_eq!(result.method, "POST");
        assert_eq!(result.url, "https://api.openai.com/v1/chat/completions");
        assert_eq!(
            result.headers.get("authorization").unwrap(),
            "Bearer sk-test-key"
        );
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
            vec![
                ("model", BexExternalValue::String("gpt-4-turbo".into())),
                ("api_key", BexExternalValue::String("sk-test-key".into())),
            ],
        );

        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are a helpful assistant."),
            msg("user", "Write a nice short story about Dr. Pepper"),
        ]));

        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
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
            vec![("model", BexExternalValue::String("gpt-4o".into()))],
        );
        let prompt = msg("user", "Hello world");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        let body = parse_body(&result);
        assert!(body["messages"][0]["content"].is_array());
        assert_eq!(body["messages"][0]["content"][0]["type"], "text");
        assert_eq!(body["messages"][0]["content"][0]["text"], "Hello world");
    }

    #[tokio::test]
    async fn test_openai_custom_base_url() {
        let client = make_client(
            "openai",
            vec![(
                "base_url",
                BexExternalValue::String("https://custom.api.com".into()),
            )],
        );
        let prompt = msg("user", "hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        assert_eq!(result.url, "https://custom.api.com/chat/completions");
    }

    #[tokio::test]
    async fn test_openai_forwards_options_to_body() {
        let client = make_client(
            "openai",
            vec![
                ("model", BexExternalValue::String("gpt-4o".into())),
                ("temperature", BexExternalValue::Float(0.7)),
            ],
        );
        let prompt = msg("user", "hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(body["temperature"], 0.7);
    }

    #[tokio::test]
    async fn test_openai_skips_internal_options_in_body() {
        let client = make_client(
            "openai",
            vec![
                ("api_key", BexExternalValue::String("sk-secret".into())),
                (
                    "base_url",
                    BexExternalValue::String("https://api.openai.com".into()),
                ),
                ("model", BexExternalValue::String("gpt-4o".into())),
            ],
        );
        let prompt = msg("user", "hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        let body = parse_body(&result);
        assert!(body.get("api_key").is_none());
        assert!(body.get("base_url").is_none());
        // model IS in the body
        assert_eq!(body["model"], "gpt-4o");
    }

    // ========================================================================
    // Anthropic tests — modeled after integ-tests/python/tests/test_request.py
    // ========================================================================

    /// Matches `test_expose_request_round_robin` from `test_request.py`.
    #[tokio::test]
    async fn test_anthropic_claude_system_extracted() {
        let client = make_client(
            "anthropic",
            vec![
                (
                    "model",
                    BexExternalValue::String("claude-3-haiku-20240307".into()),
                ),
                ("api_key", BexExternalValue::String("sk-ant-test".into())),
                ("max_tokens", BexExternalValue::Int(1000)),
            ],
        );

        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are a helpful assistant."),
            msg("user", "Write a nice short story about Dr. Pepper"),
        ]));

        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();

        // Verify envelope
        assert_eq!(result.method, "POST");
        assert_eq!(result.url, "https://api.anthropic.com/v1/messages");
        assert_eq!(result.headers.get("x-api-key").unwrap(), "sk-ant-test");
        assert_eq!(
            result.headers.get("content-type").unwrap(),
            "application/json"
        );
        assert!(result.headers.contains_key("anthropic-version"));

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
            vec![
                (
                    "model",
                    BexExternalValue::String("claude-3-haiku-20240307".into()),
                ),
                ("max_tokens", BexExternalValue::Int(1000)),
            ],
        );
        let prompt = msg("user", "Hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        let body = parse_body(&result);
        assert!(body.get("system").is_none());
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_anthropic_custom_headers() {
        let mut header_entries = IndexMap::new();
        header_entries.insert(
            "anthropic-beta".to_string(),
            BexExternalValue::String("prompt-caching-2024-07-31".into()),
        );

        let client = make_client(
            "anthropic",
            vec![
                (
                    "model",
                    BexExternalValue::String("claude-3-haiku-20240307".into()),
                ),
                ("api_key", BexExternalValue::String("sk-ant-test".into())),
                ("max_tokens", BexExternalValue::Int(500)),
                (
                    "allowed_role_metadata",
                    BexExternalValue::Array {
                        element_type: Ty::String {
                            attr: baml_type::TyAttr::default(),
                        },
                        items: vec![BexExternalValue::String("cache_control".into())],
                    },
                ),
                (
                    "headers",
                    BexExternalValue::Map {
                        key_type: Ty::String {
                            attr: baml_type::TyAttr::default(),
                        },
                        value_type: Ty::String {
                            attr: baml_type::TyAttr::default(),
                        },
                        entries: header_entries,
                    },
                ),
            ],
        );

        let prompt = msg("user", "hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();

        assert_eq!(
            result.headers.get("anthropic-beta").unwrap(),
            "prompt-caching-2024-07-31"
        );

        let body = parse_body(&result);
        assert!(body.get("allowed_role_metadata").is_none());
        assert!(body.get("headers").is_none());
    }

    #[tokio::test]
    async fn test_anthropic_custom_version() {
        let client = make_client(
            "anthropic",
            vec![(
                "anthropic_version",
                BexExternalValue::String("2024-01-01".into()),
            )],
        );
        let prompt = msg("user", "hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        assert_eq!(
            result.headers.get("anthropic-version").unwrap(),
            "2024-01-01"
        );
    }

    #[tokio::test]
    async fn test_anthropic_default_version() {
        let client = make_client("anthropic", vec![]);
        let prompt = msg("user", "hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        assert_eq!(
            result.headers.get("anthropic-version").unwrap(),
            "2023-06-01"
        );
    }

    #[tokio::test]
    async fn test_anthropic_forwards_max_tokens() {
        let client = make_client(
            "anthropic",
            vec![
                (
                    "model",
                    BexExternalValue::String("claude-3-haiku-20240307".into()),
                ),
                ("max_tokens", BexExternalValue::Int(1000)),
            ],
        );
        let prompt = msg("user", "hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(body["max_tokens"], 1000);
    }

    // ========================================================================
    // OpenAI Responses API tests
    // ========================================================================

    #[tokio::test]
    async fn test_responses_url() {
        let client = make_client(
            "openai-responses",
            vec![
                ("model", BexExternalValue::String("gpt-4o".into())),
                ("api_key", BexExternalValue::String("sk-test".into())),
            ],
        );
        let prompt = msg("user", "hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        assert_eq!(result.url, "https://api.openai.com/v1/responses");
        assert_eq!(
            result.headers.get("authorization").unwrap(),
            "Bearer sk-test"
        );
    }

    #[tokio::test]
    async fn test_responses_uses_input_key() {
        let client = make_client(
            "openai-responses",
            vec![("model", BexExternalValue::String("gpt-4o".into()))],
        );
        let prompt = msg("user", "hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        let body = parse_body(&result);
        assert!(
            body.get("input").is_some(),
            "Responses API should use 'input' key"
        );
        assert!(
            body.get("messages").is_none(),
            "Responses API should not have 'messages' key"
        );
    }

    #[tokio::test]
    async fn test_responses_input_text_type() {
        let client = make_client(
            "openai-responses",
            vec![("model", BexExternalValue::String("gpt-4o".into()))],
        );
        let prompt = msg("user", "hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "model": "gpt-4o",
                "input": [
                    {
                        "role": "user",
                        "content": [{"type": "input_text", "text": "hello"}]
                    }
                ]
            })
        );
    }

    #[tokio::test]
    async fn test_responses_output_text_for_assistant() {
        let client = make_client(
            "openai-responses",
            vec![("model", BexExternalValue::String("gpt-4o".into()))],
        );
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("user", "hello"),
            msg("assistant", "hi there"),
        ]));
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(body["input"][0]["content"][0]["type"], "input_text");
        assert_eq!(body["input"][1]["content"][0]["type"], "output_text");
    }

    #[tokio::test]
    async fn test_responses_system_and_user() {
        let client = make_client(
            "openai-responses",
            vec![("model", BexExternalValue::String("gpt-4o".into()))],
        );
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful."),
            msg("user", "Hi"),
        ]));
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(body["input"][0]["role"], "system");
        assert_eq!(body["input"][0]["content"][0]["type"], "input_text");
        assert_eq!(body["input"][1]["role"], "user");
    }

    #[tokio::test]
    async fn test_responses_custom_base_url() {
        let client = make_client(
            "openai-responses",
            vec![(
                "base_url",
                BexExternalValue::String("https://custom.api.com/v1".into()),
            )],
        );
        let prompt = msg("user", "hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        assert_eq!(result.url, "https://custom.api.com/v1/responses");
    }

    #[tokio::test]
    async fn test_responses_forwards_options() {
        let client = make_client(
            "openai-responses",
            vec![
                ("model", BexExternalValue::String("gpt-4o".into())),
                ("temperature", BexExternalValue::Float(0.5)),
            ],
        );
        let prompt = msg("user", "hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(body["temperature"], 0.5);
    }

    // ========================================================================
    // Bedrock integration tests (build + SigV4 auth through the full pipeline)
    // ========================================================================

    fn bedrock_options() -> Vec<(&'static str, BexExternalValue)> {
        vec![
            ("region", BexExternalValue::String("us-east-1".into())),
            (
                "model",
                BexExternalValue::String("anthropic.claude-3-haiku-20240307-v1:0".into()),
            ),
            (
                "access_key_id",
                BexExternalValue::String("AKIAIOSFODNN7EXAMPLE".into()),
            ),
            (
                "secret_access_key",
                BexExternalValue::String("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into()),
            ),
        ]
    }

    #[tokio::test]
    async fn test_bedrock_url_and_method() {
        let client = make_client("aws-bedrock", bedrock_options());
        let prompt = msg("user", "Hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        assert_eq!(result.method, "POST");
        assert_eq!(
            result.url,
            "https://bedrock-runtime.us-east-1.amazonaws.com/model/anthropic.claude-3-haiku-20240307-v1:0/converse"
        );
    }

    #[tokio::test]
    async fn test_bedrock_sigv4_headers_present() {
        let client = make_client("aws-bedrock", bedrock_options());
        let prompt = msg("user", "Hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        assert!(result.headers.contains_key("authorization"));
        assert!(result.headers.contains_key("x-amz-date"));
        assert_eq!(
            result.headers.get("content-type").unwrap(),
            "application/json"
        );
    }

    #[tokio::test]
    async fn test_bedrock_stream_url() {
        let client = make_client("aws-bedrock", bedrock_options());
        let prompt = msg("user", "Hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, true, &h, &e, &f).await
        }
        .unwrap();
        assert!(result.url.ends_with("/converse-stream"));
    }

    #[tokio::test]
    async fn test_bedrock_body_structure() {
        let client = make_client("aws-bedrock", bedrock_options());
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful."),
            msg("user", "Hi"),
        ]));
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "system": [{"text": "You are helpful."}],
                "messages": [
                    {"role": "user", "content": [{"text": "Hi"}]}
                ]
            })
        );
    }

    #[tokio::test]
    async fn test_bedrock_no_creds_or_region_in_body() {
        let client = make_client("aws-bedrock", bedrock_options());
        let prompt = msg("user", "Hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "messages": [
                    {"role": "user", "content": [{"text": "Hello"}]}
                ]
            })
        );
    }

    #[tokio::test]
    async fn test_bedrock_inference_config_through_pipeline() {
        let mut opts = bedrock_options();
        opts.push(("max_tokens", BexExternalValue::Int(500)));
        opts.push(("temperature", BexExternalValue::Float(0.5)));
        let client = make_client("aws-bedrock", opts);
        let prompt = msg("user", "Hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        let body = parse_body(&result);
        assert_eq!(
            body,
            serde_json::json!({
                "messages": [
                    {"role": "user", "content": [{"text": "Hello"}]}
                ],
                "inferenceConfig": {
                    "maxTokens": 500,
                    "temperature": 0.5,
                }
            })
        );
    }

    // ========================================================================
    // Vertex AI tests (Gemini + Anthropic-on-Vertex)
    // ========================================================================

    #[tokio::test]
    async fn test_vertex_gemini_url_and_body() {
        let client = make_client(
            "vertex-ai",
            vec![
                ("model", BexExternalValue::String("gemini-1.5-pro".into())),
                ("location", BexExternalValue::String("us-central1".into())),
                ("project_id", BexExternalValue::String("my-project".into())),
            ],
        );
        let prompt = msg("user", "Hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        assert_eq!(result.method, "POST");
        assert!(result.url.contains("generateContent"));
        assert!(!result.url.contains("rawPredict"));
        let body = parse_body(&result);
        assert!(body.get("contents").is_some());
    }

    #[tokio::test]
    async fn test_vertex_anthropic_uses_raw_predict() {
        let client = make_client(
            "vertex-ai",
            vec![
                (
                    "model",
                    BexExternalValue::String("claude-3-5-sonnet@20241022".into()),
                ),
                ("location", BexExternalValue::String("us-east5".into())),
                ("project_id", BexExternalValue::String("my-project".into())),
                (
                    "anthropic_version",
                    BexExternalValue::String("vertex-2023-10-16".into()),
                ),
                ("max_tokens", BexExternalValue::Int(1000)),
            ],
        );
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "Be helpful."),
            msg("user", "Hello"),
        ]));
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();

        // URL should use rawPredict, not generateContent
        assert!(
            result.url.contains("rawPredict"),
            "expected rawPredict in URL, got: {}",
            result.url
        );
        assert!(result.url.contains("claude-3-5-sonnet@20241022:rawPredict"),);
        assert_eq!(
            result.url,
            "https://us-east5-aiplatform.googleapis.com/v1/projects/my-project/locations/us-east5/publishers/google/models/claude-3-5-sonnet@20241022:rawPredict"
        );

        // Body should be Anthropic format (messages, system, max_tokens)
        let body = parse_body(&result);
        assert!(
            body.get("messages").is_some(),
            "expected Anthropic body format"
        );
        assert!(body.get("system").is_some());
        assert!(
            body.get("contents").is_none(),
            "should NOT have Gemini contents"
        );
        assert_eq!(body["max_tokens"], 1000);

        // Should have anthropic-version header
        assert_eq!(
            result.headers.get("anthropic-version").unwrap(),
            "vertex-2023-10-16"
        );
    }

    #[tokio::test]
    async fn test_vertex_anthropic_stream_uses_stream_raw_predict() {
        let client = make_client(
            "vertex-ai",
            vec![
                (
                    "model",
                    BexExternalValue::String("claude-3-5-sonnet@20241022".into()),
                ),
                ("location", BexExternalValue::String("us-east5".into())),
                ("project_id", BexExternalValue::String("my-project".into())),
                (
                    "anthropic_version",
                    BexExternalValue::String("vertex-2023-10-16".into()),
                ),
            ],
        );
        let prompt = msg("user", "Hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, true, &h, &e, &f).await
        }
        .unwrap();
        assert!(result.url.contains("streamRawPredict"));

        // Anthropic body should have stream: true
        let body = parse_body(&result);
        assert_eq!(body["stream"], true);
    }

    #[tokio::test]
    async fn test_vertex_claude_model_detected_without_anthropic_version() {
        let client = make_client(
            "vertex-ai",
            vec![
                ("model", BexExternalValue::String("claude-3-haiku".into())),
                ("location", BexExternalValue::String("us-east5".into())),
                ("project_id", BexExternalValue::String("my-project".into())),
            ],
        );
        let prompt = msg("user", "Hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        // Model name starts with "claude" so it should use rawPredict
        assert!(result.url.contains("rawPredict"));
    }

    // ========================================================================
    // Google AI tests
    // ========================================================================

    #[tokio::test]
    async fn test_google_ai_url() {
        let client = make_client(
            "google-ai",
            vec![("model", BexExternalValue::String("gemini-1.5-flash".into()))],
        );
        let prompt = msg("user", "Hello");
        let result = {
            let (h, e, f) = stub_callbacks();
            build_request(&client, prompt, false, &h, &e, &f).await
        }
        .unwrap();
        assert_eq!(
            result.url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-1.5-flash:generateContent"
        );
    }
}
