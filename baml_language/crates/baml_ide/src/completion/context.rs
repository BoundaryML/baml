//! Cursor context detection for completions.
//!
//! This module analyzes the syntax tree and text around the cursor
//! to determine what kind of completions are appropriate.

use baml_db::baml_compiler_syntax::{SyntaxKind, SyntaxNode, SyntaxToken, TokenAtOffset};
use text_size::TextSize;

/// The context at the cursor position for completions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionContext {
    /// At file top-level (suggest: function, class, enum, etc.)
    TopLevel,

    /// In type position (suggest: primitives, classes, enums)
    TypeAnnotation {
        /// Partial type name being typed
        partial: Option<String>,
    },

    /// After a dot in expression (suggest: fields, methods)
    FieldAccess {
        /// The text before the dot
        base_text: String,
    },

    /// After `@` for field attributes
    FieldAttribute {
        /// Partial attribute name
        partial: Option<String>,
    },

    /// After `@@` for block attributes
    BlockAttribute {
        /// Partial attribute name
        partial: Option<String>,
    },

    /// Inside function body - prompt template context
    PromptTemplate {
        /// Whether we're inside {{ ... }} interpolation
        in_interpolation: bool,
    },

    /// Inside client/generator config block
    ConfigBlock {
        /// The type of config block
        block_type: ConfigBlockType,
    },

    /// After `_` in prompt (role helpers)
    PromptUnderscore,

    /// After `ctx` in prompt
    PromptContext {
        /// Path segments after ctx (e.g., `["client"]` for `ctx.client.`)
        partial_path: Vec<String>,
    },

    /// General expression position
    Expression,

    /// Unknown/unsupported context
    Unknown,
}

/// Type of config block for context-specific completions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigBlockType {
    Client,
    Generator,
    RetryPolicy,
    Test,
    Unknown,
}

/// Detect the completion context at the given offset.
pub(super) fn detect_context(root: &SyntaxNode, offset: TextSize, text: &str) -> CompletionContext {
    // First check for special prefix patterns in the text
    if let Some(ctx) = check_special_prefix(text, offset) {
        return ctx;
    }

    // Try to find a token at the offset
    let token = match root.token_at_offset(offset) {
        TokenAtOffset::None => {
            // No token at offset - check if we're at the end of the file
            return if is_at_file_level(root, text, offset) {
                CompletionContext::TopLevel
            } else {
                CompletionContext::Unknown
            };
        }
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::Between(left, right) => {
            // Prefer the right token (what we're about to type)
            // unless it's whitespace/trivia
            if right.kind().is_trivia() {
                left
            } else {
                right
            }
        }
    };

    // Check the character immediately before the cursor
    if let Some(ctx) = check_char_before_cursor(text, offset) {
        return ctx;
    }

    // Walk up the syntax tree to find context
    detect_from_ancestors(&token, text, offset)
}

/// Check for special prefix patterns like `_.`, `ctx.`, etc.
fn check_special_prefix(text: &str, offset: TextSize) -> Option<CompletionContext> {
    let offset_usize: usize = offset.into();
    if offset_usize > text.len() {
        return None;
    }

    // Get up to 20 chars before cursor for pattern matching
    let start = offset_usize.saturating_sub(20);
    let prefix = &text[start..offset_usize];

    // Check for ctx.client. pattern
    if prefix.ends_with("ctx.client.") {
        return Some(CompletionContext::PromptContext {
            partial_path: vec!["client".to_string()],
        });
    }

    // Check for ctx. pattern
    if prefix.ends_with("ctx.") {
        return Some(CompletionContext::PromptContext {
            partial_path: vec![],
        });
    }

    // Check for _. pattern (role helpers)
    if prefix.ends_with("_.") {
        return Some(CompletionContext::PromptUnderscore);
    }

    // Check for @@ (block attribute)
    if prefix.ends_with("@@") {
        return Some(CompletionContext::BlockAttribute { partial: None });
    }

    // Check for @ but not @@ (field attribute)
    if prefix.ends_with('@') && !prefix.ends_with("@@") {
        return Some(CompletionContext::FieldAttribute { partial: None });
    }

    None
}

/// Check the character immediately before the cursor for context hints.
fn check_char_before_cursor(text: &str, offset: TextSize) -> Option<CompletionContext> {
    let offset_usize: usize = offset.into();
    if offset_usize == 0 || offset_usize > text.len() {
        return None;
    }

    // Get character before cursor (handle UTF-8 properly)
    let before = &text[..offset_usize];
    let last_char = before.chars().last()?;

    match last_char {
        '.' => {
            // Could be field access - try to get the base
            if let Some(base) = get_base_before_dot(before) {
                return Some(CompletionContext::FieldAccess { base_text: base });
            }
        }
        ':' => {
            // Could be after type annotation colon
            // Check if there's another colon (::) or just one (:)
            let second_last = before.chars().rev().nth(1);
            if second_last != Some(':') {
                // Single colon - likely type annotation position
                return Some(CompletionContext::TypeAnnotation { partial: None });
            }
        }
        _ => {}
    }

    None
}

/// Extract the identifier before a dot for field access.
fn get_base_before_dot(text_before_dot: &str) -> Option<String> {
    // Remove the trailing dot if present
    let text = text_before_dot.trim_end_matches('.');

    // Scan backward to find the start of the identifier
    let mut end = text.len();
    let bytes = text.as_bytes();

    // Find where the identifier/path ends
    while end > 0 {
        let b = bytes[end - 1];
        if b.is_ascii_alphanumeric() || b == b'_' || b == b'.' {
            end -= 1;
        } else {
            break;
        }
    }

    let base = &text[end..];
    if base.is_empty() {
        None
    } else {
        Some(base.to_string())
    }
}

/// Check if offset is at file level (not inside any block).
fn is_at_file_level(_root: &SyntaxNode, text: &str, offset: TextSize) -> bool {
    let offset_usize: usize = offset.into();

    // Simple heuristic: count brace depth
    let text_before = &text[..offset_usize.min(text.len())];
    let open_braces = text_before.matches('{').count();
    let close_braces = text_before.matches('}').count();

    open_braces <= close_braces
}

/// Detect context by walking up the syntax tree from the token.
fn detect_from_ancestors(token: &SyntaxToken, text: &str, offset: TextSize) -> CompletionContext {
    for ancestor in token.parent_ancestors() {
        match ancestor.kind() {
            // Top-level definitions
            SyntaxKind::SOURCE_FILE => {
                return CompletionContext::TopLevel;
            }

            // Type expression context
            SyntaxKind::TYPE_EXPR
            | SyntaxKind::UNION_TYPE
            | SyntaxKind::OPTIONAL_TYPE
            | SyntaxKind::ARRAY_TYPE
            | SyntaxKind::MAP_TYPE => {
                let partial = get_partial_word_at(text, offset);
                return CompletionContext::TypeAnnotation { partial };
            }

            // Attribute contexts
            SyntaxKind::ATTRIBUTE => {
                let partial = get_partial_word_at(text, offset);
                return CompletionContext::FieldAttribute { partial };
            }
            SyntaxKind::BLOCK_ATTRIBUTE => {
                let partial = get_partial_word_at(text, offset);
                return CompletionContext::BlockAttribute { partial };
            }

            // Template/prompt contexts
            SyntaxKind::TEMPLATE_INTERPOLATION => {
                return CompletionContext::PromptTemplate {
                    in_interpolation: true,
                };
            }
            SyntaxKind::TEMPLATE_CONTENT | SyntaxKind::RAW_STRING_LITERAL => {
                return CompletionContext::PromptTemplate {
                    in_interpolation: false,
                };
            }

            // Config blocks
            SyntaxKind::CONFIG_BLOCK | SyntaxKind::CONFIG_ITEM => {
                let block_type = detect_config_block_type(&ancestor);
                return CompletionContext::ConfigBlock { block_type };
            }

            // Expression contexts
            SyntaxKind::FIELD_ACCESS_EXPR => {
                // We're in a field access - get the base
                if let Some(base) = extract_field_access_base(&ancestor) {
                    return CompletionContext::FieldAccess { base_text: base };
                }
            }
            SyntaxKind::PATH_EXPR => {
                // Could be a path expression - check if it looks like field access
                let path_text = ancestor.text().to_string();
                if path_text.contains('.') {
                    // Has a dot - might be typing after the dot
                    if let Some(pos) = path_text.rfind('.') {
                        let base = &path_text[..pos];
                        if !base.is_empty() {
                            return CompletionContext::FieldAccess {
                                base_text: base.to_string(),
                            };
                        }
                    }
                }
            }

            // General expression position
            SyntaxKind::EXPR
            | SyntaxKind::BINARY_EXPR
            | SyntaxKind::CALL_EXPR
            | SyntaxKind::CALL_ARGS => {
                return CompletionContext::Expression;
            }

            // Inside various definition bodies
            SyntaxKind::LLM_FUNCTION_BODY | SyntaxKind::PROMPT_FIELD => {
                return CompletionContext::PromptTemplate {
                    in_interpolation: false,
                };
            }
            SyntaxKind::EXPR_FUNCTION_BODY => {
                return CompletionContext::Expression;
            }

            // Field definitions - might be typing a type
            SyntaxKind::FIELD => {
                // Check if we're after the field name (in type position)
                let field_text = ancestor.text().to_string();
                let offset_usize: usize = offset.into();
                let start_usize: usize = ancestor.text_range().start().into();
                let offset_in_field = offset_usize.saturating_sub(start_usize);
                // If there's a space or we're past the first word, likely type position
                if field_text[..offset_in_field.min(field_text.len())].contains(char::is_whitespace)
                {
                    let partial = get_partial_word_at(text, offset);
                    return CompletionContext::TypeAnnotation { partial };
                }
            }

            _ => continue,
        }
    }

    CompletionContext::Unknown
}

/// Get the partial word being typed at the offset.
fn get_partial_word_at(text: &str, offset: TextSize) -> Option<String> {
    let offset_usize: usize = offset.into();
    if offset_usize > text.len() {
        return None;
    }

    let bytes = text.as_bytes();

    // Scan backward to find word start
    let mut start = offset_usize;
    while start > 0 {
        let b = bytes[start - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            start -= 1;
        } else {
            break;
        }
    }

    if start == offset_usize {
        None
    } else {
        Some(text[start..offset_usize].to_string())
    }
}

/// Detect the type of config block from an ancestor node.
fn detect_config_block_type(node: &SyntaxNode) -> ConfigBlockType {
    // Walk up to find the definition type
    for ancestor in node.ancestors() {
        match ancestor.kind() {
            SyntaxKind::CLIENT_DEF => return ConfigBlockType::Client,
            SyntaxKind::GENERATOR_DEF => return ConfigBlockType::Generator,
            SyntaxKind::RETRY_POLICY_DEF => return ConfigBlockType::RetryPolicy,
            SyntaxKind::TEST_DEF => return ConfigBlockType::Test,
            _ => continue,
        }
    }
    ConfigBlockType::Unknown
}

/// Extract the base expression text from a field access expression.
fn extract_field_access_base(node: &SyntaxNode) -> Option<String> {
    // FIELD_ACCESS_EXPR structure: <base_expr> DOT WORD
    // We want the text of the base expression
    for child in node.children() {
        // The first non-dot child should be the base
        if child.kind() != SyntaxKind::DOT {
            return Some(child.text().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_base_before_dot() {
        assert_eq!(get_base_before_dot("user."), Some("user".to_string()));
        assert_eq!(
            get_base_before_dot("ctx.client."),
            Some("ctx.client".to_string())
        );
        assert_eq!(get_base_before_dot("  foo."), Some("foo".to_string()));
        assert_eq!(get_base_before_dot("."), None);
    }

    #[test]
    fn test_check_special_prefix() {
        let text = "ctx.";
        let offset = TextSize::from(4u32);
        let ctx = check_special_prefix(text, offset);
        assert_eq!(
            ctx,
            Some(CompletionContext::PromptContext {
                partial_path: vec![]
            })
        );

        let text2 = "_.";
        let offset2 = TextSize::from(2u32);
        let ctx2 = check_special_prefix(text2, offset2);
        assert_eq!(ctx2, Some(CompletionContext::PromptUnderscore));
    }

    #[test]
    fn test_get_partial_word_at() {
        let text = "function MyFun";
        let offset = TextSize::from(14u32); // at end
        assert_eq!(get_partial_word_at(text, offset), Some("MyFun".to_string()));

        let text2 = "class ";
        let offset2 = TextSize::from(6u32);
        assert_eq!(get_partial_word_at(text2, offset2), None);
    }
}
