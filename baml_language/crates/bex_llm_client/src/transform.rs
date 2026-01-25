//! Prompt AST transformation based on LLM client metadata.
//!
//! This module applies client-specific transformations to a `PromptAst`:
//! - Role remapping (e.g., "system" → "user" for clients without system support)
//! - Role validation against allowed_roles
//! - System message consolidation (max_one_system_prompt)
//! - Metadata filtering based on allowed_metadata

use bex_llm_types::{AllowedMetadata, PromptAst, PromptAstNode, ResolvedClient};

/// Apply client-specific transformations to a prompt AST.
///
/// This function transforms the prompt based on the client's configuration:
/// 1. Remaps roles according to `remap_roles`
/// 2. Validates roles against `allowed_roles` (if specified)
/// 3. Filters metadata according to `allowed_metadata`
/// 4. Consolidates system messages if `max_one_system_prompt` is true
///
/// Returns an error if a role is used that is not in `allowed_roles`.
pub fn apply_client(ast: PromptAst, client: &ResolvedClient) -> Result<PromptAst, TransformError> {
    // First pass: remap roles, validate, and filter metadata
    let ast = transform_node(ast, client)?;

    // Second pass: consolidate system messages if needed
    if client.features.max_one_system_prompt {
        consolidate_system_messages(ast)
    } else {
        Ok(ast)
    }
}

/// Errors that can occur during prompt transformation.
#[derive(Debug, Clone)]
pub enum TransformError {
    /// A role was used that is not allowed by the client.
    DisallowedRole {
        role: String,
        allowed: Vec<String>,
    },
}

impl std::fmt::Display for TransformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransformError::DisallowedRole { role, allowed } => {
                write!(
                    f,
                    "role '{}' is not allowed; allowed roles are: {:?}",
                    role, allowed
                )
            }
        }
    }
}

impl std::error::Error for TransformError {}

/// Transform a single AST node, applying role remapping, validation, and metadata filtering.
fn transform_node(ast: PromptAst, client: &ResolvedClient) -> Result<PromptAst, TransformError> {
    let node = match ast.node {
        PromptAstNode::Str(_) | PromptAstNode::Media(_) => ast.node,
        PromptAstNode::Message {
            role,
            content,
            metadata,
        } => {
            // 1. Apply role remapping
            let role = client
                .roles
                .remap_roles
                .get(&role)
                .cloned()
                .unwrap_or(role);

            // 2. Validate role if allowed_roles is specified
            if !client.roles.allowed_roles.is_empty()
                && !client.roles.allowed_roles.contains(&role)
            {
                return Err(TransformError::DisallowedRole {
                    role,
                    allowed: client.roles.allowed_roles.clone(),
                });
            }

            // 3. Filter metadata according to allowed_metadata
            let metadata = filter_metadata(metadata, &client.roles.allowed_metadata);

            // 4. Recursively transform content
            let content = Box::new(transform_node(*content, client)?);

            PromptAstNode::Message {
                role,
                content,
                metadata,
            }
        }
        PromptAstNode::Vec(nodes) => {
            let transformed: Result<Vec<_>, _> = nodes
                .into_iter()
                .map(|node| transform_node(node, client))
                .collect();
            PromptAstNode::Vec(transformed?)
        }
    };

    Ok(PromptAst {
        span: ast.span,
        node,
    })
}

/// Filter metadata according to the allowed_metadata configuration.
fn filter_metadata(
    metadata: serde_json::Map<String, serde_json::Value>,
    allowed: &AllowedMetadata,
) -> serde_json::Map<String, serde_json::Value> {
    match allowed {
        AllowedMetadata::All => metadata,
        AllowedMetadata::None => serde_json::Map::new(),
        AllowedMetadata::Only(keys) => metadata
            .into_iter()
            .filter(|(k, _)| keys.iter().any(|allowed_key| allowed_key == k))
            .collect(),
    }
}

/// Consolidate system messages when max_one_system_prompt is true.
///
/// Rules from engine/:
/// - If there is only one message and it's system, change it to user
/// - Otherwise, keep the first system message (if any) and change all other
///   system messages to user
fn consolidate_system_messages(ast: PromptAst) -> Result<PromptAst, TransformError> {
    let node = match ast.node {
        // For non-Vec nodes, just check if it's a lone system message
        PromptAstNode::Message { role, content, metadata } if role == "system" => {
            // Single system message → convert to user
            PromptAstNode::Message {
                role: "user".to_string(),
                content,
                metadata,
            }
        }
        PromptAstNode::Vec(nodes) => {
            // Check if all messages at top level
            let messages: Vec<_> = nodes
                .into_iter()
                .enumerate()
                .map(|(idx, node)| consolidate_system_in_vec(node, idx > 0))
                .collect();
            PromptAstNode::Vec(messages)
        }
        // Non-message nodes pass through unchanged
        other => other,
    };

    Ok(PromptAst {
        span: ast.span,
        node,
    })
}

/// Consolidate system messages within a Vec.
/// If `convert_system` is true, convert system messages to user messages.
fn consolidate_system_in_vec(ast: PromptAst, convert_system: bool) -> PromptAst {
    let node = match ast.node {
        PromptAstNode::Message {
            role,
            content,
            metadata,
        } => {
            let role = if convert_system && role == "system" {
                "user".to_string()
            } else {
                role
            };
            PromptAstNode::Message {
                role,
                content,
                metadata,
            }
        }
        // Recursively handle nested Vecs
        PromptAstNode::Vec(nodes) => {
            let messages: Vec<_> = nodes
                .into_iter()
                .enumerate()
                .map(|(idx, node)| {
                    // In nested vecs, all system messages should be converted
                    // since the "first" system message is at the top level
                    consolidate_system_in_vec(node, convert_system || idx > 0)
                })
                .collect();
            PromptAstNode::Vec(messages)
        }
        other => other,
    };

    PromptAst {
        span: ast.span,
        node,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bex_llm_types::{ModelFeatures, RoleConfig};
    use indexmap::IndexMap;
    use std::collections::HashMap;

    fn make_client(roles: RoleConfig, features: ModelFeatures) -> ResolvedClient {
        ResolvedClient {
            name: "test-client".to_string(),
            provider: "test".to_string(),
            roles,
            features,
            options: IndexMap::new(),
            request_config: Default::default(),
        }
    }

    fn make_message(role: &str, text: &str) -> PromptAst {
        PromptAst::without_span(PromptAstNode::Message {
            role: role.to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Str(text.to_string()))),
            metadata: serde_json::Map::new(),
        })
    }

    fn make_message_with_metadata(
        role: &str,
        text: &str,
        metadata: serde_json::Map<String, serde_json::Value>,
    ) -> PromptAst {
        PromptAst::without_span(PromptAstNode::Message {
            role: role.to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Str(text.to_string()))),
            metadata,
        })
    }

    #[test]
    fn test_role_remapping() {
        let mut remap = HashMap::new();
        remap.insert("system".to_string(), "user".to_string());

        let client = make_client(
            RoleConfig {
                remap_roles: remap,
                ..Default::default()
            },
            ModelFeatures::default(),
        );

        let ast = make_message("system", "Hello");
        let result = apply_client(ast, &client).unwrap();

        match result.node {
            PromptAstNode::Message { role, .. } => {
                assert_eq!(role, "user");
            }
            _ => panic!("expected Message"),
        }
    }

    #[test]
    fn test_disallowed_role() {
        let client = make_client(
            RoleConfig {
                allowed_roles: vec!["user".to_string()],
                ..Default::default()
            },
            ModelFeatures::default(),
        );

        let ast = make_message("system", "Hello");
        let result = apply_client(ast, &client);
        assert!(result.is_err());

        match result.unwrap_err() {
            TransformError::DisallowedRole { role, .. } => {
                assert_eq!(role, "system");
            }
        }
    }

    #[test]
    fn test_remap_then_validate() {
        // Remap system -> user, then validate against allowed_roles
        let mut remap = HashMap::new();
        remap.insert("system".to_string(), "user".to_string());

        let client = make_client(
            RoleConfig {
                allowed_roles: vec!["user".to_string()],
                remap_roles: remap,
                ..Default::default()
            },
            ModelFeatures::default(),
        );

        let ast = make_message("system", "Hello");
        let result = apply_client(ast, &client).unwrap();

        match result.node {
            PromptAstNode::Message { role, .. } => {
                assert_eq!(role, "user");
            }
            _ => panic!("expected Message"),
        }
    }

    #[test]
    fn test_metadata_filtering_none() {
        let mut metadata = serde_json::Map::new();
        metadata.insert("cache_control".to_string(), serde_json::json!({"type": "ephemeral"}));
        metadata.insert("other_key".to_string(), serde_json::json!("value"));

        let client = make_client(
            RoleConfig {
                allowed_metadata: AllowedMetadata::None,
                ..Default::default()
            },
            ModelFeatures::default(),
        );

        let ast = make_message_with_metadata("user", "Hello", metadata);
        let result = apply_client(ast, &client).unwrap();

        match result.node {
            PromptAstNode::Message { metadata, .. } => {
                assert!(metadata.is_empty());
            }
            _ => panic!("expected Message"),
        }
    }

    #[test]
    fn test_metadata_filtering_only() {
        let mut metadata = serde_json::Map::new();
        metadata.insert("cache_control".to_string(), serde_json::json!({"type": "ephemeral"}));
        metadata.insert("other_key".to_string(), serde_json::json!("value"));

        let client = make_client(
            RoleConfig {
                allowed_metadata: AllowedMetadata::Only(vec!["cache_control".to_string()]),
                ..Default::default()
            },
            ModelFeatures::default(),
        );

        let ast = make_message_with_metadata("user", "Hello", metadata);
        let result = apply_client(ast, &client).unwrap();

        match result.node {
            PromptAstNode::Message { metadata, .. } => {
                assert_eq!(metadata.len(), 1);
                assert!(metadata.contains_key("cache_control"));
            }
            _ => panic!("expected Message"),
        }
    }

    #[test]
    fn test_metadata_filtering_all() {
        let mut metadata = serde_json::Map::new();
        metadata.insert("cache_control".to_string(), serde_json::json!({"type": "ephemeral"}));
        metadata.insert("other_key".to_string(), serde_json::json!("value"));

        let client = make_client(
            RoleConfig {
                allowed_metadata: AllowedMetadata::All,
                ..Default::default()
            },
            ModelFeatures::default(),
        );

        let ast = make_message_with_metadata("user", "Hello", metadata);
        let result = apply_client(ast, &client).unwrap();

        match result.node {
            PromptAstNode::Message { metadata, .. } => {
                assert_eq!(metadata.len(), 2);
            }
            _ => panic!("expected Message"),
        }
    }

    #[test]
    fn test_max_one_system_single_system_message() {
        // Single system message should become user
        let client = make_client(
            RoleConfig::default(),
            ModelFeatures {
                max_one_system_prompt: true,
                ..Default::default()
            },
        );

        let ast = make_message("system", "Hello");
        let result = apply_client(ast, &client).unwrap();

        match result.node {
            PromptAstNode::Message { role, .. } => {
                assert_eq!(role, "user");
            }
            _ => panic!("expected Message"),
        }
    }

    #[test]
    fn test_max_one_system_keeps_first() {
        // In a vec, first system stays system, others become user
        let client = make_client(
            RoleConfig::default(),
            ModelFeatures {
                max_one_system_prompt: true,
                ..Default::default()
            },
        );

        let ast = PromptAst::without_span(PromptAstNode::Vec(vec![
            make_message("system", "System 1"),
            make_message("user", "User 1"),
            make_message("system", "System 2"),
            make_message("assistant", "Assistant 1"),
        ]));

        let result = apply_client(ast, &client).unwrap();

        match result.node {
            PromptAstNode::Vec(nodes) => {
                assert_eq!(nodes.len(), 4);

                // First message stays system
                match &nodes[0].node {
                    PromptAstNode::Message { role, .. } => assert_eq!(role, "system"),
                    _ => panic!("expected Message"),
                }

                // Second stays user
                match &nodes[1].node {
                    PromptAstNode::Message { role, .. } => assert_eq!(role, "user"),
                    _ => panic!("expected Message"),
                }

                // Third system becomes user
                match &nodes[2].node {
                    PromptAstNode::Message { role, .. } => assert_eq!(role, "user"),
                    _ => panic!("expected Message"),
                }

                // Fourth stays assistant
                match &nodes[3].node {
                    PromptAstNode::Message { role, .. } => assert_eq!(role, "assistant"),
                    _ => panic!("expected Message"),
                }
            }
            _ => panic!("expected Vec"),
        }
    }

    #[test]
    fn test_max_one_system_disabled() {
        // When disabled, multiple system messages are allowed
        let client = make_client(
            RoleConfig::default(),
            ModelFeatures {
                max_one_system_prompt: false,
                ..Default::default()
            },
        );

        let ast = PromptAst::without_span(PromptAstNode::Vec(vec![
            make_message("system", "System 1"),
            make_message("system", "System 2"),
        ]));

        let result = apply_client(ast, &client).unwrap();

        match result.node {
            PromptAstNode::Vec(nodes) => {
                // Both stay system
                match &nodes[0].node {
                    PromptAstNode::Message { role, .. } => assert_eq!(role, "system"),
                    _ => panic!("expected Message"),
                }
                match &nodes[1].node {
                    PromptAstNode::Message { role, .. } => assert_eq!(role, "system"),
                    _ => panic!("expected Message"),
                }
            }
            _ => panic!("expected Vec"),
        }
    }

    // =========================================================================
    // Nested content and edge case tests
    // =========================================================================

    #[test]
    fn test_nested_vec_messages() {
        // Test transformation with nested Vec of messages
        let client = make_client(
            RoleConfig::default(),
            ModelFeatures::default(),
        );

        let ast = PromptAst::without_span(PromptAstNode::Vec(vec![
            PromptAst::without_span(PromptAstNode::Vec(vec![
                make_message("system", "System prompt"),
                make_message("user", "User message 1"),
            ])),
            make_message("user", "User message 2"),
        ]));

        let result = apply_client(ast, &client).unwrap();

        match result.node {
            PromptAstNode::Vec(nodes) => {
                assert_eq!(nodes.len(), 2);
                // First element should be a Vec
                match &nodes[0].node {
                    PromptAstNode::Vec(inner) => {
                        assert_eq!(inner.len(), 2);
                    }
                    _ => panic!("expected nested Vec"),
                }
            }
            _ => panic!("expected Vec"),
        }
    }

    #[test]
    fn test_nested_vec_role_remapping() {
        // Test that role remapping works through nested structures
        let mut remap = HashMap::new();
        remap.insert("system".to_string(), "user".to_string());

        let client = make_client(
            RoleConfig {
                remap_roles: remap,
                ..Default::default()
            },
            ModelFeatures::default(),
        );

        let ast = PromptAst::without_span(PromptAstNode::Vec(vec![
            PromptAst::without_span(PromptAstNode::Vec(vec![
                make_message("system", "Nested system"),
            ])),
            make_message("system", "Top-level system"),
        ]));

        let result = apply_client(ast, &client).unwrap();

        // Both system messages should be remapped to user
        match result.node {
            PromptAstNode::Vec(nodes) => {
                match &nodes[0].node {
                    PromptAstNode::Vec(inner) => {
                        match &inner[0].node {
                            PromptAstNode::Message { role, .. } => assert_eq!(role, "user"),
                            _ => panic!("expected Message"),
                        }
                    }
                    _ => panic!("expected nested Vec"),
                }
                match &nodes[1].node {
                    PromptAstNode::Message { role, .. } => assert_eq!(role, "user"),
                    _ => panic!("expected Message"),
                }
            }
            _ => panic!("expected Vec"),
        }
    }

    #[test]
    fn test_nested_vec_max_one_system() {
        // Test max_one_system_prompt with nested structures
        let client = make_client(
            RoleConfig::default(),
            ModelFeatures {
                max_one_system_prompt: true,
                ..Default::default()
            },
        );

        let ast = PromptAst::without_span(PromptAstNode::Vec(vec![
            make_message("system", "First system - stays system"),
            PromptAst::without_span(PromptAstNode::Vec(vec![
                make_message("system", "Nested system - becomes user"),
                make_message("user", "Regular user"),
            ])),
        ]));

        let result = apply_client(ast, &client).unwrap();

        match result.node {
            PromptAstNode::Vec(nodes) => {
                // First message stays system
                match &nodes[0].node {
                    PromptAstNode::Message { role, .. } => assert_eq!(role, "system"),
                    _ => panic!("expected Message"),
                }
                // Nested system becomes user
                match &nodes[1].node {
                    PromptAstNode::Vec(inner) => {
                        match &inner[0].node {
                            PromptAstNode::Message { role, .. } => assert_eq!(role, "user"),
                            _ => panic!("expected Message"),
                        }
                    }
                    _ => panic!("expected Vec"),
                }
            }
            _ => panic!("expected Vec"),
        }
    }

    #[test]
    fn test_nested_metadata_filtering() {
        // Test that metadata filtering works through nested structures
        let mut metadata = serde_json::Map::new();
        metadata.insert("cache_control".to_string(), serde_json::json!({"type": "ephemeral"}));
        metadata.insert("should_remove".to_string(), serde_json::json!("value"));

        let client = make_client(
            RoleConfig {
                allowed_metadata: AllowedMetadata::Only(vec!["cache_control".to_string()]),
                ..Default::default()
            },
            ModelFeatures::default(),
        );

        let ast = PromptAst::without_span(PromptAstNode::Vec(vec![
            PromptAst::without_span(PromptAstNode::Vec(vec![
                make_message_with_metadata("user", "Nested message", metadata.clone()),
            ])),
        ]));

        let result = apply_client(ast, &client).unwrap();

        match result.node {
            PromptAstNode::Vec(nodes) => {
                match &nodes[0].node {
                    PromptAstNode::Vec(inner) => {
                        match &inner[0].node {
                            PromptAstNode::Message { metadata, .. } => {
                                assert_eq!(metadata.len(), 1);
                                assert!(metadata.contains_key("cache_control"));
                                assert!(!metadata.contains_key("should_remove"));
                            }
                            _ => panic!("expected Message"),
                        }
                    }
                    _ => panic!("expected Vec"),
                }
            }
            _ => panic!("expected Vec"),
        }
    }

    #[test]
    fn test_str_node_passthrough() {
        // Test that Str nodes pass through unchanged
        let client = make_client(
            RoleConfig::default(),
            ModelFeatures::default(),
        );

        let ast = PromptAst::without_span(PromptAstNode::Str("Hello, world!".to_string()));
        let result = apply_client(ast, &client).unwrap();

        match result.node {
            PromptAstNode::Str(s) => assert_eq!(s, "Hello, world!"),
            _ => panic!("expected Str"),
        }
    }

    #[test]
    fn test_media_node_passthrough() {
        use baml_base::MediaKind;
        use bex_vm_types::{MediaContent, MediaValue};

        // Test that Media nodes pass through unchanged
        let client = make_client(
            RoleConfig::default(),
            ModelFeatures::default(),
        );

        let ast = PromptAst::without_span(PromptAstNode::Media(MediaValue {
            kind: MediaKind::Image,
            content: MediaContent::Url {
                url: "https://example.com/image.png".to_string(),
                base64_data: None,
            },
            mime_type: Some("image/png".to_string()),
        }));

        let result = apply_client(ast, &client).unwrap();

        match result.node {
            PromptAstNode::Media(media) => {
                assert_eq!(media.kind, MediaKind::Image);
                match media.content {
                    MediaContent::Url { url, .. } => {
                        assert_eq!(url, "https://example.com/image.png");
                    }
                    _ => panic!("expected Url content"),
                }
            }
            _ => panic!("expected Media"),
        }
    }

    #[test]
    fn test_empty_allowed_roles_allows_all() {
        // Empty allowed_roles should allow any role
        let client = make_client(
            RoleConfig {
                allowed_roles: vec![], // Empty = all allowed
                ..Default::default()
            },
            ModelFeatures::default(),
        );

        // Various roles should all be allowed
        let ast = PromptAst::without_span(PromptAstNode::Vec(vec![
            make_message("system", "System"),
            make_message("user", "User"),
            make_message("assistant", "Assistant"),
            make_message("custom_role", "Custom"),
        ]));

        let result = apply_client(ast, &client);
        assert!(result.is_ok());
    }

    #[test]
    fn test_disallowed_role_error_message() {
        let client = make_client(
            RoleConfig {
                allowed_roles: vec!["user".to_string(), "assistant".to_string()],
                ..Default::default()
            },
            ModelFeatures::default(),
        );

        let ast = make_message("system", "Hello");
        let result = apply_client(ast, &client);

        match result {
            Err(TransformError::DisallowedRole { role, allowed }) => {
                assert_eq!(role, "system");
                assert!(allowed.contains(&"user".to_string()));
                assert!(allowed.contains(&"assistant".to_string()));
            }
            _ => panic!("expected DisallowedRole error"),
        }
    }

    #[test]
    fn test_transform_error_display() {
        let err = TransformError::DisallowedRole {
            role: "system".to_string(),
            allowed: vec!["user".to_string(), "assistant".to_string()],
        };
        let display = format!("{}", err);
        assert!(display.contains("system"));
        assert!(display.contains("not allowed"));
    }

    #[test]
    fn test_complex_remap_chain() {
        // Test multiple remappings
        let mut remap = HashMap::new();
        remap.insert("admin".to_string(), "system".to_string());
        remap.insert("system".to_string(), "user".to_string());

        let client = make_client(
            RoleConfig {
                remap_roles: remap,
                ..Default::default()
            },
            ModelFeatures::default(),
        );

        // "system" should be remapped to "user"
        let ast = make_message("system", "Hello");
        let result = apply_client(ast, &client).unwrap();

        match result.node {
            PromptAstNode::Message { role, .. } => assert_eq!(role, "user"),
            _ => panic!("expected Message"),
        }

        // "admin" should be remapped to "system" (remapping doesn't chain)
        let ast = make_message("admin", "Hello");
        let result = apply_client(ast, &client).unwrap();

        match result.node {
            PromptAstNode::Message { role, .. } => assert_eq!(role, "system"),
            _ => panic!("expected Message"),
        }
    }

    #[test]
    fn test_max_one_system_no_system_messages() {
        // Test max_one_system_prompt when there are no system messages
        let client = make_client(
            RoleConfig::default(),
            ModelFeatures {
                max_one_system_prompt: true,
                ..Default::default()
            },
        );

        let ast = PromptAst::without_span(PromptAstNode::Vec(vec![
            make_message("user", "User 1"),
            make_message("assistant", "Assistant 1"),
            make_message("user", "User 2"),
        ]));

        let result = apply_client(ast, &client).unwrap();

        match result.node {
            PromptAstNode::Vec(nodes) => {
                assert_eq!(nodes.len(), 3);
                // All roles should remain unchanged
                match &nodes[0].node {
                    PromptAstNode::Message { role, .. } => assert_eq!(role, "user"),
                    _ => panic!("expected Message"),
                }
                match &nodes[1].node {
                    PromptAstNode::Message { role, .. } => assert_eq!(role, "assistant"),
                    _ => panic!("expected Message"),
                }
                match &nodes[2].node {
                    PromptAstNode::Message { role, .. } => assert_eq!(role, "user"),
                    _ => panic!("expected Message"),
                }
            }
            _ => panic!("expected Vec"),
        }
    }

    #[test]
    fn test_vec_with_mixed_content_types() {
        // Test Vec containing messages and non-messages
        let client = make_client(
            RoleConfig::default(),
            ModelFeatures::default(),
        );

        let ast = PromptAst::without_span(PromptAstNode::Vec(vec![
            make_message("user", "User message"),
            PromptAst::without_span(PromptAstNode::Str("Bare string".to_string())),
        ]));

        let result = apply_client(ast, &client).unwrap();

        match result.node {
            PromptAstNode::Vec(nodes) => {
                assert_eq!(nodes.len(), 2);
                match &nodes[0].node {
                    PromptAstNode::Message { role, .. } => assert_eq!(role, "user"),
                    _ => panic!("expected Message"),
                }
                match &nodes[1].node {
                    PromptAstNode::Str(s) => assert_eq!(s, "Bare string"),
                    _ => panic!("expected Str"),
                }
            }
            _ => panic!("expected Vec"),
        }
    }

    #[test]
    fn test_deeply_nested_content_in_message() {
        // Test message with deeply nested content
        use baml_base::MediaKind;
        use bex_vm_types::{MediaContent, MediaValue};

        let client = make_client(
            RoleConfig::default(),
            ModelFeatures::default(),
        );

        // Message with nested Vec content containing text and media
        let ast = PromptAst::without_span(PromptAstNode::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::without_span(PromptAstNode::Vec(vec![
                PromptAst::without_span(PromptAstNode::Str("Text part".to_string())),
                PromptAst::without_span(PromptAstNode::Media(MediaValue {
                    kind: MediaKind::Image,
                    content: MediaContent::Url {
                        url: "https://example.com/image.png".to_string(),
                        base64_data: None,
                    },
                    mime_type: Some("image/png".to_string()),
                })),
            ]))),
            metadata: serde_json::Map::new(),
        });

        let result = apply_client(ast, &client).unwrap();

        match result.node {
            PromptAstNode::Message { role, content, .. } => {
                assert_eq!(role, "user");
                match content.node {
                    PromptAstNode::Vec(parts) => {
                        assert_eq!(parts.len(), 2);
                        match &parts[0].node {
                            PromptAstNode::Str(s) => assert_eq!(s, "Text part"),
                            _ => panic!("expected Str"),
                        }
                        match &parts[1].node {
                            PromptAstNode::Media(_) => {}
                            _ => panic!("expected Media"),
                        }
                    }
                    _ => panic!("expected Vec content"),
                }
            }
            _ => panic!("expected Message"),
        }
    }
}
