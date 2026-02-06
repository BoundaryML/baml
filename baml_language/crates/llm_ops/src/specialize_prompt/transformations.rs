use std::sync::Arc;

use baml_builtins::PromptAst;
use serde_json::Value;

use crate::{AllowedMetadata, ModelFeatures};

/// Merge adjacent messages with the same role.
///
/// Walks the top-level `PromptAst::Vec` and merges consecutive `Message` nodes
/// that share the same role by combining their contents into a `Vec` node.
///
/// Ported from: engine/baml-runtime/src/internal/llm_client/traits/mod.rs:89-102
pub(super) fn merge_adjacent_roles(prompt: bex_vm_types::PromptAst) -> bex_vm_types::PromptAst {
    // first handle invariants (strings next to strings, etc.)
    let prompt = prompt.merge_adjacent();

    match prompt.as_ref() {
        PromptAst::Vec(messages) => {
            // First merge any inner nodes, so we're guaranteed to have a flat list of messages.
            let mut final_messages: Vec<bex_vm_types::PromptAst> =
                Vec::with_capacity(messages.len());

            for curr in messages {
                let Some(last) = final_messages.last().map(std::convert::AsRef::as_ref) else {
                    final_messages.push(curr.clone());
                    continue;
                };

                match (last, curr.as_ref()) {
                    (
                        PromptAst::Message {
                            role: last_role,
                            content: last_content,
                            metadata: last_metadata,
                        },
                        PromptAst::Message {
                            role: curr_role,
                            content: curr_content,
                            metadata: curr_metadata,
                        },
                    ) if last_role == curr_role && (last_metadata == curr_metadata) => {
                        let merged = Arc::new(PromptAst::Message {
                            role: last_role.clone(),
                            content: last_content.clone().join(curr_content.clone()),
                            metadata: last_metadata.clone(),
                        });
                        final_messages
                            .pop()
                            .expect("invariant violated: final_messages is not empty");
                        final_messages.push(merged);
                    }
                    _ => {
                        final_messages.push(curr.clone());
                    }
                }
            }

            if final_messages.len() == 1 {
                final_messages.pop().unwrap()
            } else {
                Arc::new(PromptAst::Vec(final_messages))
            }
        }
        _ => prompt,
    }
}

/// Consolidate system prompts based on provider capabilities.
///
/// When `max_one_system_prompt` is true:
/// - If the entire prompt is a single system message, convert it to "user"
/// - Otherwise, keep the first system message, convert all subsequent
///   system messages to "user"
///
/// Ported from: engine/baml-runtime/src/internal/llm_client/traits/mod.rs:280-296
pub(super) fn consolidate_system_prompts(
    prompt: bex_vm_types::PromptAst,
    features: &ModelFeatures,
) -> bex_vm_types::PromptAst {
    if !features.max_one_system_prompt {
        return prompt;
    }

    match prompt.as_ref() {
        PromptAst::Vec(messages) => {
            let total = messages.len();
            let mut seen_first_system = false;

            let transformed: Vec<_> = messages
                .iter()
                .map(|msg| match msg.as_ref() {
                    PromptAst::Message {
                        role,
                        content,
                        metadata,
                    } if role == "system" => {
                        if total == 1 {
                            return Arc::new(PromptAst::Message {
                                role: "user".to_string(),
                                content: content.clone(),
                                metadata: metadata.clone(),
                            });
                        }

                        if !seen_first_system {
                            seen_first_system = true;
                            Arc::new(PromptAst::Message {
                                role: role.clone(),
                                content: content.clone(),
                                metadata: metadata.clone(),
                            })
                        } else {
                            Arc::new(PromptAst::Message {
                                role: "user".to_string(),
                                content: content.clone(),
                                metadata: metadata.clone(),
                            })
                        }
                    }
                    _ => msg.clone(),
                })
                .collect();

            Arc::new(PromptAst::Vec(transformed))
        }
        PromptAst::Message {
            role,
            content,
            metadata,
        } if role == "system" => Arc::new(PromptAst::Message {
            role: "user".to_string(),
            content: content.clone(),
            metadata: metadata.clone(),
        }),
        _ => prompt,
    }
}

/// Filter metadata on messages based on allowed metadata configuration.
///
/// Walks all Message nodes and removes disallowed metadata keys.
///
/// Ported from: engine/baml-runtime/src/internal/llm_client/traits/mod.rs:110-128
pub(super) fn filter_metadata(
    prompt: bex_vm_types::PromptAst,
    features: &ModelFeatures,
) -> bex_vm_types::PromptAst {
    if matches!(features.allowed_metadata, AllowedMetadata::All) {
        return prompt;
    }

    filter_metadata_recursive(prompt, features)
}

fn filter_metadata_recursive(
    prompt: bex_vm_types::PromptAst,
    features: &ModelFeatures,
) -> bex_vm_types::PromptAst {
    match prompt.as_ref() {
        PromptAst::Message {
            role,
            content,
            metadata,
        } => {
            let filtered_metadata = filter_metadata_value(metadata, features);
            Arc::new(PromptAst::Message {
                role: role.clone(),
                content: content.clone(),
                metadata: filtered_metadata,
            })
        }
        PromptAst::Vec(items) => Arc::new(PromptAst::Vec(
            items
                .iter()
                .map(|item| filter_metadata_recursive(item.clone(), features))
                .collect(),
        )),
        PromptAst::Simple(_) => prompt,
    }
}

/// Filter metadata (`serde_json::Value`) by allowed keys.
/// Returns Null for non-Object values or when no metadata is allowed.
fn filter_metadata_value(metadata: &Value, features: &ModelFeatures) -> Value {
    if matches!(features.allowed_metadata, AllowedMetadata::None) {
        return Value::Null;
    }

    match metadata {
        Value::Object(map) => {
            let filtered_map = map
                .iter()
                .filter(|(key, _)| {
                    matches!(
                        &features.allowed_metadata,
                        AllowedMetadata::Only(keys) if keys.contains(key)
                    )
                })
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            Value::Object(filtered_map)
        }
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use indexmap::IndexMap;

    use super::*;
    use crate::{AllowedMetadata, LlmProvider, ModelFeatures};

    fn msg(role: &str, text: &str) -> Arc<PromptAst> {
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(text.to_string().into()),
            metadata: Value::Null,
        })
    }

    // ---- ModelFeatures tests ----

    #[test]
    fn test_openai_defaults() {
        let features = ModelFeatures::for_provider(LlmProvider::OpenAi, &IndexMap::new());
        assert!(!features.max_one_system_prompt);
    }

    #[test]
    fn test_anthropic_defaults() {
        let features = ModelFeatures::for_provider(LlmProvider::Anthropic, &IndexMap::new());
        assert!(features.max_one_system_prompt);
    }

    #[test]
    fn test_strategy_provider_defaults() {
        let features = ModelFeatures::for_provider(LlmProvider::BamlFallback, &IndexMap::new());
        assert!(features.max_one_system_prompt);
    }

    #[test]
    fn test_override_max_one_system_prompt() {
        let mut options = IndexMap::new();
        options.insert(
            "max_one_system_prompt".to_string(),
            bex_external_types::BexExternalValue::Bool(false),
        );
        let features = ModelFeatures::for_provider(LlmProvider::Anthropic, &options);
        assert!(!features.max_one_system_prompt);
    }

    // ---- merge_adjacent_messages tests ----

    #[test]
    fn test_merge_adjacent_same_role() {
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("user", "Hello"),
            msg("user", "World"),
        ]));
        let result = merge_adjacent_roles(prompt);
        let expected = Arc::new(PromptAst::Message {
            role: "user".to_string(),
            content: Arc::new("HelloWorld".to_string().into()),
            metadata: Value::Null,
        });
        assert_eq!(result, expected);
    }

    #[test]
    fn test_no_merge_different_roles() {
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful"),
            msg("user", "Hello"),
        ]));
        let result = merge_adjacent_roles(prompt);
        let expected = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful"),
            msg("user", "Hello"),
        ]));
        assert_eq!(result, expected);
    }

    #[test]
    fn test_merge_three_same_role() {
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("user", "A"),
            msg("user", "B"),
            msg("user", "C"),
        ]));
        let result = merge_adjacent_roles(prompt);
        let expected = Arc::new(PromptAst::Message {
            role: "user".to_string(),
            content: Arc::new("ABC".to_string().into()),
            metadata: Value::Null,
        });
        assert_eq!(result, expected);
    }

    #[test]
    fn test_merge_passthrough_non_vec() {
        let prompt = msg("user", "Hello");
        let result = merge_adjacent_roles(prompt);
        assert_eq!(result, msg("user", "Hello"));
    }

    // ---- consolidate_system_prompts tests ----

    #[test]
    fn test_consolidate_single_system_to_user() {
        let features = ModelFeatures {
            max_one_system_prompt: true,
            allowed_metadata: AllowedMetadata::All,
        };
        let prompt = msg("system", "You are helpful");
        let result = consolidate_system_prompts(prompt, &features);
        assert_eq!(result, msg("user", "You are helpful"));
    }

    #[test]
    fn test_consolidate_keeps_first_system() {
        let features = ModelFeatures {
            max_one_system_prompt: true,
            allowed_metadata: AllowedMetadata::All,
        };
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "First system"),
            msg("user", "Hello"),
            msg("system", "Second system"),
        ]));
        let result = consolidate_system_prompts(prompt, &features);
        let expected = Arc::new(PromptAst::Vec(vec![
            msg("system", "First system"),
            msg("user", "Hello"),
            msg("user", "Second system"),
        ]));
        assert_eq!(result, expected);
    }

    #[test]
    fn test_consolidate_noop_when_disabled() {
        let features = ModelFeatures {
            max_one_system_prompt: false,
            allowed_metadata: AllowedMetadata::All,
        };
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "First"),
            msg("system", "Second"),
        ]));
        let result = consolidate_system_prompts(prompt, &features);
        let expected = Arc::new(PromptAst::Vec(vec![
            msg("system", "First"),
            msg("system", "Second"),
        ]));
        assert_eq!(result, expected);
    }

    // ---- filter_metadata tests ----

    #[test]
    fn test_filter_metadata_all_allowed() {
        let features = ModelFeatures {
            max_one_system_prompt: false,
            allowed_metadata: AllowedMetadata::All,
        };
        let prompt = msg("user", "Hello");
        let result = filter_metadata(prompt, &features);
        assert_eq!(result, msg("user", "Hello"));
    }

    #[test]
    fn test_filter_metadata_none_allowed() {
        let features = ModelFeatures {
            max_one_system_prompt: false,
            allowed_metadata: AllowedMetadata::None,
        };
        let prompt = msg("user", "Hello");
        let result = filter_metadata(prompt, &features);
        assert_eq!(result, msg("user", "Hello"));
    }

    #[test]
    fn test_filter_metadata_only_specific() {
        let features = ModelFeatures {
            max_one_system_prompt: false,
            allowed_metadata: AllowedMetadata::Only(vec!["cache_control".to_string()]),
        };
        let prompt = msg("user", "Hello");
        let result = filter_metadata(prompt, &features);
        assert_eq!(result, msg("user", "Hello"));
    }

    // ---- Integration: full specialize_prompt pipeline ----

    #[test]
    fn test_specialize_anthropic_prompt() {
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful"),
            msg("user", "Hello"),
            msg("user", "How are you?"),
            msg("system", "Also be concise"),
            msg("assistant", "I'm fine"),
        ]));

        let features = ModelFeatures::for_provider(LlmProvider::Anthropic, &IndexMap::new());

        let result = merge_adjacent_roles(prompt);
        let result = consolidate_system_prompts(result, &features);
        let result = filter_metadata(result, &features);

        let expected = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful"),
            Arc::new(PromptAst::Message {
                role: "user".to_string(),
                content: Arc::new("HelloHow are you?".to_string().into()),
                metadata: Value::Null,
            }),
            msg("user", "Also be concise"),
            msg("assistant", "I'm fine"),
        ]));
        assert_eq!(result, expected);
    }
}
