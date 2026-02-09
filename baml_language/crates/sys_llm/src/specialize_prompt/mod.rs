//! Prompt specialization for specific LLM providers.
//!
//! Applies provider-specific transformations to a generic `PromptAst`:
//! 1. Merge adjacent same-role messages
//! 2. Consolidate system prompts
//! 3. Filter metadata

mod transformations;

use std::str::FromStr;

use bex_heap::builtin_types::owned::LlmPrimitiveClient;

use crate::{LlmProvider, ModelFeatures};

/// Apply prompt specialization given already-extracted owned types.
/// Specialize a prompt for a specific provider.
///
/// Applies three transformations in order:
/// 1. Merge adjacent same-role messages
/// 2. Consolidate system prompts (when `max_one_system_prompt` is true)
/// 3. Filter role metadata (strip disallowed metadata keys)
pub(crate) fn specialize_prompt_from_owned(
    client: &LlmPrimitiveClient,
    prompt: bex_vm_types::PromptAst,
) -> bex_vm_types::PromptAst {
    let provider = LlmProvider::from_str(&client.provider).unwrap_or(LlmProvider::OpenAiGeneric);

    let features = ModelFeatures::for_provider(provider, &client.options);
    let prompt = transformations::merge_adjacent_roles(prompt);
    let prompt = transformations::consolidate_system_prompts(prompt, &features);

    transformations::filter_metadata(prompt, &features)
}
