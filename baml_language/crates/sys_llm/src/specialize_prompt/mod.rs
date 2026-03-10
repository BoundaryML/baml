//! Prompt specialization for specific LLM providers.
//!
//! Applies provider-specific transformations to a generic `PromptAst`:
//! 1. Merge adjacent same-role messages
//! 2. Consolidate system prompts
//! 3. Filter metadata

mod transformations;

use std::str::FromStr;

use bex_external_types::BexExternalValue;
use bex_heap::builtin_types::owned::LlmPrimitiveClient;

use crate::{LlmProvider, ModelFeatures};

/// Check if the model name indicates an o1-family model, which does not
/// support the system role.
fn uses_o1_model(client: &LlmPrimitiveClient) -> bool {
    match client.options.get("model") {
        Some(BexExternalValue::String(m)) => m == "o1" || m.starts_with("o1-"),
        _ => false,
    }
}

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

    // If the user explicitly set allowed_roles, respect that. Otherwise,
    // detect o1-family models at runtime and disallow the system role.
    let system_role_allowed =
        client.allowed_roles.iter().any(|r| r == "system") && !uses_o1_model(client);

    let features = ModelFeatures::for_provider(provider, &client.options);
    let prompt = transformations::merge_adjacent_roles(prompt);
    let prompt =
        transformations::consolidate_system_prompts(prompt, &features, system_role_allowed);

    transformations::filter_metadata(prompt, &features)
}
