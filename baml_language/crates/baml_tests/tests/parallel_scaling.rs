//! Measure parallel vs sequential scaling of `spawn`.
//!
//! Bench-real-world shows only ~1.19x speedup for 4-way parallel sum vs
//! sequential, despite tokio being multi-threaded by default. This test
//! isolates *execution* time (compile + engine init excluded) so we can
//! see how much speedup the runtime actually delivers.

use std::{sync::Arc, time::Instant};

use baml_tests::engine::{OptLevel, compile_source_with_opt};
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use sys_native::SysOpsExt;

const TOTAL_WORK: i64 = 1_000_000;
const RUNS: usize = 10;

async fn time_fn(engine: &Arc<BexEngine>, fn_name: &str) -> std::time::Duration {
    // Every config sums the same range [0..TOTAL_WORK); validate the answer
    // each time so a silently-broken scheduler or container path doesn't
    // produce convincing-but-wrong timing numbers.
    let expected = (TOTAL_WORK - 1) * TOTAL_WORK / 2;
    let start = Instant::now();
    let result = engine
        .call_function_bound_args(
            fn_name,
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;
    let elapsed = start.elapsed();
    match result {
        Ok(BexExternalValue::Int(v)) => {
            assert_eq!(v, expected, "{fn_name} returned wrong sum");
        }
        other => panic!("{fn_name} expected Int({expected}), got {other:?}"),
    }
    elapsed
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "benchmark-style scaling measurement, not a pass/fail correctness test; \
            run manually with `cargo test --test parallel_scaling -- --ignored --nocapture`"]
async fn parallel_sum_scaling() {
    let program = compile_source_with_opt(
        r#"
        function sum_range(start: int, end: int) -> int {
          let s = 0;
          let i = start;
          while i < end { s += i; i += 1; };
          s
        }

        function sequential() -> int {
          sum_range(0, 1000000)
        }

        function parallel_2() -> int {
          let a = spawn { sum_range(0, 500000) };
          let b = spawn { sum_range(500000, 1000000) };
          (await a) + (await b)
        }

        function parallel_4() -> int {
          let a = spawn { sum_range(0, 250000) };
          let b = spawn { sum_range(250000, 500000) };
          let c = spawn { sum_range(500000, 750000) };
          let d = spawn { sum_range(750000, 1000000) };
          (await a) + (await b) + (await c) + (await d)
        }

        function parallel_8() -> int {
          let a = spawn { sum_range(0, 125000) };
          let b = spawn { sum_range(125000, 250000) };
          let c = spawn { sum_range(250000, 375000) };
          let d = spawn { sum_range(375000, 500000) };
          let e = spawn { sum_range(500000, 625000) };
          let f = spawn { sum_range(625000, 750000) };
          let g = spawn { sum_range(750000, 875000) };
          let h = spawn { sum_range(875000, 1000000) };
          (await a) + (await b) + (await c) + (await d)
            + (await e) + (await f) + (await g) + (await h)
        }
        "#,
        OptLevel::One,
    );
    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_ops::SysOps::native()), Vec::new()).expect("engine"),
    );

    // Warm up
    for fn_name in [
        "user.sequential",
        "user.parallel_2",
        "user.parallel_4",
        "user.parallel_8",
    ] {
        let _ = time_fn(&engine, fn_name).await;
    }

    // Measure
    let metrics = tokio::runtime::Handle::current().metrics();
    eprintln!("\ntokio worker threads: {}", metrics.num_workers());
    eprintln!("total work: sum(0..{})", TOTAL_WORK);
    eprintln!("runs per config: {}", RUNS);
    eprintln!();

    for fn_name in [
        "user.sequential",
        "user.parallel_2",
        "user.parallel_4",
        "user.parallel_8",
    ] {
        let mut times = Vec::with_capacity(RUNS);
        for _ in 0..RUNS {
            times.push(time_fn(&engine, fn_name).await);
        }
        times.sort();
        let median = times[RUNS / 2];
        let min = times[0];
        let max = times[RUNS - 1];
        eprintln!(
            "  {:<20} median {:>6.2}ms  min {:>6.2}ms  max {:>6.2}ms",
            fn_name,
            median.as_secs_f64() * 1000.0,
            min.as_secs_f64() * 1000.0,
            max.as_secs_f64() * 1000.0,
        );
    }
}
