//! Provider-specific authentication for LLM HTTP requests.
//!
//! Each provider implements [`LlmRequestAuthorizer`] to authorize a built
//! HTTP request by adding auth headers, signatures, etc.
//!
//! Today this is called as a step after request building inside the
//! [`crate::build_request::build_request`] dispatch function. Eventually it
//! can be promoted to a standalone operation in the BAML LLM function pipeline
//! so that auth can be resolved, cached, or refreshed independently of
//! request building (e.g. short-lived STS tokens, OAuth token refresh, or
//! vault-based secret retrieval).

mod bedrock;

pub(crate) use bedrock::{BedrockAuth, load_aws_sdk_config};
use bex_heap::builtin_types::owned::LlmPrimitiveClient;

use crate::build_request::{
    BuildRequestCallbacks, BuildRequestError, RawHttpRequest, get_string_option,
};

/// Trait for provider-specific request authorization.
///
/// Takes a fully-built [`RawHttpRequest`] and returns the same request with
/// auth headers (API keys, bearer tokens, `SigV4` signatures, etc.) applied.
pub(crate) trait LlmRequestAuthorizer {
    /// Authorize the given request.
    ///
    /// Implementations may be async (e.g. Bedrock credential resolution via
    /// the AWS provider chain, or future OAuth token refresh).
    async fn authorize(
        &self,
        request: RawHttpRequest,
        client: &LlmPrimitiveClient,
        callbacks: &BuildRequestCallbacks<'_>,
    ) -> Result<RawHttpRequest, BuildRequestError>;
}

// ---------------------------------------------------------------------------
// Anthropic
// ---------------------------------------------------------------------------

/// Default Anthropic API version.
const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";

pub(crate) struct AnthropicAuth;

impl LlmRequestAuthorizer for AnthropicAuth {
    async fn authorize(
        &self,
        mut request: RawHttpRequest,
        client: &LlmPrimitiveClient,
        _callbacks: &BuildRequestCallbacks<'_>,
    ) -> Result<RawHttpRequest, BuildRequestError> {
        if let Some(api_key) = get_string_option(client, "api_key") {
            request.headers.insert("x-api-key".to_string(), api_key);
        }
        let version = get_string_option(client, "anthropic_version")
            .unwrap_or_else(|| DEFAULT_ANTHROPIC_VERSION.to_string());
        request
            .headers
            .insert("anthropic-version".to_string(), version);
        Ok(request)
    }
}

// ---------------------------------------------------------------------------
// OpenAI (Chat Completions + Responses, including Azure)
// ---------------------------------------------------------------------------

use crate::LlmProvider;

pub(crate) struct OpenAiAuth<'a> {
    pub provider: &'a LlmProvider,
}

impl LlmRequestAuthorizer for OpenAiAuth<'_> {
    async fn authorize(
        &self,
        mut request: RawHttpRequest,
        client: &LlmPrimitiveClient,
        _callbacks: &BuildRequestCallbacks<'_>,
    ) -> Result<RawHttpRequest, BuildRequestError> {
        if let Some(api_key) = get_string_option(client, "api_key") {
            if *self.provider == LlmProvider::AzureOpenAi {
                request.headers.insert("api-key".to_string(), api_key);
            } else {
                request
                    .headers
                    .insert("authorization".to_string(), format!("Bearer {api_key}"));
            }
        }
        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    use bex_external_types::BexExternalValue;
    use indexmap::IndexMap;

    use super::*;

    fn make_client(options: Vec<(&str, BexExternalValue)>) -> LlmPrimitiveClient {
        let mut opts = IndexMap::new();
        for (k, v) in options {
            opts.insert(k.to_string(), v);
        }
        LlmPrimitiveClient {
            name: "test".to_string(),
            provider: "test".to_string(),
            default_role: "user".to_string(),
            allowed_roles: vec!["user".to_string()],
            options: opts,
        }
    }

    fn fake_request() -> RawHttpRequest {
        RawHttpRequest {
            method: "POST".to_string(),
            url: "https://example.com/v1/chat/completions".to_string(),
            headers: IndexMap::new(),
            body: r#"{"messages":[]}"#.to_string(),
        }
    }

    fn stub_callbacks() -> (crate::HttpSendFn, crate::EnvReadFn, crate::FsReadFn) {
        crate::build_request::stub_callbacks()
    }

    // -----------------------------------------------------------------------
    // Anthropic
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn anthropic_sets_api_key_header() {
        let client = make_client(vec![(
            "api_key",
            BexExternalValue::String("sk-ant-test".into()),
        )]);
        let (h, e, f) = stub_callbacks();
        let cb = BuildRequestCallbacks {
            http_send: &h,
            env_read: &e,
            fs_read: &f,
        };
        let result = AnthropicAuth
            .authorize(fake_request(), &client, &cb)
            .await
            .unwrap();
        assert_eq!(result.headers.get("x-api-key").unwrap(), "sk-ant-test");
    }

    #[tokio::test]
    async fn anthropic_default_version() {
        let client = make_client(vec![]);
        let (h, e, f) = stub_callbacks();
        let cb = BuildRequestCallbacks {
            http_send: &h,
            env_read: &e,
            fs_read: &f,
        };
        let result = AnthropicAuth
            .authorize(fake_request(), &client, &cb)
            .await
            .unwrap();
        assert_eq!(
            result.headers.get("anthropic-version").unwrap(),
            DEFAULT_ANTHROPIC_VERSION,
        );
    }

    #[tokio::test]
    async fn anthropic_custom_version() {
        let client = make_client(vec![(
            "anthropic_version",
            BexExternalValue::String("2024-01-01".into()),
        )]);
        let (h, e, f) = stub_callbacks();
        let cb = BuildRequestCallbacks {
            http_send: &h,
            env_read: &e,
            fs_read: &f,
        };
        let result = AnthropicAuth
            .authorize(fake_request(), &client, &cb)
            .await
            .unwrap();
        assert_eq!(
            result.headers.get("anthropic-version").unwrap(),
            "2024-01-01"
        );
    }

    #[tokio::test]
    async fn anthropic_no_api_key_omits_header() {
        let client = make_client(vec![]);
        let (h, e, f) = stub_callbacks();
        let cb = BuildRequestCallbacks {
            http_send: &h,
            env_read: &e,
            fs_read: &f,
        };
        let result = AnthropicAuth
            .authorize(fake_request(), &client, &cb)
            .await
            .unwrap();
        assert!(!result.headers.contains_key("x-api-key"));
    }

    // -----------------------------------------------------------------------
    // OpenAI
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn openai_sets_bearer_token() {
        let client = make_client(vec![(
            "api_key",
            BexExternalValue::String("sk-test-key".into()),
        )]);
        let (h, e, f) = stub_callbacks();
        let cb = BuildRequestCallbacks {
            http_send: &h,
            env_read: &e,
            fs_read: &f,
        };
        let auth = OpenAiAuth {
            provider: &LlmProvider::OpenAi,
        };
        let result = auth.authorize(fake_request(), &client, &cb).await.unwrap();
        assert_eq!(
            result.headers.get("authorization").unwrap(),
            "Bearer sk-test-key",
        );
    }

    #[tokio::test]
    async fn openai_no_api_key_omits_header() {
        let client = make_client(vec![]);
        let (h, e, f) = stub_callbacks();
        let cb = BuildRequestCallbacks {
            http_send: &h,
            env_read: &e,
            fs_read: &f,
        };
        let auth = OpenAiAuth {
            provider: &LlmProvider::OpenAi,
        };
        let result = auth.authorize(fake_request(), &client, &cb).await.unwrap();
        assert!(!result.headers.contains_key("authorization"));
    }

    #[tokio::test]
    async fn azure_uses_api_key_header() {
        let client = make_client(vec![("api_key", BexExternalValue::String("az-key".into()))]);
        let (h, e, f) = stub_callbacks();
        let cb = BuildRequestCallbacks {
            http_send: &h,
            env_read: &e,
            fs_read: &f,
        };
        let auth = OpenAiAuth {
            provider: &LlmProvider::AzureOpenAi,
        };
        let result = auth.authorize(fake_request(), &client, &cb).await.unwrap();
        assert_eq!(result.headers.get("api-key").unwrap(), "az-key");
        assert!(!result.headers.contains_key("authorization"));
    }

    #[tokio::test]
    async fn openai_generic_uses_bearer() {
        let client = make_client(vec![("api_key", BexExternalValue::String("sk-gen".into()))]);
        let (h, e, f) = stub_callbacks();
        let cb = BuildRequestCallbacks {
            http_send: &h,
            env_read: &e,
            fs_read: &f,
        };
        let auth = OpenAiAuth {
            provider: &LlmProvider::OpenAiGeneric,
        };
        let result = auth.authorize(fake_request(), &client, &cb).await.unwrap();
        assert_eq!(
            result.headers.get("authorization").unwrap(),
            "Bearer sk-gen"
        );
    }

    #[tokio::test]
    async fn openrouter_uses_bearer() {
        let client = make_client(vec![("api_key", BexExternalValue::String("sk-or".into()))]);
        let (h, e, f) = stub_callbacks();
        let cb = BuildRequestCallbacks {
            http_send: &h,
            env_read: &e,
            fs_read: &f,
        };
        let auth = OpenAiAuth {
            provider: &LlmProvider::OpenRouter,
        };
        let result = auth.authorize(fake_request(), &client, &cb).await.unwrap();
        assert_eq!(result.headers.get("authorization").unwrap(), "Bearer sk-or");
    }

    #[tokio::test]
    async fn ollama_uses_bearer() {
        let client = make_client(vec![(
            "api_key",
            BexExternalValue::String("ollama-key".into()),
        )]);
        let (h, e, f) = stub_callbacks();
        let cb = BuildRequestCallbacks {
            http_send: &h,
            env_read: &e,
            fs_read: &f,
        };
        let auth = OpenAiAuth {
            provider: &LlmProvider::Ollama,
        };
        let result = auth.authorize(fake_request(), &client, &cb).await.unwrap();
        assert_eq!(
            result.headers.get("authorization").unwrap(),
            "Bearer ollama-key",
        );
    }

    #[tokio::test]
    async fn authorize_preserves_existing_headers() {
        let client = make_client(vec![(
            "api_key",
            BexExternalValue::String("sk-test".into()),
        )]);
        let (h, e, f) = stub_callbacks();
        let cb = BuildRequestCallbacks {
            http_send: &h,
            env_read: &e,
            fs_read: &f,
        };
        let mut req = fake_request();
        req.headers
            .insert("content-type".to_string(), "application/json".to_string());
        req.headers
            .insert("x-custom".to_string(), "value".to_string());
        let auth = OpenAiAuth {
            provider: &LlmProvider::OpenAi,
        };
        let result = auth.authorize(req, &client, &cb).await.unwrap();
        assert_eq!(
            result.headers.get("content-type").unwrap(),
            "application/json"
        );
        assert_eq!(result.headers.get("x-custom").unwrap(), "value");
        assert_eq!(
            result.headers.get("authorization").unwrap(),
            "Bearer sk-test"
        );
    }
}
