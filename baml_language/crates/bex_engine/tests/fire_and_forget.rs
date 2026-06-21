//! BEP-034 fire-and-forget error propagation.
//!
//! A spawned child's unhandled throw surfaces at the spawner's next `await`
//! — but ONLY if no other task observed (awaited) the future first. An error
//! consumed by an awaiter (where a `catch` can handle it) must not re-surface
//! at the spawner. The engine implements this by DEFERRING the error settle
//! (see `FutureManagerGuard::defer_error` / `future_ready`).

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
