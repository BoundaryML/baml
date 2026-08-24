//! Direct profiling backend overhead benchmarks.
//!
//! Run with: cargo bench --bench profiling_overhead
//!
//! Each benchmark injects an immutable on/off session, so paired cases run in
//! one process without environment mutation. On-mode stores use a fresh
//! temporary `profiles-v1` root.
//!
//! Overhead ratio = this target's median ÷ the same workload's median in
//! `runtime_benchmark`. The subset sources come from the same speedtest corpus
//! export as the main benches (single source of truth; see `build.rs`).
//!
use std::{path::Path, sync::Arc, time::Duration};

use baml_compiler2_emit::{CompileOptions, generate_project_bytecode};
use baml_project::ProjectDatabase;
use bex_engine::{BexEngine, FunctionCallContextBuilder, logger::TraceLogger};
use bex_events::prof::backend::{DiskBudget, ProfilerConfig, ProfilerSession, list_executions};
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
    divan::main();
}

// ============================================================================
// Helpers — duplicated from runtime_benchmark.rs (bench targets are separate
// binaries and cannot share modules without moving these into the crate's lib)
// ============================================================================

#[derive(Clone, Copy)]
enum BenchMode {
    Off,
    On,
    Suppressed,
    LoggingOnly,
}

/// Compile BAML source into a ready-to-run engine and retain its store root.
fn compile_source(
    source: &str,
    mode: BenchMode,
) -> (ProjectDatabase, BexEngine, tempfile::TempDir) {
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new("."));
    db.add_file("bench.baml", source);
    let bytecode = generate_project_bytecode(
        &db,
        &CompileOptions {
            emit_test_cases: false,
        },
    )
    .expect("benchmark compilation failed");
    let store = tempfile::Builder::new()
        .prefix("baml-profiling-overhead-bench-")
        .tempdir()
        .expect("failed to create profile store");
    let (session, diagnostic) = ProfilerSession::from_config(ProfilerConfig {
        enabled: matches!(mode, BenchMode::On | BenchMode::Suppressed),
        store_root: store.path().join("profiles-v1"),
        // The recursive stress row produces several hundred MiB of compact
        // records in one burst. Give the performance process enough transient
        // headroom that it measures admitted work; 32/256 MiB production
        // sizing and denial behavior are pinned separately by backend tests.
        process_memory_bytes: 1024 * 1024 * 1024,
        disk: DiskBudget {
            max_project_bytes: 10 * 1024 * 1024 * 1024,
            minimum_free_bytes: 0,
        },
        publish_interval: Duration::from_secs(1),
        stream: None,
    });
    assert!(diagnostic.is_none(), "{diagnostic:?}");
    let engine = BexEngine::new_with_profiler_session(
        bytecode,
        Arc::new(sys_native::SysOps::native()),
        vec![],
        session,
    )
    .expect("benchmark engine creation failed");
    (db, engine, store)
}

/// Compile once, then measure only the cost of calling `main()` — identical
/// harness to `runtime_benchmark::bench_vm_main` so medians divide cleanly.
/// Workload sources are empty when the speedtest corpus was unavailable at
/// build time; the bench then skips itself.
fn bench_vm_main(bencher: Bencher, source: &str, mode: BenchMode) {
    if source.is_empty() {
        eprintln!("speedtest corpus unavailable at build time; skipping");
        return;
    }
    let (_db, engine, store) = compile_source(source, mode);
    let engine = Arc::new(engine);
    let rt = tokio::runtime::Runtime::new().expect("failed to build tokio runtime");
    let logger = matches!(mode, BenchMode::LoggingOnly).then(|| TraceLogger::bounded(2048));
    bencher
        .with_inputs(|| {
            // Divan excludes input generation from the measurement. Start
            // every sample with an idle backend so work from the previous
            // sample cannot inflate producer-path timing.
            if matches!(mode, BenchMode::On | BenchMode::Suppressed) {
                assert!(bex_events::prof::flush_and_join(Duration::from_secs(30)));
            }
        })
        .bench_values(|()| {
            let mut context = FunctionCallContextBuilder::new(CallId::next());
            if matches!(mode, BenchMode::Suppressed) {
                context = context.suppress_internal_profile();
            }
            if let Some(logger) = &logger {
                context = context.with_logger(logger.clone());
            }
            black_box(
                rt.block_on(engine.call_function("main", vec![], context.build(), true))
                    .expect("benchmark execution failed"),
            );
            if let Some(logger) = &logger {
                black_box(logger.drain_encoded_logs());
            }
        });
    if matches!(mode, BenchMode::On) {
        assert!(bex_events::prof::flush_and_join(Duration::from_secs(30)));
        let executions = list_executions(&store.path().join("profiles-v1"))
            .expect("on benchmark must publish a readable stream");
        assert!(
            !executions.is_empty(),
            "on benchmark must publish executions"
        );
        for execution in executions {
            let health = execution
                .health
                .expect("benchmark execution must be durably ended");
            assert_eq!(
                health.structural_transport_exceeded, 0,
                "performance samples are invalid when transport records were lost"
            );
        }
    }
}

// Subset sources exported by build.rs from the speedtest corpus.
include!(concat!(env!("OUT_DIR"), "/speedtest_profiling_sources.rs"));

/// compute::pure call 1m — per-call CallFunction/EndFunction ring-pair cost.
#[divan::bench]
fn prof_on_compute_pure_call_1m(bencher: Bencher) {
    bench_vm_main(bencher, PROF_SRC_COMPUTE_PURE_CALL_1M, BenchMode::On);
}

#[divan::bench]
fn prof_off_compute_pure_call_1m(bencher: Bencher) {
    bench_vm_main(bencher, PROF_SRC_COMPUTE_PURE_CALL_1M, BenchMode::Off);
}

#[divan::bench]
fn prof_suppressed_compute_pure_call_1m(bencher: Bencher) {
    bench_vm_main(
        bencher,
        PROF_SRC_COMPUTE_PURE_CALL_1M,
        BenchMode::Suppressed,
    );
}

/// compute::array build sum 100k — allocation-heavy loop under tracing.
#[divan::bench]
fn prof_on_compute_array_build_sum_100k(bencher: Bencher) {
    bench_vm_main(
        bencher,
        PROF_SRC_COMPUTE_ARRAY_BUILD_SUM_100K,
        BenchMode::On,
    );
}

/// compute::fib32 recursive — the historical ring-overflow reproducer.
#[divan::bench]
fn prof_on_compute_fib32_recursive(bencher: Bencher) {
    bench_vm_main(bencher, PROF_SRC_COMPUTE_FIB32_RECURSIVE, BenchMode::On);
}

/// concurrency::spawn await x10k — ownership and awaited-end cost.
#[divan::bench]
fn prof_on_concurrency_spawn_await_x10k(bencher: Bencher) {
    bench_vm_main(
        bencher,
        PROF_SRC_CONCURRENCY_SPAWN_AWAIT_X10K,
        BenchMode::On,
    );
}

#[divan::bench]
fn prof_off_concurrency_spawn_await_x10k(bencher: Bencher) {
    bench_vm_main(
        bencher,
        PROF_SRC_CONCURRENCY_SPAWN_AWAIT_X10K,
        BenchMode::Off,
    );
}

/// string::concat loop 10k — string-path baseline under tracing.
#[divan::bench]
fn prof_on_string_concat_loop_10k(bencher: Bencher) {
    bench_vm_main(bencher, PROF_SRC_STRING_CONCAT_LOOP_10K, BenchMode::On);
}

const LOGGING_ONLY_SOURCE: &str = r#"
    function main() -> int {
        log.info("hello")
        1
    }
"#;

#[divan::bench]
fn logging_only_one_record(bencher: Bencher) {
    bench_vm_main(bencher, LOGGING_ONLY_SOURCE, BenchMode::LoggingOnly);
}

const ONE_WAIT_PER_CALL_SOURCE: &str = r#"
    function waited() -> int throws unknown {
        baml.sys.sleep(baml.time.Duration.from_nanoseconds(0n));
        1
    }
    function main() -> int throws unknown {
        let sum = 0;
        for (let i = 0; i < 100; i += 1) { sum += waited(); };
        sum
    }
"#;

const MANY_WAITS_PER_CALL_SOURCE: &str = r#"
    function waited() -> int throws unknown {
        baml.sys.sleep(baml.time.Duration.from_nanoseconds(0n));
        baml.sys.sleep(baml.time.Duration.from_nanoseconds(0n));
        baml.sys.sleep(baml.time.Duration.from_nanoseconds(0n));
        baml.sys.sleep(baml.time.Duration.from_nanoseconds(0n));
        baml.sys.sleep(baml.time.Duration.from_nanoseconds(0n));
        baml.sys.sleep(baml.time.Duration.from_nanoseconds(0n));
        baml.sys.sleep(baml.time.Duration.from_nanoseconds(0n));
        baml.sys.sleep(baml.time.Duration.from_nanoseconds(0n));
        baml.sys.sleep(baml.time.Duration.from_nanoseconds(0n));
        baml.sys.sleep(baml.time.Duration.from_nanoseconds(0n));
        10
    }
    function main() -> int throws unknown {
        let sum = 0;
        for (let i = 0; i < 10; i += 1) { sum += waited(); };
        sum
    }
"#;

/// One hundred suspensions spread over one hundred calls.
#[divan::bench]
fn prof_on_one_wait_per_call_x100(bencher: Bencher) {
    bench_vm_main(bencher, ONE_WAIT_PER_CALL_SOURCE, BenchMode::On);
}

#[divan::bench]
fn prof_off_one_wait_per_call_x100(bencher: Bencher) {
    bench_vm_main(bencher, ONE_WAIT_PER_CALL_SOURCE, BenchMode::Off);
}

/// The same suspension count folded into one sparse entry per ten waits.
#[divan::bench]
fn prof_on_ten_waits_per_call_x10(bencher: Bencher) {
    bench_vm_main(bencher, MANY_WAITS_PER_CALL_SOURCE, BenchMode::On);
}

#[divan::bench]
fn prof_off_ten_waits_per_call_x10(bencher: Bencher) {
    bench_vm_main(bencher, MANY_WAITS_PER_CALL_SOURCE, BenchMode::Off);
}
