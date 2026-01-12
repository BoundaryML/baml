//! Common orchestration utilities shared between call and stream.

use crate::errors::RuntimeError;
use crate::llm_request::openai::{OpenAiClientConfig, OpenAiRequest};
use baml_llm_interface::RenderedPrompt;

use super::{ClientConfig, OrchestratorNode, ProviderType};

/// Result of preparing a request for a single node.
pub struct PreparedNodeRequest {
    /// The rendered prompt.
    pub rendered_prompt: RenderedPrompt,
    /// The provider-specific request.
    pub request: OpenAiRequest,
}

/// Prepare request for a single node (steps 3a-3b.1).
///
/// This is the shared setup code used by both call and stream orchestrators.
pub fn prepare_node_request(
    prompt: &RenderedPrompt,
    node: &OrchestratorNode,
    env_vars: &std::collections::HashMap<String, String>,
    stream: bool,
) -> Result<PreparedNodeRequest, RuntimeError> {
    // Build client config from node
    let client_config = build_client_config(&node.client, env_vars)?;

    // Build provider request
    let request = match node.client.provider {
        ProviderType::OpenAi => {
            OpenAiRequest::from_rendered(prompt, &client_config, stream)?
        }
        ProviderType::Anthropic => {
            // TODO: Implement Anthropic provider
            return Err(RuntimeError::BuildRequest(
                crate::errors::BuildRequestError::UnsupportedProvider {
                    provider: "anthropic".to_string(),
                },
            ));
        }
    };

    Ok(PreparedNodeRequest {
        rendered_prompt: prompt.clone(),
        request,
    })
}

/// Build client config from node configuration.
fn build_client_config(
    client: &ClientConfig,
    env_vars: &std::collections::HashMap<String, String>,
) -> Result<OpenAiClientConfig, RuntimeError> {
    // Try to get API key from options or environment
    let api_key = client
        .options
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| env_vars.get("OPENAI_API_KEY").cloned())
        .unwrap_or_default();

    let base_url = client
        .options
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or("https://api.openai.com/v1")
        .to_string();

    let model = client
        .options
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("gpt-4")
        .to_string();

    let temperature = client
        .options
        .get("temperature")
        .and_then(|v| v.as_f64())
        .map(|t| t as f32);

    let max_tokens = client
        .options
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .map(|t| t as u32);

    Ok(OpenAiClientConfig {
        base_url,
        api_key,
        model,
        temperature,
        max_tokens,
        timeout: Some(std::time::Duration::from_secs(60)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::OrchestrationScope;

    #[test]
    fn test_build_client_config_from_options() {
        let client = ClientConfig {
            name: "test".to_string(),
            provider: ProviderType::OpenAi,
            options: serde_json::json!({
                "api_key": "sk-test",
                "model": "gpt-3.5-turbo",
                "temperature": 0.7
            }),
        };

        let config = build_client_config(&client, &std::collections::HashMap::new()).unwrap();

        assert_eq!(config.api_key, "sk-test");
        assert_eq!(config.model, "gpt-3.5-turbo");
        assert_eq!(config.temperature, Some(0.7));
    }

    #[test]
    fn test_build_client_config_from_env() {
        let client = ClientConfig {
            name: "test".to_string(),
            provider: ProviderType::OpenAi,
            options: serde_json::json!({}),
        };

        let mut env = std::collections::HashMap::new();
        env.insert("OPENAI_API_KEY".to_string(), "sk-env-key".to_string());

        let config = build_client_config(&client, &env).unwrap();

        assert_eq!(config.api_key, "sk-env-key");
    }

    #[test]
    fn test_prepare_node_request() {
        let prompt = RenderedPrompt::Completion { text: "Hello".to_string() };
        let node = OrchestratorNode {
            client: ClientConfig {
                name: "openai".to_string(),
                provider: ProviderType::OpenAi,
                options: serde_json::json!({
                    "api_key": "sk-test",
                    "model": "gpt-4"
                }),
            },
            scope: OrchestrationScope::Direct,
            delay: None,
        };

        let result = prepare_node_request(&prompt, &node, &std::collections::HashMap::new(), false);
        assert!(result.is_ok());

        let prepared = result.unwrap();
        assert!(prepared.request.url.contains("chat/completions"));
    }
}
