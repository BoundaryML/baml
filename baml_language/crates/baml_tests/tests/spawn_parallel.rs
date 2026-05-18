//! BEP-034: spawned bodies actually run on the tokio runtime in parallel.
//!
//! Three concurrent `baml.sys.sleep(D)` calls should complete in wall-clock
//! time close to one sleep, not three. The previous version of this test
//! used pure-literal spawns and did not exercise concurrency at all.

use std::time::Instant;

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn spawn_three_sleeps_runs_in_parallel() {
    // Each sleep is 200ms. Sequential would be ~600ms; parallel ~200ms.
    // Engine bootstrap (BAML parse + MIR + first-call warmup) adds
    // ~500ms locally, so the bound has to be wider than the sleeps
    // themselves. <1000ms passes parallel (~200ms + bootstrap) and
    // fails sequential (~600ms + bootstrap > 1s).
    let started = Instant::now();
    let output = baml_test!(
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
        "#
    );
    let elapsed = started.elapsed();

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
    assert!(
        elapsed.as_millis() < 1000,
        "expected parallel execution (~200ms + bootstrap); got {}ms",
        elapsed.as_millis(),
    );
}
