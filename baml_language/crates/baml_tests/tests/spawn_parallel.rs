//! BEP-034: spawned bodies actually run on the tokio runtime in parallel.
//!
//! Three concurrent `baml.sys.sleep(D)` calls should complete in wall-clock
//! time close to one sleep, not three. The previous version of this test
//! used pure-literal spawns and did not exercise concurrency at all.

use std::{sync::Arc, time::Instant};

use baml_tests::engine::{OptLevel, compile_source_with_opt};
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use sys_native::SysOpsExt;

#[tokio::test]
async fn spawn_three_sleeps_runs_in_parallel() {
    // Compile + engine bootstrap is excluded from the timing budget by doing
    // it explicitly here, so the threshold only has to absorb scheduler
    // jitter — not BAML parsing and MIR construction (which can take >1s
    // on a cold CI runner).
    let program = compile_source_with_opt(
        r#"
        function nap() -> int {
            baml.sys.sleep(200) catch (e) {
                let e => 0
            };
            1
        }
        function main() -> int {
            let a = spawn { nap() };
            let b = spawn { nap() };
            let c = spawn { nap() };
            (await a) + (await b) + (await c)
        }
        "#,
        OptLevel::One,
    );
    let engine = Arc::new(
        BexEngine::new(
            program,
            Arc::new(sys_ops::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("engine"),
    );

    // Each sleep is 200ms. Sequential would be ~600ms; parallel ~200ms.
    // 500ms gives parallel a comfortable margin while still failing fast
    // if anyone re-introduces sequential awaits.
    let started = Instant::now();
    let result = engine
        .call_function_bound_args(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;
    let elapsed = started.elapsed();

    assert_eq!(result, Ok(BexExternalValue::Int(3)));
    assert!(
        elapsed.as_millis() < 500,
        "expected parallel execution (~200ms); got {}ms",
        elapsed.as_millis(),
    );
}
