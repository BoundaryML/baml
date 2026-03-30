//! Execution tests for lambda expressions (Phase 3 non-capturing, Phase 4 capturing).
//!
//! These tests verify that lambdas compile to correct bytecode and execute at
//! runtime, returning the expected values.

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

// ============================================================================
// Phase 4: Capturing Lambdas
// ============================================================================

/// Lambda capturing a local variable from the enclosing function.
/// let base = 10; let add_base = (x: int) -> int { x + base }; add_base(5) returns 15
#[tokio::test]
async fn lambda_captures_local_variable() {
    let output = baml_test!(
        "
        function main() -> int {
            let base = 10
            let add_base = (x: int) -> int { x + base }
            add_base(5)
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(15)));
}

/// Lambda capturing a parameter from the enclosing function.
/// function f(n: int) -> int { let double = () -> int { n * 2 }; double() }
/// f(21) returns 42
#[tokio::test]
async fn lambda_captures_function_parameter() {
    let output = baml_test!(
        "
        function main() -> int {
            let n = 21
            let double = () -> int { n * 2 }
            double()
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

/// Lambda capturing multiple variables.
/// let a = 10; let b = 5; let compute = () -> int { a * b - 8 }; compute() returns 42
#[tokio::test]
async fn lambda_captures_multiple_variables() {
    let output = baml_test!(
        "
        function main() -> int {
            let a = 10
            let b = 5
            let compute = () -> int { a * b - 8 }
            compute()
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}
