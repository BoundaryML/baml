//! Client validation logic for HIR lowering.
//!
//! This module contains validation for client definitions, including:
//! - `client_response_type` validation
//! - `http` configuration block validation

use baml_base::Name;
use baml_compiler_diagnostics::HirDiagnostic;
use baml_compiler_syntax::ast::{ClientDef, ConfigBlock};
use rowan::ast::AstNode;

use crate::{LoweringContext, item_tree::Client};

/// Valid values for `client_response_type`.
pub(crate) const VALID_RESPONSE_TYPES: &[&str] = &[
    "openai",
    "openai-responses",
    "anthropic",
    "google",
    "vertex",
    "openrouter",
];

/// Valid http config fields for regular (non-composite) clients.
pub(crate) const REGULAR_HTTP_FIELDS: &[&str] = &[
    "connect_timeout_ms",
    "request_timeout_ms",
    "time_to_first_token_timeout_ms",
    "idle_timeout_ms",
];

/// Valid http config fields for composite clients (fallback/round-robin).
pub(crate) const COMPOSITE_HTTP_FIELDS: &[&str] = &["total_timeout_ms"];

/// Composite client providers.
pub(crate) const COMPOSITE_PROVIDERS: &[&str] = &["fallback", "round-robin"];

/// Extract client configuration from CST with validation.
pub(crate) fn lower_client(
    node: &baml_compiler_syntax::SyntaxNode,
    ctx: &mut LoweringContext,
) -> Option<Client> {
    let client_def = ClientDef::cast(node.clone())?;

    // Extract name using AST accessor
    let name = client_def
        .name()
        .map(|t| Name::new(t.text()))
        .unwrap_or_else(|| Name::new("UnnamedClient"));
    let client_name = name.to_string();

    // Extract provider from config block using AST accessors
    let provider_item = client_def
        .config_block()
        .and_then(|block| block.items().find(|item| item.matches_key("provider")));

    let provider = provider_item
        .as_ref()
        .and_then(baml_compiler_syntax::ast::ConfigItem::value_word)
        .map(|t| Name::new(t.text()))
        .unwrap_or_else(|| Name::new("unknown"));

    // Validate that provider field exists
    if provider_item.is_none() {
        // Get the span for the entire client definition
        ctx.push_diagnostic(HirDiagnostic::MissingProvider {
            client_name: client_name.clone(),
            span: ctx.span(node.text_range()),
        });
    }

    let is_composite = COMPOSITE_PROVIDERS.contains(&provider.as_str());

    // Extract and validate config block fields
    let mut default_role: Option<String> = None;
    let mut allowed_roles: Vec<String> = Vec::new();

    if let Some(config_block) = client_def.config_block() {
        // Check for unknown properties in client config block
        for item in config_block.items() {
            if let Some(key) = item.key() {
                let key_text = key.text();
                if key_text != "provider" && key_text != "options" {
                    ctx.push_diagnostic(HirDiagnostic::UnknownClientProperty {
                        client_name: client_name.clone(),
                        field_name: key_text.to_string(),
                        span: ctx.span(key.text_range()),
                    });
                }
            }
        }

        // Find the options block for further validation and extraction
        if let Some(options_item) = config_block
            .items()
            .find(|item| item.matches_key("options"))
        {
            if let Some(options_block) = options_item.nested_block() {
                // Validate client_response_type
                validate_client_response_type(ctx, &client_name, &options_block);

                // Validate http block
                validate_http_block(ctx, &client_name, &options_block, is_composite);

                // Validate allowed_roles and remap_roles
                validate_roles(ctx, &client_name, &options_block);

                // Extract default_role if present
                if let Some(role_item) = options_block
                    .items()
                    .find(|item| item.matches_key("default_role"))
                {
                    default_role = role_item.value_str();
                }

                // Extract allowed_roles if present
                if let Some(roles_item) = options_block
                    .items()
                    .find(|item| item.matches_key("allowed_roles"))
                {
                    if let Some(elements) = roles_item.array_string_elements() {
                        allowed_roles = elements.into_iter().filter_map(|(s, _)| s).collect();
                    }
                }
            }
        }
    }

    // Use default allowed_roles if none specified
    if allowed_roles.is_empty() {
        allowed_roles = DEFAULT_ALLOWED_ROLES
            .iter()
            .map(|s| (*s).to_string())
            .collect();
    }

    Some(Client {
        name,
        provider,
        default_role,
        allowed_roles,
    })
}

/// Validate `client_response_type` field.
fn validate_client_response_type(
    ctx: &mut LoweringContext,
    client_name: &str,
    options_block: &ConfigBlock,
) {
    if let Some(response_type_item) = options_block
        .items()
        .find(|item| item.matches_key("client_response_type"))
    {
        if let Some(value) = response_type_item.value_str() {
            if !VALID_RESPONSE_TYPES.contains(&value.as_str()) {
                if let Some(value_range) = response_type_item.value_text_range() {
                    ctx.push_diagnostic(HirDiagnostic::InvalidClientResponseType {
                        client_name: client_name.to_string(),
                        value,
                        span: ctx.span(value_range),
                        valid_values: VALID_RESPONSE_TYPES.to_vec(),
                    });
                }
            }
        }
    }
}

/// Validate http configuration block.
fn validate_http_block(
    ctx: &mut LoweringContext,
    client_name: &str,
    options_block: &ConfigBlock,
    is_composite: bool,
) {
    if let Some(http_item) = options_block.items().find(|item| item.matches_key("http")) {
        // Check if http has a nested block or a scalar value
        if let Some(http_block) = http_item.nested_block() {
            // Validate http config fields
            validate_http_config_fields(ctx, client_name, &http_block, is_composite);
        } else if http_item.has_value() {
            // http is a scalar value, not a block - this is an error
            if let Some(key_token) = http_item.key() {
                ctx.push_diagnostic(HirDiagnostic::HttpConfigNotBlock {
                    client_name: client_name.to_string(),
                    span: ctx.span(key_token.text_range()),
                });
            }
        }
    }
}

/// Validate http configuration block fields.
fn validate_http_config_fields(
    ctx: &mut LoweringContext,
    client_name: &str,
    http_block: &ConfigBlock,
    is_composite: bool,
) {
    let valid_fields = if is_composite {
        COMPOSITE_HTTP_FIELDS
    } else {
        REGULAR_HTTP_FIELDS
    };

    for item in http_block.items() {
        let Some(key_token) = item.key() else {
            continue;
        };
        let field_name = key_token.text();
        let field_span = ctx.span(key_token.text_range());

        // Check if field is valid
        if !valid_fields.contains(&field_name) {
            // For composite clients, check if they're trying to use regular fields
            // For regular clients, check if they're trying to use composite fields
            let suggestion = find_similar_field(field_name, valid_fields);

            ctx.push_diagnostic(HirDiagnostic::UnknownHttpConfigField {
                client_name: client_name.to_string(),
                field_name: field_name.to_string(),
                span: field_span,
                suggestion,
                is_composite,
            });
            continue;
        }

        // Validate timeout values are non-negative
        if field_name.ends_with("_ms") {
            // Check if negative (has minus sign)
            if item.is_negative() {
                if let Some(value) = item.value_int() {
                    ctx.push_diagnostic(HirDiagnostic::NegativeTimeout {
                        client_name: client_name.to_string(),
                        field_name: field_name.to_string(),
                        value: -value.abs(), // Make sure it's negative for display
                        span: field_span,
                    });
                }
            }
        }
    }
}

/// Default allowed roles when none are specified.
const DEFAULT_ALLOWED_ROLES: &[&str] = &["user", "assistant", "system"];

/// Validate `allowed_roles` and `remap_roles` fields.
fn validate_roles(ctx: &mut LoweringContext, client_name: &str, options_block: &ConfigBlock) {
    // First, extract and validate allowed_roles
    let allowed_roles = validate_allowed_roles(ctx, client_name, options_block);

    // Then validate remap_roles against the allowed_roles
    validate_remap_roles(ctx, client_name, options_block, &allowed_roles);
}

/// Validate `allowed_roles` field and return the list of allowed roles.
fn validate_allowed_roles(
    ctx: &mut LoweringContext,
    client_name: &str,
    options_block: &ConfigBlock,
) -> Vec<String> {
    let Some(allowed_roles_item) = options_block
        .items()
        .find(|item| item.matches_key("allowed_roles"))
    else {
        // No allowed_roles specified, use defaults
        return DEFAULT_ALLOWED_ROLES
            .iter()
            .map(|s| (*s).to_string())
            .collect();
    };

    // Check if it's an array
    if !allowed_roles_item.is_array() {
        // Not an array - this is an error, but we'll handle it gracefully
        if let Some(value_range) = allowed_roles_item.value_text_range() {
            ctx.push_diagnostic(HirDiagnostic::AllowedRoleNotString {
                client_name: client_name.to_string(),
                span: ctx.span(value_range),
            });
        }
        return DEFAULT_ALLOWED_ROLES
            .iter()
            .map(|s| (*s).to_string())
            .collect();
    }

    let Some(elements) = allowed_roles_item.array_string_elements() else {
        return DEFAULT_ALLOWED_ROLES
            .iter()
            .map(|s| (*s).to_string())
            .collect();
    };

    // Check for empty array
    if elements.is_empty() {
        if let Some(array_node) = allowed_roles_item.array_node() {
            ctx.push_diagnostic(HirDiagnostic::AllowedRolesEmpty {
                client_name: client_name.to_string(),
                span: ctx.span(array_node.text_range()),
            });
        }
        return vec![];
    }

    // Validate each element is a string literal and collect them
    let mut allowed_roles = Vec::new();
    for (value, range) in elements {
        match value {
            Some(s) => {
                allowed_roles.push(s);
            }
            None => {
                // Not a string literal
                ctx.push_diagnostic(HirDiagnostic::AllowedRoleNotString {
                    client_name: client_name.to_string(),
                    span: ctx.span(range),
                });
            }
        }
    }

    allowed_roles
}

/// Validate `remap_roles` field.
fn validate_remap_roles(
    ctx: &mut LoweringContext,
    client_name: &str,
    options_block: &ConfigBlock,
    allowed_roles: &[String],
) {
    // If allowed_roles is empty, we've already emitted AllowedRolesEmpty.
    // Skip validation here - the user needs to fix allowed_roles first.
    if allowed_roles.is_empty() {
        return;
    }

    let Some(remap_roles_item) = options_block
        .items()
        .find(|item| item.matches_key("remap_roles"))
    else {
        return;
    };

    // remap_roles must be a map/block
    let Some(remap_block) = remap_roles_item.nested_block() else {
        // Not a block - check if it has a scalar value
        if remap_roles_item.has_value() {
            if let Some(value_range) = remap_roles_item.value_text_range() {
                // Determine the type of the value
                let actual_type = if remap_roles_item.value_str().is_some() {
                    "string"
                } else if remap_roles_item.value_int().is_some() {
                    "number"
                } else {
                    "non-map"
                };
                ctx.push_diagnostic(HirDiagnostic::RemapRolesNotMap {
                    client_name: client_name.to_string(),
                    actual_type: actual_type.to_string(),
                    span: ctx.span(value_range),
                });
            }
        }
        return;
    };

    // Validate each key-value pair in the remap_roles block
    for item in remap_block.items() {
        let Some(key_token) = item.key() else {
            continue;
        };
        let role_key = key_token.text();
        let key_span = ctx.span(key_token.text_range());

        // Check if the key is in allowed_roles
        if !allowed_roles.iter().any(|r| r == role_key) {
            ctx.push_diagnostic(HirDiagnostic::RemapRoleNotAllowed {
                client_name: client_name.to_string(),
                role_key: role_key.to_string(),
                allowed_roles: allowed_roles.to_vec(),
                span: key_span,
            });
            continue;
        }

        // Check if the value is a string
        if let Some(value_range) = item.value_text_range() {
            // Check if it's a number or nested block (not a string)
            if item.value_int().is_some() || item.nested_block().is_some() {
                ctx.push_diagnostic(HirDiagnostic::RemapRoleValueNotString {
                    client_name: client_name.to_string(),
                    span: ctx.span(value_range),
                });
            }
        }
    }
}

/// Find a similar field name for suggestions using edit distance.
fn find_similar_field(field_name: &str, valid_fields: &[&str]) -> Option<String> {
    // If there's only one valid field, always suggest it
    if valid_fields.len() == 1 {
        return Some(valid_fields[0].to_string());
    }

    let mut best_match: Option<(&str, usize)> = None;

    for valid in valid_fields {
        let distance = edit_distance(field_name, valid);
        // Only suggest if edit distance is at most 3 (for reasonable typos)
        if distance <= 3 {
            if best_match.is_none() || distance < best_match.unwrap().1 {
                best_match = Some((valid, distance));
            }
        }
    }

    best_match.map(|(s, _)| s.to_string())
}

/// Compute Levenshtein edit distance between two strings.
fn edit_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    // Use two rows for space efficiency
    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = usize::from(a_chars[i - 1] != b_chars[j - 1]);
            curr[j] = (prev[j] + 1) // deletion
                .min(curr[j - 1] + 1) // insertion
                .min(prev[j - 1] + cost); // substitution
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}
