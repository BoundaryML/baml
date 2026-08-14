//! Test runner for inline assertion tests (compiler2 / lsp2_actions pipeline).

use std::{collections::HashMap, path::Path};

use baml_compiler_diagnostics::{RenderConfig, render_diagnostic};
use baml_lsp2_actions::{
    annotations::{AnnotationKind, InlineAnnotation, file_annotations},
    check::check_file,
    completions::completions_at,
    tokens::{SemanticToken, semantic_tokens},
    type_info::type_at,
};
use baml_project::ProjectDatabase;
use indexmap::IndexMap;
use text_size::TextSize;

use super::parser::{ParsedTestFile, VirtualFile};

/// Result of running an inline test.
#[derive(Debug)]
pub struct TestResult {
    pub passed: bool,
    pub actual_diagnostics: String,
    pub actual_hovers: Option<String>,
    pub completion_result: Option<CompletionTestResult>,
    pub actual_inlay_hints: Option<String>,
    pub actual_semantic_tokens: Option<String>,
    pub diff: Option<String>,
    pub diagnostics_comments: Vec<String>,
    pub hovers_comments: Vec<String>,
    pub completions_comments: Vec<String>,
    pub inlay_hints_comments: Vec<String>,
    pub semantic_tokens_comments: Vec<String>,
}

/// Result of completion testing.
#[derive(Debug)]
pub struct CompletionTestResult {
    pub found_labels: Vec<String>,
    pub missing_labels: Vec<String>,
    /// The original SHOULD_CONTAIN labels from the test expectation.
    pub expected_labels: Vec<String>,
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
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new("."));

    let mut file_map = IndexMap::new();

    for (filename, vfile) in &parsed.files {
        let source_file = db.add_or_update_file(Path::new(filename), &vfile.content);
        file_map.insert(filename.clone(), source_file);
    }

    // 2. Collect diagnostics via check_file() per file
    let mut all_diagnostics = Vec::new();
    for source_file in file_map.values() {
        let file_diags = check_file(&db, *source_file);
        all_diagnostics.extend(file_diags);
    }

    // Build sources and file_paths maps for rendering
    let mut sources = HashMap::new();
    let mut file_paths = HashMap::new();
    for source_file in file_map.values() {
        let file_id = source_file.file_id(&db);
        sources.insert(file_id, source_file.text(&db).clone());
        file_paths.insert(file_id, source_file.path(&db));
    }

    // 3. Format diagnostics output
    let actual_diagnostics = if all_diagnostics.is_empty() {
        "// <no-diagnostics-expected>".to_string()
    } else {
        let config = RenderConfig::test();
        all_diagnostics
            .iter()
            .map(|d| {
                let rendered = render_diagnostic(d, &sources, &file_paths, &config);
                format_as_comment(rendered.trim())
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    // 4. Handle cursor-based hovers via type_at()
    let actual_hovers = if !parsed.cursor_markers.is_empty() && parsed.expected_hovers.is_some() {
        let cursor_hover_results: Vec<CursorHoverResult> = parsed
            .cursor_markers
            .iter()
            .map(|marker| {
                let source_file = file_map[&marker.file];
                let offset = TextSize::from(marker.offset as u32);
                let hover_result = type_at(&db, source_file, offset);

                let actual_text = hover_result
                    .map(|info| info.to_hover_markdown())
                    .unwrap_or_else(|| "No hover content".to_string());

                CursorHoverResult {
                    file: marker.file.clone(),
                    line: marker.line + 1,
                    column: marker.column + 1,
                    actual_text,
                }
            })
            .collect();

        Some(format_cursor_hover_results(&cursor_hover_results))
    } else {
        None
    };

    // 5. Handle cursor-based completions via completions_at()
    let completion_result = if let (false, Some(expected)) = (
        parsed.cursor_markers.is_empty(),
        parsed.expected_completions.as_ref(),
    ) {
        let mut all_labels: std::collections::HashSet<String> = std::collections::HashSet::new();

        for marker in &parsed.cursor_markers {
            let source_file = file_map[&marker.file];
            let offset = TextSize::from(marker.offset as u32);
            let completions = completions_at(&db, source_file, offset);

            for item in completions {
                all_labels.insert(item.label);
            }
        }

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
            expected_labels: expected.should_contain.clone(),
            passed,
        })
    } else {
        None
    };

    // 6. Handle inlay hints via annotations()
    let actual_inlay_hints = if parsed.expected_inlay_hints.is_some() {
        let mut all_hints: Vec<(String, InlineAnnotation)> = Vec::new();
        for (filename, source_file) in &file_map {
            let hints = file_annotations(&db, *source_file);
            for hint in hints {
                all_hints.push((filename.clone(), hint.clone()));
            }
        }

        all_hints.sort_by(|(fa, ha), (fb, hb)| fa.cmp(fb).then_with(|| ha.offset.cmp(&hb.offset)));

        Some(format_annotations(&all_hints, &parsed.files))
    } else {
        None
    };

    // 7. Handle semantic tokens
    let actual_semantic_tokens = if parsed.expected_semantic_tokens.is_some() {
        let mut all_tokens: Vec<(String, SemanticToken)> = Vec::new();
        for (filename, source_file) in &file_map {
            let tokens = semantic_tokens(&db, *source_file);
            for token in tokens {
                all_tokens.push((filename.clone(), token.clone()));
            }
        }

        all_tokens.sort_by(|(fa, ta), (fb, tb)| {
            fa.cmp(fb)
                .then_with(|| ta.range.start().cmp(&tb.range.start()))
        });

        Some(format_semantic_tokens_results(&all_tokens, &parsed.files))
    } else {
        None
    };

    // Compare against expectations
    let diagnostics_passed = expectation_text_matches(
        Some(&parsed.expected_diagnostics),
        Some(&actual_diagnostics),
    );
    let hovers_passed =
        expectation_text_matches(parsed.expected_hovers.as_deref(), actual_hovers.as_deref());
    let completions_passed = completion_result.as_ref().map(|r| r.passed).unwrap_or(true);
    let inlay_hints_passed = expectation_text_matches(
        parsed.expected_inlay_hints.as_deref(),
        actual_inlay_hints.as_deref(),
    );
    let semantic_tokens_passed = expectation_text_matches(
        parsed.expected_semantic_tokens.as_deref(),
        actual_semantic_tokens.as_deref(),
    );

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

/// Format cursor hover results for output.
fn format_cursor_hover_results(results: &[CursorHoverResult]) -> String {
    let mut output = String::new();

    for result in results {
        output.push_str(&format!(
            "// hover at {}:{}:{}\n",
            result.file, result.line, result.column
        ));

        for line in result.actual_text.lines() {
            // The expectation parser skips empty `//` lines (they double as
            // section separators), so blank lines in the hover markdown — e.g.
            // the blank line before a `Run \`baml describe …\`` hint — cannot
            // round-trip. Drop them here so actual matches the parsed expected.
            if line.is_empty() {
                continue;
            }
            output.push_str("// ");
            output.push_str(line);
            output.push('\n');
        }
    }

    output.trim_end().to_string()
}

/// Prefix each line with `// `.
fn format_as_comment(text: &str) -> String {
    text.lines()
        .map(|line| {
            let line = line.trim_end();
            if line.is_empty() {
                "//".to_string()
            } else {
                format!("// {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn expectation_text_matches(expected: Option<&str>, actual: Option<&str>) -> bool {
    match (expected, actual) {
        (Some(expected), Some(actual)) => {
            normalize_line_endings(expected) == normalize_line_endings(actual)
        }
        (None, None) => true,
        _ => false,
    }
}

fn normalize_line_endings(text: &str) -> String {
    text.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Convert a byte offset to a 1-indexed (line, column) pair.
fn offset_to_line_col(content: &str, offset: u32) -> (usize, usize) {
    let offset = offset as usize;
    let clamped = offset.min(content.len());

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

/// Format annotations (inlay hints) for output.
fn format_annotations(
    hints: &[(String, InlineAnnotation)],
    files: &IndexMap<String, VirtualFile>,
) -> String {
    if hints.is_empty() {
        return "// <no-inlay-hints>".to_string();
    }

    let mut output = String::new();
    for (filename, hint) in hints {
        let file_content = &files[filename].content;
        let (line, col) = offset_to_line_col(file_content, u32::from(hint.offset));
        let kind_str = match hint.kind {
            AnnotationKind::Type => " (Type)",
            AnnotationKind::Parameter => " (Parameter)",
        };

        output.push_str(&format!(
            "// {filename}:{line}:{col}{kind_str} {:?}\n",
            hint.label
        ));
    }

    output.trim_end().to_string()
}

/// Format semantic tokens for output.
fn format_semantic_tokens_results(
    tokens: &[(String, SemanticToken)],
    files: &IndexMap<String, VirtualFile>,
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
        let mods: Vec<&str> = token.modifiers.names().collect();
        let mods_str = if mods.is_empty() {
            String::new()
        } else {
            format!(" [{}]", mods.join(","))
        };

        output.push_str(&format!(
            "// {filename}:{line}:{col} ({token_type_str}){mods_str} len={len} {text:?}\n"
        ));
    }

    output.trim_end().to_string()
}

/// Generate a full diff including all five sections.
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
