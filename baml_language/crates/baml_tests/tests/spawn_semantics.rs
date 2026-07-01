//! BEP-034: spawn/await semantic invariants beyond the basic round-trip.
//!
//! `parent_throw_cancels_running_children` asserts on wall-clock timing, which
//! is not expressible in a BAML test block. The `never_awaited_*` tests below
//! exercise the end-of-run drain (B-612): a fire-and-forget child that throws
//! must surface its error when the root finalizes, and a successful one must
//! not false-surface.

use std::time::{Duration, Instant};

use baml_tests::engine::{IndexMap, OptLevel, compile_source_with_opt, run_compiled};
use bex_engine::BexExternalValue;

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
            let _ = spawn { baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n)); 42 };
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

/// B-612: a fire-and-forget child that throws must surface its error at the
/// root's end-of-run drain, not be silently swallowed.
///
/// A default `spawn` whose spawner never `await`s it parks its unhandled error
/// on the root thread's `pending_child_errors` queue. That queue used to be
/// drained ONLY at `Await` opcodes, so a root that completes normally dropped
/// the parked error and exited 0. The `sleep` guarantees the child has thrown
/// (and enqueued) before the root reaches `Complete`, so the drain must find
/// it and surface it as an unhandled `baml.errors.Io`.
#[tokio::test]
async fn never_awaited_spawn_error_surfaces_at_completion() {
    let program = compile_source_with_opt(
        r#"
        function main() -> string {
            let f = spawn { throw baml.errors.Io { message: "boom" } };
            baml.sys.sleep(baml.time.Duration.from_milliseconds(250n));
            "done"
        }
        "#,
        OptLevel::One,
    );
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    let err = output
        .result
        .expect_err("never-awaited spawn throw must surface at completion");
    let msg = format!("{err:?}");
    assert!(msg.contains("baml.errors.Io"), "got {msg}");
}

/// B-612: same as above but with `detach = true`, which per the documented
/// `spawn`/`detach` contract "routes its unhandled errors to the root task
/// instead of the spawner". A detached child's error lands on the root queue,
/// so the end-of-run drain must surface it too.
#[tokio::test]
async fn never_awaited_detached_spawn_error_surfaces_at_completion() {
    let program = compile_source_with_opt(
        r#"
        function main() -> string {
            let f = spawn with baml.spawn.options(detach = true) {
                throw baml.errors.Io { message: "boom" }
            };
            baml.sys.sleep(baml.time.Duration.from_milliseconds(250n));
            "done"
        }
        "#,
        OptLevel::One,
    );
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    let err = output
        .result
        .expect_err("never-awaited detached spawn throw must surface at completion");
    let msg = format!("{err:?}");
    assert!(msg.contains("baml.errors.Io"), "got {msg}");
}

/// B-612 negative guard: the end-of-run drain must not false-surface. A
/// never-awaited child that completes *successfully* enqueues nothing, so the
/// root must return its value cleanly.
#[tokio::test]
async fn never_awaited_successful_spawn_returns_cleanly() {
    let program = compile_source_with_opt(
        r#"
        function main() -> string {
            let f = spawn { 42 };
            baml.sys.sleep(baml.time.Duration.from_milliseconds(250n));
            "done"
        }
        "#,
        OptLevel::One,
    );
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    let value = output
        .result
        .expect("successful never-awaited spawn must return cleanly");
    let BexExternalValue::String(s) = value else {
        panic!("expected String, got {value:?}");
    };
    assert_eq!(s.to_string(), "done");
}
