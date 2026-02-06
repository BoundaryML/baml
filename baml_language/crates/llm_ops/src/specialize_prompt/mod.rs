//! Prompt specialization for specific LLM providers.
//!
//! Applies provider-specific transformations to a generic `PromptAst`:
//! 1. Merge adjacent same-role messages
//! 2. Consolidate system prompts
//! 3. Filter metadata

mod transformations;

use std::str::FromStr;

use bex_heap::{
    BexHeap,
    builtin_types::{PrimitiveClient as HeapPrimitiveClient, owned::PrimitiveClient},
};
use sys_types::OpErrorKind;

use crate::{LlmProvider, ModelFeatures};

/// Specialize a prompt for a specific provider.
///
/// Applies three transformations in order:
/// 1. Merge adjacent same-role messages
/// 2. Consolidate system prompts (when `max_one_system_prompt` is true)
/// 3. Filter role metadata (strip disallowed metadata keys)
fn specialize_prompt(
    client: &PrimitiveClient,
    prompt: bex_vm_types::PromptAst,
) -> bex_vm_types::PromptAst {
    let provider = LlmProvider::from_str(&client.provider).unwrap_or(LlmProvider::OpenAiGeneric);

    let features = ModelFeatures::for_provider(provider, &client.options);
    let prompt = transformations::merge_adjacent_roles(prompt);
    let prompt = transformations::consolidate_system_prompts(prompt, &features);

    transformations::filter_metadata(prompt, &features)
}

/// Execute the `specialize_prompt` LLM `SysOp`.
///
/// Arguments: `[PrimitiveClient, prompt: PromptAst]`
pub fn execute_specialize_prompt(
    heap: &std::sync::Arc<BexHeap>,
    mut args: Vec<bex_heap::BexValue<'_>>,
) -> Result<bex_vm_types::PromptAst, OpErrorKind> {
    if args.len() != 2 {
        return Err(OpErrorKind::InvalidArgumentCount {
            expected: 2,
            actual: args.len(),
        });
    }

    let arg1 = args.pop().expect("len is 2");
    let arg0 = args.pop().expect("len is 1");

    let (client, prompt) = heap
        .with_gc_protection(|protected| {
            let client = arg0.as_builtin_class::<HeapPrimitiveClient>(&protected)?;
            let client = client.into_owned(&protected)?;
            let prompt = arg1.as_prompt_ast_owned(&protected)?;
            Ok((client, prompt))
        })
        .map_err(OpErrorKind::AccessError)?;

    Ok(specialize_prompt(&client, prompt))
}
