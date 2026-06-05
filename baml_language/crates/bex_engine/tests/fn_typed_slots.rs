//! Function-typed parameters and class fields accepting lambdas.

mod common;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use common::compile_for_engine;
use std::sync::Arc;
use sys_native::SysOpsExt;

async fn run(source: &str) -> BexExternalValue {
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
async fn clean_lambda_into_fn_param() {
    let r = run(r#"
        function take_f(f: (int) -> int) -> int { f(20) }
        function main() -> int { take_f((x) -> { x + 22 }) }
    "#)
    .await;
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
    "#)
    .await;
    assert_eq!(r, BexExternalValue::Int(42));
}

/// Throws semantics around fn-typed slots: an omitted `throws` on a callback
/// PARAM is effect-polymorphic (the callee forwards whatever the callback
/// throws — it does NOT mean `throws never`), and a function without a
/// `throws` clause infers its surface. Enforcement happens where a contract
/// is EXPLICIT: a `throws never` caller passing a throwing lambda through a
/// forwarding callee is rejected at compile time (E0096).
#[tokio::test]
#[should_panic(expected = "declared throws is `never`")]
async fn throwing_lambda_violates_explicit_never_contract() {
    run(r#"
        function take_f(f: (int) -> int) -> int { f(20) }
        function main() -> int throws never {
            take_f((x: int) -> int { throw "boom" })
        }
    "#)
    .await;
}
