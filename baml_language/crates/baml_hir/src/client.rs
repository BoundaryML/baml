//! Client validation logic for HIR lowering.
//!
//! This module contains validation for client definitions, including:
//! - `client_response_type` validation
//! - `http` configuration block validation

use baml_base::Name;
use baml_diagnostics::HirDiagnostic;
use baml_syntax::ast::{ClientDef, ConfigBlock};
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
    node: &baml_syntax::SyntaxNode,
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
    let provider_item = client_def.config_block().and_then(|block| {
        block
            .items()
            .find(|item| item.key().map(|k| k.text() == "provider").unwrap_or(false))
    });

    let provider = provider_item
        .as_ref()
        .and_then(baml_syntax::ast::ConfigItem::value_word)
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

    // Validate config block fields
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

        // Find the options block for further validation
        if let Some(options_item) = config_block
            .items()
            .find(|item| item.key().map(|k| k.text() == "options").unwrap_or(false))
        {
            if let Some(options_block) = options_item.nested_block() {
                // Validate client_response_type
                validate_client_response_type(ctx, &client_name, &options_block);

                // Validate http block
                validate_http_block(ctx, &client_name, &options_block, is_composite);
            }
        }
    }

    Some(Client { name, provider })
}

/// Validate `client_response_type` field.
fn validate_client_response_type(
    ctx: &mut LoweringContext,
    client_name: &str,
    options_block: &ConfigBlock,
) {
    if let Some(response_type_item) = options_block.items().find(|item| {
        item.key()
            .map(|k| k.text() == "client_response_type")
            .unwrap_or(false)
    }) {
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
    if let Some(http_item) = options_block
        .items()
        .find(|item| item.key().map(|k| k.text() == "http").unwrap_or(false))
    {
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
