//! LlmProvider-specific HTTP request building.
//!
//! Converts a `PrimitiveClient` + `PromptAst` into a `baml.http.Request` instance.

mod anthropic;
mod openai;

use std::str::FromStr;

use bex_external_types::{BexExternalValue, PrimitiveClientValue, PromptAst, Ty};
use indexmap::indexmap;

use crate::LlmProvider;

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

/// Trait for building provider-specific HTTP requests.
///
/// Default methods handle shared logic (body assembly, option forwarding, header
/// merging). Each provider implements only the parts that differ.
pub(crate) trait LlmRequestBuilder {
    /// LlmProvider-specific option keys to skip (in addition to the shared skip-key lists).
    fn provider_skip_keys(&self) -> &'static [&'static str];

    /// Build the request URL.
    fn build_url(&self, client: &PrimitiveClientValue) -> Result<String, BuildRequestError>;

    /// Build auth + provider-specific headers (without content-type or custom headers).
    fn build_auth_headers(
        &self,
        client: &PrimitiveClientValue,
    ) -> indexmap::IndexMap<String, String>;

    /// Convert a specialized prompt into the JSON body fields specific to this provider.
    fn build_prompt_body(&self, prompt: PromptAst) -> serde_json::Map<String, serde_json::Value>;

    // --- Default methods (shared logic) ---

    /// Build the full request. Default: POST with url/headers/body from trait methods.
    fn build_request(
        &self,
        client: &PrimitiveClientValue,
        prompt: PromptAst,
    ) -> Result<RawHttpRequest, BuildRequestError> {
        let url = self.build_url(client)?;
        let headers = self.build_headers(client);
        let body = self.build_body(client, prompt)?;
        Ok(RawHttpRequest {
            method: "POST".to_string(),
            url,
            headers,
            body,
        })
    }

    /// Build headers: auth headers + content-type + custom headers from options.
    fn build_headers(&self, client: &PrimitiveClientValue) -> indexmap::IndexMap<String, String> {
        let mut headers = indexmap::IndexMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        headers.extend(self.build_auth_headers(client));
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

    /// Build JSON body: model + prompt fields + forwarded options.
    fn build_body(
        &self,
        client: &PrimitiveClientValue,
        prompt: PromptAst,
    ) -> Result<String, BuildRequestError> {
        let mut body = serde_json::Map::new();
        if let Some(model) = get_string_option(client, "model") {
            body.insert("model".to_string(), serde_json::Value::String(model));
        }
        body.extend(self.build_prompt_body(prompt));
        self.forward_options(client, &mut body);
        serde_json::to_string(&body).map_err(|e| BuildRequestError::InvalidOption {
            key: "body".into(),
            reason: e.to_string(),
        })
    }

    /// Forward non-skipped options to body.
    fn forward_options(
        &self,
        client: &PrimitiveClientValue,
        body: &mut serde_json::Map<String, serde_json::Value>,
    ) {
        let provider_keys = self.provider_skip_keys();
        for (key, value) in &client.options {
            if SPECIALIZE_PROMPT_SKIP_KEYS.contains(&key.as_str())
                || BUILD_REQUEST_SKIP_KEYS.contains(&key.as_str())
                || provider_keys.contains(&key.as_str())
            {
                continue;
            }
            if let Some(json_val) = bex_value_to_json(value) {
                body.insert(key.clone(), json_val);
            }
        }
    }
}

/// Build a provider-specific HTTP request from a specialized prompt.
///
/// Returns a `BexExternalValue::Instance` matching the `baml.http.Request` class:
/// `{ method: String, url: String, headers: Map<String, String>, body: String }`
pub(crate) fn build_request(
    client: &PrimitiveClientValue,
    prompt: PromptAst,
) -> Result<BexExternalValue, BuildRequestError> {
    let provider = LlmProvider::from_str(&client.provider)
        .map_err(|_| BuildRequestError::UnsupportedLlmProvider(client.provider.clone()))?;

    let raw = match provider {
        LlmProvider::OpenAi
        | LlmProvider::OpenAiGeneric
        | LlmProvider::AzureOpenAi
        | LlmProvider::Ollama
        | LlmProvider::OpenRouter
        | LlmProvider::OpenAiResponses => {
            openai::OpenAiBuilder::new(&provider).build_request(client, prompt)?
        }
        LlmProvider::Anthropic => anthropic::AnthropicBuilder.build_request(client, prompt)?,
        LlmProvider::GoogleAi
        | LlmProvider::VertexAi
        | LlmProvider::AwsBedrock
        | LlmProvider::BamlFallback
        | LlmProvider::BamlRoundRobin => {
            return Err(BuildRequestError::UnsupportedLlmProvider(
                client.provider.clone(),
            ));
        }
    };

    // Convert RawHttpRequest to BexExternalValue::Instance matching baml.http.Request
    Ok(raw.into_instance())
}

/// Intermediate struct before converting to `BexExternalValue::Instance`.
pub(crate) struct RawHttpRequest {
    pub method: String,
    pub url: String,
    pub headers: indexmap::IndexMap<String, String>,
    pub body: String,
}

impl RawHttpRequest {
    /// Convert to `BexExternalValue::Instance` matching `baml.http.Request`.
    ///
    /// Field order must match the builtin struct definition:
    /// `method`, `url`, `headers`, `body`.
    fn into_instance(self) -> BexExternalValue {
        BexExternalValue::Instance {
            class_name: "baml.http.Request".to_string(),
            fields: indexmap! {
                "method".to_string() => BexExternalValue::String(self.method),
                "url".to_string() => BexExternalValue::String(self.url),
                "headers".to_string() => BexExternalValue::Map {
                    key_type: Ty::String,
                    value_type: Ty::String,
                    entries: self.headers.into_iter()
                        .map(|(k, v)| (k, BexExternalValue::String(v)))
                        .collect(),
                },
                "body".to_string() => BexExternalValue::String(self.body),
            },
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
}

/// Helper to extract a string option from client.options.
pub(crate) fn get_string_option(client: &PrimitiveClientValue, key: &str) -> Option<String> {
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

/// Convert `PromptAst` content to JSON content parts.
///
/// Used by both `OpenAI` and Anthropic builders.
pub(crate) fn prompt_to_content_parts(content: PromptAst) -> Vec<serde_json::Value> {
    match content {
        PromptAst::String(s) => {
            vec![serde_json::json!({"type": "text", "text": s})]
        }
        PromptAst::Vec(items) => items
            .into_iter()
            .flat_map(prompt_to_content_parts)
            .collect(),
        PromptAst::Media(_handle) => {
            // Media resolution deferred — emit placeholder
            vec![
                serde_json::json!({"type": "text", "text": "[media placeholder - resolution deferred]"}),
            ]
        }
        PromptAst::Message { .. } => {
            // Nested messages shouldn't appear in content parts
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::*;

    fn make_client(provider: &str, options: Vec<(&str, BexExternalValue)>) -> PrimitiveClientValue {
        let mut opts = IndexMap::new();
        for (k, v) in options {
            opts.insert(k.to_string(), v);
        }
        PrimitiveClientValue {
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

    fn msg(role: &str, text: &str) -> PromptAst {
        PromptAst::Message {
            role: role.to_string(),
            content: Box::new(PromptAst::String(text.to_string())),
            metadata: Box::new(BexExternalValue::Null),
        }
    }

    /// Parse the body field out of a Request instance.
    fn parse_body(instance: &BexExternalValue) -> serde_json::Value {
        let BexExternalValue::Instance { fields, .. } = instance else {
            panic!("expected Instance");
        };
        let BexExternalValue::String(body) = &fields["body"] else {
            panic!("expected String body");
        };
        serde_json::from_str(body).unwrap()
    }

    fn get_field_str<'a>(instance: &'a BexExternalValue, field: &str) -> &'a str {
        let BexExternalValue::Instance { fields, .. } = instance else {
            panic!("expected Instance");
        };
        let BexExternalValue::String(s) = &fields[field] else {
            panic!("expected String for {field}");
        };
        s.as_str()
    }

    fn get_header<'a>(instance: &'a BexExternalValue, header: &str) -> Option<&'a str> {
        let BexExternalValue::Instance { fields, .. } = instance else {
            panic!("expected Instance");
        };
        let BexExternalValue::Map { entries, .. } = &fields["headers"] else {
            panic!("expected Map for headers");
        };
        match entries.get(header) {
            Some(BexExternalValue::String(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    // ---- Result shape tests ----

    #[test]
    fn test_instance_class_name() {
        let client = make_client(
            "openai",
            vec![("model", BexExternalValue::String("gpt-4o".into()))],
        );
        let prompt = msg("user", "hello");
        let result = build_request(&client, prompt).unwrap();
        let BexExternalValue::Instance { class_name, .. } = &result else {
            panic!("expected Instance");
        };
        assert_eq!(class_name, "baml.http.Request");
    }

    #[test]
    fn test_unsupported_provider() {
        let client = make_client("unknown-provider", vec![]);
        let prompt = msg("user", "hello");
        let result = build_request(&client, prompt);
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
    #[test]
    fn test_openai_gpt4o_system_only() {
        let client = make_client(
            "openai",
            vec![
                ("model", BexExternalValue::String("gpt-4o".into())),
                ("api_key", BexExternalValue::String("sk-test-key".into())),
            ],
        );

        let system_text = "Given the receipt below:\n\n```\ntest@email.com\n```\n\nAnswer in JSON using this schema:\n{\n  items: [\n    {\n      name: string,\n      description: string or null,\n      quantity: int,\n      price: float,\n    }\n  ],\n  total_cost: float or null,\n  venue: \"barisa\" or \"ox_burger\",\n}";
        let prompt = PromptAst::Vec(vec![msg("system", system_text)]);

        let result = build_request(&client, prompt).unwrap();

        // Verify envelope
        assert_eq!(get_field_str(&result, "method"), "POST");
        assert_eq!(
            get_field_str(&result, "url"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            get_header(&result, "authorization").unwrap(),
            "Bearer sk-test-key"
        );
        assert_eq!(
            get_header(&result, "content-type").unwrap(),
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
    #[test]
    fn test_openai_gpt4_turbo_system_and_user() {
        let client = make_client(
            "openai",
            vec![
                ("model", BexExternalValue::String("gpt-4-turbo".into())),
                ("api_key", BexExternalValue::String("sk-test-key".into())),
            ],
        );

        let prompt = PromptAst::Vec(vec![
            msg("system", "You are a helpful assistant."),
            msg("user", "Write a nice short story about Dr. Pepper"),
        ]);

        let result = build_request(&client, prompt).unwrap();

        assert_eq!(
            get_field_str(&result, "url"),
            "https://api.openai.com/v1/chat/completions"
        );

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

    #[test]
    fn test_openai_content_always_array() {
        let client = make_client(
            "openai",
            vec![("model", BexExternalValue::String("gpt-4o".into()))],
        );
        let prompt = msg("user", "Hello world");
        let result = build_request(&client, prompt).unwrap();
        let body = parse_body(&result);
        assert!(body["messages"][0]["content"].is_array());
        assert_eq!(body["messages"][0]["content"][0]["type"], "text");
        assert_eq!(body["messages"][0]["content"][0]["text"], "Hello world");
    }

    #[test]
    fn test_openai_custom_base_url() {
        let client = make_client(
            "openai",
            vec![(
                "base_url",
                BexExternalValue::String("https://custom.api.com".into()),
            )],
        );
        let prompt = msg("user", "hello");
        let result = build_request(&client, prompt).unwrap();
        assert_eq!(
            get_field_str(&result, "url"),
            "https://custom.api.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_openai_forwards_options_to_body() {
        let client = make_client(
            "openai",
            vec![
                ("model", BexExternalValue::String("gpt-4o".into())),
                ("temperature", BexExternalValue::Float(0.7)),
            ],
        );
        let prompt = msg("user", "hello");
        let result = build_request(&client, prompt).unwrap();
        let body = parse_body(&result);
        assert_eq!(body["temperature"], 0.7);
    }

    #[test]
    fn test_openai_skips_internal_options_in_body() {
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
        let result = build_request(&client, prompt).unwrap();
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
    #[test]
    fn test_anthropic_claude_system_extracted() {
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

        let prompt = PromptAst::Vec(vec![
            msg("system", "You are a helpful assistant."),
            msg("user", "Write a nice short story about Dr. Pepper"),
        ]);

        let result = build_request(&client, prompt).unwrap();

        // Verify envelope
        assert_eq!(get_field_str(&result, "method"), "POST");
        assert_eq!(
            get_field_str(&result, "url"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(get_header(&result, "x-api-key").unwrap(), "sk-ant-test");
        assert_eq!(
            get_header(&result, "content-type").unwrap(),
            "application/json"
        );
        assert!(get_header(&result, "anthropic-version").is_some());

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

    #[test]
    fn test_anthropic_no_system_message() {
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
        let result = build_request(&client, prompt).unwrap();
        let body = parse_body(&result);
        assert!(body.get("system").is_none());
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_anthropic_custom_headers() {
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
                        element_type: Ty::String,
                        items: vec![BexExternalValue::String("cache_control".into())],
                    },
                ),
                (
                    "headers",
                    BexExternalValue::Map {
                        key_type: Ty::String,
                        value_type: Ty::String,
                        entries: header_entries,
                    },
                ),
            ],
        );

        let prompt = msg("user", "hello");
        let result = build_request(&client, prompt).unwrap();

        assert_eq!(
            get_header(&result, "anthropic-beta").unwrap(),
            "prompt-caching-2024-07-31"
        );

        let body = parse_body(&result);
        assert!(body.get("allowed_role_metadata").is_none());
        assert!(body.get("headers").is_none());
    }

    #[test]
    fn test_anthropic_custom_version() {
        let client = make_client(
            "anthropic",
            vec![(
                "anthropic_version",
                BexExternalValue::String("2024-01-01".into()),
            )],
        );
        let prompt = msg("user", "hello");
        let result = build_request(&client, prompt).unwrap();
        assert_eq!(
            get_header(&result, "anthropic-version").unwrap(),
            "2024-01-01"
        );
    }

    #[test]
    fn test_anthropic_default_version() {
        let client = make_client("anthropic", vec![]);
        let prompt = msg("user", "hello");
        let result = build_request(&client, prompt).unwrap();
        assert_eq!(
            get_header(&result, "anthropic-version").unwrap(),
            "2023-06-01"
        );
    }

    #[test]
    fn test_anthropic_forwards_max_tokens() {
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
        let result = build_request(&client, prompt).unwrap();
        let body = parse_body(&result);
        assert_eq!(body["max_tokens"], 1000);
    }
}
