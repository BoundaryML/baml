use bex_external_types::BexExternalValue;
use indexmap::IndexMap;

/// Subset of engine's `ModelFeatures` relevant to prompt specialization.
///
/// Derived from provider name + client options at specialization time.
/// Media resolution fields are deferred to a future phase.
#[derive(Clone, Debug)]
pub struct ModelFeatures {
    /// If true, only one system message is allowed. Additional system messages
    /// are converted to user messages.
    pub max_one_system_prompt: bool,

    /// Controls which metadata keys are allowed on messages.
    pub allowed_metadata: AllowedMetadata,
}

/// Controls which metadata keys are allowed on messages.
#[derive(Clone, Debug)]
pub enum AllowedMetadata {
    /// All metadata keys are allowed.
    All,
    /// No metadata keys are allowed.
    None,
    /// Only these specific keys are allowed.
    Only(Vec<String>),
}

impl AllowedMetadata {
    pub fn is_allowed(&self, key: &str) -> bool {
        match self {
            Self::All => true,
            Self::None => false,
            Self::Only(allowed) => allowed.iter().any(|a| a == key),
        }
    }
}

impl ModelFeatures {
    /// Build model features from provider name and client options.
    ///
    /// Uses a hardcoded lookup table for provider defaults, then overrides
    /// with any matching keys in the `options` map.
    pub fn for_provider(provider: &str, options: &IndexMap<String, BexExternalValue>) -> Self {
        let mut features = Self::defaults_for_provider(provider);
        features.apply_overrides(options);
        features
    }

    /// Hardcoded defaults per provider.
    ///
    /// Source: engine/baml-runtime/src/internal/llm_client/primitive/*/
    fn defaults_for_provider(provider: &str) -> Self {
        match provider {
            // OpenAI variants: multiple system prompts allowed
            "openai" | "openai-generic" | "azure" | "ollama" | "openrouter"
            | "openai-responses" => Self {
                max_one_system_prompt: false,
                allowed_metadata: AllowedMetadata::All,
            },
            // Anthropic: single system prompt only
            "anthropic" => Self {
                max_one_system_prompt: true,
                allowed_metadata: AllowedMetadata::All,
            },
            // AWS Bedrock: single system prompt only
            "aws-bedrock" => Self {
                max_one_system_prompt: true,
                allowed_metadata: AllowedMetadata::All,
            },
            // Google AI: single system prompt only
            "google-ai" => Self {
                max_one_system_prompt: true,
                allowed_metadata: AllowedMetadata::All,
            },
            // Vertex AI: single system prompt only
            "vertex-ai" => Self {
                max_one_system_prompt: true,
                allowed_metadata: AllowedMetadata::All,
            },
            // Unknown provider: conservative defaults (single system prompt)
            _ => Self {
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
