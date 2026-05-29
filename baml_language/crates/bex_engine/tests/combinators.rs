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
