//! BEP-034 future combinators: `baml.future.{race, any, all, all_complete, all_settled}`.

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

async fn run_main(source: &str) -> Result<BexExternalValue, EngineError> {
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
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

/// Repro: awaiting inside a `.map()` lambda (closure invoked by a native
/// `YieldToCall` continuation). The lambda param must survive the await
/// suspend/resume.
#[tokio::test]
async fn await_inside_map_lambda() {
    let source = r#"
        function one() -> int { 1 }
        function two() -> int { 2 }
        function three() -> int { 3 }
        function main() -> int {
            let fs = [spawn { one() }, spawn { two() }, spawn { three() }];
            let rs = fs.map((f) -> { await f });
            rs[0] * 100 + rs[1] * 10 + rs[2]
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(123));
}

/// Control: awaiting inside a closure called DIRECTLY (not via a native
/// continuation). Isolates whether the bug is closure-general or map-specific.
#[tokio::test]
async fn await_inside_direct_closure() {
    let source = r#"
        function one() -> int { 7 }
        function main() -> int {
            let fut = spawn { one() };
            let g = (x: baml.future.Future<int, null>) -> { await x };
            g(fut)
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(7));
}

/// Control: a NON-await closure called directly. If this works, the slot-1
/// reservation is specific to await/block-bodied lambdas.
#[tokio::test]
async fn closure_no_await_direct() {
    let source = r#"
        function main() -> int {
            let g = (x: int) -> { x + 1 };
            g(5)
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(6));
}

/// `all_complete` awaits every future and returns their values in input order.
#[tokio::test]
async fn all_complete_collects_in_order() {
    let source = r#"
        function main() -> int {
            let fs = [spawn { 1 }, spawn { 2 }, spawn { 3 }];
            let results = await baml.future.all_complete(fs);
            results[0] * 100 + results[1] * 10 + results[2]
        }
    "#;
    // [1, 2, 3] -> 123
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(123));
}

/// `all` collects every value in input order on the happy path.
#[tokio::test]
async fn all_collects_in_order() {
    let source = r#"
        function main() -> int {
            let fs = [spawn { 1 }, spawn { 2 }, spawn { 3 }];
            let r = await baml.future.all(fs);
            r[0] * 100 + r[1] * 10 + r[2]
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(123));
}

/// `all` re-throws the first failure; the surrounding `catch` observes it.
///
/// KNOWN ISSUE (separate from the combinators): an input future spawned in the
/// SAME task that awaits the combinator, if it throws unhandled, gets its error
/// queued on the spawner's fire-and-forget queue (`notify_parent_of_error`).
/// The spawner's `await` then surfaces it as `UnhandledThrow`, bypassing this
/// `catch`. Lives in the spawn fire-and-forget model; exposed by, not caused
/// by, the combinators.
#[ignore = "fire-and-forget input error bypasses combinator catch — see comment"]
#[tokio::test]
async fn all_rethrows_failure_to_catch() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function main() -> int {
            let fs = [spawn { 1 }, spawn { bad() }];
            let r = await baml.future.all(fs) catch (e) {
                let e => [99]
            };
            r[0]
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(99));
}

/// `race` settles with the FIRST future to settle — here the faster one wins.
#[tokio::test]
async fn race_returns_first_to_settle() {
    let source = r#"
        function slow() -> int { baml.sys.sleep(300); 2 }
        function fast() -> int { baml.sys.sleep(20); 1 }
        function main() -> int {
            let fs = [spawn { slow() }, spawn { fast() }];
            await baml.future.race(fs)
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(1));
}

/// `any` returns the first SUCCESS even when a faster future fails first.
///
/// BLOCKED ON BUG 2 (fire-and-forget): the failing input `spawn { bad() }` is a
/// child of `main`, so its unhandled "boom" is queued on `main` and surfaces at
/// `main`'s `await any(fs)` as UnhandledThrow, before `any` skips it. `any`
/// itself is correct (see `any_all_success_returns_a_value`); un-ignore once the
/// fire-and-forget fix lands.
#[ignore = "blocked on bug 2 (fire-and-forget) — any logic itself works; passes once that lands"]
#[tokio::test]
async fn any_returns_first_success() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function good() -> int { baml.sys.sleep(80); 42 }
        function main() -> int {
            let fs = [spawn { bad() }, spawn { good() }];
            await baml.future.any(fs) catch (e) {
                let e => -1
            }
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(42));
}

/// `any` throws `AllFailed` carrying every error when all futures fail.
///
/// BLOCKED ON BUG 2 (fire-and-forget): the failing inputs surface at `main`'s
/// await before `any` aggregates them. Un-ignore once the fire-and-forget fix
/// lands.
#[ignore = "blocked on bug 2 (fire-and-forget) — passes once that lands"]
#[tokio::test]
async fn any_all_fail_throws_allfailed() {
    let source = r#"
        function bad1() -> int throws string { throw "a" }
        function bad2() -> int throws string { throw "b" }
        function main() -> int {
            let fs = [spawn { bad1() }, spawn { bad2() }];
            await baml.future.any(fs) catch (e) {
                let e: baml.future.AllFailed<string> => e.errors.length()
            }
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(2));
}

/// `await f catch (e) {…}` must parse as `(await f) catch …` so the handler
/// catches the error `await` re-throws from the future (regression test for
/// the await/catch precedence fix).
#[tokio::test]
async fn await_catch_binds_to_await_not_future() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function main() -> int {
            let f = spawn { bad() };
            await f catch (e) {
                let e => 7
            }
        }
    "#;
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(7));
}

/// The inputs run concurrently: three 200ms sleeps complete in ~200ms, not
/// ~600ms (compile/bootstrap excluded from the timing budget).
#[tokio::test]
async fn all_complete_runs_concurrently() {
    let source = r#"
        function work() -> int {
            baml.sys.sleep(200);
            1
        }
        function main() -> int {
            let fs = [spawn { work() }, spawn { work() }, spawn { work() }];
            let results = await baml.future.all_complete(fs);
            results[0] + results[1] + results[2]
        }
    "#;
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
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
        "all_complete inputs should run concurrently (~200ms); got {elapsed:?}"
    );
}
// temp probe appended
// Bug 2 repro WITHOUT combinators: f errors, g awaits+handles f's error,
// main awaits g. f's error must NOT bypass g's handling and surface at main.
// Lives in the pre-existing spawn fire-and-forget model; being fixed on a
// parallel branch. Remove `#[ignore]` once that lands.
#[ignore = "bug 2: fire-and-forget error bypasses the future's consumer (separate branch)"]
#[tokio::test]
async fn bug2_fire_and_forget_bypasses_consumer() {
    let source = r#"
        function bad() -> int throws string { throw "boom" }
        function main() -> int {
            let f = spawn { bad() };
            let g = spawn { (await f) catch (e) { let e => 7 } };
            (await g) catch (e) { let e => -1 }
        }
    "#;
    // g handles f's error and returns 7; main should see 7.
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(7));
}

#[tokio::test]
async fn any_all_success_returns_a_value() {
    let source = r#"
        function five() -> int { 5 }
        function six() -> int { 6 }
        function main() -> int {
            let fs = [spawn { five() }, spawn { six() }];
            await baml.future.any(fs) catch (e) { let e => -1 }
        }
    "#;
    // first success wins (both succeed) — either 5 or 6; deterministic-ish but
    // assert it's a real success, not -1 / error.
    let r = run_main(source).await.unwrap();
    assert!(matches!(r, BexExternalValue::Int(5) | BexExternalValue::Int(6)), "got {r:?}");
}
