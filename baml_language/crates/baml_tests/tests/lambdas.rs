//! Execution tests for lambda expressions (non-capturing, Phase 3).
//!
//! These tests verify that non-capturing lambdas compile to correct bytecode
//! and execute at runtime, returning the expected values.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// Basic single-param lambda (IIFE): let f = (x: int) -> int { x + 1 }; f(10) returns 11
#[tokio::test]
async fn iife_single_param_returns_correct_value() {
    let output = baml_test!(
        "
        function main() -> int {
            let f = (x: int) -> int { x + 1 }
            f(10)
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(11)));
}

/// Zero-param lambda: let constant = () -> int { 42 }; constant() returns 42
#[tokio::test]
async fn zero_param_lambda_returns_constant() {
    let output = baml_test!(
        "
        function main() -> int {
            let constant = () -> int { 42 }
            constant()
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

/// Multi-param lambda: let add = (a: int, b: int) -> int { a + b }; add(20, 22) returns 42
#[tokio::test]
async fn multi_param_lambda_returns_sum() {
    let output = baml_test!(
        "
        function main() -> int {
            let add = (a: int, b: int) -> int { a + b }
            add(20, 22)
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

/// Annotated lambda: let double = (x: int) -> int { x * 2 }; double(21) returns 42
#[tokio::test]
async fn annotated_lambda_doubles_value() {
    let output = baml_test!(
        "
        function main() -> int {
            let double = (x: int) -> int { x * 2 }
            double(21)
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

/// Lambda with inferred return type: let double = (x: int) -> { x * 2 }; double(21) returns 42
#[tokio::test]
async fn inferred_return_type_lambda() {
    let output = baml_test!(
        "
        function main() -> int {
            let double = (x: int) -> { x * 2 }
            double(21)
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}
