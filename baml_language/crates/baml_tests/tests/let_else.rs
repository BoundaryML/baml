//! Runtime tests for Rust-style `let ... else` — the success path binds
//! the pattern's variables for the rest of the scope; the else path
//! runs when the pattern doesn't match and must diverge.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn let_else_success_path_binds_value() {
    // Pattern matches → bindings flow into the rest of the block.
    let output = baml_test!(
        r#"
        function pick(v: int | string) -> int {
            let n: int = v else { return 0; };
            n
        }

        function main() -> int {
            pick(42)
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn let_else_failure_path_runs_else_block() {
    // Pattern doesn't match → else block fires (here, early-return 0).
    let output = baml_test!(
        r#"
        function pick(v: int | string) -> int {
            let n: int = v else { return -1; };
            n
        }

        function main() -> int {
            pick("hello")
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn let_else_two_in_a_row() {
    // Each let-else independently narrows. Both must succeed for the
    // sum to be computed.
    let output = baml_test!(
        r#"
        function add_ints(a: int | string, b: int | string) -> int {
            let x: int = a else { return -10; };
            let y: int = b else { return -20; };
            x + y
        }

        function main() -> int {
            add_ints(3, 4)
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

#[tokio::test]
async fn let_else_short_circuits_on_first_failure() {
    // First let-else fails → returns -10 without evaluating the second.
    let output = baml_test!(
        r#"
        function add_ints(a: int | string, b: int | string) -> int {
            let x: int = a else { return -10; };
            let y: int = b else { return -20; };
            x + y
        }

        function main() -> int {
            add_ints("nope", 4)
        }
    "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Int(-10)));
}
