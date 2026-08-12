//! Prompt specialization for specific LLM providers.
//!
//! Applies provider-specific transformations to a generic `PromptAst`:
//! 1. Wrap simple nodes as messages with the default role
//! 2. Promote provider-incompatible media roles when needed
//! 3. Merge adjacent same-role messages
//! 4. Consolidate system prompts
//! 5. Validate roles against `allowed_roles`, then remap
//! 6. Filter metadata

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
    let prompt = if provider_promotes_media_to_user(provider) {
        transformations::promote_media_to_user_when_no_user_message(prompt)
    } else {
        prompt
    };
    let prompt = transformations::merge_adjacent_roles(prompt);
    let prompt = transformations::consolidate_system_prompts(prompt, &features);

    let prompt = transformations::validate_and_remap_roles(
        prompt,
        &client.allowed_roles,
        client.options.remap_roles.as_ref(),
    )?;

    Ok(transformations::filter_metadata(prompt, &features))
}

fn provider_promotes_media_to_user(provider: LlmProvider) -> bool {
    matches!(
        provider,
        LlmProvider::OpenAi
            | LlmProvider::OpenAiGeneric
            | LlmProvider::AzureOpenAi
            | LlmProvider::Ollama
            | LlmProvider::OpenRouter
            | LlmProvider::OpenAiResponses
            | LlmProvider::Anthropic
    )
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SpecializePromptError {
    #[error("role '{role}' is not in allowed_roles: {allowed:?}")]
    DisallowedRole { role: String, allowed: Vec<String> },
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_builtins2::{MediaContent, MediaValue, PromptAst, PromptAstSimple};

    use super::*;

    fn msg(role: &str, text: &str) -> Arc<PromptAst> {
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(text.to_string().into()),
            metadata: serde_json::Value::Null,
        })
    }

    fn image_media() -> Arc<MediaValue> {
        Arc::new(MediaValue::new(
            baml_base::MediaKind::Image,
            MediaContent::Url {
                url: "https://example.com/image.png".to_string(),
                base64_data: None,
            },
            Some("image/png".to_string()),
        ))
    }

    fn msg_with_content(role: &str, content: PromptAstSimple) -> Arc<PromptAst> {
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(content),
            metadata: serde_json::Value::Null,
        })
    }

    fn openai_responses_client() -> crate::baml_std::PrimitiveClient {
        crate::baml_std::PrimitiveClient::new(
            "test".to_string(),
            "openai-responses".to_string(),
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gpt-5".to_string()),
                ..Default::default()
            },
        )
        .unwrap()
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
    fn openai_responses_unrole_tagged_media_prompt_becomes_user_without_existing_user() {
        let client = openai_responses_client();
        assert_eq!(client.default_role, "system");

        let prompt = Arc::new(PromptAst::Simple(Arc::new(PromptAstSimple::Multiple(
            vec![
                Arc::new(PromptAstSimple::String("Describe this:".to_string())),
                Arc::new(PromptAstSimple::Media(image_media())),
            ],
        ))));

        let result = specialize_prompt_from_owned(&client, prompt).unwrap();

        match result.as_ref() {
            PromptAst::Message { role, content, .. } => {
                assert_eq!(role, "user");
                assert!(matches!(content.as_ref(), PromptAstSimple::Multiple(_)));
            }
            other => panic!("expected message, got {other:?}"),
        }
    }

    #[test]
    fn openai_responses_keeps_explicit_roles_when_user_message_exists() {
        let client = openai_responses_client();
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg_with_content("system", PromptAstSimple::Media(image_media())),
            msg("user", "What is in it?"),
        ]));

        let result = specialize_prompt_from_owned(&client, prompt).unwrap();

        let PromptAst::Vec(messages) = result.as_ref() else {
            panic!("expected message vec");
        };
        let PromptAst::Message { role, .. } = messages[0].as_ref() else {
            panic!("expected first message");
        };
        assert_eq!(role, "system");
    }

    #[test]
    fn openai_responses_promotes_only_media_system_message_before_merge() {
        let client = openai_responses_client();
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "Follow the style guide."),
            msg_with_content("system", PromptAstSimple::Media(image_media())),
        ]));

        let result = specialize_prompt_from_owned(&client, prompt).unwrap();

        let PromptAst::Vec(messages) = result.as_ref() else {
            panic!("expected message vec");
        };
        assert_eq!(messages.len(), 2);

        let PromptAst::Message { role, content, .. } = messages[0].as_ref() else {
            panic!("expected first message");
        };
        assert_eq!(role, "system");
        assert_eq!(
            content.as_ref(),
            &PromptAstSimple::String("Follow the style guide.".into())
        );

        let PromptAst::Message { role, content, .. } = messages[1].as_ref() else {
            panic!("expected second message");
        };
        assert_eq!(role, "user");
        assert!(matches!(content.as_ref(), PromptAstSimple::Media(_)));
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
