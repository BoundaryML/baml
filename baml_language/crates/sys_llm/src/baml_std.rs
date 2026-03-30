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
    /// Fully resolved request URL (`base_url` + provider path suffix).
    pub url: String,
    /// Resolved model name (falls back to empty string).
    pub model: String,
    /// Resolved `max_tokens` (None if not set by user or provider default).
    pub max_tokens: Option<i64>,
    /// Resolved default role (falls back to "system").
    pub default_role: String,
    /// Resolved allowed roles (falls back to \["user"\]).
    pub allowed_roles: Vec<String>,
    /// Forward options from `request_body`, pre-converted to JSON.
    pub(crate) extra_body: serde_json::Map<String, serde_json::Value>,
    pub(crate) options: PrimitiveClientOptions,
}

impl PrimitiveClient {
    pub fn new(
        name: String,
        provider: String,
        options: PrimitiveClientOptions,
    ) -> Result<Self, ClientError> {
        let llm_provider =
            LlmProvider::from_str(&provider).map_err(|_| ClientError::UnknownProvider {
                client: name.clone(),
                provider: provider.clone(),
            })?;
        let base_url = options.base_url.clone().unwrap_or_default();
        let model = options.model.clone().unwrap_or_default();
        let url = match llm_provider {
            LlmProvider::AzureOpenAi => {
                let Some(ProviderOptions::AzureOpenAi(azure)) = &options.provider_options else {
                    return Err(ClientError::MissingOption {
                        client: name,
                        option: "api_version".into(),
                    });
                };
                let base = match (
                    &options.base_url,
                    &azure.resource_name,
                    &azure.deployment_id,
                ) {
                    (Some(url), _, _) => url.clone(),
                    (None, Some(rn), Some(did)) => {
                        format!("https://{rn}.openai.azure.com/openai/deployments/{did}")
                    }
                    _ => {
                        return Err(ClientError::MissingOption {
                            client: name,
                            option: "base_url or (resource_name + deployment_id)".into(),
                        });
                    }
                };
                format!("{base}/chat/completions?api-version={}", azure.api_version)
            }
            LlmProvider::AwsBedrock => {
                let bedrock_opts = match &options.provider_options {
                    Some(ProviderOptions::Bedrock(opts)) => opts,
                    _ => &BedrockOptions::default(),
                };
                if let Some(endpoint) = &bedrock_opts.endpoint_url {
                    format!("{endpoint}/model/{model}/converse")
                } else if let Some(region) = &bedrock_opts.region {
                    format!("https://bedrock-runtime.{region}.amazonaws.com/model/{model}/converse")
                } else {
                    // Region will need to be resolved later; store a placeholder.
                    // The build_request step will error if region is still unknown.
                    String::new()
                }
            }
            _ if base_url.is_empty() => {
                return Err(ClientError::MissingOption {
                    client: name,
                    option: "base_url".into(),
                });
            }
            LlmProvider::Anthropic => format!("{base_url}/v1/messages"),
            LlmProvider::OpenAiResponses => format!("{base_url}/responses"),
            // OpenAI-compatible providers, strategies, etc.
            _ => format!("{base_url}/chat/completions"),
        };
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
        let max_tokens = options.max_tokens;
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
            url,
            model,
            max_tokens,
            default_role,
            allowed_roles,
            extra_body,
            options,
        })
    }

    pub fn is_finish_reason_allowed(&self, finish_reason: Option<&str>) -> bool {
        let Some(finish_reason) = finish_reason else {
            return true;
        };
        let finish_reason = &finish_reason.to_ascii_lowercase();
        match (
            &self.options.finish_reason_allow_list,
            &self.options.finish_reason_deny_list,
        ) {
            (Some(finish_reason_allow_list), None) => {
                finish_reason_allow_list.contains(finish_reason)
            }
            (None, Some(finish_reason_deny_list)) => {
                !finish_reason_deny_list.contains(finish_reason)
            }
            (Some(finish_reason_allow_list), Some(finish_reason_deny_list)) => {
                finish_reason_allow_list.contains(finish_reason)
                    && !finish_reason_deny_list.contains(finish_reason)
            }
            (None, None) => true,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct AnthropicOptions {
    pub anthropic_version: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AzureOpenAiOptions {
    pub resource_name: Option<String>,
    pub deployment_id: Option<String>,
    pub api_version: String,
}

#[derive(Clone, Debug, Default)]
pub struct BedrockOptions {
    pub region: Option<String>,
    pub endpoint_url: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub session_token: Option<String>,
    pub profile: Option<String>,
    pub stop_sequences: Option<Vec<String>>,
}

/// Provider-specific options, matching the BAML schema union
/// `AnthropicOptions | AzureOpenAiOptions | BedrockOptions | null`.
#[derive(Clone, Debug)]
pub enum ProviderOptions {
    Anthropic(AnthropicOptions),
    AzureOpenAi(AzureOpenAiOptions),
    Bedrock(BedrockOptions),
}

#[derive(Debug, Default)]
pub struct PrimitiveClientOptions {
    pub model: Option<String>,
    pub max_tokens: Option<i64>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub max_one_system_prompt: Option<bool>,
    pub allowed_role_metadata: Option<bex_heap::BexExternalValue>,
    pub finish_reason_allow_list: Option<Vec<String>>,
    pub finish_reason_deny_list: Option<Vec<String>>,
    pub base_url: Option<String>,
    pub default_role: Option<String>,
    pub allowed_roles: Option<Vec<String>>,
    pub remap_roles: Option<indexmap::IndexMap<String, String>>,
    pub api_key: Option<String>,
    pub provider_options: Option<ProviderOptions>,
    pub headers: indexmap::IndexMap<String, String>,
    pub query_params: indexmap::IndexMap<String, String>,
    pub request_body: indexmap::IndexMap<String, bex_heap::BexExternalValue>,
}

impl PrimitiveClientOptions {
    /// Provider-specific defaults. User-specified values are merged on top.
    pub fn provider_defaults(provider: LlmProvider) -> Self {
        match provider {
            LlmProvider::Anthropic => Self {
                base_url: Some("https://api.anthropic.com".to_string()),
                max_tokens: Some(4096),
                provider_options: Some(ProviderOptions::Anthropic(AnthropicOptions {
                    anthropic_version: Some("2023-06-01".to_string()),
                })),
                ..Default::default()
            },
            LlmProvider::OpenAi | LlmProvider::OpenAiGeneric | LlmProvider::OpenAiResponses => {
                Self {
                    base_url: Some("https://api.openai.com/v1".to_string()),
                    default_role: Some("system".to_string()),
                    allowed_roles: Some(vec![
                        "system".to_string(),
                        "user".to_string(),
                        "assistant".to_string(),
                    ]),
                    ..Default::default()
                }
            }
            LlmProvider::Ollama => Self {
                base_url: Some("http://localhost:11434".to_string()),
                default_role: Some("user".to_string()),
                allowed_roles: Some(vec!["user".to_string(), "assistant".to_string()]),
                ..Default::default()
            },
            LlmProvider::OpenRouter => Self {
                base_url: Some("https://openrouter.ai/api".to_string()),
                default_role: Some("system".to_string()),
                allowed_roles: Some(vec![
                    "system".to_string(),
                    "user".to_string(),
                    "assistant".to_string(),
                ]),
                ..Default::default()
            },
            LlmProvider::AzureOpenAi => Self {
                default_role: Some("system".to_string()),
                allowed_roles: Some(vec![
                    "system".to_string(),
                    "user".to_string(),
                    "assistant".to_string(),
                ]),
                max_tokens: Some(4096),
                ..Default::default()
            },
            LlmProvider::AwsBedrock => Self {
                default_role: Some("user".to_string()),
                allowed_roles: Some(vec![
                    "system".to_string(),
                    "user".to_string(),
                    "assistant".to_string(),
                ]),
                ..Default::default()
            },
            LlmProvider::GoogleAi
            | LlmProvider::VertexAi
            | LlmProvider::BamlFallback
            | LlmProvider::BamlRoundRobin => PrimitiveClientOptions::default(),
        }
    }

    /// Merge user-specified values on top of defaults. User values take precedence.
    #[must_use]
    pub fn with_defaults(self, defaults: Self) -> Self {
        Self {
            model: self.model.or(defaults.model),
            max_tokens: self.max_tokens.or(defaults.max_tokens),
            temperature: self.temperature.or(defaults.temperature),
            top_p: self.top_p.or(defaults.top_p),
            max_one_system_prompt: self
                .max_one_system_prompt
                .or(defaults.max_one_system_prompt),
            allowed_role_metadata: self
                .allowed_role_metadata
                .or(defaults.allowed_role_metadata),
            finish_reason_allow_list: self
                .finish_reason_allow_list
                .or(defaults.finish_reason_allow_list),
            finish_reason_deny_list: self
                .finish_reason_deny_list
                .or(defaults.finish_reason_deny_list),
            base_url: self.base_url.or(defaults.base_url),
            default_role: self.default_role.or(defaults.default_role),
            allowed_roles: self.allowed_roles.or(defaults.allowed_roles),
            remap_roles: self.remap_roles.or(defaults.remap_roles),
            api_key: self.api_key.or(defaults.api_key),
            provider_options: self.provider_options.or(defaults.provider_options),
            headers: if self.headers.is_empty() {
                defaults.headers
            } else {
                self.headers
            },
            query_params: if self.query_params.is_empty() {
                defaults.query_params
            } else {
                self.query_params
            },
            request_body: if self.request_body.is_empty() {
                defaults.request_body
            } else {
                self.request_body
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: indexmap::IndexMap<String, String>,
    pub body: String,
}
