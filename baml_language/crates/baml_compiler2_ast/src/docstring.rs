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
