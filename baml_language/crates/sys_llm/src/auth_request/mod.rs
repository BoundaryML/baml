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

pub(crate) use bedrock::BedrockAuth;
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
