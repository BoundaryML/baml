use std::str::FromStr;

use crate::LlmProvider;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("client '{client}': missing required option '{option}'")]
    MissingOption { client: String, option: String },
    #[error("client '{client}': unknown provider '{provider}'")]
    UnknownProvider { client: String, provider: String },
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

        // Apply provider-specific defaults for any fields the user didn't set.
        apply_provider_defaults(llm_provider, &mut options);

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
        let provider_options = resolve_provider_options(&options.provider_options);
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
pub use sys_types::generated::owned::llm::{
    AnthropicOptions, AzureOpenAiOptions, BedrockOptions, VertexAiOptions,
};

/// Provider-specific options, matching the BAML schema union
/// `AnthropicOptions | AzureOpenAiOptions | BedrockOptions | VertexAiOptions | null`.
#[derive(Clone, Debug)]
pub enum ProviderOptions {
    Anthropic(AnthropicOptions),
    AzureOpenAi(AzureOpenAiOptions),
    Bedrock(BedrockOptions),
    VertexAi(VertexAiOptions),
}

/// Convert a `BexExternalValue` (from the VM) to a typed `ProviderOptions`.
pub fn resolve_provider_options(val: &bex_heap::BexExternalValue) -> Option<ProviderOptions> {
    use bex_heap::BexExternalValue;
    let BexExternalValue::Instance { class_name, .. } = val else {
        return None;
    };
    match class_name.as_str() {
        "baml.llm.AnthropicOptions" => AnthropicOptions::from_external(val.clone())
            .ok()
            .map(ProviderOptions::Anthropic),
        "baml.llm.AzureOpenAiOptions" => AzureOpenAiOptions::from_external(val.clone())
            .ok()
            .map(ProviderOptions::AzureOpenAi),
        "baml.llm.BedrockOptions" => BedrockOptions::from_external(val.clone())
            .ok()
            .map(ProviderOptions::Bedrock),
        "baml.llm.VertexAiOptions" => VertexAiOptions::from_external(val.clone())
            .ok()
            .map(ProviderOptions::VertexAi),
        other => unreachable!(
            "unknown provider options class {other:?}: add it to resolve_provider_options"
        ),
    }
}

/// Generated from `llm_types.baml`. Fields come from the BAML class definition.
pub use sys_types::generated::owned::llm::PrimitiveClientOptions;

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
            LlmProvider::GoogleAi => {
                Some("https://generativelanguage.googleapis.com/v1beta".into())
            }
            // VertexAi, AwsBedrock, AzureOpenAi: base_url is constructed at
            // request time from provider-specific fields (location, resource_name, etc.)
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
            | LlmProvider::AiGatewayImages => sys_types::generated::owned::llm::MediaUrlHandler {
                image: Some("send_url".into()),
                audio: Some("send_base64".into()),
                video: Some("send_url".into()),
                pdf: Some("send_url".into()),
            },
            LlmProvider::Anthropic => sys_types::generated::owned::llm::MediaUrlHandler {
                image: Some("send_url".into()),
                audio: Some("send_url".into()),
                video: Some("send_url".into()),
                pdf: Some("send_url".into()),
            },
            LlmProvider::GoogleAi => sys_types::generated::owned::llm::MediaUrlHandler {
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
                    sys_types::generated::owned::llm::MediaUrlHandler {
                        image: Some("send_url".into()),
                        audio: Some("send_url".into()),
                        video: Some("send_url".into()),
                        pdf: Some("send_url".into()),
                    }
                } else {
                    sys_types::generated::owned::llm::MediaUrlHandler {
                        image: Some("send_url_add_mime_type".into()),
                        audio: Some("send_url_add_mime_type".into()),
                        video: Some("send_url".into()),
                        pdf: Some("send_url".into()),
                    }
                }
            }
            LlmProvider::AwsBedrock => sys_types::generated::owned::llm::MediaUrlHandler {
                image: Some("send_base64".into()),
                audio: Some("send_base64".into()),
                video: Some("send_url".into()),
                pdf: Some("send_base64".into()),
            },
            LlmProvider::BamlFallback | LlmProvider::BamlRoundRobin => {
                sys_types::generated::owned::llm::MediaUrlHandler {
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
