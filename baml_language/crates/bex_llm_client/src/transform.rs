//! Prompt AST transformation based on LLM client metadata.
//!
//! This module applies client-specific transformations to a `PromptAst`:
//! - Role remapping (e.g., "system" → "user" for clients without system support)
//! - Role validation against allowed_roles
//! - System message consolidation (max_one_system_prompt)
//!
//! Note: Metadata filtering is not currently supported for VM-native PromptAst
//! since metadata is stored as `Value::Null` during rendering.

use bex_llm_types::ResolvedClient;
use bex_vm_types::PromptAst;

/// Apply client-specific transformations to a prompt AST.
///
/// This function transforms the prompt based on the client's configuration:
/// 1. Remaps roles according to `remap_roles`
/// 2. Validates roles against `allowed_roles` (if specified)
/// 3. Consolidates system messages if `max_one_system_prompt` is true
///
/// Returns an error if a role is used that is not in `allowed_roles`.
pub fn specialize_prompt(ast: PromptAst, client: &ResolvedClient) -> Result<PromptAst, TransformError> {
    // First pass: remap roles and validate
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

/// Transform a single AST node, applying role remapping and validation.
fn transform_node(ast: PromptAst, client: &ResolvedClient) -> Result<PromptAst, TransformError> {
    match ast {
        PromptAst::String(_) | PromptAst::Media(_) | PromptAst::PrintType { .. } => Ok(ast),
        PromptAst::Message {
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

            // 3. Recursively transform content
            let content = Box::new(transform_node(*content, client)?);

            // Note: metadata filtering is not implemented for VM Value type
            // The metadata is typically Value::Null anyway

            Ok(PromptAst::Message {
                role,
                content,
                metadata,
            })
        }
        PromptAst::Vec(nodes) => {
            let transformed: Result<Vec<_>, _> = nodes
                .into_iter()
                .map(|node| transform_node(node, client))
                .collect();
            Ok(PromptAst::Vec(transformed?))
        }
    }
}

/// Consolidate system messages when max_one_system_prompt is true.
///
/// Rules from engine/:
/// - If there is only one message and it's system, change it to user
/// - Otherwise, keep the first system message (if any) and change all other
///   system messages to user
fn consolidate_system_messages(ast: PromptAst) -> Result<PromptAst, TransformError> {
    match ast {
        // For non-Vec nodes, just check if it's a lone system message
        PromptAst::Message { role, content, metadata } if role == "system" => {
            // Single system message → convert to user
            Ok(PromptAst::Message {
                role: "user".to_string(),
                content,
                metadata,
            })
        }
        PromptAst::Vec(nodes) => {
            // Check if all messages at top level
            let messages: Vec<_> = nodes
                .into_iter()
                .enumerate()
                .map(|(idx, node)| consolidate_system_in_vec(node, idx > 0))
                .collect();
            Ok(PromptAst::Vec(messages))
        }
        // Non-message nodes pass through unchanged
        other => Ok(other),
    }
}

/// Consolidate system messages within a Vec.
/// If `convert_system` is true, convert system messages to user messages.
fn consolidate_system_in_vec(ast: PromptAst, convert_system: bool) -> PromptAst {
    match ast {
        PromptAst::Message {
            role,
            content,
            metadata,
        } => {
            let role = if convert_system && role == "system" {
                "user".to_string()
            } else {
                role
            };
            PromptAst::Message {
                role,
                content,
                metadata,
            }
        }
        // Recursively handle nested Vecs
        PromptAst::Vec(nodes) => {
            let messages: Vec<_> = nodes
                .into_iter()
                .enumerate()
                .map(|(idx, node)| {
                    // In nested vecs, all system messages should be converted
                    // since the "first" system message is at the top level
                    consolidate_system_in_vec(node, convert_system || idx > 0)
                })
                .collect();
            PromptAst::Vec(messages)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bex_llm_types::{ModelFeatures, RoleConfig};
    use bex_vm_types::Value;
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
        PromptAst::Message {
            role: role.to_string(),
            content: Box::new(PromptAst::String(text.to_string())),
            metadata: Value::Null,
        }
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
        let result = specialize_prompt(ast, &client).unwrap();

        match result {
            PromptAst::Message { role, .. } => {
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
        let result = specialize_prompt(ast, &client);
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
        let result = specialize_prompt(ast, &client).unwrap();

        match result {
            PromptAst::Message { role, .. } => {
                assert_eq!(role, "user");
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
        let result = specialize_prompt(ast, &client).unwrap();

        match result {
            PromptAst::Message { role, .. } => {
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

        let ast = PromptAst::Vec(vec![
            make_message("system", "System 1"),
            make_message("user", "User 1"),
            make_message("system", "System 2"),
            make_message("assistant", "Assistant 1"),
        ]);

        let result = specialize_prompt(ast, &client).unwrap();

        match result {
            PromptAst::Vec(nodes) => {
                assert_eq!(nodes.len(), 4);

                // First message stays system
                match &nodes[0] {
                    PromptAst::Message { role, .. } => assert_eq!(role, "system"),
                    _ => panic!("expected Message"),
                }

                // Second stays user
                match &nodes[1] {
                    PromptAst::Message { role, .. } => assert_eq!(role, "user"),
                    _ => panic!("expected Message"),
                }

                // Third system becomes user
                match &nodes[2] {
                    PromptAst::Message { role, .. } => assert_eq!(role, "user"),
                    _ => panic!("expected Message"),
                }

                // Fourth stays assistant
                match &nodes[3] {
                    PromptAst::Message { role, .. } => assert_eq!(role, "assistant"),
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

        let ast = PromptAst::Vec(vec![
            make_message("system", "System 1"),
            make_message("system", "System 2"),
        ]);

        let result = specialize_prompt(ast, &client).unwrap();

        match result {
            PromptAst::Vec(nodes) => {
                // Both stay system
                match &nodes[0] {
                    PromptAst::Message { role, .. } => assert_eq!(role, "system"),
                    _ => panic!("expected Message"),
                }
                match &nodes[1] {
                    PromptAst::Message { role, .. } => assert_eq!(role, "system"),
                    _ => panic!("expected Message"),
                }
            }
            _ => panic!("expected Vec"),
        }
    }

    #[test]
    fn test_str_node_passthrough() {
        // Test that String nodes pass through unchanged
        let client = make_client(
            RoleConfig::default(),
            ModelFeatures::default(),
        );

        let ast = PromptAst::String("Hello, world!".to_string());
        let result = specialize_prompt(ast, &client).unwrap();

        match result {
            PromptAst::String(s) => assert_eq!(s, "Hello, world!"),
            _ => panic!("expected String"),
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
        let ast = PromptAst::Vec(vec![
            make_message("system", "System"),
            make_message("user", "User"),
            make_message("assistant", "Assistant"),
            make_message("custom_role", "Custom"),
        ]);

        let result = specialize_prompt(ast, &client);
        assert!(result.is_ok());
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
}
