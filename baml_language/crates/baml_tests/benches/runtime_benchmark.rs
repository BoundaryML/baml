//! BAML Runtime / VM Execution Benchmarks
//!
//! Run with: cargo bench --bench runtime_benchmark
//!
//! Every bench in this file is auto-generated from the cross-language speedtest
//! corpus under `tools/speedtest/workloads/*.md` (see `build.rs`:
//! `generate_speedtest_benches`). There are intentionally no hand-written
//! benches here — to add or change one, edit a workload `.md` so the speedtest
//! harness and CodSpeed stay in sync from a single source of truth.
//!
//! Each generated bench measures *pure VM execution*: the BAML source is
//! compiled and the tokio runtime is built ONCE, outside the measured region
//! (see `bench_vm_main`), so only the cost of calling `main()` is timed.

use std::{path::Path, sync::Arc};

use baml_compiler2_emit::generate_project_bytecode;
use baml_db::ProjectDatabase;
use baml_tests::engine::TestDbExt;
use bex_engine::{BexEngine, FunctionCallContextBuilder};
use divan::{Bencher, black_box};
use sys_native::{CallId, SysOpsExt};

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("Skipping runtime_benchmark in debug/test profile.");
        return;
    }
    // Adaptive sampling by default: bound each bench to ~2s of wall time rather
    // than walltime mode's fixed 100 samples/bench. divan then runs as many
    // samples as fit the budget — ~100 for cheap benches (full statistical
    // power), down to 1 for the heavy O(n²)/1M-iter workloads (whose run-to-run
    // variance is already tiny). Without this the suite blows past CI's 30-min
    // timeout: compute::bubble_sort_5k alone is ~14s/sample on the codspeed
    // runner, so 100 samples ≈ 23min for ONE bench. CI overrides the budget via
    // the DIVAN_MAX_TIME env var (see ci.yaml); we only set a default when the
    // caller hasn't, so `cargo bench`/`cargo codspeed run` are adaptive too.
    if std::env::var_os("DIVAN_MAX_TIME").is_none() {
        // SAFETY: single-threaded here at the very top of main, before divan
        // reads its args/env. No other thread can observe the environment.
        unsafe { std::env::set_var("DIVAN_MAX_TIME", "2") };
    }
    // Hermetic wall-time: BAML profiling ships default-ON, which both skews
    // per-call timings and can abort hot workloads outright when the event
    // ring's overflow cap is hit (compute::fib32_recursive reproduced this at
    // the 1 GiB cap on bare metal). Pin it OFF unless the caller explicitly
    // chose a value; tracing cost is measured deliberately in the
    // `profiling_overhead` bench target instead of contaminating every number
    // here.
    if std::env::var_os("BAML_PROFILE").is_none() {
        // SAFETY: as above — single-threaded, before any engine reads env.
        unsafe { std::env::set_var("BAML_PROFILE", "0") };
    }
    divan::main();
}

// ============================================================================
// Helpers (shared by every generated workload bench)
// ============================================================================

/// Compile BAML source into a ready-to-run engine.
fn compile_source(source: &str) -> (ProjectDatabase, BexEngine) {
    let mut db = ProjectDatabase::new();
    db.workspace(Path::new("."));
    db.file("bench.baml", source);
    let bytecode = generate_project_bytecode(&db).expect("benchmark compilation failed");
    let engine = BexEngine::new(bytecode, Arc::new(sys_native::SysOps::native()), vec![])
        .expect("benchmark engine creation failed");
    (db, engine)
}

/// Benchmark a pure-runtime workload: compile `source` and build the tokio
/// runtime ONCE (outside the measured region), then measure only the cost of
/// calling `main()` on the resulting engine.
///
/// `BexEngine::call_function` takes `&Arc<Self>` and builds a fresh call context
/// per call, so one compiled engine serves every sample — there is no need to
/// recompile (the expensive part) on each iteration.
fn bench_vm_main(bencher: Bencher, source: &str) {
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

// ============================================================================
// Speedtest workload benches (auto-generated)
// ============================================================================
//
// One `vm_speedtest_<slug>` bench per workload under tools/speedtest/workloads/,
// with the BAML source lifted from that corpus at build time. Workloads that
// call the blocking `baml.sys.sleep` are excluded (their wall-clock time is
// dominated by sleeping, not VM work). If the corpus is unavailable at build
// time this include expands to a comment and no benches are added.
include!(concat!(env!("OUT_DIR"), "/speedtest_benches.rs"));
