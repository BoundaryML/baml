//! LSP v1 vs v2 diagnostics diff harness.
//!
//! Compares diagnostics expectations between `baml_lsp_actions_tests/test_files/syntax/`
//! (compiler v1 expectations) and `baml_lsp2_actions_tests/test_files/syntax/` (compiler2
//! expectations), and runs each file through compiler2 to check actual output.
//!
//! Run:
//!   cargo test -p baml_tests lsp_diagnostics_diff -- --nocapture
//!
//! The report is written to:
//!   baml_language/crates/baml_tests/lsp_diagnostics_diff.json

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use baml_compiler_diagnostics::{RenderConfig, Severity, render_diagnostic};
use baml_project::ProjectDatabase;
use serde::Serialize;

const LSP_V1_ROOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../baml_lsp_actions_tests/test_files/syntax"
);

const LSP_V2_ROOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../baml_lsp2_actions_tests/test_files/syntax"
);

const REPORT_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/lsp_diagnostics_diff.json"
);

// ---------------------------------------------------------------------------
// JSON report types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct DiffReport {
    summary: Summary,
    files: Vec<FileDiff>,
}

#[derive(Debug, Serialize)]
struct Summary {
    total_files: usize,
    /// V1 and V2 expectations are identical
    expectations_match: usize,
    /// V1 and V2 expectations differ (compiler2 changed behavior)
    expectations_differ: usize,
    /// Actual compiler2 output matches V2 expectations
    v2_passing: usize,
    /// Actual compiler2 output does NOT match V2 expectations
    v2_failing: usize,
    /// Compiler2 panicked
    v2_panic: usize,
}

#[derive(Debug, Serialize)]
struct FileDiff {
    file: String,
    /// Whether V1 and V2 expectations are the same
    expectations_match: bool,
    /// Whether actual compiler2 output matches V2 expectations
    v2_passes: bool,
    /// V1 expected diagnostics (from baml_lsp_actions_tests)
    v1_expected: String,
    /// V2 expected diagnostics (from baml_lsp2_actions_tests)
    v2_expected: String,
    /// Actual compiler2 diagnostics output
    v2_actual: String,
    /// Status string for easy filtering
    status: String,
}

// ---------------------------------------------------------------------------
// Test file parsing (subset of baml_lsp_actions_tests parser)
// ---------------------------------------------------------------------------

/// Extract the diagnostics expectation section from an LSP test file.
/// Returns the raw text after `//- diagnostics` up to the next section or EOF.
fn extract_diagnostics_expectation(content: &str) -> String {
    let Some(separator_idx) = content.find("//----") else {
        return String::new();
    };

    let after_separator = &content[separator_idx..];
    let lines: Vec<&str> = after_separator.lines().collect();

    let mut in_diagnostics = false;
    let mut diag_lines = Vec::new();

    for line in &lines {
        let trimmed = line.trim();

        // Check for section markers
        if trimmed == "//- diagnostics" || trimmed.starts_with("//- diagnostics ") {
            in_diagnostics = true;
            continue;
        }

        if trimmed.starts_with("//- ") && trimmed != "//- diagnostics" {
            // Hit a different section, stop collecting
            if in_diagnostics {
                break;
            }
            continue;
        }

        if in_diagnostics {
            // Skip empty separator comments and preserved comments
            if trimmed == "//" {
                continue;
            }
            if trimmed.starts_with("// (") {
                continue;
            }
            diag_lines.push(*line);
        }
    }

    diag_lines.join("\n").trim().to_string()
}

/// Extract BAML source from a test file (everything before `//----`),
/// stripping `// file:` markers and cursor markers, handling virtual files.
fn extract_source_and_files(content: &str, default_filename: &str) -> HashMap<String, String> {
    let source = if let Some(idx) = content.find("//----") {
        &content[..idx]
    } else {
        content
    };

    let mut files: HashMap<String, String> = HashMap::new();
    let mut current_filename = default_filename.to_string();
    let mut current_content = String::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("// file:") {
            // Save current file
            if !current_content.is_empty() {
                files.insert(
                    current_filename.clone(),
                    current_content.trim_end().to_string(),
                );
            }
            current_filename = trimmed
                .strip_prefix("// file:")
                .unwrap()
                .trim()
                .to_string();
            current_content = String::new();
            continue;
        }

        if !current_content.is_empty() {
            current_content.push('\n');
        }
        // Strip cursor markers
        let clean_line = line.replace("<[CURSOR]", "");
        current_content.push_str(&clean_line);
    }

    if !current_content.is_empty() {
        files.insert(current_filename, current_content.trim_end().to_string());
    }

    files
}

// ---------------------------------------------------------------------------
// Compiler2 runner
// ---------------------------------------------------------------------------

fn run_through_compiler2(
    virtual_files: &HashMap<String, String>,
) -> Result<String, String> {
    let files = virtual_files.clone();

    std::panic::catch_unwind(move || {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("."));

        for (filename, content) in &files {
            db.add_file(filename, content);
        }

        let check_result = db.check();

        // Build rendering context
        let config = RenderConfig::test();

        if check_result.diagnostics.is_empty() {
            return "// <no-diagnostics-expected>".to_string();
        }

        check_result
            .diagnostics
            .iter()
            .map(|d| {
                let rendered = render_diagnostic(
                    d,
                    &check_result.sources,
                    &check_result.file_paths,
                    &config,
                );
                // Format as comment (same as the LSP test runner)
                rendered
                    .trim()
                    .lines()
                    .map(|line| {
                        if line.is_empty() {
                            "//".to_string()
                        } else {
                            format!("// {line}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
    .map_err(|e| {
        if let Some(s) = e.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = e.downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        }
    })
}

// ---------------------------------------------------------------------------
// Main test
// ---------------------------------------------------------------------------

#[test]
fn lsp_diagnostics_diff() {
    let v1_root = Path::new(LSP_V1_ROOT);
    let v2_root = Path::new(LSP_V2_ROOT);
    assert!(v1_root.exists(), "LSP v1 test files not found at {LSP_V1_ROOT}");
    assert!(v2_root.exists(), "LSP v2 test files not found at {LSP_V2_ROOT}");

    let mut files: Vec<FileDiff> = Vec::new();
    let mut summary = Summary {
        total_files: 0,
        expectations_match: 0,
        expectations_differ: 0,
        v2_passing: 0,
        v2_failing: 0,
        v2_panic: 0,
    };

    // Walk all .baml files in v1 syntax test dir
    let mut entries: Vec<PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(v1_root)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "baml"))
    {
        entries.push(entry.path().to_path_buf());
    }
    entries.sort();

    for v1_path in &entries {
        let relative = v1_path.strip_prefix(v1_root).unwrap();
        let filename = relative.to_str().unwrap();
        let v2_path = v2_root.join(relative);

        if !v2_path.exists() {
            continue; // Skip files that only exist in v1
        }

        summary.total_files += 1;

        let v1_content = fs::read_to_string(v1_path).unwrap();
        let v2_content = fs::read_to_string(&v2_path).unwrap();

        let v1_expected = extract_diagnostics_expectation(&v1_content);
        let v2_expected = extract_diagnostics_expectation(&v2_content);

        let expectations_match = v1_expected == v2_expected;
        if expectations_match {
            summary.expectations_match += 1;
        } else {
            summary.expectations_differ += 1;
        }

        // Use the v2 content for source (it may have updated virtual files)
        let default_name = relative
            .to_str()
            .unwrap()
            .replace(['/', '\\'], "_");
        let virtual_files = extract_source_and_files(&v2_content, &default_name);

        let (v2_actual, status) = match run_through_compiler2(&virtual_files) {
            Ok(actual) => {
                let passes = actual.trim() == v2_expected.trim();
                if passes {
                    summary.v2_passing += 1;
                } else {
                    summary.v2_failing += 1;
                }

                let status = if expectations_match && passes {
                    "OK".to_string()
                } else if expectations_match && !passes {
                    "ACTUAL_DIFFERS".to_string()
                } else if !expectations_match && passes {
                    "UPDATED_AND_PASSING".to_string()
                } else {
                    "UPDATED_BUT_FAILING".to_string()
                };

                (actual, status)
            }
            Err(panic_msg) => {
                summary.v2_panic += 1;
                (String::new(), format!("PANIC: {panic_msg}"))
            }
        };

        files.push(FileDiff {
            file: filename.to_string(),
            expectations_match,
            v2_passes: v2_actual.trim() == v2_expected.trim(),
            v1_expected: v1_expected.clone(),
            v2_expected: v2_expected.clone(),
            v2_actual: v2_actual.clone(),
            status,
        });
    }

    let report = DiffReport { summary, files };

    // Write JSON report
    let json = serde_json::to_string_pretty(&report).unwrap();
    fs::write(REPORT_PATH, &json).unwrap();

    // Print summary
    println!("\n=== LSP V1 vs V2 Diagnostics Diff ===");
    println!("Total files:            {}", report.summary.total_files);
    println!("Expectations match:     {}", report.summary.expectations_match);
    println!("Expectations differ:    {} (compiler2 changed behavior)", report.summary.expectations_differ);
    println!("V2 passing:             {}", report.summary.v2_passing);
    println!("V2 failing:             {} <-- need attention", report.summary.v2_failing);
    println!("V2 panic:               {}", report.summary.v2_panic);
    println!("\nFull report: {REPORT_PATH}");

    // Print details for failing files
    let failing: Vec<_> = report
        .files
        .iter()
        .filter(|f| !f.v2_passes && !f.status.starts_with("PANIC"))
        .collect();
    if !failing.is_empty() {
        println!("\n--- V2 FAILING ({} files) ---", failing.len());
        for f in &failing {
            println!("  {} [{}]", f.file, f.status);
        }
    }

    // Print files where expectations were updated
    let updated: Vec<_> = report
        .files
        .iter()
        .filter(|f| !f.expectations_match)
        .collect();
    if !updated.is_empty() {
        println!("\n--- EXPECTATIONS DIFFER ({} files) ---", updated.len());
        for f in &updated {
            let pass_str = if f.v2_passes { "PASS" } else { "FAIL" };
            println!("  {} [{}]", f.file, pass_str);
        }
    }

    // Print panics
    let panics: Vec<_> = report
        .files
        .iter()
        .filter(|f| f.status.starts_with("PANIC"))
        .collect();
    if !panics.is_empty() {
        println!("\n--- PANICS ({} files) ---", panics.len());
        for f in &panics {
            println!("  {} -> {}", f.file, f.status);
        }
    }
}
