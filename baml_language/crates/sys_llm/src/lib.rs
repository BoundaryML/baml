//! LLM prompt specialization.
//!
//! This crate provides `specialize_prompt()`, which transforms a generic
//! `PromptAst` into one tailored for a specific LLM provider. It applies
//! provider-specific transformations based on `ModelFeatures`.

mod model_features;
mod transformations;

use bex_external_types::{PrimitiveClientValue, PromptAst};
pub use model_features::{AllowedMetadata, ModelFeatures};

/// Specialize a prompt for a specific provider.
///
/// Applies three transformations in order:
/// 1. Merge adjacent same-role messages
/// 2. Consolidate system prompts (when `max_one_system_prompt` is true)
/// 3. Filter role metadata (strip disallowed metadata keys)
pub fn specialize_prompt(client: &PrimitiveClientValue, prompt: PromptAst) -> PromptAst {
    let features = ModelFeatures::for_provider(&client.provider, &client.options);

    let prompt = transformations::merge_adjacent_messages(prompt);
    let prompt = transformations::consolidate_system_prompts(prompt, &features);
    transformations::filter_metadata(prompt, &features)
}
