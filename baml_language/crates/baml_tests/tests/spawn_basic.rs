//! BEP-034 phase G: basic `spawn { body }` + `await` end-to-end tests.
//!
//! These tests exercise the smallest spawn/await flows to confirm the
//! runtime wiring (compiler → MIR → bytecode → VM yield →
//! `spawn_thread` → child `run_thread_event_loop` → `FutureManager`
//! fulfillment → awaiter resume) is end-to-end correct.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn spawn_returning_int_literal() {
    let output = baml_test!(
        "
        function main() -> int {
            let f = spawn { 42 };
            await f
        }
        "
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn spawn_returning_string() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let f = spawn { "hello from spawn" };
            await f
        }
        "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello from spawn".to_string()))
    );
}

#[tokio::test]
async fn spawn_with_name_literal() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let f = spawn "answer" { 42 };
            await f
        }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}
