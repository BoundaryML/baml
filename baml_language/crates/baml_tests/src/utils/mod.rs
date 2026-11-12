//! Test utilities shared by the generated project tests.

use std::collections::HashMap;

use baml_compiler_diagnostics::{Diagnostic, DiagnosticPhase};
use baml_db::FileId;
use baml_syntax::nodes::FileTree;

// Test-database conveniences, re-exported so the generated project tests
// (`use crate::utils::*`) and the in-crate test modules share the one
// definition with the integration tests (`baml_tests::engine::TestDbExt`).
pub use crate::engine::{TestDbExt, db_with_root};

/// Assert that no diagnostic annotation span begins or ends on a whitespace
/// character.
///
/// Diagnostic spans must tightly cover the offending construct. A node's span
/// in the typed tree excludes trivia (whitespace, newlines, comments) by
/// construction, so a span with whitespace at an edge was built from raw
/// offset arithmetic rather than from a node or token range (e.g. underlining
/// the space after `->` instead of the return type). This check enforces it
/// across the whole snapshot corpus so the bug cannot regress.
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
         Diagnostic spans must tightly cover the construct: build them from node and token \
         ranges of the typed tree instead of raw offset arithmetic.\n{}",
        violations.len(),
        violations.join("\n"),
    );
}

/// The token array reconstructs the source exactly: every byte belongs to
/// one token.
pub fn assert_tree_is_lossless(tree: &FileTree, original: &str) {
    let reconstructed: String = tree
        .tokens()
        .iter()
        .map(|idx| tree.token_text(idx))
        .collect();
    assert_eq!(
        reconstructed, original,
        "Tree is not lossless: reconstruction doesn't match original"
    );
}
