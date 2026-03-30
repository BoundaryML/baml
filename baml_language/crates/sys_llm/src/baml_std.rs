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
    /// Resolved default role (falls back to "system").
    pub default_role: String,
    /// Resolved allowed roles (falls back to \["user"\]).
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
        options: PrimitiveClientOptions,
    ) -> Result<Self, ClientError> {
        let _ = LlmProvider::from_str(&provider).map_err(|_| ClientError::UnknownProvider {
            client: name.clone(),
            provider: provider.clone(),
        })?;
        let model = options.model.clone().unwrap_or_default();
        let allowed_roles = options.allowed_roles.clone().unwrap_or_else(|| {
            vec![
                "user".to_string(),
                "assistant".to_string(),
                "system".to_string(),
            ]
        });
        // Falls back to the first allowed role, or "user" if allowed_roles is empty.
        // An empty allowed_roles is not a valid configuration but we handle it
        // gracefully rather than panicking.
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

// Provider option structs are generated from llm_types.baml via sys_types.
pub use sys_types::generated::owned::llm::{AnthropicOptions, AzureOpenAiOptions, BedrockOptions};

/// Provider-specific options, matching the BAML schema union
/// `AnthropicOptions | AzureOpenAiOptions | BedrockOptions | null`.
#[derive(Clone, Debug)]
pub enum ProviderOptions {
    Anthropic(AnthropicOptions),
    AzureOpenAi(AzureOpenAiOptions),
    Bedrock(BedrockOptions),
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
        other => unreachable!(
            "unknown provider options class {other:?}: add it to resolve_provider_options"
        ),
    }
}

/// Generated from `llm_types.baml`. Fields come from the BAML class definition.
pub use sys_types::generated::owned::llm::PrimitiveClientOptions;

// Provider defaults are now applied at compile time in lower_cst.rs.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: indexmap::IndexMap<String, String>,
    pub body: String,
}
