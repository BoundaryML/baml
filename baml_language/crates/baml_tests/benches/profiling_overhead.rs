//! Profiling (BEX tracing) overhead benchmarks.
//!
//! Run with: cargo bench --bench profiling_overhead
//!
//! The ordinary wall-time benches (`runtime_benchmark`, `compiler_benchmark`)
//! pin `BAML_PROFILE=0` so their numbers are hermetic. This target is the
//! deliberate counterpart: the same VM harness with profiling pinned ON, over a
//! small fixed workload subset chosen to characterize tracing cost —
//! per-call ring-pair overhead (`pure_call_1m`), allocation-heavy loops
//! (`array_build_sum_100k`), the known ring-overflow reproducer
//! (`fib32_recursive`, which aborted at the default 1 GiB cap on bare metal),
//! and a string baseline (`concat_loop_10k`).
//!
//! Overhead ratio = this target's median ÷ the same workload's median in
//! `runtime_benchmark`. The subset sources come from the same speedtest corpus
//! export as the main benches (single source of truth; see `build.rs`).
//!
//! Environment pins (each only when the caller hasn't set a value):
//! - `BAML_PROFILE=1` — profiling explicitly ON.
//! - `BAML_RING_MAX_OVERFLOW_BYTES=4294967296` — 4 GiB headroom so hot
//!   workloads measure tracing cost instead of aborting on the 1 GiB default.
//! - `BAML_PROFILE_DIR=<fresh temp dir>` — events sink to a throwaway
//!   location instead of the working directory.

use std::{path::Path, sync::Arc};

use baml_compiler2_emit::{CompileOptions, generate_project_bytecode};
use baml_db::ProjectDatabase;
use baml_tests::engine::TestDbExt;
use bex_engine::{BexEngine, FunctionCallContextBuilder};
use divan::{Bencher, black_box};
use sys_native::{CallId, SysOpsExt};

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("Skipping profiling_overhead in debug/test profile.");
        return;
    }
    if std::env::var_os("DIVAN_MAX_TIME").is_none() {
        // SAFETY: single-threaded here at the very top of main, before divan
        // reads its args/env. No other thread can observe the environment.
        unsafe { std::env::set_var("DIVAN_MAX_TIME", "2") };
    }
    if std::env::var_os("BAML_PROFILE").is_none() {
        // SAFETY: as above.
        unsafe { std::env::set_var("BAML_PROFILE", "1") };
    }
    if std::env::var_os("BAML_RING_MAX_OVERFLOW_BYTES").is_none() {
        // SAFETY: as above.
        unsafe { std::env::set_var("BAML_RING_MAX_OVERFLOW_BYTES", "4294967296") };
    }
    // Drop-backed so the sink and its artifacts are removed when the run ends;
    // only used when the caller didn't choose a sink of their own.
    let _sink_dir = if std::env::var_os("BAML_PROFILE_DIR").is_none() {
        let dir = tempfile::Builder::new()
            .prefix("baml-profiling-overhead-bench-")
            .tempdir()
            .expect("failed to create profile sink dir");
        // SAFETY: as above.
        unsafe { std::env::set_var("BAML_PROFILE_DIR", dir.path()) };
        Some(dir)
    } else {
        None
    };
    divan::main();
}

// ============================================================================
// Helpers — duplicated from runtime_benchmark.rs (bench targets are separate
// binaries and cannot share modules without moving these into the crate's lib)
// ============================================================================

/// Compile BAML source into a ready-to-run engine.
fn compile_source(source: &str) -> (ProjectDatabase, BexEngine) {
    let mut db = ProjectDatabase::new();
    db.workspace(Path::new("."));
    db.file("bench.baml", source);
    let bytecode = generate_project_bytecode(
        &db,
        &CompileOptions {
            emit_test_cases: false,
        },
    )
    .expect("benchmark compilation failed");
    let engine = BexEngine::new(bytecode, Arc::new(sys_native::SysOps::native()), vec![])
        .expect("benchmark engine creation failed");
    (db, engine)
}

/// Compile once, then measure only the cost of calling `main()` — identical
/// harness to `runtime_benchmark::bench_vm_main` so medians divide cleanly.
/// Workload sources are empty when the speedtest corpus was unavailable at
/// build time; the bench then skips itself.
fn bench_vm_main(bencher: Bencher, source: &str) {
    if source.is_empty() {
        eprintln!("speedtest corpus unavailable at build time; skipping");
        return;
    }
    let (_db, engine) = compile_source(source);
    let engine = Arc::new(engine);
    let rt = tokio::runtime::Runtime::new().expect("failed to build tokio runtime");
    bencher.bench(|| {
        black_box(
            rt.block_on(engine.call_function(
                "main",
                vec![],
                FunctionCallContextBuilder::new(CallId::next()).build(),
                true,
            ))
            .expect("benchmark execution failed"),
        )
    });
}

// Subset sources exported by build.rs from the speedtest corpus.
include!(concat!(env!("OUT_DIR"), "/speedtest_profiling_sources.rs"));

/// compute::pure call 1m — per-call CallFunction/EndFunction ring-pair cost.
#[divan::bench]
fn prof_on_compute_pure_call_1m(bencher: Bencher) {
    bench_vm_main(bencher, PROF_SRC_COMPUTE_PURE_CALL_1M);
}

/// compute::array build sum 100k — allocation-heavy loop under tracing.
#[divan::bench]
fn prof_on_compute_array_build_sum_100k(bencher: Bencher) {
    bench_vm_main(bencher, PROF_SRC_COMPUTE_ARRAY_BUILD_SUM_100K);
}

/// compute::fib32 recursive — the historical ring-overflow reproducer.
#[divan::bench]
fn prof_on_compute_fib32_recursive(bencher: Bencher) {
    bench_vm_main(bencher, PROF_SRC_COMPUTE_FIB32_RECURSIVE);
}

/// string::concat loop 10k — string-path baseline under tracing.
#[divan::bench]
fn prof_on_string_concat_loop_10k(bencher: Bencher) {
    bench_vm_main(bencher, PROF_SRC_STRING_CONCAT_LOOP_10K);
}
