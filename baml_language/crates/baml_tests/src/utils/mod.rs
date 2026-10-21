//! Test utilities for parser testing, especially incremental parsing verification.

use std::collections::HashMap;

use baml_compiler_diagnostics::{Diagnostic, DiagnosticPhase};
use baml_db::{FileId, baml_compiler_syntax::SyntaxNode};

// Test-database conveniences, re-exported so the generated project tests
// (`use crate::utils::*`) and the in-crate test modules share the one
// definition with the integration tests (`baml_tests::engine::TestDbExt`).
pub use crate::engine::{TestDbExt, db_with_root};

/// Assert that no diagnostic annotation span begins or ends on a whitespace
/// character.
///
/// Diagnostic spans must tightly cover the offending construct. The rowan CST
/// keeps trivia (whitespace, newlines, comments) as child tokens of nodes, so a
/// node's raw `text_range()` includes any leading/trailing trivia. Spans built
/// from raw `text_range()` therefore underline the inter-token gap before the
/// real token (e.g. the space after `->` in a return type). Routing node spans
/// through `baml_compiler_syntax::trimmed_range` (exposed as
/// `SyntaxNodeExt::span_range`) eliminates this; this check enforces it across
/// the whole snapshot corpus so the bug cannot regress.
///
/// Zero-width / EOF carets (empty ranges) are skipped — they legitimately point
/// at a position rather than covering a token.
pub fn assert_diagnostic_spans_exclude_trivia(
    project: &str,
    diagnostics: &[Diagnostic],
    sources: &HashMap<FileId, String>,
) {
    let phase_name = |phase: DiagnosticPhase| match phase {
        DiagnosticPhase::Parse => "parse",
        DiagnosticPhase::Hir => "hir",
        DiagnosticPhase::Validation => "validation",
        DiagnosticPhase::Type => "type",
    };

    let mut violations = Vec::new();
    for diag in diagnostics {
        for ann in &diag.annotations {
            let Some(src) = sources.get(&ann.span.file_id) else {
                continue;
            };
            let start: usize = u32::from(ann.span.range.start()) as usize;
            let end: usize = u32::from(ann.span.range.end()) as usize;
            if start >= end {
                continue; // zero-width / EOF caret: nothing to cover
            }
            let Some(covered) = src.get(start..end) else {
                continue; // out-of-bounds or non-char-boundary range: skip
            };
            let leading = covered.chars().next().is_some_and(char::is_whitespace);
            let trailing = covered.chars().next_back().is_some_and(char::is_whitespace);
            if leading || trailing {
                let edge = match (leading, trailing) {
                    (true, true) => "leading+trailing",
                    (true, false) => "leading",
                    (false, true) => "trailing",
                    (false, false) => unreachable!(),
                };
                violations.push(format!(
                    "  [{}] {} span has {} whitespace: {covered:?} (bytes {start}..{end})\n      {}",
                    phase_name(diag.phase),
                    diag.code(),
                    edge,
                    diag.message,
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "project '{project}': {} diagnostic span(s) include leading/trailing whitespace.\n\
         Diagnostic spans must tightly cover the construct: route node spans through \
         `SyntaxNodeExt::span_range` / `baml_compiler_syntax::trimmed_range` instead of raw \
         `text_range()`.\n{}",
        violations.len(),
        violations.join("\n"),
    );
}

/// Metrics for measuring node reuse in incremental parsing
#[derive(Debug)]
pub struct ReuseMetrics {
    pub total_old_nodes: usize,
    pub total_new_nodes: usize,
    pub reused_nodes: usize,
    pub reuse_percentage: f64,
}

/// Verify the parse tree can reconstruct the original source exactly
pub fn assert_tree_is_lossless(tree: &SyntaxNode, original: &str) {
    let reconstructed = tree.to_string();
    assert_eq!(
        reconstructed, original,
        "Tree is not lossless: reconstruction doesn't match original"
    );
}

/// Test that no panics occur when traversing the tree
pub fn assert_no_panics(tree: &SyntaxNode) {
    fn traverse(node: &SyntaxNode) {
        // Access all node properties to ensure no panics
        let _ = node.kind();
        let _ = node.text();
        let _ = node.text_range();

        for child in node.children() {
            traverse(&child);
        }
    }

    traverse(tree);
}

/// Insert a character at the given position in a string
pub fn insert_char(source: &str, pos: usize, ch: char) -> String {
    let mut result = String::new();
    let chars: Vec<char> = source.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        if i == pos {
            result.push(ch);
        }
        result.push(c);
    }

    if pos == chars.len() {
        result.push(ch);
    }

    result
}

/// Delete a character at the given position
pub fn delete_char(source: &str, pos: usize) -> String {
    let mut result = String::new();
    let chars: Vec<char> = source.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        if i != pos {
            result.push(c);
        }
    }

    result
}

/// Replace a character at the given position
pub fn replace_char(source: &str, pos: usize, ch: char) -> String {
    let mut result = String::new();
    let chars: Vec<char> = source.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        if i == pos {
            result.push(ch);
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_edit_functions() {
        let source = "hello world";

        assert_eq!(insert_char(source, 0, 'X'), "Xhello world");
        assert_eq!(insert_char(source, 5, 'X'), "helloX world");
        assert_eq!(insert_char(source, 11, 'X'), "hello worldX");

        assert_eq!(delete_char(source, 0), "ello world");
        assert_eq!(delete_char(source, 5), "helloworld");

        assert_eq!(replace_char(source, 0, 'X'), "Xello world");
        assert_eq!(replace_char(source, 6, 'X'), "hello Xorld");
    }
}
