//! Regression: a lambda is a valid block TAIL expression (it was silently
//! dropped from block elements, typing the block as void).

mod common;
use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

async fn run(source: &str) -> BexExternalValue {
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("engine"),
    );
    engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn tail_lambda_annotated() {
    let r = run(r#"
        function mk() -> (int) -> int throws never {
            (x: int) -> int { x + 1 }
        }
        function main() -> int { let f = mk(); f(41) }
    "#)
    .await;
    assert_eq!(r, BexExternalValue::Int(42));
}

#[tokio::test]
async fn return_lambda_annotated() {
    let r = run(r#"
        function mk() -> (int) -> int throws never {
            return (x: int) -> int { x + 1 }
        }
        function main() -> int { let f = mk(); f(41) }
    "#)
    .await;
    assert_eq!(r, BexExternalValue::Int(42));
}
