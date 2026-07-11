//! Fire-and-forget error propagation.
//!
//! A spawned child's unhandled throw must surface if no task ever awaits the
//! future, but must NOT surface if the future is awaited (where a `catch` can
//! handle it). The engine implements this by DEFERRING the error settle (see
//! `FutureManagerGuard::defer_error` / `future_ready`): an awaiter consumes the
//! deferred error through `future_ready`, while a genuinely-never-awaited error
//! is surfaced only where "never awaited" is certain — the owning thread's
//! termination (root end-of-run, or a child thread's own completion). Surfacing
//! is NOT done at every intervening `await`, since that cannot distinguish a
//! future awaited later from one never awaited, and would pre-empt the former's
//! `catch`.

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

async fn run_main(source: &str) -> Result<BexExternalValue, EngineError> {
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );
    engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
}

/// The canonical bug-2 repro: `g` awaits `f` and HANDLES its error, so
/// `main` must observe `g`'s result — not `f`'s error resurfacing
/// fire-and-forget at `main`'s `await g`.
#[tokio::test]
async fn handled_child_error_does_not_resurface_at_spawner() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function main() -> int {
            let f = spawn { bad() };
            let g = spawn { (await f) catch (e) { let e => 7 } };
            (await g) catch (e) { let e => -1 }
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(7));
}

/// Re-throw variant: `g` awaits `f` and re-throws. `main`'s `catch` must
/// observe the error exactly once, through `g` — not pre-empted by `f`'s
/// fire-and-forget entry bypassing the `catch`.
#[tokio::test]
async fn rethrown_child_error_arrives_via_consumer() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function main() -> int {
            let f = spawn { bad() };
            let g = spawn { (await f) catch (e) { let e => throw e } };
            (await g) catch (e) { let e => 9 }
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(9));
}

/// Slow variant: `f` errors while `g` is already parked awaiting it, so the
/// deferred error's wake signal (rather than an already-settled read) drives
/// the observation.
#[tokio::test]
async fn handled_slow_child_error_does_not_resurface() {
    let source = r#"
        function bad_slow() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
            throw "boom"
        }
        function main() -> int {
            let f = spawn { bad_slow() };
            let g = spawn { (await f) catch (e) { let e => 7 } };
            (await g) catch (e) { let e => -1 }
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(7));
}

/// Guard: a genuinely fire-and-forget error — the handle is dropped and
/// nobody awaits it — must STILL surface at the spawner's next `await`
/// (BEP-034: "the error propagates to the parent task at its next await
/// point").
#[tokio::test]
async fn unobserved_child_error_still_surfaces_at_spawner() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function other() -> int { baml.sys.sleep(baml.time.Duration.from_milliseconds(50n)); 1 }
        function main() -> int {
            spawn { bad() };
            let g = spawn { other() };
            await g
        }
    "#;
    let err = run_main(source)
        .await
        .expect_err("dropped error must surface");
    match err {
        EngineError::UnhandledThrow { value, .. } => {
            assert_eq!(*value, BexExternalValue::String("boom".into()));
        }
        other => panic!("expected UnhandledThrow(\"boom\"), got {other:?}"),
    }
}

/// Regression: an errored future that IS awaited-and-caught must not be
/// pre-empted by an intervening `await` of an UNRELATED future. `f` throws
/// and enqueues its fire-and-forget error; `g`'s sleep guarantees `f` has
/// thrown by the time we `await g`. Surfacing the error at that unrelated
/// await stole it from under `(await f) catch` and escaped as an uncaught
/// throw. With surfacing deferred to termination, `await f` routes the error
/// into the `catch`, which returns 99.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn caught_error_not_preempted_by_unrelated_await() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function slow() -> int { baml.sys.sleep(baml.time.Duration.from_milliseconds(50n)); 7 }
        function main() -> int {
            let f = spawn { bad() };
            let g = spawn { slow() };
            let _ = await g;
            (await f) catch (e) { let e => 99 }
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(99));
}

/// Regression for the intermediate-thread drain: a child thread `t` that
/// spawns its OWN child `gc` which throws, never awaits it, and then
/// completes must still surface `gc`'s error rather than dropping it. A
/// child thread has no end-of-run, so the surfacing happens at `t`'s own
/// completion, propagating up so `main`'s `await t` observes it and catches.
/// `t`'s sleep lets `gc` throw and enqueue on `t`'s fire-and-forget queue
/// before `t` completes, so the completion drain deterministically finds it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn intermediate_thread_surfaces_never_awaited_grandchild_error() {
    let source = r#"
        function gc_bad() -> int throws string { throw "gc boom" }
        function t_body() -> int {
            spawn { gc_bad() };
            baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
            42
        }
        function main() -> int {
            let t = spawn { t_body() };
            (await t) catch (e) { let e => -1 }
        }
    "#;
    // `t`'s body reaches 42, but its never-awaited grandchild error surfaces at
    // `t`'s completion (settling `t` Errored), so `main`'s `await t` catches it.
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(-1));
}
