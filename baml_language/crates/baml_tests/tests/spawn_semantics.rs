//! BEP-034: spawn/await semantic invariants beyond the basic round-trip.
//!
//! `parent_throw_cancels_running_children` asserts on wall-clock timing, which
//! is not expressible in a BAML test block. The `never_awaited_*` tests below
//! exercise the end-of-run drain (B-612): a fire-and-forget child that throws
//! must surface its error when the root finalizes, and a successful one must
//! not false-surface.
//!
//! The `racing_*` / `*_waited_*` tests exercise the B-650 end-of-run **wait**
//! (BEP-034 end-of-run amendment): on root exit the runtime WAITS for every
//! outstanding spawn to run to completion — it does NOT cancel them — so even a
//! *racing* throw (a child whose task had not been polled when the root
//! completed) surfaces, and even a delayed throw surfaces (a cancel-at-shutdown
//! design would have dropped it). `detach` is not exempt from the wait.

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

/// B-650: the fully-*racing* case B-612 scoped out. With NO sleep, the child's
/// tokio task has not run by the time the root reaches `Complete`, so nothing is
/// enqueued yet and the B-612 drain alone would exit 0 — dropping the error. The
/// end-of-run WAIT parks on the still-outstanding child's settle signal first;
/// the child's body throws (settling `ErrorPending` and enqueuing its error via
/// the enqueue-before-defer order), so the drain then surfaces it as an
/// unhandled `baml.errors.Io`. No cancellation is involved.
#[tokio::test]
async fn racing_never_awaited_spawn_error_surfaces_at_completion() {
    let program = compile_source_with_opt(
        r#"
        function main() -> string {
            let f = spawn { throw baml.errors.Io { message: "boom" } };
            "done"
        }
        "#,
        OptLevel::One,
    );
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    let err = output
        .result
        .expect_err("racing never-awaited spawn throw must surface at completion");
    let msg = format!("{err:?}");
    assert!(msg.contains("baml.errors.Io"), "got {msg}");
}

/// B-650: the racing case with `detach = true`. Per the `detach` contract the
/// child's unhandled error routes to the *root* task's queue; `detach` is NOT
/// exempt from the end-of-run wait, so the root waits for it and the racing
/// throw surfaces at completion just like the default case.
#[tokio::test]
async fn racing_never_awaited_detached_spawn_error_surfaces_at_completion() {
    let program = compile_source_with_opt(
        r#"
        function main() -> string {
            let f = spawn with baml.spawn.options(detach = true) {
                throw baml.errors.Io { message: "boom" }
            };
            "done"
        }
        "#,
        OptLevel::One,
    );
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    let err = output
        .result
        .expect_err("racing never-awaited detached spawn throw must surface at completion");
    let msg = format!("{err:?}");
    assert!(msg.contains("baml.errors.Io"), "got {msg}");
}

/// B-650 negative guard: a *racing* never-awaited child that completes
/// successfully (no sleep, so the wait actually runs over an outstanding child)
/// must still return cleanly. The child settles `Fulfilled`, so nothing is
/// enqueued and the drain finds nothing.
#[tokio::test]
async fn racing_never_awaited_successful_spawn_returns_cleanly() {
    let program = compile_source_with_opt(
        r#"
        function main() -> string {
            let f = spawn { 42 };
            "done"
        }
        "#,
        OptLevel::One,
    );
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    let value = output
        .result
        .expect("racing successful never-awaited spawn must return cleanly");
    let BexExternalValue::String(s) = value else {
        panic!("expected String, got {value:?}");
    };
    assert_eq!(s.to_string(), "done");
}

/// B-650 wait-not-cancel proof: a never-awaited child that *sleeps then throws*
/// must be waited to completion and surface its error. This is the sharpest
/// distinction from the rejected cancel-at-shutdown prototype: had the runtime
/// cancelled outstanding work at exit, the child's `sleep` would settle
/// `Cancelled` and the throw would never happen (exit 0). Because we WAIT, the
/// child runs through the sleep, throws, and the error surfaces (exit 1).
#[tokio::test]
async fn never_awaited_delayed_throw_is_waited_and_surfaces() {
    let program = compile_source_with_opt(
        r#"
        function main() -> string {
            let f = spawn {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(200n));
                throw baml.errors.Io { message: "boom" }
            };
            "done"
        }
        "#,
        OptLevel::One,
    );
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    let err = output
        .result
        .expect_err("delayed never-awaited spawn throw must be waited-for and surface");
    let msg = format!("{err:?}");
    assert!(msg.contains("baml.errors.Io"), "got {msg}");
}

/// B-650 wait-not-cancel proof, detached variant: a never-awaited `detach = true`
/// child that sleeps then throws is likewise waited to completion (detach is not
/// exempt from the wait) and its error routes to the root and surfaces.
#[tokio::test]
async fn never_awaited_detached_delayed_throw_is_waited_and_surfaces() {
    let program = compile_source_with_opt(
        r#"
        function main() -> string {
            let f = spawn with baml.spawn.options(detach = true) {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(200n));
                throw baml.errors.Io { message: "boom" }
            };
            "done"
        }
        "#,
        OptLevel::One,
    );
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    let err = output
        .result
        .expect_err("delayed never-awaited detached spawn throw must be waited-for and surface");
    let msg = format!("{err:?}");
    assert!(msg.contains("baml.errors.Io"), "got {msg}");
}

/// B-650: a finite detached "telemetry flush" style spawn — sleeps briefly then
/// completes successfully, never awaited — must be waited to completion at exit
/// (its side effects run) and must NOT false-surface or hang. The root returns
/// its value cleanly.
#[tokio::test]
async fn finite_detached_spawn_is_waited_to_completion() {
    let program = compile_source_with_opt(
        r#"
        function main() -> string {
            let f = spawn with baml.spawn.options(detach = true) {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(150n));
                1
            };
            "done"
        }
        "#,
        OptLevel::One,
    );
    let output = run_compiled(program, "main", IndexMap::new(), false).await;
    let value = output
        .result
        .expect("finite detached spawn must be waited-for and the root return cleanly");
    let BexExternalValue::String(s) = value else {
        panic!("expected String, got {value:?}");
    };
    assert_eq!(s.to_string(), "done");
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
