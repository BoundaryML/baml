//! OpenAI-format HTTP request builders.
//!
//! Supports: `OpenAi`, `OpenAiGeneric`, `AzureOpenAi`, Ollama, `OpenRouter`,
//! and `OpenAiResponses` (Responses API).

mod chat_completions;
mod responses;

pub(crate) use chat_completions::OpenAiBuilder;
pub(crate) use responses::OpenAiResponsesBuilder;

use super::{BuildRequestError, LlmPrimitiveClient, get_string_option};
use crate::LlmProvider;

/// Builds the full request URL for an OpenAI-compatible provider.
///
/// `path` must begin with a slash.
pub(super) fn build_openai_url(
    provider: LlmProvider,
    client: &LlmPrimitiveClient,
    path: &str,
) -> Result<String, BuildRequestError> {
    if let Some(base_url) = get_string_option(client, "base_url") {
        let base_url = base_url.trim_end_matches('/');
        return Ok(format!("{base_url}{path}"));
    }

    Ok(match provider {
        LlmProvider::OpenAi | LlmProvider::OpenAiResponses => {
            format!("https://api.openai.com/v1{path}")
        }
        LlmProvider::AzureOpenAi => {
            let deployment = get_string_option(client, "resource_name")
                .ok_or_else(|| BuildRequestError::MissingOption("resource_name".into()))?;
            let model = get_string_option(client, "model")
                .ok_or_else(|| BuildRequestError::MissingOption("model".into()))?;
            let api_version = get_string_option(client, "api_version")
                .ok_or_else(|| BuildRequestError::MissingOption("api_version".into()))?;
            let url = format!(
                "https://{deployment}.openai.azure.com/openai/deployments/{model}{path}?api-version={api_version}"
            );

            url
        }
        LlmProvider::OpenRouter => format!("https://openrouter.ai/api/v1{path}"),
        LlmProvider::Ollama => format!("http://localhost:11434/v1{path}"),
        _ => {
            return Err(BuildRequestError::MissingOption("base_url".into()));
        }
    })
}
