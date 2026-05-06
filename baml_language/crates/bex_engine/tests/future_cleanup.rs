//! Verify that the engine's `FutureManager` removes entries for completed
//! futures so the tracking map does not grow unboundedly.

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

fn make_engine(source: &str) -> Arc<BexEngine> {
    let snapshot = compile_for_engine(source);
    Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    )
}

async fn call_main(engine: &Arc<BexEngine>) -> BexExternalValue {
    engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("call should succeed")
}

#[tokio::test]
async fn active_futures_drains_after_awaited_sleep() {
    // A function that schedules and awaits several async sys-ops should
    // leave no entries behind in the future manager once it returns.
    let source = r#"
        function main() -> int {
            baml.sys.sleep(0);
            baml.sys.sleep(0);
            baml.sys.sleep(0);
            42
        }
    "#;

    let engine = make_engine(source);
    assert_eq!(engine.active_future_count().await, 0);

    let result = call_main(&engine).await;
    assert_eq!(result, BexExternalValue::Int(42));
    assert_eq!(engine.active_future_count().await, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn future_state_race_under_multithread_stress() {
    // Regression test for the data race between the engine's spawned task
    // (writer of the `Future` heap object) and the VM (reader). Many
    // fast-resolving sys-ops (`baml.sys.sleep(0)`) on a multi-threaded
    // tokio runtime exercise the path where the spawned task's write to
    // `Future::Ready` can land before — or alongside — the VM's first
    // read in `Await`. The atomic discriminant on `Future` makes this
    // race-free; without the fix, this test would tear discriminant reads
    // and (in debug) panic on a non-Pending state.
    let source = r#"
        function main() -> int {
            baml.sys.sleep(0);
            baml.sys.sleep(0);
            baml.sys.sleep(0);
            baml.sys.sleep(0);
            baml.sys.sleep(0);
            baml.sys.sleep(0);
            baml.sys.sleep(0);
            baml.sys.sleep(0);
            1
        }
    "#;

    let engine = make_engine(source);

    let mut handles = Vec::new();
    for _ in 0..32 {
        let engine = Arc::clone(&engine);
        handles.push(tokio::spawn(async move { call_main(&engine).await }));
    }
    for handle in handles {
        let value = handle.await.expect("task should not panic");
        assert_eq!(value, BexExternalValue::Int(1));
    }

    assert_eq!(engine.active_future_count().await, 0);
}

#[tokio::test]
async fn active_futures_drains_across_concurrent_calls() {
    // Run several calls concurrently and verify the count is back to zero
    // after they all complete. This catches a regression where one call's
    // entries linger and bias the count.
    let source = r#"
        function main() -> int {
            baml.sys.sleep(0);
            7
        }
    "#;

    let engine = make_engine(source);

    let mut handles = Vec::new();
    for _ in 0..8 {
        let engine = Arc::clone(&engine);
        handles.push(tokio::spawn(async move { call_main(&engine).await }));
    }
    for handle in handles {
        let value = handle.await.expect("task should not panic");
        assert_eq!(value, BexExternalValue::Int(7));
    }

    assert_eq!(engine.active_future_count().await, 0);
}
