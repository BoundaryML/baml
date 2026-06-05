//! Function-typed parameters and class fields accepting lambdas.

mod common;
use std::sync::Arc;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

async fn run(source: &str) -> BexExternalValue {
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), None, Vec::new()).expect("engine"));
    engine.call_function("main", vec![], FunctionCallContextBuilder::new(sys_types::CallId::next()).build(), true).await.unwrap()
}

#[tokio::test]
async fn clean_lambda_into_fn_param() {
    let r = run(r#"
        function take_f(f: (int) -> int) -> int { f(20) }
        function main() -> int { take_f((x) -> { x + 22 }) }
    "#).await;
    assert_eq!(r, BexExternalValue::Int(42));
}

#[tokio::test]
async fn clean_lambda_into_class_field() {
    let r = run(r#"
        class F { f (int) -> int }
        function main() -> int {
            let holder = F { f: (x: int) -> int { x * 2 } };
            let g = holder.f;
            g(21)
        }
    "#).await;
    assert_eq!(r, BexExternalValue::Int(42));
}
