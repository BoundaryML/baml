//! Prompt specialization for specific LLM providers.
//!
//! Applies provider-specific transformations to a generic `PromptAst`:
//! 1. Wrap simple nodes as messages with the default role
//! 2. Merge adjacent same-role messages
//! 3. Consolidate system prompts
//! 4. Validate roles against `allowed_roles`, then remap
//! 5. Filter metadata

mod transformations;

use std::str::FromStr;

use crate::{LlmProvider, ModelFeatures};

/// Apply prompt specialization given already-extracted owned types.
pub(crate) fn specialize_prompt_from_owned(
    client: &crate::baml_std::PrimitiveClient,
    prompt: bex_vm_types::PromptAst,
) -> Result<bex_vm_types::PromptAst, SpecializePromptError> {
    let provider = LlmProvider::from_str(&client.provider).unwrap_or(LlmProvider::OpenAiGeneric);

    let features = ModelFeatures::for_provider(provider, &client.options);
    let prompt = transformations::wrap_simple_as_message(prompt, &client.default_role);
    let prompt = transformations::merge_adjacent_roles(prompt);
    let prompt = transformations::consolidate_system_prompts(prompt, &features);
    let prompt = transformations::validate_and_remap_roles(
        prompt,
        &client.allowed_roles,
        client.options.remap_roles.as_ref(),
    )?;

    Ok(transformations::filter_metadata(prompt, &features))
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SpecializePromptError {
    #[error("role '{role}' is not in allowed_roles: {allowed:?}")]
    DisallowedRole { role: String, allowed: Vec<String> },
}
