use bex_external_types::BexExternalValue;
use indexmap::IndexMap;

use crate::LlmProvider;

/// Subset of engine's `ModelFeatures` relevant to prompt specialization.
///
/// Derived from provider name + client options at specialization time.
/// Media resolution fields are deferred to a future phase.
#[derive(Clone, Debug)]
pub(crate) struct ModelFeatures {
    /// If true, only one system message is allowed. Additional system messages
    /// are converted to user messages.
    pub max_one_system_prompt: bool,

    /// Controls which metadata keys are allowed on messages.
    pub allowed_metadata: AllowedMetadata,
}

/// Controls which metadata keys are allowed on messages.
#[derive(Clone, Debug)]
pub(crate) enum AllowedMetadata {
    /// All metadata keys are allowed.
    All,
    /// No metadata keys are allowed.
    None,
    /// Only these specific keys are allowed.
    Only(Vec<String>),
}

impl AllowedMetadata {
    #[allow(dead_code)]
    pub(crate) fn is_allowed(&self, key: &str) -> bool {
        match self {
            Self::All => true,
            Self::None => false,
            Self::Only(allowed) => allowed.iter().any(|a| a == key),
        }
    }
}

impl ModelFeatures {
    /// Build model features from provider and client options.
    ///
    /// Uses a hardcoded lookup table for provider defaults, then overrides
    /// with any matching keys in the `options` map.
    pub(crate) fn for_provider(
        provider: LlmProvider,
        options: &IndexMap<String, BexExternalValue>,
    ) -> Self {
        let mut features = Self::defaults_for_provider(provider);
        features.apply_overrides(options);
        features
    }

    /// Hardcoded defaults per provider.
    ///
    /// Source: engine/baml-runtime/src/internal/llm_client/primitive/*/
    fn defaults_for_provider(provider: LlmProvider) -> Self {
        match provider {
            // OpenAI variants: multiple system prompts allowed
            LlmProvider::OpenAi
            | LlmProvider::OpenAiGeneric
            | LlmProvider::AzureOpenAi
            | LlmProvider::Ollama
            | LlmProvider::OpenRouter
            | LlmProvider::OpenAiResponses => Self {
                max_one_system_prompt: false,
                allowed_metadata: AllowedMetadata::All,
            },
            // Anthropic: multiple system messages allowed (extracted into system array)
            LlmProvider::Anthropic => Self {
                max_one_system_prompt: false,
                allowed_metadata: AllowedMetadata::All,
            },
            // AWS Bedrock, Google AI, Vertex AI: single system prompt only
            LlmProvider::AwsBedrock | LlmProvider::GoogleAi | LlmProvider::VertexAi => Self {
                max_one_system_prompt: true,
                allowed_metadata: AllowedMetadata::All,
            },
            // Strategy providers — shouldn't reach here, but conservative defaults
            LlmProvider::BamlFallback | LlmProvider::BamlRoundRobin => Self {
                max_one_system_prompt: true,
                allowed_metadata: AllowedMetadata::All,
            },
        }
    }

    /// Override defaults with values from the client options map.
    fn apply_overrides(&mut self, options: &IndexMap<String, BexExternalValue>) {
        if let Some(BexExternalValue::Bool(v)) = options.get("max_one_system_prompt") {
            self.max_one_system_prompt = *v;
        }

        if let Some(val) = options.get("allowed_role_metadata") {
            match val {
                BexExternalValue::String(s) if s == "all" => {
                    self.allowed_metadata = AllowedMetadata::All;
                }
                BexExternalValue::String(s) if s == "none" => {
                    self.allowed_metadata = AllowedMetadata::None;
                }
                BexExternalValue::Array { items, .. } => {
                    let keys: Vec<String> = items
                        .iter()
                        .filter_map(|item| {
                            if let BexExternalValue::String(s) = item {
                                Some(s.clone())
                            } else {
                                None
                            }
                        })
                        .collect();
                    self.allowed_metadata = AllowedMetadata::Only(keys);
                }
                _ => {}
            }
        }
    }
}
