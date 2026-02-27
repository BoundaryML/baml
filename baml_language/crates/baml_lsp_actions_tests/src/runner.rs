//! Test runner for inline assertion tests.
//!
//! This module uses the centralized `ProjectDatabase::check()` method for diagnostic
//! collection, eliminating code duplication with the LSP server.

use std::{collections::HashMap, path::Path};

use baml_compiler_diagnostics::{RenderConfig, render_diagnostic};
use baml_lsp_actions::{
    MarkupKind,
    hover::hover as lsp_ide_hover,
    inlay_hints::{InlayHint, InlayHintKind, inlay_hints as lsp_inlay_hints},
    semantic_tokens::{SemanticToken, semantic_tokens as lsp_semantic_tokens},
};
use baml_project::ProjectDatabase;
use text_size::TextSize;

use super::parser::{ParsedTestFile, VirtualFile};

/// Result of running an inline test.
#[derive(Debug)]
pub struct TestResult {
    /// Whether the test passed.
    pub passed: bool,
    /// Actual diagnostics output (Ariadne-formatted).
    pub actual_diagnostics: String,
    /// Actual hover output (collected from cursor markers).
    pub actual_hovers: Option<String>,
    /// Completion test result (if completions were expected).
    pub completion_result: Option<CompletionTestResult>,
    /// Actual inlay hints output.
    pub actual_inlay_hints: Option<String>,
    /// Actual semantic tokens output.
    pub actual_semantic_tokens: Option<String>,
    /// Diff between expected and actual (if failed).
    pub diff: Option<String>,
    /// Preserved comments from diagnostics section.
    pub diagnostics_comments: Vec<String>,
    /// Preserved comments from hovers section.
    pub hovers_comments: Vec<String>,
    /// Preserved comments from completions section.
    pub completions_comments: Vec<String>,
    /// Preserved comments from inlay hints section.
    pub inlay_hints_comments: Vec<String>,
    /// Preserved comments from semantic tokens section.
    pub semantic_tokens_comments: Vec<String>,
}

/// Result of completion testing.
#[derive(Debug)]
pub struct CompletionTestResult {
    /// Completion labels that were found.
    pub found_labels: Vec<String>,
    /// Expected labels that were missing.
    pub missing_labels: Vec<String>,
    /// Whether the completion test passed.
    pub passed: bool,
}

/// Result of hover at a cursor position.
#[derive(Debug)]
struct CursorHoverResult {
    file: String,
    line: usize,
    column: usize,
    actual_text: String,
}

/// Run an inline assertion test.
pub fn run_test(parsed: &ParsedTestFile) -> TestResult {
    // 1. Create ProjectDatabase and add all virtual files
    let mut lsp_db = ProjectDatabase::new();
    lsp_db.set_project_root(Path::new("."));

    let mut file_map = std::collections::HashMap::new();

    for (filename, vfile) in &parsed.files {
        let source_file = lsp_db.add_or_update_file(Path::new(filename), &vfile.content);
        file_map.insert(filename.clone(), source_file);
    }

    // 2. Collect all diagnostics using the centralized check() method
    // This replaces ~50 lines of manual diagnostic collection!
    let check_result = lsp_db.check();
    let diagnostics = &check_result.diagnostics;
    let sources = &check_result.sources;
    let file_paths = &check_result.file_paths;

    // 3. Format diagnostics output using the unified render
    let actual_diagnostics = if diagnostics.is_empty() {
        "// <no-diagnostics-expected>".to_string()
    } else {
        let config = RenderConfig::test();
        diagnostics
            .iter()
            .map(|d| {
                let rendered = render_diagnostic(d, sources, file_paths, &config);
                format_as_comment(rendered.trim())
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    // 4. Handle cursor-based hovers
    let actual_hovers = if !parsed.cursor_markers.is_empty() && parsed.expected_hovers.is_some() {
        let db = lsp_db.db();
        let project = lsp_db.project().expect("Project should be set");

        let cursor_hover_results: Vec<CursorHoverResult> = parsed
            .cursor_markers
            .iter()
            .map(|marker| {
                let source_file = file_map[&marker.file];
                let offset = TextSize::from(marker.offset as u32);
                let hover_result = lsp_ide_hover(db, source_file, project, offset);

                let actual_text = hover_result
                    .map(|h| h.display(MarkupKind::PlainText))
                    .unwrap_or_else(|| "No hover content".to_string());

                CursorHoverResult {
                    file: marker.file.clone(),
                    line: marker.line + 1,     // 1-indexed for display
                    column: marker.column + 1, // 1-indexed for display
                    actual_text,
                }
            })
            .collect();

        Some(format_cursor_hover_results(&cursor_hover_results))
    } else {
        None
    };

    // 5. Handle cursor-based completions
    let completion_result = if let (false, Some(expected)) = (
        parsed.cursor_markers.is_empty(),
        parsed.expected_completions.as_ref(),
    ) {
        let db = lsp_db.db();
        let project = lsp_db.project().expect("Project should be set");

        // Collect all completion labels from all cursor positions
        let mut all_labels: std::collections::HashSet<String> = std::collections::HashSet::new();

        for marker in &parsed.cursor_markers {
            let source_file = file_map[&marker.file];
            let offset = TextSize::from(marker.offset as u32);
            let completions = baml_lsp_actions::complete(db, source_file, project, offset);

            for item in completions {
                all_labels.insert(item.label);
            }
        }

        // Check which expected items are missing
        let found_labels: Vec<String> = all_labels.into_iter().collect();
        let missing_labels: Vec<String> = expected
            .should_contain
            .iter()
            .filter(|label| !found_labels.contains(label))
            .cloned()
            .collect();

        let passed = missing_labels.is_empty();

        Some(CompletionTestResult {
            found_labels,
            missing_labels,
            passed,
        })
    } else {
        None
    };

    // 6. Handle inlay hints
    let actual_inlay_hints = if parsed.expected_inlay_hints.is_some() {
        let db = lsp_db.db();
        let project = lsp_db.project().expect("Project should be set");

        // Collect all inlay hints from all files.
        let mut all_hints: Vec<(String, InlayHint)> = Vec::new();
        for (filename, source_file) in &file_map {
            let hints = lsp_inlay_hints(db, *source_file, project);
            for hint in hints {
                all_hints.push((filename.clone(), hint));
            }
        }

        // Sort inlay hints by filename and offset so the order is consistent between runs.
        all_hints.sort_by(|(fa, ha), (fb, hb)| fa.cmp(fb).then_with(|| ha.offset.cmp(&hb.offset)));

        Some(format_inlay_hints_results(&all_hints, &parsed.files))
    } else {
        None
    };

    // 7. Handle semantic tokens
    let actual_semantic_tokens = if parsed.expected_semantic_tokens.is_some() {
        let db = lsp_db.db();

        // Collect all semantic tokens from all files.
        let mut all_tokens: Vec<(String, SemanticToken)> = Vec::new();
        for (filename, source_file) in &file_map {
            let tokens = lsp_semantic_tokens(db, *source_file);
            for token in tokens {
                all_tokens.push((filename.clone(), token));
            }
        }

        // Sort by filename and then by range start for deterministic output.
        all_tokens.sort_by(|(fa, ta), (fb, tb)| {
            fa.cmp(fb)
                .then_with(|| ta.range.start().cmp(&tb.range.start()))
        });

        Some(format_semantic_tokens_results(&all_tokens, &parsed.files))
    } else {
        None
    };

    // Compare against expectations
    let diagnostics_passed = parsed.expected_diagnostics == actual_diagnostics;
    let hovers_passed = parsed.expected_hovers == actual_hovers;
    let completions_passed = completion_result.as_ref().map(|r| r.passed).unwrap_or(true);
    let inlay_hints_passed = parsed.expected_inlay_hints == actual_inlay_hints;
    let semantic_tokens_passed = parsed.expected_semantic_tokens == actual_semantic_tokens;

    let passed = diagnostics_passed
        && hovers_passed
        && completions_passed
        && inlay_hints_passed
        && semantic_tokens_passed;

    let diff = if !passed {
        Some(generate_full_diff(
            &parsed.expected_diagnostics,
            &actual_diagnostics,
            parsed.expected_hovers.as_deref(),
            actual_hovers.as_deref(),
            completion_result.as_ref(),
            parsed.expected_inlay_hints.as_deref(),
            actual_inlay_hints.as_deref(),
            parsed.expected_semantic_tokens.as_deref(),
            actual_semantic_tokens.as_deref(),
        ))
    } else {
        None
    };

    TestResult {
        passed,
        actual_diagnostics,
        actual_hovers,
        completion_result,
        actual_inlay_hints,
        actual_semantic_tokens,
        diff,
        diagnostics_comments: parsed.diagnostics_comments.clone(),
        hovers_comments: parsed.hovers_comments.clone(),
        completions_comments: parsed.completions_comments.clone(),
        inlay_hints_comments: parsed.inlay_hints_comments.clone(),
        semantic_tokens_comments: parsed.semantic_tokens_comments.clone(),
    }
}

/// Format cursor hover results for output (used in expectations section).
fn format_cursor_hover_results(results: &[CursorHoverResult]) -> String {
    let mut output = String::new();

    for result in results {
        // Header: // hover at file:line:col
        output.push_str(&format!(
            "// hover at {}:{}:{}\n",
            result.file, result.line, result.column
        ));

        // Content as comments
        for line in result.actual_text.lines() {
            if line.is_empty() {
                output.push_str("//\n");
            } else {
                output.push_str("// ");
                output.push_str(line);
                output.push('\n');
            }
        }
    }

    output.trim_end().to_string()
}

/// Prefix each line with `// `.
fn format_as_comment(text: &str) -> String {
    text.lines()
        .map(|line| {
            if line.is_empty() {
                "//".to_string()
            } else {
                format!("// {}", line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Convert a byte offset to a 1-indexed (line, column) pair.
fn offset_to_line_col(content: &str, offset: u32) -> (usize, usize) {
    let offset = offset as usize;
    let clamped = offset.min(content.len());

    // Walk back to the nearest valid char boundary to avoid a possible panic.
    let safe = (0..=clamped)
        .rev()
        .find(|&i| content.is_char_boundary(i))
        .unwrap_or(0);
    let before = &content[..safe];
    let line = before.matches('\n').count() + 1;
    let last_newline = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
    let column = safe - last_newline + 1;

    (line, column)
}

/// Format inlay hints for output (used in expectations section).
fn format_inlay_hints_results(
    hints: &[(String, InlayHint)],
    files: &HashMap<String, VirtualFile>,
) -> String {
    if hints.is_empty() {
        return "// <no-inlay-hints>".to_string();
    }

    let mut output = String::new();
    for (filename, hint) in hints {
        let file_content = &files[filename].content;
        let (line, col) = offset_to_line_col(file_content, u32::from(hint.offset));
        let kind_str = match hint.kind {
            Some(InlayHintKind::Type) => " (Type)",
            Some(InlayHintKind::Parameter) => " (Parameter)",
            None => "",
        };
        let label_text: String = hint.label.iter().map(|p| p.value.as_str()).collect();

        output.push_str(&format!(
            "// {filename}:{line}:{col}{kind_str} {label_text:?}\n"
        ));

        // Show label parts that have navigation targets.
        for part in &hint.label {
            if let Some(target) = &part.target {
                let target_filename = target.file_path.display();
                // Resolve target line:col from the target file's content.
                let (target_line, target_col) = if let Some(target_vfile) =
                    files.get(&target.file_path.display().to_string())
                {
                    offset_to_line_col(&target_vfile.content, u32::from(target.span.range.start()))
                } else {
                    // If something goes wrong and we can't find the file, show the raw offset.
                    let raw = u32::from(target.span.range.start());
                    output.push_str(&format!(
                                            "//   target {:?} -> {target_filename}:+{raw} (raw offset, file not in test)\n",
                                            part.value
                                        ));
                    continue;
                };
                output.push_str(&format!(
                    "//   target {:?} -> {target_filename}:{target_line}:{target_col}\n",
                    part.value
                ));
            }
        }

        // Show text edits.
        for edit in &hint.text_edits {
            let (edit_line, edit_col) = offset_to_line_col(file_content, u32::from(edit.offset));
            output.push_str(&format!(
                "//   edit@{edit_line}:{edit_col} {:?}\n",
                edit.new_text
            ));
        }
    }

    output.trim_end().to_string()
}

/// Format semantic tokens for output (used in expectations section).
fn format_semantic_tokens_results(
    tokens: &[(String, SemanticToken)],
    files: &HashMap<String, VirtualFile>,
) -> String {
    if tokens.is_empty() {
        return "// <no-semantic-tokens>".to_string();
    }

    let mut output = String::new();
    for (filename, token) in tokens {
        let file_content = &files[filename].content;
        let start_offset: usize = token.range.start().into();
        let end_offset: usize = token.range.end().into();
        let (line, col) = offset_to_line_col(file_content, token.range.start().into());
        let len = end_offset - start_offset;
        let text = &file_content[start_offset..end_offset];
        let token_type_str = token.token_type.as_str();

        output.push_str(&format!(
            "// {filename}:{line}:{col} ({token_type_str}) len={len} {text:?}\n"
        ));
    }

    output.trim_end().to_string()
}

/// Generate a full diff including diagnostics, hovers, completions, inlay hints, and semantic tokens.
#[allow(clippy::too_many_arguments)]
fn generate_full_diff(
    expected_diag: &str,
    actual_diag: &str,
    expected_hovers: Option<&str>,
    actual_hovers: Option<&str>,
    completion_result: Option<&CompletionTestResult>,
    expected_inlay_hints: Option<&str>,
    actual_inlay_hints: Option<&str>,
    expected_semantic_tokens: Option<&str>,
    actual_semantic_tokens: Option<&str>,
) -> String {
    let mut diff = String::new();

    diff.push_str("=== DIAGNOSTICS ===\n");
    diff.push_str("Expected:\n");
    diff.push_str(expected_diag);
    diff.push_str("\n\nActual:\n");
    diff.push_str(actual_diag);

    if expected_hovers.is_some() || actual_hovers.is_some() {
        diff.push_str("\n\n=== HOVERS ===\n");
        diff.push_str("Expected:\n");
        diff.push_str(expected_hovers.unwrap_or("<none>"));
        diff.push_str("\n\nActual:\n");
        diff.push_str(actual_hovers.unwrap_or("<none>"));
    }

    if let Some(result) = completion_result
        && !result.passed
    {
        diff.push_str("\n\n=== COMPLETIONS ===\n");
        diff.push_str("Missing expected completions:\n");
        for label in &result.missing_labels {
            diff.push_str(&format!("  - {label}\n"));
        }
        diff.push_str("\nFound completions:\n");
        let mut found: Vec<_> = result.found_labels.iter().collect();
        found.sort();
        for label in found {
            diff.push_str(&format!("  - {label}\n"));
        }
    }

    if expected_inlay_hints.is_some() || actual_inlay_hints.is_some() {
        diff.push_str("\n\n=== INLAY HINTS ===\n");
        diff.push_str("Expected:\n");
        diff.push_str(expected_inlay_hints.unwrap_or("<none>"));
        diff.push_str("\n\nActual:\n");
        diff.push_str(actual_inlay_hints.unwrap_or("<none>"));
    }

    if expected_semantic_tokens.is_some() || actual_semantic_tokens.is_some() {
        diff.push_str("\n\n=== SEMANTIC TOKENS ===\n");
        diff.push_str("Expected:\n");
        diff.push_str(expected_semantic_tokens.unwrap_or("<none>"));
        diff.push_str("\n\nActual:\n");
        diff.push_str(actual_semantic_tokens.unwrap_or("<none>"));
    }

    diff
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_test_file;

    #[test]
    fn test_no_errors() {
        let content = r#"class Foo {
    name string
}

//----
//- diagnostics
// <no-diagnostics-expected>"#;

        let parsed = parse_test_file(content, "test.baml");
        let result = run_test(&parsed);

        assert!(result.passed, "Test should pass: {:?}", result.diff);
    }

    #[test]
    fn test_with_parse_error() {
        let content = r#"class Foo {

//----
//- diagnostics
// placeholder"#;

        let parsed = parse_test_file(content, "test.baml");
        let result = run_test(&parsed);

        // This should fail because parse error doesn't match placeholder
        assert!(!result.passed);
        // But we should have actual diagnostics
        assert!(
            !result
                .actual_diagnostics
                .contains("<no-diagnostics-expected>")
        );
    }

    #[test]
    fn test_cursor_hover() {
        let content = r#"class Person<[CURSOR] {
    name string
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at test.baml:1:13
// class Person {
//   name string
// }"#;

        let parsed = parse_test_file(content, "test.baml");
        let result = run_test(&parsed);

        assert!(result.passed, "Test should pass: {:?}", result.diff);
        assert!(result.actual_hovers.is_some());
        let hovers = result.actual_hovers.unwrap();
        assert!(hovers.contains("class Person"));
        assert!(hovers.contains("name string"));
    }

    #[test]
    fn test_completion_top_level() {
        let content = r#"<[CURSOR]

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- completions
// SHOULD_CONTAIN: function, class, enum"#;

        let parsed = parse_test_file(content, "test.baml");
        let result = run_test(&parsed);

        assert!(
            result.completion_result.is_some(),
            "Should have completion result"
        );
        let comp = result.completion_result.unwrap();
        assert!(
            comp.passed,
            "Completion test should pass. Missing: {:?}, Found: {:?}",
            comp.missing_labels, comp.found_labels
        );
    }
}
