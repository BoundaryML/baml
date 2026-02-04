//! Prompt specialization for specific LLM providers.
//!
//! Applies provider-specific transformations to a generic `PromptAst`:
//! 1. Merge adjacent same-role messages
//! 2. Consolidate system prompts
//! 3. Filter metadata

mod transformations;

use std::str::FromStr;

use bex_external_types::{BexExternalValue, BexValue, PrimitiveClientValue, PromptAst};
use sys_types::OpError;

use crate::{LlmProvider, ModelFeatures};

/// Specialize a prompt for a specific provider.
///
/// Applies three transformations in order:
/// 1. Merge adjacent same-role messages
/// 2. Consolidate system prompts (when `max_one_system_prompt` is true)
/// 3. Filter role metadata (strip disallowed metadata keys)
pub fn specialize_prompt(client: &PrimitiveClientValue, prompt: PromptAst) -> PromptAst {
    let provider = LlmProvider::from_str(&client.provider).unwrap_or(LlmProvider::OpenAiGeneric);
    let features = ModelFeatures::for_provider(provider, &client.options);

    let prompt = transformations::merge_adjacent_messages(prompt);
    let prompt = transformations::consolidate_system_prompts(prompt, &features);
    transformations::filter_metadata(prompt, &features)
}

/// Execute the `specialize_prompt` LLM `SysOp`.
///
/// Arguments: `[PrimitiveClient, prompt: PromptAst]`
pub fn execute_specialize_prompt(args: &[BexValue]) -> Result<PromptAst, OpError> {
    let BexValue::External(BexExternalValue::PrimitiveClient(client)) = &args[0] else {
        return Err(llm_jinja::RenderPromptError::InvalidArgument {
            message: "expected PrimitiveClient, got something else".to_string(),
        }
        .into());
    };

    let BexValue::External(BexExternalValue::PromptAst(prompt)) = &args[1] else {
        return Err(llm_jinja::RenderPromptError::InvalidArgument {
            message: "expected PromptAst, got something else".to_string(),
        }
        .into());
    };

    Ok(specialize_prompt(client, prompt.clone()))
}
