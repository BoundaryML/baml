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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_builtins2::PromptAst;

    use super::*;

    fn msg(role: &str, text: &str) -> Arc<PromptAst> {
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(text.to_string().into()),
            metadata: serde_json::Value::Null,
        })
    }

    fn vertex_client(model: &str) -> crate::baml_std::PrimitiveClient {
        crate::baml_std::PrimitiveClient::new(
            "test".to_string(),
            "vertex-ai".to_string(),
            crate::baml_std::PrimitiveClientOptions {
                model: Some(model.to_string()),
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn vertex_gemini_remaps_assistant_to_model() {
        let client = vertex_client("gemini-2.0-flash");
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("user", "Hi"),
            msg("assistant", "Hello!"),
        ]));
        let result = specialize_prompt_from_owned(&client, prompt).unwrap();
        let expected = Arc::new(PromptAst::Vec(vec![
            msg("user", "Hi"),
            msg("model", "Hello!"),
        ]));
        assert_eq!(result, expected);
    }

    #[test]
    fn vertex_claude_keeps_assistant_role() {
        let client = vertex_client("claude-sonnet-4-20250514");
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("user", "Hi"),
            msg("assistant", "Hello!"),
        ]));
        let result = specialize_prompt_from_owned(&client, prompt).unwrap();
        let expected = Arc::new(PromptAst::Vec(vec![
            msg("user", "Hi"),
            msg("assistant", "Hello!"),
        ]));
        assert_eq!(result, expected);
    }
}
