use bex_external_types::{BexExternalValue, PromptAst};
use indexmap::IndexMap;

use crate::{AllowedMetadata, ModelFeatures};

/// Merge adjacent messages with the same role.
///
/// Walks the top-level `PromptAst::Vec` and merges consecutive `Message` nodes
/// that share the same role by combining their contents into a `Vec` node.
///
/// Ported from: engine/baml-runtime/src/internal/llm_client/traits/mod.rs:89-102
pub(super) fn merge_adjacent_messages(prompt: PromptAst) -> PromptAst {
    match prompt {
        PromptAst::Vec(messages) => {
            let mut merged: Vec<PromptAst> = Vec::with_capacity(messages.len());

            for msg in messages {
                let should_merge = match (&merged.last(), &msg) {
                    (
                        Some(PromptAst::Message {
                            role: prev_role, ..
                        }),
                        PromptAst::Message {
                            role: next_role, ..
                        },
                    ) => prev_role == next_role,
                    _ => false,
                };

                if should_merge {
                    let prev = merged.pop().unwrap();
                    if let (
                        PromptAst::Message {
                            role,
                            content: prev_content,
                            metadata: prev_meta,
                        },
                        PromptAst::Message {
                            content: next_content,
                            ..
                        },
                    ) = (prev, msg)
                    {
                        let combined = match *prev_content {
                            PromptAst::Vec(mut items) => {
                                items.push(*next_content);
                                PromptAst::Vec(items)
                            }
                            other => PromptAst::Vec(vec![other, *next_content]),
                        };
                        merged.push(PromptAst::Message {
                            role,
                            content: Box::new(combined),
                            metadata: prev_meta,
                        });
                    }
                } else {
                    merged.push(msg);
                }
            }

            if merged.len() == 1 {
                merged.pop().unwrap()
            } else {
                PromptAst::Vec(merged)
            }
        }
        other => other,
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
pub(super) fn consolidate_system_prompts(prompt: PromptAst, features: &ModelFeatures) -> PromptAst {
    if !features.max_one_system_prompt {
        return prompt;
    }

    match prompt {
        PromptAst::Vec(messages) => {
            let total = messages.len();
            let mut seen_first_system = false;

            let transformed: Vec<PromptAst> = messages
                .into_iter()
                .map(|msg| match msg {
                    PromptAst::Message {
                        role,
                        content,
                        metadata,
                    } if role == "system" => {
                        if total == 1 {
                            return PromptAst::Message {
                                role: "user".to_string(),
                                content,
                                metadata,
                            };
                        }

                        if !seen_first_system {
                            seen_first_system = true;
                            PromptAst::Message {
                                role,
                                content,
                                metadata,
                            }
                        } else {
                            PromptAst::Message {
                                role: "user".to_string(),
                                content,
                                metadata,
                            }
                        }
                    }
                    other => other,
                })
                .collect();

            PromptAst::Vec(transformed)
        }
        PromptAst::Message {
            role,
            content,
            metadata,
        } if role == "system" => PromptAst::Message {
            role: "user".to_string(),
            content,
            metadata,
        },
        other => other,
    }
}

/// Filter metadata on messages based on allowed metadata configuration.
///
/// Walks all Message nodes and removes disallowed metadata keys.
///
/// Ported from: engine/baml-runtime/src/internal/llm_client/traits/mod.rs:110-128
pub(super) fn filter_metadata(prompt: PromptAst, features: &ModelFeatures) -> PromptAst {
    if matches!(features.allowed_metadata, AllowedMetadata::All) {
        return prompt;
    }

    filter_metadata_recursive(prompt, features)
}

fn filter_metadata_recursive(prompt: PromptAst, features: &ModelFeatures) -> PromptAst {
    match prompt {
        PromptAst::Message {
            role,
            content,
            metadata,
        } => {
            let filtered_metadata = filter_metadata_value(*metadata, features);
            PromptAst::Message {
                role,
                content: Box::new(filter_metadata_recursive(*content, features)),
                metadata: Box::new(filtered_metadata),
            }
        }
        PromptAst::Vec(items) => PromptAst::Vec(
            items
                .into_iter()
                .map(|item| filter_metadata_recursive(item, features))
                .collect(),
        ),
        other => other,
    }
}

fn filter_metadata_value(metadata: BexExternalValue, features: &ModelFeatures) -> BexExternalValue {
    match metadata {
        BexExternalValue::Map {
            key_type,
            value_type,
            entries,
        } => {
            let filtered: IndexMap<String, BexExternalValue> = entries
                .into_iter()
                .filter(|(key, _)| features.allowed_metadata.is_allowed(key))
                .collect();
            BexExternalValue::Map {
                key_type,
                value_type,
                entries: filtered,
            }
        }
        other => {
            if matches!(features.allowed_metadata, AllowedMetadata::None) {
                BexExternalValue::Null
            } else {
                other
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bex_external_types::{BexExternalValue, PromptAst};
    use indexmap::IndexMap;

    use super::*;
    use crate::{AllowedMetadata, LlmProvider, ModelFeatures};

    fn msg(role: &str, text: &str) -> PromptAst {
        PromptAst::Message {
            role: role.to_string(),
            content: Box::new(PromptAst::String(text.to_string())),
            metadata: Box::new(BexExternalValue::Null),
        }
    }

    fn msg_with_meta(role: &str, text: &str, meta: BexExternalValue) -> PromptAst {
        PromptAst::Message {
            role: role.to_string(),
            content: Box::new(PromptAst::String(text.to_string())),
            metadata: Box::new(meta),
        }
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
            BexExternalValue::Bool(false),
        );
        let features = ModelFeatures::for_provider(LlmProvider::Anthropic, &options);
        assert!(!features.max_one_system_prompt);
    }

    // ---- merge_adjacent_messages tests ----

    #[test]
    fn test_merge_adjacent_same_role() {
        let prompt = PromptAst::Vec(vec![msg("user", "Hello"), msg("user", "World")]);
        let result = merge_adjacent_messages(prompt);
        let expected = PromptAst::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::Vec(vec![
                PromptAst::String("Hello".to_string()),
                PromptAst::String("World".to_string()),
            ])),
            metadata: Box::new(BexExternalValue::Null),
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_no_merge_different_roles() {
        let prompt = PromptAst::Vec(vec![msg("system", "You are helpful"), msg("user", "Hello")]);
        let result = merge_adjacent_messages(prompt);
        let expected = PromptAst::Vec(vec![msg("system", "You are helpful"), msg("user", "Hello")]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_merge_three_same_role() {
        let prompt = PromptAst::Vec(vec![msg("user", "A"), msg("user", "B"), msg("user", "C")]);
        let result = merge_adjacent_messages(prompt);
        let expected = PromptAst::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::Vec(vec![
                PromptAst::String("A".to_string()),
                PromptAst::String("B".to_string()),
                PromptAst::String("C".to_string()),
            ])),
            metadata: Box::new(BexExternalValue::Null),
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn test_merge_passthrough_non_vec() {
        let prompt = msg("user", "Hello");
        let result = merge_adjacent_messages(prompt);
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
        let prompt = PromptAst::Vec(vec![
            msg("system", "First system"),
            msg("user", "Hello"),
            msg("system", "Second system"),
        ]);
        let result = consolidate_system_prompts(prompt, &features);
        let expected = PromptAst::Vec(vec![
            msg("system", "First system"),
            msg("user", "Hello"),
            msg("user", "Second system"),
        ]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_consolidate_noop_when_disabled() {
        let features = ModelFeatures {
            max_one_system_prompt: false,
            allowed_metadata: AllowedMetadata::All,
        };
        let prompt = PromptAst::Vec(vec![msg("system", "First"), msg("system", "Second")]);
        let result = consolidate_system_prompts(prompt, &features);
        let expected = PromptAst::Vec(vec![msg("system", "First"), msg("system", "Second")]);
        assert_eq!(result, expected);
    }

    // ---- filter_metadata tests ----

    #[test]
    fn test_filter_metadata_all_allowed() {
        let features = ModelFeatures {
            max_one_system_prompt: false,
            allowed_metadata: AllowedMetadata::All,
        };
        let mut meta_entries = IndexMap::new();
        meta_entries.insert("cache_control".to_string(), BexExternalValue::Bool(true));
        let meta = BexExternalValue::Map {
            key_type: bex_program::Ty::String,
            value_type: bex_program::Ty::Bool,
            entries: meta_entries.clone(),
        };
        let prompt = msg_with_meta("user", "Hello", meta);
        let result = filter_metadata(prompt, &features);
        let expected = msg_with_meta(
            "user",
            "Hello",
            BexExternalValue::Map {
                key_type: bex_program::Ty::String,
                value_type: bex_program::Ty::Bool,
                entries: meta_entries,
            },
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn test_filter_metadata_none_allowed() {
        let features = ModelFeatures {
            max_one_system_prompt: false,
            allowed_metadata: AllowedMetadata::None,
        };
        let mut meta_entries = IndexMap::new();
        meta_entries.insert("cache_control".to_string(), BexExternalValue::Bool(true));
        let meta = BexExternalValue::Map {
            key_type: bex_program::Ty::String,
            value_type: bex_program::Ty::Bool,
            entries: meta_entries,
        };
        let prompt = msg_with_meta("user", "Hello", meta);
        let result = filter_metadata(prompt, &features);
        let expected = msg_with_meta(
            "user",
            "Hello",
            BexExternalValue::Map {
                key_type: bex_program::Ty::String,
                value_type: bex_program::Ty::Bool,
                entries: IndexMap::new(),
            },
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn test_filter_metadata_only_specific() {
        let features = ModelFeatures {
            max_one_system_prompt: false,
            allowed_metadata: AllowedMetadata::Only(vec!["cache_control".to_string()]),
        };
        let mut meta_entries = IndexMap::new();
        meta_entries.insert("cache_control".to_string(), BexExternalValue::Bool(true));
        meta_entries.insert(
            "secret_field".to_string(),
            BexExternalValue::String("x".to_string()),
        );
        let meta = BexExternalValue::Map {
            key_type: bex_program::Ty::String,
            value_type: bex_program::Ty::String,
            entries: meta_entries,
        };
        let prompt = msg_with_meta("user", "Hello", meta);
        let result = filter_metadata(prompt, &features);
        let mut expected_entries = IndexMap::new();
        expected_entries.insert("cache_control".to_string(), BexExternalValue::Bool(true));
        let expected = msg_with_meta(
            "user",
            "Hello",
            BexExternalValue::Map {
                key_type: bex_program::Ty::String,
                value_type: bex_program::Ty::String,
                entries: expected_entries,
            },
        );
        assert_eq!(result, expected);
    }

    // ---- Integration: full specialize_prompt pipeline ----

    #[test]
    fn test_specialize_anthropic_prompt() {
        let prompt = PromptAst::Vec(vec![
            msg("system", "You are helpful"),
            msg("user", "Hello"),
            msg("user", "How are you?"),
            msg("system", "Also be concise"),
            msg("assistant", "I'm fine"),
        ]);

        let features = ModelFeatures::for_provider(LlmProvider::Anthropic, &IndexMap::new());

        let result = merge_adjacent_messages(prompt);
        let result = consolidate_system_prompts(result, &features);
        let result = filter_metadata(result, &features);

        let expected = PromptAst::Vec(vec![
            msg("system", "You are helpful"),
            PromptAst::Message {
                role: "user".to_string(),
                content: Box::new(PromptAst::Vec(vec![
                    PromptAst::String("Hello".to_string()),
                    PromptAst::String("How are you?".to_string()),
                ])),
                metadata: Box::new(BexExternalValue::Null),
            },
            msg("user", "Also be concise"),
            msg("assistant", "I'm fine"),
        ]);
        assert_eq!(result, expected);
    }
}
