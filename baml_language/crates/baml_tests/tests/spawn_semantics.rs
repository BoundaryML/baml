//! BEP-034: spawn/await semantic invariants beyond the basic round-trip.
//!
//! Pulls in BEP requirements that aren't covered by `spawn_basic`,
//! `spawn_parallel`, `future_methods`, `cancel_cascade`, or
//! `spawn_throws`:
//!   - Multiple awaits on the same future return the cached value.
//!   - Nested spawn (spawn within a spawn body).
//!   - Parent throw cascades cancellation to children.

use std::time::{Duration, Instant};

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// BEP-034: "Multiple `await`s on the same future return the same
/// value (or re-throw the same error)." Computation runs once.
#[tokio::test]
async fn multiple_awaits_on_same_future_return_cached_value() {
    let output = baml_test!(
        r#"
        function compute() -> int { 42 }
        function main() -> int {
            let f = spawn { compute() };
            let a = await f;
            let b = await f;
            let c = await f;
            a + b + c
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(126)));
}

/// Three levels of nesting — child of child of root. Sum at each level
/// proves each spawn ran and its result reached the awaiter. Subsumes
/// a two-level test (if 3 levels work, 2 trivially do).
#[tokio::test]
async fn three_level_nested_spawn_sum() {
    let output = baml_test!(
        r#"
        function leaf() -> int { 1 }
        function mid() -> int {
            let l = spawn { leaf() };
            (await l) + 10
        }
        function top() -> int {
            let m = spawn { mid() };
            (await m) + 100
        }
        function main() -> int {
            let t = spawn { top() };
            await t
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(111)));
}

/// BEP-034: function-typed return of `Future<T, E>` — the spawning
/// helper's return value flows through the await at the call site.
/// Mirrors the `spawn_returns_future.rs` item from the original
/// Phase G plan. The fully-qualified `baml.future.Future<T, E>` form
/// resolves to `Ty::Future`; bare `Future<T, E>` would need to be in
/// lexical scope and isn't auto-imported in v1.
#[tokio::test]
async fn function_can_return_future_typed_value() {
    let output = baml_test!(
        r#"
        function spawn_child() -> baml.future.Future<int, null> {
            spawn { 99 }
        }
        function main() -> int {
            let f = spawn_child();
            await f
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

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
    let started = Instant::now();
    let output = baml_test!(
        r#"
        function main() -> int throws baml.errors.Io {
            // Spawn a long-running child. The cascade should fire its
            // cancel token when main throws below.
            let _ = spawn { baml.sys.sleep(60000); 42 };
            throw baml.errors.Io { message: "parent boom" }
        }
        "#
    );
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
