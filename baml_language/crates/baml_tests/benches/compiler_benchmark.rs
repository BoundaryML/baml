//! BAML Compiler (compiler2) Cold-Compile Benchmarks
//!
//! Run with:
//!   cargo bench -p baml_tests --bench compiler_benchmark
//!
//! Per-stage single-shot profile (no divan harness, plain wall-clock split):
//!   BAML_PROFILE=1 cargo bench -p baml_tests --bench compiler_benchmark
//!
//! Environment overrides:
//!   BAML_CORPUS=<abs project dir>   (default: crates/baml_tests/baml_src)
//!   BAML_RUNS=<n>                    (single-shot repeat count for the profile path)
//!
//! What this measures:
//!   A FULL cold compile of a multi-file BAML project all the way through
//!   bytecode via `generate_project_bytecode_with_opt`. Each measured divan
//!   iteration constructs a FRESH `ProjectDatabase` so salsa memoization does
//!   not make subsequent runs free -- i.e. every sample is a cold compile.
//!
//! Why a fresh db per iter: salsa caches query results inside a single db. The
//! db is built in the `with_inputs` setup closure (NOT timed) and only
//! `generate_project_bytecode_with_opt` is timed inside `bench_values`.

use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use baml_compiler2_emit::{generate_project_bytecode_with_opt, CompileOptions, OptLevel};
use baml_project::{collect_compiler2_diagnostics, ProjectDatabase};
use divan::{black_box, Bencher};

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("Skipping compiler_benchmark in debug/test profile (build with --release).");
        return;
    }
    // Plain single-shot per-stage / split profile, bypassing the divan harness.
    if std::env::var("BAML_PROFILE").ok().as_deref() == Some("1") {
        run_profile();
        return;
    }
    divan::main();
}

// ============================================================================
// Corpus loading
// ============================================================================

/// Resolve the corpus directory: BAML_CORPUS env override, else the in-repo
/// baml_src project relative to this crate.
fn corpus_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("BAML_CORPUS") {
        return PathBuf::from(dir);
    }
    // CARGO_MANIFEST_DIR points at crates/baml_tests at build time.
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join("baml_src")
}

/// Recursively collect every `*.baml` file under `root`, returning
/// (relative_path_string, contents). Sorted for determinism.
fn collect_baml_files(root: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    collect_rec(root, root, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn collect_rec(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rec(root, &path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("baml") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                out.push((rel, content));
            }
        }
    }
}

/// Build a FRESH ProjectDatabase with the whole corpus loaded but NOT yet
/// compiled. All file reads happen in `collect_baml_files` (outside this fn) so
/// they are excluded from the timed region; `add_file` here is part of setup.
fn fresh_db(root: &Path, files: &[(String, String)]) -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.set_project_root(root);
    for (rel, content) in files {
        db.add_file(rel, content);
    }
    db
}

fn compile_opts() -> CompileOptions {
    CompileOptions {
        emit_test_cases: false,
    }
}

// ============================================================================
// Divan benchmark: cold compile through bytecode
// ============================================================================

#[divan::bench(sample_count = 5, sample_size = 1)]
fn cold_compile_to_bytecode(bencher: Bencher) {
    let root = corpus_dir();
    let files = collect_baml_files(&root);
    assert!(
        !files.is_empty(),
        "no .baml files found under corpus dir {}",
        root.display()
    );

    bencher
        // Setup (NOT timed): a fresh db per sample so every run is cold.
        .with_inputs(|| fresh_db(&root, &files))
        .bench_values(|db| {
            let program =
                generate_project_bytecode_with_opt(&db, &compile_opts(), OptLevel::Two)
                    .expect("benchmark compilation failed");
            black_box(program)
        });
}

// ============================================================================
// Single-shot per-stage profile (BAML_PROFILE=1)
// ============================================================================

fn run_profile() {
    let root = corpus_dir();
    let files = collect_baml_files(&root);
    let runs: usize = std::env::var("BAML_RUNS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    eprintln!(
        "BAML compiler profile: corpus={} files={} runs={}",
        root.display(),
        files.len(),
        runs
    );

    for run in 0..runs {
        // Fresh db => cold compile. db build (add_file + salsa input set) is
        // measured separately as "load".
        let t_load = Instant::now();
        let db = fresh_db(&root, &files);
        let load_ms = t_load.elapsed().as_secs_f64() * 1000.0;

        // Full end-to-end cold compile through bytecode.
        let t_total = Instant::now();
        let program = generate_project_bytecode_with_opt(&db, &compile_opts(), OptLevel::Two)
            .expect("profile compilation failed");
        let total_ms = t_total.elapsed().as_secs_f64() * 1000.0;
        black_box(&program);

        eprintln!("run {run}: load_db={load_ms:.1}ms  cold_compile_to_bytecode={total_ms:.1}ms");

        // Frontend-vs-codegen split + incremental no-op on independent fresh dbs.
        split_profile(&root, &files);
    }
}

/// Coarse frontend/codegen split. On a fresh db, run the public
/// `collect_compiler2_diagnostics` driver (runs the full compiler2 frontend:
/// parse + HIR + PPIR + TIR type-checking for all files, collecting
/// diagnostics, but stops before bytecode emission) and time it; then run full
/// bytecode generation on the
/// SAME (now frontend-warm) db and report the remaining cost as the codegen +
/// MIR/emit tail. Finally, a no-op recompile on the fully-warm db gives the
/// incremental re-query number.
fn split_profile(root: &Path, files: &[(String, String)]) {
    let db = fresh_db(root, files);

    let t_front = Instant::now();
    let diags = collect_compiler2_diagnostics(&db);
    let front_ms = t_front.elapsed().as_secs_f64() * 1000.0;
    black_box(&diags);

    let t_codegen = Instant::now();
    let program = generate_project_bytecode_with_opt(&db, &compile_opts(), OptLevel::Two)
        .expect("split profile compilation failed");
    let codegen_ms = t_codegen.elapsed().as_secs_f64() * 1000.0;
    black_box(&program);

    eprintln!(
        "  split: frontend_through_typecheck(check)={front_ms:.1}ms  \
         codegen_tail(bytecode, frontend warm)={codegen_ms:.1}ms"
    );

    // Incremental no-op re-query on the now fully-warm db: no edits.
    let t_noop = Instant::now();
    let program2 = generate_project_bytecode_with_opt(&db, &compile_opts(), OptLevel::Two)
        .expect("noop recompile failed");
    let noop_ms = t_noop.elapsed().as_secs_f64() * 1000.0;
    black_box(&program2);
    eprintln!("  incremental_noop_recompile(warm db)={noop_ms:.1}ms");
}
