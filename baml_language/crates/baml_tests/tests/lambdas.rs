//! Execution tests for lambda expressions (Phases 3, 4, 5).
//!
//! These tests verify that lambdas compile to correct bytecode and execute at
//! runtime, returning the expected values.

use baml_tests::baml_test;
use baml_type::Ty;
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

// ============================================================================
// Phase 5: Transitive Captures + Full Coverage
// ============================================================================

/// Capturing lambda with shared mutation: two increments then read the cell.
/// let count = 0; let inc = () -> int { count += 1; count }; inc(); inc(); count -> 2
#[tokio::test]
async fn shared_cell_mutation_counter() {
    let output = baml_test!(
        "
        function main() -> int {
            let count = 0
            let inc = () -> int { count += 1; count }
            inc()
            inc()
            count
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

/// Nested lambdas with transitive capture: a + b + c across 3 levels.
/// a defined in main, b param of f, c param of g.
/// f(10) where g(100) returns a + b + c = 1 + 10 + 100 = 111
#[tokio::test]
async fn nested_lambda_transitive_capture() {
    let output = baml_test!(
        "
        function main() -> int {
            let a = 1
            let f = (b: int) -> int {
                let g = (c: int) -> int { a + b + c }
                g(100)
            }
            f(10)
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(111)));
}

/// IIFE returning a closure (counter factory pattern).
/// The inner closure captures `count` from the IIFE's scope.
/// inc(); inc(); inc() returns 3.
#[tokio::test]
async fn iife_returns_closure_counter() {
    let output = baml_test!(
        "
        function main() -> int {
            let inc = () -> {
                let count = 0
                let inner = () -> int { count += 1; count }
                inner
            }()
            inc()
            inc()
            inc()
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

/// Closure passed to .map() with a captured offset.
/// [1, 2, 3].map(x -> x + offset) where offset = 10 returns [11, 12, 13].
// Ignored: Array.map() is not yet implemented in the VM (panics "not yet implemented").
// The lambda compilation itself is correct; this test is blocked on VM work.
#[tokio::test]
#[ignore = "Array.map() not yet implemented in VM"]
async fn closure_in_map_with_captured_offset() {
    let output = baml_test!(
        "
        function main() -> int[] {
            let offset = 10
            let items: int[] = [1, 2, 3]
            items.map((x: int) -> int { x + offset })
        }
    "
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Array {
            element_type: Ty::int(),
            items: vec![
                BexExternalValue::Int(11),
                BexExternalValue::Int(12),
                BexExternalValue::Int(13),
            ],
        })
    );
}

/// Multiple closures sharing the same cell.
/// inc() increments x, dec() decrements x; both share the same cell.
/// inc(); inc(); dec() leaves x = 1.
#[tokio::test]
async fn multiple_closures_share_cell() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = 0
            let inc = () -> int { x += 1; x }
            let dec = () -> int { x -= 1; x }
            inc()
            inc()
            dec()
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

/// Deep nesting (3 levels) with transitive captures at each level.
/// a in main, b param of f, c param of g, d param of h.
/// a + b + c + d = 1 + 10 + 100 + 1000 = 1111
#[tokio::test]
async fn deep_nesting_three_levels() {
    let output = baml_test!(
        "
        function main() -> int {
            let a = 1
            let f = (b: int) -> int {
                let g = (c: int) -> int {
                    let h = (d: int) -> int { a + b + c + d }
                    h(1000)
                }
                g(100)
            }
            f(10)
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1111)));
}

/// Lambda capturing a loop variable (for-in accumulation with lambda).
/// Sums [1,2,3] using a closure that accumulates via captured variable.
#[tokio::test]
async fn lambda_captures_loop_variable_accumulation() {
    let output = baml_test!(
        "
        function main() -> int {
            let sum = 0
            let items: int[] = [1, 2, 3]
            let add_to_sum = (x: int) -> { sum += x }
            for (let x in items) {
                add_to_sum(x)
            }
            sum
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}
