//! Wall-clock regression coverage for concurrent future combinators.
//! Result-only cases live in `baml_tests/baml_src/ns_future_combinators`.

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

/// The inputs run concurrently: three 200ms sleeps complete in ~200ms, not
/// ~600ms (compile/bootstrap excluded from the timing budget).
#[tokio::test]
async fn all_settled_runs_concurrently() {
    let source = r#"
        function work() -> int {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(200n));
            1
        }
        function main() -> int {
            let fs = [spawn { work() }, spawn { work() }, spawn { work() }];
            let outcomes = await baml.future.all_settled(fs);
            let results = outcomes.map((outcome) -> {
                match (outcome) {
                    let s: baml.future.Success<int> => s.value,
                    _ => baml.sys.panic("expected success"),
                }
            });
            results[0] + results[1] + results[2]
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
        elapsed < std::time::Duration::from_millis(500),
        "all_settled inputs should run concurrently (~200ms); got {elapsed:?}"
    );
}
