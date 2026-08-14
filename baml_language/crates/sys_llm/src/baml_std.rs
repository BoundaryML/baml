use std::str::FromStr;

use baml_base::{ClientOptionsPresence, ClientOptionsValidationError};

use crate::LlmProvider;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("client '{client}': missing required option '{option}'")]
    MissingOption { client: String, option: String },
    #[error("client '{client}': unknown provider '{provider}'")]
    UnknownProvider { client: String, provider: String },
    #[error("client '{client}': {error}")]
    InvalidOptions {
        client: String,
        #[source]
        error: ClientOptionsValidationError,
    },
}

#[derive(Debug)]
pub struct PrimitiveClient {
    pub name: String,
    pub provider: String,
    /// Resolved model name (falls back to empty string).
    pub model: String,
    /// Resolved default role (falls back to the first allowed role, or "user").
    pub default_role: String,
    /// Resolved allowed roles (falls back to \["user", "assistant", "system"\]).
    pub allowed_roles: Vec<String>,
    /// Forward options from `request_body`, pre-converted to JSON.
    pub(crate) extra_body: serde_json::Map<String, serde_json::Value>,
    /// Resolved provider-specific options (converted from `BexExternalValue`).
    pub(crate) provider_options: Option<ProviderOptions>,
    pub(crate) options: PrimitiveClientOptions,
}

impl PrimitiveClient {
    pub fn new(
        name: String,
        provider: String,
        mut options: PrimitiveClientOptions,
    ) -> Result<Self, ClientError> {
        let llm_provider =
            LlmProvider::from_str(&provider).map_err(|_| ClientError::UnknownProvider {
                client: name.clone(),
                provider: provider.clone(),
            })?;

        // Provider options must be parsed before defaults are applied: a
        // google-ai client with `enterprise = true` uses the Vertex backend,
        // so its unset fields need Vertex defaults rather than Google AI
        // Studio defaults.
        let provider_options = resolve_provider_options(&options.provider_options);
        let defaults_provider = match (&provider_options, llm_provider) {
            (Some(ProviderOptions::GoogleAi(options)), LlmProvider::GoogleAi)
                if options.enterprise == Some(true) =>
            {
                LlmProvider::VertexAi
            }
            _ => llm_provider,
        };
        apply_provider_defaults(defaults_provider, &mut options);

        let (resource_name, deployment_id) = match &provider_options {
            Some(ProviderOptions::AzureOpenAi(options)) => (
                options.resource_name.is_some(),
                options.deployment_id.is_some(),
            ),
            _ => (false, false),
        };
        baml_base::validate_client_options(ClientOptionsPresence {
            provider: &provider,
            base_url: options.base_url.is_some(),
            resource_name,
            deployment_id,
        })
        .map_err(|error| ClientError::InvalidOptions {
            client: name.clone(),
            error,
        })?;

        let model = options
            .model
            .clone()
            .filter(|m| !m.trim().is_empty())
            .ok_or_else(|| ClientError::MissingOption {
                client: name.clone(),
                option: "model".to_string(),
            })?;
        let allowed_roles = options.allowed_roles.clone().unwrap_or_else(|| {
            vec![
                "user".to_string(),
                "assistant".to_string(),
                "system".to_string(),
            ]
        });
        let default_role = options.default_role.clone().unwrap_or_else(|| {
            allowed_roles
                .first()
                .cloned()
                .unwrap_or_else(|| "user".to_string())
        });
        let extra_body = {
            let mut map = serde_json::Map::new();
            for (key, value) in &options.request_body {
                if let Some(json_val) = crate::build_request::bex_value_to_json(value) {
                    map.insert(key.clone(), json_val);
                }
            }
            map
        };
        Ok(Self {
            name,
            provider,
            model,
            default_role,
            allowed_roles,
            extra_body,
            provider_options,
            options,
        })
    }

    pub fn is_finish_reason_allowed(&self, finish_reason: Option<&str>) -> bool {
        let Some(finish_reason) = finish_reason else {
            return true;
        };
        let contains_ci = |list: &[String]| {
            list.iter()
                .any(|item| item.eq_ignore_ascii_case(finish_reason))
        };
        match (
            &self.options.finish_reason_allow_list,
            &self.options.finish_reason_deny_list,
        ) {
            (Some(allow), None) => contains_ci(allow),
            (None, Some(deny)) => !contains_ci(deny),
            (Some(allow), Some(deny)) => contains_ci(allow) && !contains_ci(deny),
            (None, None) => true,
        }
    }
}

// Provider option structs are generated from llm_types.baml via sys_types.
pub use sys_types::generated::owned::prompt::{
    AnthropicOptions, AzureOpenAiOptions, BedrockOptions, GoogleAiOptions, VertexAiOptions,
};

/// Provider-specific options, matching the BAML schema union
/// `AnthropicOptions | AzureOpenAiOptions | BedrockOptions | GoogleAiOptions |
/// VertexAiOptions | null`.
#[derive(Clone, Debug)]
pub enum ProviderOptions {
    Anthropic(AnthropicOptions),
    AzureOpenAi(AzureOpenAiOptions),
    Bedrock(BedrockOptions),
    GoogleAi(GoogleAiOptions),
    VertexAi(VertexAiOptions),
}

impl ProviderOptions {
    /// View either Google provider options or native Vertex options as the
    /// common Vertex configuration used after backend selection.
    pub(crate) fn vertex_ai(&self) -> Option<VertexAiOptions> {
        match self {
            Self::GoogleAi(options) => Some(VertexAiOptions {
                credentials: options.credentials.clone(),
                credentials_content: options.credentials_content.clone(),
                location: options.location.clone(),
                project_id: options.project_id.clone(),
            }),
            Self::VertexAi(options) => Some(options.clone()),
            _ => None,
        }
    }
}

/// Convert a `BexExternalValue` (from the VM) to a typed `ProviderOptions`.
pub fn resolve_provider_options(val: &bex_heap::BexExternalValue) -> Option<ProviderOptions> {
    use bex_heap::BexExternalValue;
    let BexExternalValue::Instance { class_name, .. } = val else {
        return None;
    };
    match class_name.as_str() {
        "baml.prompt.AnthropicOptions" => AnthropicOptions::from_external(val.clone())
            .ok()
            .map(ProviderOptions::Anthropic),
        "baml.prompt.AzureOpenAiOptions" => AzureOpenAiOptions::from_external(val.clone())
            .ok()
            .map(ProviderOptions::AzureOpenAi),
        "baml.prompt.BedrockOptions" => BedrockOptions::from_external(val.clone())
            .ok()
            .map(ProviderOptions::Bedrock),
        "baml.prompt.GoogleAiOptions" => GoogleAiOptions::from_external(val.clone())
            .ok()
            .map(ProviderOptions::GoogleAi),
        "baml.prompt.VertexAiOptions" => VertexAiOptions::from_external(val.clone())
            .ok()
            .map(ProviderOptions::VertexAi),
        other => unreachable!(
            "unknown provider options class {other:?}: add it to resolve_provider_options"
        ),
    }
}

/// Generated from `llm_types.baml`. Fields come from the BAML class definition.
pub use sys_types::generated::owned::prompt::PrimitiveClientOptions;

/// Apply provider-specific defaults to options for any fields the user didn't set.
///
/// This replaces the old compile-time defaults that were baked into `lower_cst.rs`.
/// Applying defaults at runtime lets us handle cases like Vertex AI + Anthropic
/// models correctly (where the provider is vertex-ai but the model behavior is
/// anthropic).
fn apply_provider_defaults(provider: LlmProvider, options: &mut PrimitiveClientOptions) {
    if options.base_url.is_none() {
        options.base_url = match provider {
            LlmProvider::Anthropic => Some("https://api.anthropic.com".into()),
            LlmProvider::OpenAi | LlmProvider::OpenAiGeneric | LlmProvider::OpenAiResponses => {
                Some("https://api.openai.com/v1".into())
            }
            LlmProvider::AiGatewayImages => Some("https://ai-gateway.vercel.sh/v4/ai".into()),
            LlmProvider::Ollama => Some("http://localhost:11434".into()),
            LlmProvider::OpenRouter => Some("https://openrouter.ai/api/v1".into()),
            // GoogleAi and VertexAi select their provider-specific default URL
            // at request time, after any Google AI -> Vertex routing has been
            // applied. AwsBedrock and AzureOpenAi also construct their URLs at
            // request time from provider-specific fields.
            LlmProvider::GoogleAi
            | LlmProvider::VertexAi
            | LlmProvider::AwsBedrock
            | LlmProvider::AzureOpenAi => None,
            _ => None,
        };
    }

    if options.allowed_roles.is_none() {
        options.allowed_roles = Some(match provider {
            LlmProvider::Ollama => vec!["user".into(), "assistant".into()],
            _ => vec!["system".into(), "user".into(), "assistant".into()],
        });
    }

    if options.default_role.is_none() {
        let preferred: &str = match provider {
            LlmProvider::OpenAi
            | LlmProvider::OpenAiGeneric
            | LlmProvider::OpenAiResponses
            | LlmProvider::AiGatewayImages
            | LlmProvider::OpenRouter
            | LlmProvider::AzureOpenAi => "system",
            _ => "user",
        };
        // Clamp to the first allowed role if the provider default is not in the set.
        options.default_role = Some(
            if options
                .allowed_roles
                .as_ref()
                .is_some_and(|roles| roles.iter().any(|r| r == preferred))
            {
                preferred.into()
            } else {
                options
                    .allowed_roles
                    .as_ref()
                    .and_then(|r| r.first().cloned())
                    .unwrap_or_else(|| preferred.into())
            },
        );
    }

    if options.remap_roles.is_none() {
        let is_anthropic_model = options
            .model
            .as_deref()
            .is_some_and(|m| m.starts_with("claude"));
        options.remap_roles = match provider {
            LlmProvider::GoogleAi => Some(indexmap::indexmap! {
                "assistant".into() => "model".into(),
            }),
            LlmProvider::VertexAi if !is_anthropic_model => Some(indexmap::indexmap! {
                "assistant".into() => "model".into(),
            }),
            _ => None,
        };
    }

    if options.media_url_handler.is_none() {
        let handler = match provider {
            LlmProvider::OpenAi
            | LlmProvider::OpenAiGeneric
            | LlmProvider::AzureOpenAi
            | LlmProvider::Ollama
            | LlmProvider::OpenRouter
            | LlmProvider::OpenAiResponses
            | LlmProvider::AiGatewayImages => {
                sys_types::generated::owned::prompt::MediaUrlHandler {
                    image: Some("send_url".into()),
                    audio: Some("send_base64".into()),
                    video: Some("send_url".into()),
                    pdf: Some("send_url".into()),
                }
            }
            LlmProvider::Anthropic => sys_types::generated::owned::prompt::MediaUrlHandler {
                image: Some("send_url".into()),
                audio: Some("send_url".into()),
                video: Some("send_url".into()),
                pdf: Some("send_url".into()),
            },
            LlmProvider::GoogleAi => sys_types::generated::owned::prompt::MediaUrlHandler {
                image: Some("send_base64_unless_google_url".into()),
                audio: Some("send_base64".into()),
                video: Some("send_base64".into()),
                pdf: Some("send_base64".into()),
            },
            LlmProvider::VertexAi => {
                let is_anthropic_model = options
                    .model
                    .as_deref()
                    .is_some_and(|m| m.starts_with("claude"));
                if is_anthropic_model {
                    // Claude on Vertex uses the Anthropic rawPredict path,
                    // so media handling should match the Anthropic provider.
                    sys_types::generated::owned::prompt::MediaUrlHandler {
                        image: Some("send_url".into()),
                        audio: Some("send_url".into()),
                        video: Some("send_url".into()),
                        pdf: Some("send_url".into()),
                    }
                } else {
                    sys_types::generated::owned::prompt::MediaUrlHandler {
                        image: Some("send_url_add_mime_type".into()),
                        audio: Some("send_url_add_mime_type".into()),
                        video: Some("send_url".into()),
                        pdf: Some("send_url".into()),
                    }
                }
            }
            LlmProvider::AwsBedrock => sys_types::generated::owned::prompt::MediaUrlHandler {
                image: Some("send_base64".into()),
                audio: Some("send_base64".into()),
                video: Some("send_url".into()),
                pdf: Some("send_base64".into()),
            },
            LlmProvider::BamlFallback | LlmProvider::BamlRoundRobin => {
                sys_types::generated::owned::prompt::MediaUrlHandler {
                    image: Some("send_base64".into()),
                    audio: Some("send_base64".into()),
                    video: Some("send_base64".into()),
                    pdf: Some("send_base64".into()),
                }
            }
        };
        options.media_url_handler = Some(handler);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: indexmap::IndexMap<String, String>,
    pub body: String,
}

#[cfg(test)]
mod tests {
    use bex_external_types::AsBexExternalValue;

    use super::*;

    fn azure_options() -> PrimitiveClientOptions {
        PrimitiveClientOptions {
            model: Some("gpt-4o".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn runtime_azure_client_requires_an_endpoint() {
        let error = PrimitiveClient::new(
            "runtime-azure".to_string(),
            "azure-openai".to_string(),
            azure_options(),
        )
        .unwrap_err();

        assert!(matches!(error, ClientError::InvalidOptions { .. }));
        assert_eq!(
            error.to_string(),
            "client 'runtime-azure': azure-openai requires either base_url or both resource_name and deployment_id (missing: resource_name and deployment_id)"
        );
    }

    #[test]
    fn runtime_azure_client_accepts_base_url() {
        let client = PrimitiveClient::new(
            "runtime-azure".to_string(),
            "azure-openai".to_string(),
            PrimitiveClientOptions {
                base_url: Some("https://example.openai.azure.com".to_string()),
                ..azure_options()
            },
        );

        assert!(client.is_ok());
    }

    #[test]
    fn runtime_azure_client_accepts_resource_and_deployment() {
        let client = PrimitiveClient::new(
            "runtime-azure".to_string(),
            "azure-openai".to_string(),
            PrimitiveClientOptions {
                provider_options: AzureOpenAiOptions {
                    resource_name: Some("example".to_string()),
                    deployment_id: Some("gpt-4o".to_string()),
                    api_version: "2024-02-15-preview".to_string(),
                    max_tokens: None,
                }
                .into_bex_external_value(),
                ..azure_options()
            },
        );

        assert!(client.is_ok());
    }
}
