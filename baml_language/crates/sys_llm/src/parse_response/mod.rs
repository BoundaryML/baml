mod anthropic;
mod bedrock;
mod google;
mod openai;
mod openai_responses;
mod types;

pub(super) use types::LlmProviderResponse;

use crate::LlmProvider;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ParseResponseError {
    #[error("failed to deserialize {provider} response: {source}. Body:\n{content}")]
    Deserialize {
        provider: &'static str,
        #[source]
        source: serde_json::Error,
        content: String,
    },

    #[error("API error from {provider}: {message}. Code: {code:?}. Param: {param:?}")]
    ApiError {
        provider: &'static str,
        message: String,
        code: Option<String>,
        param: Option<String>,
    },

    #[error("{provider} response has no content: {detail}")]
    NoContent {
        provider: &'static str,
        detail: String,
    },

    #[error("{provider} response has unsupported shape: {detail}")]
    UnsupportedResponseFormat {
        provider: &'static str,
        detail: String,
    },

    #[error("provider {0} is not yet supported for response parsing")]
    UnsupportedProvider(String),
}

/// Parse a raw HTTP response body into a normalized `LlmProviderResponse`.
///
/// The `provider` determines which deserialization format to use.
pub(crate) fn parse_response(
    provider: LlmProvider,
    body: &str,
) -> Result<LlmProviderResponse, ParseResponseError> {
    match provider {
        LlmProvider::OpenAi
        | LlmProvider::OpenAiGeneric
        | LlmProvider::AzureOpenAi
        | LlmProvider::Ollama
        | LlmProvider::OpenRouter => openai::parse_openai_response(body),

        LlmProvider::OpenAiResponses => openai_responses::parse_openai_responses_response(body),

        LlmProvider::Anthropic => anthropic::parse_anthropic_response(body),

        LlmProvider::GoogleAi | LlmProvider::VertexAi => google::parse_gemini_response(body),

        LlmProvider::AwsBedrock => bedrock::parse_bedrock_response(body),

        LlmProvider::BamlFallback | LlmProvider::BamlRoundRobin => Err(
            ParseResponseError::UnsupportedProvider(format!("{provider:?}")),
        ),
    }
}

/// Resolve which provider's response format to use: if `client_response_type`
/// is set, parse it as an `LlmProvider` override; otherwise use the client's
/// own provider.
pub(crate) fn resolve_response_provider(
    provider: LlmProvider,
    client_response_type: Option<&str>,
) -> Result<LlmProvider, ParseResponseError> {
    match client_response_type {
        Some(override_str) => override_str
            .parse::<LlmProvider>()
            .map_err(|_| ParseResponseError::UnsupportedProvider(override_str.to_string())),
        None => Ok(provider),
    }
}
