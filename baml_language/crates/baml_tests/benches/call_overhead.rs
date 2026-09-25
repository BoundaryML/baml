//! Runtime call and structured logging overhead benchmarks.
//! Run with: cargo bench --bench call_overhead

use std::{path::Path, sync::Arc};

use baml_compiler2_emit::generate_project_bytecode;
use baml_db::ProjectDatabase;
use baml_tests::engine::TestDbExt;
use bex_engine::{BexEngine, FunctionCallContextBuilder, logger::TraceLogger};
use divan::{Bencher, black_box};
use sys_native::{CallId, SysOpsExt};

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("Skipping call_overhead in debug/test profile.");
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
    LoggingOnly,
}

/// Compile BAML source into a ready-to-run engine.
fn compile_source(source: &str) -> (ProjectDatabase, BexEngine) {
    let mut db = ProjectDatabase::new();
    let package = db.workspace(Path::new("."));
    db.file("bench.baml", source);
    let bytecode = generate_project_bytecode(&db, package).expect("benchmark compilation failed");
    let engine = BexEngine::new(bytecode, Arc::new(sys_native::SysOps::native()), vec![])
        .expect("benchmark engine creation failed");
    (db, engine)
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
    let (_db, engine) = compile_source(source);
    let engine = Arc::new(engine);
    let rt = tokio::runtime::Runtime::new().expect("failed to build tokio runtime");
    let logger = matches!(mode, BenchMode::LoggingOnly).then(|| TraceLogger::bounded(2048));
    bencher.with_inputs(|| ()).bench_values(|()| {
        let mut context = FunctionCallContextBuilder::new(CallId::next());

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
}

// Subset sources exported by build.rs from the speedtest corpus.
include!(concat!(env!("OUT_DIR"), "/speedtest_profiling_sources.rs"));

#[divan::bench]
fn call_compute_pure_call_1m(bencher: Bencher) {
    bench_vm_main(bencher, PROF_SRC_COMPUTE_PURE_CALL_1M, BenchMode::Off);
}

#[divan::bench]
fn call_concurrency_spawn_await_x10k(bencher: Bencher) {
    bench_vm_main(
        bencher,
        PROF_SRC_CONCURRENCY_SPAWN_AWAIT_X10K,
        BenchMode::Off,
    );
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
    function waited() -> int {
        baml.sys.sleep(baml.time.Duration.from_nanoseconds(0n));
        1
    }
    function main() -> int {
        let sum = 0;
        for (let i = 0; i < 100; i += 1) { sum += waited(); };
        sum
    }
"#;

const MANY_WAITS_PER_CALL_SOURCE: &str = r#"
    function waited() -> int {
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
    function main() -> int {
        let sum = 0;
        for (let i = 0; i < 10; i += 1) { sum += waited(); };
        sum
    }
"#;

#[divan::bench]
fn call_one_wait_per_call_x100(bencher: Bencher) {
    bench_vm_main(bencher, ONE_WAIT_PER_CALL_SOURCE, BenchMode::Off);
}

#[divan::bench]
fn call_ten_waits_per_call_x10(bencher: Bencher) {
    bench_vm_main(bencher, MANY_WAITS_PER_CALL_SOURCE, BenchMode::Off);
}
