mod anthropic;
#[allow(unreachable_pub)]
mod anthropic_types;
mod openai;
#[allow(unreachable_pub)]
mod openai_types;
mod types;

pub use types::{FinishReason, LlmProviderResponse, TokenUsage};

use crate::LlmProvider;

#[derive(Debug, thiserror::Error)]
pub enum ParseResponseError {
    #[error("failed to deserialize {provider} response: {source}")]
    Deserialize {
        provider: &'static str,
        #[source]
        source: serde_json::Error,
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
pub fn parse_response(
    provider: LlmProvider,
    body: &str,
) -> Result<LlmProviderResponse, ParseResponseError> {
    match provider {
        LlmProvider::OpenAi
        | LlmProvider::OpenAiGeneric
        | LlmProvider::AzureOpenAi
        | LlmProvider::Ollama
        | LlmProvider::OpenRouter => openai::parse_openai_response(body),

        LlmProvider::Anthropic | LlmProvider::AwsBedrock => {
            anthropic::parse_anthropic_response(body)
        }

        LlmProvider::OpenAiResponses => Err(ParseResponseError::UnsupportedProvider(
            "openai-responses".into(),
        )),
        LlmProvider::GoogleAi | LlmProvider::VertexAi => Err(
            ParseResponseError::UnsupportedProvider(format!("{provider:?}")),
        ),
        LlmProvider::BamlFallback | LlmProvider::BamlRoundRobin => Err(
            ParseResponseError::UnsupportedProvider(format!("{provider:?}")),
        ),
    }
}
