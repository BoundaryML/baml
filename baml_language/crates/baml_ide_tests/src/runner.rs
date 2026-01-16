//! Test runner for inline assertion tests.
//!
//! This module uses the centralized `ProjectDatabase::check()` method for diagnostic
//! collection, eliminating code duplication with the LSP server.

use std::path::Path;

use baml_compiler_diagnostics::{RenderConfig, render_diagnostic};
use baml_ide::{MarkupKind, hover::hover as lsp_ide_hover};
use baml_project::ProjectDatabase;
use text_size::TextSize;

use super::parser::ParsedTestFile;

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
    /// Diff between expected and actual (if failed).
    pub diff: Option<String>,
    /// Preserved comments from diagnostics section.
    pub diagnostics_comments: Vec<String>,
    /// Preserved comments from hovers section.
    pub hovers_comments: Vec<String>,
    /// Preserved comments from completions section.
    pub completions_comments: Vec<String>,
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
    let completion_result = if !parsed.cursor_markers.is_empty()
        && parsed.expected_completions.is_some()
    {
        let db = lsp_db.db();
        let project = lsp_db.project().expect("Project should be set");
        let expected = parsed.expected_completions.as_ref().unwrap();

        // Collect all completion labels from all cursor positions
        let mut all_labels: std::collections::HashSet<String> = std::collections::HashSet::new();

        for marker in &parsed.cursor_markers {
            let source_file = file_map[&marker.file];
            let offset = TextSize::from(marker.offset as u32);
            let completions = baml_ide::complete(db, source_file, project, offset);

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

    // 6. Compare against expectations
    let diagnostics_passed = parsed.expected_diagnostics == actual_diagnostics;
    let hovers_passed = parsed.expected_hovers == actual_hovers;
    let completions_passed = completion_result.as_ref().map(|r| r.passed).unwrap_or(true);

    let passed = diagnostics_passed && hovers_passed && completions_passed;

    let diff = if !passed {
        Some(generate_full_diff_with_completions(
            &parsed.expected_diagnostics,
            &actual_diagnostics,
            parsed.expected_hovers.as_deref(),
            actual_hovers.as_deref(),
            completion_result.as_ref(),
        ))
    } else {
        None
    };

    TestResult {
        passed,
        actual_diagnostics,
        actual_hovers,
        completion_result,
        diff,
        diagnostics_comments: parsed.diagnostics_comments.clone(),
        hovers_comments: parsed.hovers_comments.clone(),
        completions_comments: parsed.completions_comments.clone(),
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

/// Generate a full diff including diagnostics, hovers, and completions.
fn generate_full_diff_with_completions(
    expected_diag: &str,
    actual_diag: &str,
    expected_hovers: Option<&str>,
    actual_hovers: Option<&str>,
    completion_result: Option<&CompletionTestResult>,
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
