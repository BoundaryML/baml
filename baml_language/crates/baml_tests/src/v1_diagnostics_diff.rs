//! V1 vs V2 (compiler2) diagnostics diff harness.
//!
//! Reads V1 validation files from `engine/baml-lib/baml/tests/validation_files/`,
//! runs each through compiler2's `collect_diagnostics()`, and produces a structured
//! JSON report comparing the expected V1 output against compiler2's actual output.
//!
//! Run:
//!   cargo test -p baml_tests v1_diagnostics_diff -- --nocapture
//!
//! The report is written to:
//!   baml_language/crates/baml_tests/v1_v2_diagnostics_diff.json

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use baml_compiler_diagnostics::{RenderConfig, Severity, render_diagnostic};
use baml_project::{ProjectDatabase, collect_diagnostics};
use serde::Serialize;

/// Root of the V1 validation files, relative to this crate's manifest dir.
const V1_ROOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../engine/baml-lib/baml/tests/validation_files"
);

/// Where to write the JSON report.
const REPORT_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/v1_v2_diagnostics_diff.json"
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
    ok_clean: usize,
    has_both: usize,
    v2_new: usize,
    v2_missing: usize,
    v2_panic: usize,
}

#[derive(Debug, Serialize)]
struct FileDiff {
    file: String,
    status: String,
    v1_error_count: usize,
    v2_error_count: usize,
    v1_errors: Vec<String>,
    v2_errors: Vec<String>,
    v1_raw: String,
    v2_raw: String,
}

// ---------------------------------------------------------------------------
// V1 expected-output extraction (mirrors V1's `run_validation_test` logic)
// ---------------------------------------------------------------------------

/// Extract the trailing `// ` comment block from a V1 validation file.
/// Returns `None` if there is no trailing comment block (file should compile clean).
fn extract_v1_expected(source: &str) -> Option<String> {
    let mut idx = None;
    let newlines = source.char_indices().filter(|(_, c)| *c == '\n');

    for (newline_idx, _) in newlines {
        match (source.get(newline_idx + 1..newline_idx + 3), idx) {
            (Some("//"), None) => idx = Some(newline_idx + 1),
            (Some("//"), Some(_)) => (),
            (None, _) => (),
            (Some(_), _) => idx = None,
        }
    }

    idx.map(|i| {
        let mut out = String::new();
        for line in source[i..].lines() {
            out.push_str(line.trim_start_matches("// "));
            out.push('\n');
        }
        out
    })
}

/// Parse V1 expected output into individual error message lines.
/// V1 errors start with "error:" or "warning:" at the beginning of a line.
fn extract_v1_error_messages(raw: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let mut current: Option<String> = None;

    for line in raw.lines() {
        if line.starts_with("error:") || line.starts_with("warning:") {
            if let Some(err) = current.take() {
                errors.push(err);
            }
            current = Some(line.to_string());
        } else if current.is_some() {
            // continuation line — part of the span display, skip
        }
    }
    if let Some(err) = current {
        errors.push(err);
    }

    errors
}

// ---------------------------------------------------------------------------
// Compiler2 runner
// ---------------------------------------------------------------------------

/// Run a single file through compiler2 and return rendered error diagnostics.
fn run_through_compiler2(filename: &str, source: &str) -> Result<(String, Vec<String>), String> {
    // Catch panics from compiler2 so one bad file doesn't kill the whole run.
    let filename = filename.to_string();
    let source = source.to_string();

    std::panic::catch_unwind(|| {
        let mut db = ProjectDatabase::new();
        db.set_project_root(Path::new("."));
        db.add_file(&filename, &source);

        let project = db.get_project().expect("project must be set");
        let all_files = db.get_source_files();
        let diagnostics = collect_diagnostics(&db, project, &all_files);

        // Build rendering context
        let mut sources: HashMap<baml_db::FileId, String> = HashMap::new();
        let mut file_paths: HashMap<baml_db::FileId, PathBuf> = HashMap::new();
        for sf in &all_files {
            let fid = sf.file_id(&db);
            sources.insert(fid, sf.text(&db).to_string());
            file_paths.insert(fid, sf.path(&db));
        }

        let config = RenderConfig::test();

        // Filter to errors only (V1 expected output is errors, not warnings)
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| matches!(d.severity, Severity::Error))
            .collect();

        let error_messages: Vec<String> = errors
            .iter()
            .map(|d| d.message.clone())
            .collect();

        let raw = if errors.is_empty() {
            String::new()
        } else {
            errors
                .iter()
                .map(|d| render_diagnostic(d, &sources, &file_paths, &config))
                .collect::<Vec<_>>()
                .join("\n")
        };

        (raw, error_messages)
    })
    .map_err(|e| {
        let msg = if let Some(s) = e.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = e.downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        };
        msg
    })
}

// ---------------------------------------------------------------------------
// Main test
// ---------------------------------------------------------------------------

#[test]
fn v1_diagnostics_diff() {
    let v1_root = Path::new(V1_ROOT);
    assert!(v1_root.exists(), "V1 validation files not found at {V1_ROOT}");

    let mut files: Vec<FileDiff> = Vec::new();
    let mut summary = Summary {
        total_files: 0,
        ok_clean: 0,
        has_both: 0,
        v2_new: 0,
        v2_missing: 0,
        v2_panic: 0,
    };

    // Walk all .baml files, sorted for deterministic output.
    let mut entries: Vec<_> = walkdir::WalkDir::new(v1_root)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map_or(false, |ext| ext == "baml")
        })
        .collect();
    entries.sort_by_key(|e| e.path().to_path_buf());

    for entry in &entries {
        let path = entry.path();
        let relative = path.strip_prefix(v1_root).unwrap();
        let filename = relative.to_str().unwrap();

        // Skip headers (V1 already excludes them in its test generation)
        if filename.starts_with("headers/") || filename.starts_with("headers\\") {
            continue;
        }

        summary.total_files += 1;

        let source = fs::read_to_string(path).unwrap();
        let v1_expected = extract_v1_expected(&source);
        let v1_errors = v1_expected
            .as_ref()
            .map(|raw| extract_v1_error_messages(raw))
            .unwrap_or_default();

        let (status, v2_raw, v2_errors) = match run_through_compiler2(filename, &source) {
            Ok((raw, msgs)) => {
                let status = match (v1_expected.is_some(), !raw.is_empty()) {
                    (false, false) => {
                        summary.ok_clean += 1;
                        "OK_CLEAN"
                    }
                    (true, true) => {
                        summary.has_both += 1;
                        "HAS_BOTH"
                    }
                    (false, true) => {
                        summary.v2_new += 1;
                        "V2_NEW"
                    }
                    (true, false) => {
                        summary.v2_missing += 1;
                        "V2_MISSING"
                    }
                };
                (status.to_string(), raw, msgs)
            }
            Err(panic_msg) => {
                summary.v2_panic += 1;
                (
                    format!("V2_PANIC: {panic_msg}"),
                    String::new(),
                    Vec::new(),
                )
            }
        };

        files.push(FileDiff {
            file: filename.to_string(),
            status,
            v1_error_count: v1_errors.len(),
            v2_error_count: v2_errors.len(),
            v1_errors,
            v2_errors,
            v1_raw: v1_expected.unwrap_or_default(),
            v2_raw,
        });
    }

    let report = DiffReport { summary, files };

    // Write JSON report
    let json = serde_json::to_string_pretty(&report).unwrap();
    fs::write(REPORT_PATH, &json).unwrap();

    // Print summary to stdout
    println!("\n=== V1 vs V2 Diagnostics Diff ===");
    println!("Total files:  {}", report.summary.total_files);
    println!("OK_CLEAN:     {}", report.summary.ok_clean);
    println!("HAS_BOTH:     {}", report.summary.has_both);
    println!("V2_NEW:       {}", report.summary.v2_new);
    println!("V2_MISSING:   {} <-- regressions", report.summary.v2_missing);
    println!("V2_PANIC:     {}", report.summary.v2_panic);
    println!("\nFull report: {REPORT_PATH}");

    // Print details for regressions
    let regressions: Vec<_> = report.files.iter().filter(|f| f.status == "V2_MISSING").collect();
    if !regressions.is_empty() {
        println!("\n--- V2_MISSING (regressions) ---");
        for f in &regressions {
            println!("  {} ({} V1 errors)", f.file, f.v1_error_count);
            for err in &f.v1_errors {
                println!("    V1: {err}");
            }
        }
    }

    // Print details for panics
    let panics: Vec<_> = report.files.iter().filter(|f| f.status.starts_with("V2_PANIC")).collect();
    if !panics.is_empty() {
        println!("\n--- V2_PANIC ---");
        for f in &panics {
            println!("  {} -> {}", f.file, f.status);
        }
    }
}
