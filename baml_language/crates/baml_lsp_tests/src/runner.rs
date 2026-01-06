//! Test runner for inline assertion tests.

use std::collections::HashMap;

use baml_db::{
    RootDatabase,
    baml_hir::{self, file_items, function_body, function_signature},
    baml_parser,
    baml_tir::{self, class_field_types, type_aliases, typing_context},
};
use baml_diagnostics::{
    render_hir_diagnostic, render_name_error, render_parse_error, render_type_error,
};
use salsa::Setter;

use super::{
    hover::get_hover_for_symbol,
    parser::{HoverAssertion, ParsedTestFile},
};

/// Result of running an inline test.
#[derive(Debug)]
pub struct TestResult {
    /// Whether the test passed.
    pub passed: bool,
    /// Actual diagnostics output (Ariadne-formatted).
    pub actual_diagnostics: String,
    /// Actual hover output (collected from assertions).
    pub actual_hovers: Option<String>,
    /// Diff between expected and actual (if failed).
    pub diff: Option<String>,
    /// Preserved comments from diagnostics section.
    pub diagnostics_comments: Vec<String>,
    /// Preserved comments from hovers section.
    pub hovers_comments: Vec<String>,
}

/// Run an inline assertion test.
pub fn run_test(parsed: &ParsedTestFile) -> TestResult {
    // 1. Create RootDatabase and add all virtual files
    let mut db = RootDatabase::new();
    let root = db.set_project_root(std::path::PathBuf::from("."));

    let mut sources: HashMap<baml_db::FileId, String> = HashMap::new();
    let mut source_files = Vec::new();

    for (filename, vfile) in &parsed.files {
        let source_file = db.add_file(filename, &vfile.content);
        sources.insert(source_file.file_id(&db), vfile.content.clone());
        source_files.push(source_file);
    }

    // Update project root with the list of files
    root.set_files(&mut db).to(source_files.clone());

    // 2. Collect all diagnostics
    let mut all_errors: Vec<String> = Vec::new();

    // Collect parse errors
    for source_file in &source_files {
        let errors = baml_parser::parse_errors(&db, *source_file);
        for error in errors.iter() {
            all_errors.push(render_parse_error(error, &sources, false));
        }
    }

    // Collect HIR lowering diagnostics (per-file validation)
    for source_file in &source_files {
        let lowering_result = baml_hir::file_lowering(&db, *source_file);
        for diag in lowering_result.diagnostics(&db) {
            all_errors.push(render_hir_diagnostic(diag, &sources, false));
        }
    }

    // Collect validation errors (duplicates across files, reserved names)
    let validation_result = baml_hir::validate_hir(&db, root);
    for diag in validation_result.hir_diagnostics {
        all_errors.push(render_hir_diagnostic(&diag, &sources, false));
    }
    for error in validation_result.name_errors {
        all_errors.push(render_name_error(&error, &sources, false));
    }

    // Collect type errors
    let globals = typing_context(&db, root).functions(&db).clone();
    let class_fields = class_field_types(&db, root).classes(&db).clone();
    let type_aliases_map = type_aliases(&db, root).aliases(&db).clone();
    let enum_variants_map = baml_tir::enum_variants(&db, root);
    let enum_variants = enum_variants_map.enums(&db).clone();

    for source_file in &source_files {
        let items_struct = file_items(&db, *source_file);
        let items = items_struct.items(&db);
        for item in items.iter() {
            if let baml_hir::ItemId::Function(func_id) = item {
                let signature = function_signature(&db, *func_id);
                let body = function_body(&db, *func_id);
                let result = baml_tir::infer_function(
                    &db,
                    &signature,
                    &body,
                    Some(globals.clone()),
                    Some(class_fields.clone()),
                    Some(type_aliases_map.clone()),
                    Some(enum_variants.clone()),
                    *func_id,
                );
                for error in &result.errors {
                    all_errors.push(render_type_error(error, &sources, false));
                }
            }
        }
    }

    // 3. Format diagnostics output
    let actual_diagnostics = if all_errors.is_empty() {
        "// <no-diagnostics-expected>".to_string()
    } else {
        all_errors
            .iter()
            .map(|e| format_as_comment(e.trim()))
            .collect::<Vec<_>>()
            .join("\n")
    };

    // 4. Verify inline hover assertions
    let hover_verifications = verify_hovers(&db, root, &parsed.hover_assertions);
    let all_hovers_passed = hover_verifications.iter().all(|v| v.passed);

    // 5. Collect hovers for expectations section comparison
    let actual_hovers = if !parsed.hover_assertions.is_empty() || parsed.expected_hovers.is_some() {
        Some(format_hover_verifications(&hover_verifications))
    } else {
        None
    };

    // 6. Compare against expectations
    // Test passes if:
    // - All inline hover assertions pass (expected text matches actual)
    // - Diagnostics section matches
    // - Hovers section matches (if present)
    let passed = all_hovers_passed
        && parsed.expected_diagnostics == actual_diagnostics
        && parsed.expected_hovers == actual_hovers;

    let diff = if !passed {
        Some(generate_full_diff(
            &parsed.expected_diagnostics,
            &actual_diagnostics,
            parsed.expected_hovers.as_deref(),
            actual_hovers.as_deref(),
            &hover_verifications,
        ))
    } else {
        None
    };

    TestResult {
        passed,
        actual_diagnostics,
        actual_hovers,
        diff,
        diagnostics_comments: parsed.diagnostics_comments.clone(),
        hovers_comments: parsed.hovers_comments.clone(),
    }
}

/// Result of verifying a single hover assertion.
#[derive(Debug)]
struct HoverVerification {
    symbol: String,
    file: String,
    line: usize,
    expected: String,
    actual: Option<String>,
    passed: bool,
}

/// Verify hover assertions and collect results.
fn verify_hovers(
    db: &RootDatabase,
    root: baml_db::baml_workspace::Project,
    assertions: &[HoverAssertion],
) -> Vec<HoverVerification> {
    let mut results = Vec::new();

    for assertion in assertions {
        let actual = get_hover_for_symbol(db, root, &assertion.symbol);

        // Normalize expected text: replace \n with actual newlines
        let expected_normalized = assertion.expected_text.replace("\\n", "\n");

        let passed = match &actual {
            Some(actual_text) => actual_text.trim() == expected_normalized.trim(),
            None => false,
        };

        results.push(HoverVerification {
            symbol: assertion.symbol.clone(),
            file: assertion.file.clone(),
            line: assertion.line + 1, // Convert to 1-indexed
            expected: expected_normalized,
            actual,
            passed,
        });
    }

    results
}

/// Format hover verifications for output (used in expectations section).
fn format_hover_verifications(verifications: &[HoverVerification]) -> String {
    let mut hover_results: Vec<String> = Vec::new();

    for v in verifications {
        if let Some(ref actual) = v.actual {
            let header = format!("// `{}` at {}:{}", v.symbol, v.file, v.line);
            hover_results.push(header);
            hover_results.push(format_as_comment(actual));
        } else {
            let header = format!("// `{}` at {}:{} - NOT FOUND", v.symbol, v.file, v.line);
            hover_results.push(header);
        }
    }

    if hover_results.is_empty() {
        "// <no-hovers-collected>".to_string()
    } else {
        hover_results.join("\n")
    }
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

/// Generate a full diff including diagnostics and hovers.
fn generate_full_diff(
    expected_diag: &str,
    actual_diag: &str,
    expected_hovers: Option<&str>,
    actual_hovers: Option<&str>,
    hover_verifications: &[HoverVerification],
) -> String {
    let mut diff = String::new();

    // Show failed inline hover assertions first (most actionable)
    let failed_hovers: Vec<_> = hover_verifications.iter().filter(|v| !v.passed).collect();
    if !failed_hovers.is_empty() {
        diff.push_str("=== FAILED INLINE HOVER ASSERTIONS ===\n");
        for v in failed_hovers {
            diff.push_str(&format!("\n`{}` at {}:{}:\n", v.symbol, v.file, v.line));
            diff.push_str("  Expected:\n");
            for line in v.expected.lines() {
                diff.push_str(&format!("    {}\n", line));
            }
            diff.push_str("  Actual:\n");
            match &v.actual {
                Some(actual) => {
                    for line in actual.lines() {
                        diff.push_str(&format!("    {}\n", line));
                    }
                }
                None => {
                    diff.push_str("    <symbol not found>\n");
                }
            }
        }
        diff.push('\n');
    }

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
}
