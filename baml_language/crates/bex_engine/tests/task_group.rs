//! BEP-034 `TaskGroup` rate limiting: `spawn ... with baml.spawn.options(group = g)`.
//!
//! A `TaskGroup` caps how many spawns referencing it run concurrently; excess
//! spawns queue FIFO and start as earlier ones settle.
//!
//! These tests are deterministic: a `TaskGroup` member is registered
//! *synchronously* at `spawn` time, so `active_count`/`queued_count` observed
//! right after spawning are exact (no reliance on scheduler timing or on
//! spawned bodies actually running in parallel).

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

/// Compile `source` into a ready-to-run engine. Kept separate from
/// [`run_engine`] so timing-sensitive tests can exclude compilation (which
/// grows with the stdlib) from what they measure.
fn build_engine(source: &str) -> Arc<BexEngine> {
    let snapshot = compile_for_engine(source);
    Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    )
}

/// Invoke `main()` on an already-built engine.
async fn run_engine(engine: &Arc<BexEngine>) -> Result<BexExternalValue, EngineError> {
    engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
}

/// Compile `source` and run its `main()` to completion.
async fn run_main(source: &str) -> Result<BexExternalValue, EngineError> {
    let engine = build_engine(source);
    run_engine(&engine).await
}

/// The cap is enforced at admission: spawning four members into a `limit = 2`
/// group leaves exactly two active and two queued. Registration is synchronous
/// at `spawn`, so the counts are exact at this point (the bodies — long sleeps —
/// have not been polled yet). `g.cancel()` then tears the sleeps down.
#[tokio::test]
async fn task_group_caps_active_and_queues_rest() {
    let source = r#"
        function block() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(5000n));
            1
        }
        function main() -> int {
            let g = baml.spawn.TaskGroup.new(2);
            let a = spawn with baml.spawn.options(group = g) { block() };
            let b = spawn with baml.spawn.options(group = g) { block() };
            let c = spawn with baml.spawn.options(group = g) { block() };
            let d = spawn with baml.spawn.options(group = g) { block() };
            let snapshot = g.active_count() * 100 + g.queued_count();
            let _ = g.cancel();
            snapshot
        }
    "#;

    // 2 active * 100 + 2 queued = 202.
    let result = run_main(source).await.expect("call should succeed");
    assert_eq!(result, BexExternalValue::Int(202));
}

/// More spawns than the limit all complete — the queued ones start as slots
/// free, and the caller gets every result.
#[tokio::test]
async fn task_group_queues_excess_and_all_complete() {
    let source = r#"
        function main() -> int {
            let g = baml.spawn.TaskGroup.new(2);
            let a = spawn with baml.spawn.options(group = g) { 1 };
            let b = spawn with baml.spawn.options(group = g) { 1 };
            let c = spawn with baml.spawn.options(group = g) { 1 };
            let d = spawn with baml.spawn.options(group = g) { 1 };
            let e = spawn with baml.spawn.options(group = g) { 1 };
            (await a) + (await b) + (await c) + (await d) + (await e)
        }
    "#;

    let result = run_main(source).await.expect("call should succeed");
    assert_eq!(result, BexExternalValue::Int(5));
}

/// `group.cancel()` fires its members' tokens — a long-running member is
/// cancelled and `await` re-throws `Cancelled` (caught here).
#[tokio::test]
async fn task_group_cancel_cancels_members() {
    let source = r#"
        function main() -> int {
            let g = baml.spawn.TaskGroup.new(5);
            let f = spawn with baml.spawn.options(group = g) {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(10000n));
                42
            };
            let _ = g.cancel();
            (await f) catch (e) { baml.panics.Cancelled => 0 }
        }
    "#;

    // Compile OUTSIDE the timed region — we're measuring cancellation
    // promptness, not the debug-build compile (which grows with the stdlib
    // and once pushed this assert past a wall-clock bound).
    let engine = build_engine(source);
    let start = std::time::Instant::now();
    let result = run_engine(&engine).await.expect("call should succeed");
    assert_eq!(result, BexExternalValue::Int(0));
    // `cancel()` must tear the 10s sleep down promptly; 2s is plenty of
    // headroom for the engine run alone while still far under that sleep.
    assert!(
        start.elapsed() < std::time::Duration::from_secs(2),
        "group.cancel() should be prompt: {:?}",
        start.elapsed()
    );
}

/// `group.cancel(active = false)` cancels only queued members; the running ones
/// keep their slots. With `limit = 1`, one member is active and one is queued —
/// cancelling pending-only leaves the active member to finish.
#[tokio::test]
async fn task_group_cancel_pending_only() {
    let source = r#"
        function main() -> int {
            let g = baml.spawn.TaskGroup.new(1);
            let active = spawn with baml.spawn.options(group = g) { 7 };
            let queued = spawn with baml.spawn.options(group = g) {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(10000n));
                1
            };
            let signalled = g.cancel(active = false);
            // Only the queued member is signalled (active stays).
            let active_result = await active;
            let queued_result = (await queued) catch (e) { baml.panics.Cancelled => 0 };
            signalled * 100 + active_result * 10 + queued_result
        }
    "#;

    // 1 signalled (the queued one) * 100 + active 7 * 10 + queued cancelled 0 = 170.
    let result = run_main(source).await.expect("call should succeed");
    assert_eq!(result, BexExternalValue::Int(170));
}

/// `limit()` / `set_limit()` round-trip.
#[tokio::test]
async fn task_group_set_limit_updates_limit() {
    let source = r#"
        function main() -> int {
            let g = baml.spawn.TaskGroup.new(3);
            let before = g.limit();
            g.set_limit(7);
            before + g.limit()
        }
    "#;

    let result = run_main(source).await.expect("call should succeed");
    assert_eq!(result, BexExternalValue::Int(10));
}

/// Spawns run concurrently for real IO: three 200ms sleeps with `limit = 3`
/// complete in ~200ms wall-clock, not ~600ms. Compilation/bootstrap is done
/// *before* the timer so the threshold only measures execution.
#[tokio::test]
async fn task_group_limit_three_runs_concurrently() {
    let source = r#"
        function work() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(200n));
            1
        }
        function main() -> int {
            let g = baml.spawn.TaskGroup.new(3);
            let a = spawn with baml.spawn.options(group = g) { work() };
            let b = spawn with baml.spawn.options(group = g) { work() };
            let c = spawn with baml.spawn.options(group = g) { work() };
            (await a) + (await b) + (await c)
        }
    "#;

    // Compile + bootstrap OUTSIDE the timing budget (MIR construction can take
    // hundreds of ms on a cold build); the threshold only has to absorb
    // scheduler jitter on top of one 200ms sleep.
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("engine"),
    );

    let start = std::time::Instant::now();
    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("call should succeed");
    let elapsed = start.elapsed();

    assert_eq!(result, BexExternalValue::Int(3));
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "limit=3 should run the three sleeps concurrently (~200ms); got {elapsed:?}"
    );
}

/// `limit = 1` serializes the same three sleeps: execution is ~600ms, not
/// ~200ms — the cap actually gates concurrency (contrast with the test above).
#[tokio::test]
async fn task_group_limit_one_serializes() {
    let source = r#"
        function work() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(200n));
            1
        }
        function main() -> int {
            let g = baml.spawn.TaskGroup.new(1);
            let a = spawn with baml.spawn.options(group = g) { work() };
            let b = spawn with baml.spawn.options(group = g) { work() };
            let c = spawn with baml.spawn.options(group = g) { work() };
            (await a) + (await b) + (await c)
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("engine"),
    );

    let start = std::time::Instant::now();
    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("call should succeed");
    let elapsed = start.elapsed();

    assert_eq!(result, BexExternalValue::Int(3));
    assert!(
        elapsed >= std::time::Duration::from_millis(450),
        "limit=1 should serialize the three sleeps (~600ms); got {elapsed:?}"
    );
}

/// Raising the cap admits queued members up to the NEW cap immediately (per
/// the `set_limit` docstring) — not just the first parked waiter.
#[tokio::test]
async fn set_limit_increase_admits_multiple_queued() {
    let source = r#"
        function work() -> int { baml.sys.sleep(baml.time.Duration.from_milliseconds(500n)); 1 }
        function main() -> int {
            let g = baml.spawn.TaskGroup.new(0);
            let a = spawn with baml.spawn.options(group = g) { work() };
            let b = spawn with baml.spawn.options(group = g) { work() };
            let c = spawn with baml.spawn.options(group = g) { work() };
            baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
            g.set_limit(3);
            baml.sys.sleep(baml.time.Duration.from_milliseconds(100n));
            let active = g.active_count();
            g.cancel();
            active
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(3));
}
