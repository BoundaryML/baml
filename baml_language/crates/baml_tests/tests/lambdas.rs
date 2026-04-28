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
#[tokio::test]
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

/// `deep_copy` of a closure creates a new closure object but preserves the
/// same captured cells — matching JS/Python semantics where closures close
/// over variables by reference.  All four closures (inc, inc2, dec, dec2)
/// mutate the shared x cell: 0 →1 →2 →1 →2 →3 →2.
#[tokio::test]
async fn multiple_closures_share_cell_deep_copy() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = 0
            let inc = () -> int { x += 1; x }
            let dec = () -> int { x -= 1; x }
            let inc2 = baml.deep_copy(inc)
            let dec2 = baml.deep_copy(dec)
            inc()
            inc()
            dec2()
            inc2()
            inc2()
            dec2()
            x
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn explicit_throwing_lambda_catches_error() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let risky = (x: int) -> int throws string {
                if (x < 0) { throw "negative" }
                x
            }
            risky(-1) catch (e) {
                "negative" => -1,
                _ => -2
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn lambda_inside_catch_base_keeps_parameter_scope() {
    let output = baml_test!(
        r#"
        function main() -> int {
            {
                let f = (x: int) -> int {
                    if (x == 7) { x } else { 0 }
                }
                f(7)
            } catch (x) {
                _ => x
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
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

// ============================================================================
// PR Review Issue Tests — Probing potential bugs identified in code review
// ============================================================================

/// Issue E: resolutions/exhaustive_matches keyed by bare AstExprId.
/// Parent resolves `.length()` on `int[]` (Array.length → 3), lambda resolves
/// `.length()` on `string` (String.length → 5).  These are different
/// MemberResolutions; if one overwrites the other due to an ExprId collision,
/// the wrong method gets dispatched and the result will be incorrect.
#[tokio::test]
async fn issue_e_method_resolution_different_types() {
    let output = baml_test!(
        "
        function main() -> int {
            let arr: int[] = [1, 2, 3]
            let arr_len = arr.length()
            let f = (s: string) -> int { s.length() }
            arr_len * 10 + f(\"hello\")
        }
    "
    );
    // arr.length() = 3, "hello".length() = 5 → 35
    assert_eq!(output.result, Ok(BexExternalValue::Int(35)));
}

/// Issue F: is_captured post-pass marks wrong local with shadowing.
/// let x = 1; let g captures x (=1); let x = "shadow"; let f captures x (="shadow")
/// Both lambdas should capture the correct x for their position.
#[tokio::test]
async fn issue_f_shadowing_capture_correct_binding() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = 1
            let g = () -> int { x }
            let x = 2
            let f = () -> int { x }
            g() * 10 + f()
        }
    "
    );
    // g() captures first x=1, f() captures second x=2 → 10 + 2 = 12
    assert_eq!(output.result, Ok(BexExternalValue::Int(12)));
}

/// Issue F (variant): shadowed capture with mutation.
/// The first x should be independently cell-wrapped from the second x.
#[tokio::test]
async fn issue_f_shadowing_capture_independent_cells() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = 10
            let inc_first = () -> int { x += 1; x }
            let x = 100
            let inc_second = () -> int { x += 1; x }
            inc_first()
            inc_second()
            // first x should be 11, second x should be 101
            // but we can only return one — return inc_first result
            inc_first()
        }
    "
    );
    // inc_first mutates first x: 10→11→12, inc_second mutates second x: 100→101
    // final inc_first() returns 12
    assert_eq!(output.result, Ok(BexExternalValue::Int(12)));
}

/// Issue A: Virtual inlining re-evaluating capture reads.
/// Read capture, mutate capture via another closure, read again.
/// If virtual inlining caches the first read, the second read is wrong.
#[tokio::test]
async fn issue_a_capture_read_after_mutation() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = 0
            let read_x = () -> int { x }
            let inc_x = () -> { x += 1 }
            let before = read_x()
            inc_x()
            let after = read_x()
            before * 10 + after
        }
    "
    );
    // before = 0, inc_x makes x=1, after = 1 → 0*10 + 1 = 1
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

/// Issue A (variant): interleaved reads and writes through shared cell.
/// Tests that each read sees the most recent write.
#[tokio::test]
async fn issue_a_interleaved_capture_reads_writes() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = 0
            let set_x = (v: int) -> { x = v }
            let get_x = () -> int { x }
            set_x(10)
            let a = get_x()
            set_x(20)
            let b = get_x()
            set_x(30)
            let c = get_x()
            a + b + c
        }
    "
    );
    // a=10, b=20, c=30 → 60
    assert_eq!(output.result, Ok(BexExternalValue::Int(60)));
}

/// Issue D: Place::Capture as destination of a function call in lambda body.
/// If cleanup.rs calls base_local() on a Call terminator destination that is
/// Place::Capture, it panics. This test exercises assigning a call result to
/// a captured variable.
#[tokio::test]
async fn issue_d_capture_as_call_destination() {
    let output = baml_test!(
        "
        function helper() -> int { 42 }
        function main() -> int {
            let x = 0
            let f = () -> int {
                x = helper()
                x
            }
            f()
        }
    "
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

/// Issue B: @watch on a captured local.
/// If cell wrapping breaks watch, the watch value won't update.
/// This is a basic test — if @watch is not applicable in this context,
/// the test just verifies capture + mutation works.
#[tokio::test]
async fn issue_b_captured_variable_mutation_visible() {
    let output = baml_test!(
        "
        function main() -> int {
            let x = 0
            let set = (v: int) -> { x = v }
            set(42)
            x
        }
    "
    );
    // Parent reads x after lambda mutated it — should see 42
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

/// Issue B (variant): watch let + lambda capture.
/// A watched variable that is also captured by a lambda. After cell wrapping,
/// the watch instruction sees Object::Cell instead of the user value. Lambda
/// mutation via StoreDeref may not trigger watch notifications.
#[tokio::test]
async fn issue_b_watch_let_captured_by_lambda() {
    let output = baml_test!(
        r#"
        function main() -> int {
            watch let x = 0;
            let inc = () -> { x += 1 }
            inc()
            inc()
            x
        }
    "#
    );
    // Even if watch notifications are broken, the value should still be correct
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

/// Issue B (variant): watch let mutated by both parent and lambda.
#[tokio::test]
async fn issue_b_watch_let_mutated_by_parent_and_lambda() {
    let output = baml_test!(
        r#"
        function main() -> int {
            watch let counter = 0;
            counter = 1;
            let bump = () -> { counter += 10 }
            bump()
            counter
        }
    "#
    );
    // counter: 0 → 1 (parent) → 11 (lambda)
    assert_eq!(output.result, Ok(BexExternalValue::Int(11)));
}

/// Lambda parameter shadowing an annotated outer let. The lambda param's
/// declared type must replace any outer entry in `declared_types` so that
/// assignments to the param inside the body type-check against the param's
/// type, not the shadowed outer's. Previously `infer_lambda_body` seeded
/// params via `add_local`, which used `or_insert_with` for `declared_types`
/// and therefore preserved the outer entry — causing a phantom TypeMismatch
/// when the param is reassigned to a value of its declared type. With the
/// bug present, `compile_source` would panic via `assert_no_diagnostic_errors`
/// before reaching execution.
#[tokio::test]
async fn lambda_param_shadows_annotated_outer_local() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x: int = 7;
            let f = (x: string) -> int {
                x = "world";
                x.length()
            };
            f("hi") + x
        }
    "#
    );
    // Inside f, x is reassigned to "world" (length 5). Outer x is unchanged
    // (the lambda param shadows the outer binding entirely). 5 + 7 = 12.
    assert_eq!(output.result, Ok(BexExternalValue::Int(12)));
}
