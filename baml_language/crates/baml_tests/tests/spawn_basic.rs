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

#[tokio::test]
async fn captured_int_arithmetic_uses_generic_binop() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let value = 1;
            let f = spawn { value };
            let _ = await f;
            value + 1
        }
        "#
    );

    assert!(
        output.bytecode.contains("bin_op +"),
        "expected captured int arithmetic to use generic bin_op:\n{}",
        output.bytecode
    );
    assert!(
        !output.bytecode.contains("add_int"),
        "captured int arithmetic must not use add_int:\n{}",
        output.bytecode
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn captured_float_array_element_arithmetic_uses_generic_binop() {
    let output = baml_test!(
        r#"
        function main() -> float {
            let values: float[] = [1.0];
            let f = spawn { values.length() };
            let _ = await f;
            values[0] + 1.0
        }
        "#
    );

    assert!(
        output.bytecode.contains("load_array_element"),
        "expected captured float[] element load in bytecode:\n{}",
        output.bytecode
    );
    assert!(
        output.bytecode.contains("bin_op +"),
        "expected captured float[] element arithmetic to use generic bin_op:\n{}",
        output.bytecode
    );
    assert!(
        !output.bytecode.contains("add_float"),
        "captured float[] element arithmetic must not use add_float:\n{}",
        output.bytecode
    );
    assert_eq!(output.result, Ok(BexExternalValue::Float(2.0)));
}
