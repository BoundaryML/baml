//! BAML Compiler Benchmarks
//!
//! Run with: cargo bench --bench compiler_benchmark
//!
//! These complement the VM execution benchmarks in runtime_benchmark.rs.
//! Compiler benchmarks measure the cost of turning BAML source into a bytecode
//! `Program` (parse → HIR → TIR → MIR → emit); the runtime benchmarks measure
//! how fast that bytecode then executes.
//!
//! Two reference workloads are provided:
//!   * `compile_empty_project` — an empty project (a single file with nothing
//!     but a comment). With no user code to lower, the wall-clock here is the
//!     compiler's *constant overhead* (loading builtin stubs, Salsa setup,
//!     empty-program emit), so it acts as a fixed-cost baseline.
//!   * `compile_baml_tests_project` — the whole `baml_src/` test project (dozens
//!     of files exercising every language feature). This is the realistic
//!     multi-file compilation workload.

use std::path::{Path, PathBuf};

use baml_compiler2_emit::{CompileOptions, generate_project_bytecode};
use baml_db::{ProjectDatabase, discover_baml_files};
use baml_tests::engine::TestDbExt;
use divan::{Bencher, black_box};

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("Skipping compiler_benchmark in debug/test profile.");
        return;
    }
    // Hermetic wall-time: pin BAML profiling OFF unless the caller explicitly
    // chose a value, so compile timings don't include the default-ON tracing
    // pipeline. See runtime_benchmark.rs for the full rationale; tracing cost
    // is measured deliberately in the `profiling_overhead` bench target.
    if std::env::var_os("BAML_PROFILE").is_none() {
        // SAFETY: single-threaded at the very top of main, before any engine
        // or divan code reads the environment.
        unsafe { std::env::set_var("BAML_PROFILE", "0") };
    }
    divan::main();
}

// ============================================================================
// Helpers
// ============================================================================

/// A project loaded from disk as `(canonical_path, contents)` pairs.
///
/// Reading the source files off disk is done once, up front, so it never
/// pollutes the measured region — each benchmark sample only pays for building
/// a fresh `ProjectDatabase` and compiling it.
type ProjectSources = Vec<(PathBuf, String)>;

/// Read every `.baml` file under `root` into memory.
fn read_project(root: &Path) -> ProjectSources {
    discover_baml_files(root)
        .into_iter()
        .map(|path| {
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            (path, content)
        })
        .collect()
}

/// Build a fresh [`ProjectDatabase`] rooted at `root` and populated with
/// `sources`. Mirrors how the CLI loads a project (the workspace root first,
/// then add each discovered file) so the compiler sees the same project shape.
fn build_db(root: &Path, sources: &ProjectSources) -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.workspace(root);
    for (path, content) in sources {
        db.file(path, content);
    }
    db
}

/// Compile a populated database to bytecode, measuring only the compile step.
fn compile(db: &ProjectDatabase) {
    let program = generate_project_bytecode(
        db,
        &CompileOptions {
            emit_test_cases: true,
        },
    )
    .expect("benchmark compilation failed");
    black_box(program);
}

/// Benchmark compiling the project rooted at `root`: read sources once, then
/// per sample build a fresh (uncached) database and compile it. A fresh
/// database is required because Salsa would otherwise memoize the first
/// compile and every later sample would measure a cache hit.
fn bench_compile_project(bencher: Bencher, root: &Path) {
    let sources = read_project(root);
    bencher
        .with_inputs(|| build_db(root, &sources))
        .bench_values(|db| compile(&db));
}

// ============================================================================
// Benchmarks
// ============================================================================

/// Constant-overhead baseline: an empty project (the `projects/empty` fixture,
/// whose only file is a comment). With no user code to compile, the measured
/// time is the compiler's fixed cost (builtin stubs, Salsa setup, empty emit).
#[divan::bench]
fn compile_empty_project(bencher: Bencher) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("projects/empty");
    bench_compile_project(bencher, &root);
}

/// Realistic multi-file workload: the full `baml_src/` test project.
///
/// A single compile of this project takes seconds, so the sample count is
/// capped at a handful (one compile per sample) to keep the overall benchmark
/// runtime reasonable — the per-sample cost is large enough that even a few
/// samples give a stable median.
#[divan::bench(sample_count = 5, sample_size = 1)]
fn compile_baml_tests_project(bencher: Bencher) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("baml_src");
    bench_compile_project(bencher, &root);
}
