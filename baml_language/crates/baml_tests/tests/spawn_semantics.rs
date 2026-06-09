//! BEP-034: spawn/await semantic invariants beyond the basic round-trip.
//!
//! Only `parent_throw_cancels_running_children` remains here — it asserts on
//! wall-clock timing, which is not expressible in a BAML test block.

use std::time::{Duration, Instant};

use baml_tests::engine::{IndexMap, OptLevel, compile_source_with_opt, run_compiled};

/// BEP-034: "Parent throws still cascade-cancel children." When the
/// parent function throws an unhandled error, the parent thread's
/// cancel token fires; spawned children's tokens (which derive from
/// the parent's via `child_token()`) fire too, and the children's
/// next await checkpoint observes Cancelled.
///
/// We observe the cascade by spawning a child that does a long sleep,
/// then having the parent throw. The whole call should terminate in
/// well under the sleep duration — without the cascade the child would
/// hold the engine alive for 60s.
#[tokio::test]
async fn parent_throw_cancels_running_children() {
    // Compile OUTSIDE the timed region: compiling the (growing) BAML stdlib
    // takes seconds and is not what this test measures. Time only the engine
    // run, which is what the cascade makes prompt.
    let program = compile_source_with_opt(
        r#"
        function main() -> int throws baml.errors.Io {
            // Spawn a long-running child. The cascade should fire its
            // cancel token when main throws below.
            let _ = spawn { baml.sys.sleep(60000); 42 };
            throw baml.errors.Io { message: "parent boom" }
        }
        "#,
        OptLevel::One,
    );
    let started = Instant::now();
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    let elapsed = started.elapsed();

    // Main throws → host receives unhandled Io.
    let err = output.result.expect_err("expected Io throw from parent");
    let msg = format!("{err:?}");
    assert!(msg.contains("baml.errors.Io"), "got {msg}");

    // Cascade test: the call must return promptly. Without
    // cascade-cancel the spawn body's 60s sleep would either keep the
    // engine alive for the full duration (if we waited for it) or
    // hang an awaiter. We don't await it here, but its tokio task is
    // alive in the background and would block clean shutdown if not
    // cancelled.
    assert!(
        elapsed < Duration::from_secs(5),
        "parent-throw call took {}ms (expected <5s — cascade missing?)",
        elapsed.as_millis(),
    );
}
