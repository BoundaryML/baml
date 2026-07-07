//! Docstring extraction from CST trivia.
//!
//! `///`-prefixed line comments preceding an item declaration are collected
//! during CST → AST lowering and stored on the corresponding AST node.

use baml_compiler_syntax::{SyntaxKind, SyntaxNode};

/// Extract `///` doc comments attached to `node`.
///
/// The BAML parser captures leading line comments as the *first children*
/// of the item node they precede (rather than as siblings before it), so
/// this walks `node.children_with_tokens()` from the start. Logic mirrors
/// what readers expect: only the contiguous run of `///` lines *immediately
/// before the declaration body* counts as its docstring. A non-doc `// …`
/// line interleaved among the leading comments resets the accumulator —
/// e.g. file-header `// …` blocks separated by a blank line from a `///`
/// block don't pollute the docstring, and a stray `// …` after the `///`
/// block detaches the docstring entirely. The walk stops at the first
/// non-trivia token or child node.
///
/// Returns `None` when no `///` lines are immediately attached; otherwise
/// returns the joined lines (one `\n` between originals, with a single
/// optional leading space stripped from each `///` body).
pub fn extract_docstring(node: &SyntaxNode) -> Option<String> {
    let mut doc_lines: Vec<String> = Vec::new();

    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Token(tok) => match tok.kind() {
                SyntaxKind::LINE_COMMENT => {
                    let text = tok.text();
                    if let Some(doc) = text.strip_prefix("///") {
                        let doc = doc.strip_prefix(' ').unwrap_or(doc);
                        doc_lines.push(doc.to_string());
                    } else {
                        // Regular `// …` line interleaved with leading
                        // trivia detaches any earlier `///` accumulation
                        // from the declaration.
                        doc_lines.clear();
                    }
                }
                SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE => {}
                _ => break,
            },
            rowan::NodeOrToken::Node(_) => break,
        }
    }

    if doc_lines.is_empty() {
        return None;
    }

    Some(doc_lines.join("\n"))
}

/// Return `true` when a `//baml:<marker>` directive appears in the leading
/// comment trivia of `node`. Walks the same children prefix as
/// `extract_docstring` so the same "immediately attached" semantics apply.
///
/// Used by BEP-049 §10 to detect `//baml:tagged_string` on a function
/// definition; can be reused for any future single-keyword directive.
pub fn has_baml_marker(node: &SyntaxNode, marker: &str) -> bool {
    let needle = format!("//baml:{marker}");
    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Token(tok) => match tok.kind() {
                SyntaxKind::LINE_COMMENT => {
                    if tok.text().trim_end() == needle {
                        return true;
                    }
                }
                SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE => {}
                _ => return false,
            },
            rowan::NodeOrToken::Node(_) => return false,
        }
    }
    false
}

/// Return the parenthesized argument of a `//baml:<marker>(<arg>)` directive
/// in the leading comment trivia of `node` — e.g. `//baml:llm_companion(stream)`
/// yields `Some("stream")`. Walks the same children prefix as
/// [`has_baml_marker`], so the same "immediately attached" semantics apply.
///
/// Returns `None` when the marker is absent or has no `( … )` argument; a
/// bare `//baml:<marker>` is *not* matched here (check `has_baml_marker` for
/// argument-less forms). The argument is whitespace-trimmed but otherwise
/// unvalidated — callers enforce identifier rules and report diagnostics.
pub fn baml_marker_arg(node: &SyntaxNode, marker: &str) -> Option<String> {
    let prefix = format!("//baml:{marker}(");
    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Token(tok) => match tok.kind() {
                SyntaxKind::LINE_COMMENT => {
                    if let Some(rest) = tok.text().trim_end().strip_prefix(&prefix) {
                        if let Some(arg) = rest.strip_suffix(')') {
                            return Some(arg.trim().to_string());
                        }
                    }
                }
                SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE => {}
                _ => return None,
            },
            rowan::NodeOrToken::Node(_) => return None,
        }
    }
    None
}
