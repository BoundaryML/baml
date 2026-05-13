//! Additional runtime tests for `if let`, `while let`, and `let else`.
//!
//! Covers behaviours not exercised by `if_let.rs` / `while_let.rs` /
//! `let_else.rs`:
//!
//!   - scrutinee evaluation order (once vs. per-iteration)
//!   - mutating the binding inside the body
//!   - narrowing against `null`, `float`, and user-defined `class` types
//!   - interaction with `for` loops
//!   - throwing inside the scrutinee
//!   - the binding remaining narrowed throughout the body (typed ops work)

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ── Scrutinee evaluation order ────────────────────────────────────────────

#[tokio::test]
async fn if_let_scrutinee_evaluated_exactly_once() {
    // The scrutinee is a function call with a side-effect (incrementing a
    // shared counter via an out-parameter). If the desugar evaluated the
    // scrutinee twice (once for the type check, once for the bind value),
    // the counter would end up at 2 instead of 1.
    let output = baml_test!(
        r#"
        function increment_and_get(counter: int[]) -> int | string {
            counter[0] = counter[0] + 1;
            return counter[0];
        }

        function main() -> int {
            let counter = [0];
            let result = if let n: int = increment_and_get(counter) {
                n * 100
            } else {
                -1
            };
            // counter[0] should be 1 (single call), result = 100.
            return counter[0] + result;
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(101)));
}

#[tokio::test]
async fn let_else_scrutinee_evaluated_exactly_once() {
    let output = baml_test!(
        r#"
        function increment_and_get(counter: int[]) -> int | string {
            counter[0] = counter[0] + 1;
            return counter[0];
        }

        function main() -> int {
            let counter = [0];
            let n: int = increment_and_get(counter) else {
                return -1;
            };
            return counter[0] + n;
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn while_let_scrutinee_evaluated_per_iteration() {
    // The scrutinee is a function call that grows a counter each time it's
    // called. Each loop iteration re-evaluates the scrutinee, so the
    // counter increments every iteration.
    let output = baml_test!(
        r#"
        function next_value(counter: int[]) -> int | string {
            counter[0] = counter[0] + 1;
            if counter[0] >= 4 {
                return "done";
            }
            return counter[0];
        }

        function main() -> int {
            let counter = [0];
            let acc = 0;
            while let n: int = next_value(counter) {
                acc = acc + n;
            };
            // counter goes 1, 2, 3, 4 (last one returns "done", loop exits).
            // acc = 1+2+3 = 6
            return acc + counter[0];
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(10)));
}

// ── Mutating the binding inside the body ──────────────────────────────────

#[tokio::test]
async fn if_let_binding_is_mutable_inside_body() {
    let output = baml_test!(
        r#"
        function compute(v: int | string) -> int {
            if let n: int = v {
                n = n + 100;
                n = n * 2;
                return n;
            }
            return -1;
        }

        function main() -> int {
            compute(5)
        }
    "#
    );
    // (5 + 100) * 2 = 210
    assert_eq!(output.result, Ok(BexExternalValue::Int(210)));
}

#[tokio::test]
async fn let_else_binding_is_mutable_after() {
    let output = baml_test!(
        r#"
        function compute(v: int | string) -> int {
            let n: int = v else {
                return -1;
            };
            n = n + 100;
            n = n * 2;
            return n;
        }

        function main() -> int {
            compute(5)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(210)));
}

#[tokio::test]
async fn while_let_binding_mutated_in_body() {
    let output = baml_test!(
        r#"
        function run() -> int {
            let v: int | string = 1;
            let acc = 0;
            while let n: int = v {
                n = n * 10;
                acc = acc + n;
                if n >= 30 {
                    v = "stop";
                } else {
                    v = (n / 10) + 1;
                }
            };
            return acc;
        }

        function main() -> int {
            run()
        }
    "#
    );
    // n goes 1→10, 2→20, 3→30. acc = 10+20+30 = 60.
    assert_eq!(output.result, Ok(BexExternalValue::Int(60)));
}

// ── Narrow against `null` (optional types) ────────────────────────────────

#[tokio::test]
async fn if_let_int_narrow_against_optional_int() {
    let output = baml_test!(
        r#"
        function pull(v: int?) -> int {
            if let n: int = v {
                return n + 1;
            }
            return -1;
        }

        function main() -> int {
            pull(7)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(8)));
}

#[tokio::test]
async fn if_let_int_narrow_against_optional_int_null() {
    let output = baml_test!(
        r#"
        function pull(v: int?) -> int {
            if let n: int = v {
                return n + 1;
            }
            return -1;
        }

        function main() -> int {
            pull(null)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn let_else_int_narrow_against_optional_int() {
    let output = baml_test!(
        r#"
        function pull(v: int?) -> int {
            let n: int = v else {
                return -1;
            };
            return n + 1;
        }

        function main() -> int {
            pull(42)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(43)));
}

#[tokio::test]
async fn let_else_int_narrow_against_optional_int_null() {
    let output = baml_test!(
        r#"
        function pull(v: int?) -> int {
            let n: int = v else {
                return -1;
            };
            return n + 1;
        }

        function main() -> int {
            pull(null)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

// ── Narrow against `float` in a numeric union ─────────────────────────────

#[tokio::test]
async fn if_let_float_branch_in_numeric_union() {
    // Use a three-way union so the trailing `else` arm covers a residual
    // (string) — otherwise BAML's exhaustiveness check reports the else as
    // unreachable once the two numeric branches have been peeled off.
    let output = baml_test!(
        r#"
        function classify(v: int | float | string) -> int {
            if let f: float = v {
                return 2;
            } else if let n: int = v {
                return 1;
            } else {
                return -1;
            }
        }

        function main() -> int {
            classify(1.5)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn if_let_int_branch_in_numeric_union() {
    let output = baml_test!(
        r#"
        function classify(v: int | float | string) -> int {
            if let f: float = v {
                return 2;
            } else if let n: int = v {
                return 1;
            } else {
                return -1;
            }
        }

        function main() -> int {
            classify(7)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// ── Narrow against a user-defined class ──────────────────────────────────

#[tokio::test]
async fn if_let_narrows_user_class_match() {
    let output = baml_test!(
        r#"
        class Point {
            x int
            y int
        }

        function magnitude_sq(v: Point | int) -> int {
            if let p: Point = v {
                return p.x * p.x + p.y * p.y;
            }
            return -1;
        }

        function main() -> int {
            magnitude_sq(Point { x: 3, y: 4 })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(25)));
}

#[tokio::test]
async fn if_let_narrows_user_class_mismatch() {
    let output = baml_test!(
        r#"
        class Point {
            x int
            y int
        }

        function magnitude_sq(v: Point | int) -> int {
            if let p: Point = v {
                return p.x * p.x + p.y * p.y;
            }
            return -1;
        }

        function main() -> int {
            magnitude_sq(99)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn let_else_narrows_user_class_match() {
    let output = baml_test!(
        r#"
        class Point {
            x int
            y int
        }

        function magnitude_sq(v: Point | int) -> int {
            let p: Point = v else {
                return -1;
            };
            return p.x * p.x + p.y * p.y;
        }

        function main() -> int {
            magnitude_sq(Point { x: 5, y: 12 })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(169)));
}

#[tokio::test]
async fn let_else_narrows_user_class_mismatch() {
    let output = baml_test!(
        r#"
        class Point {
            x int
            y int
        }

        function magnitude_sq(v: Point | int) -> int {
            let p: Point = v else {
                return -1;
            };
            return p.x * p.x + p.y * p.y;
        }

        function main() -> int {
            magnitude_sq(99)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

// ── for-loop interaction with let-else ────────────────────────────────────

#[tokio::test]
async fn let_else_continue_inside_for_loop() {
    let output = baml_test!(
        r#"
        function sum_ints(values: (int | string)[]) -> int {
            let acc = 0;
            for (let v in values) {
                let n: int = v else {
                    continue;
                };
                acc = acc + n;
            };
            return acc;
        }

        function main() -> int {
            sum_ints([10, "skip", 20, "skip2", 30])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(60)));
}

#[tokio::test]
async fn let_else_break_inside_for_loop() {
    let output = baml_test!(
        r#"
        function sum_until_string(values: (int | string)[]) -> int {
            let acc = 0;
            for (let v in values) {
                let n: int = v else {
                    break;
                };
                acc = acc + n;
            };
            return acc;
        }

        function main() -> int {
            sum_until_string([10, 20, "stop", 30, 40])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(30)));
}

#[tokio::test]
async fn if_let_inside_for_loop_body() {
    let output = baml_test!(
        r#"
        function sum_ints(values: (int | string)[]) -> int {
            let acc = 0;
            for (let v in values) {
                if let n: int = v {
                    acc = acc + n;
                }
            };
            return acc;
        }

        function main() -> int {
            sum_ints([1, "x", 2, "y", 3])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

// ── Throwing scrutinees ──────────────────────────────────────────────────

#[tokio::test]
async fn if_let_throwing_scrutinee_caught_outside() {
    let output = baml_test!(
        r#"
        function bad() -> int | string {
            throw "fail";
        }

        function main() -> int {
            let v = bad() catch (e) {
                _ => "fallback"
            };
            if let n: int = v {
                return n;
            }
            return -7;
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-7)));
}

// ── Binding is properly narrowed in the body ─────────────────────────────

#[tokio::test]
async fn if_let_binding_supports_int_arithmetic() {
    // The binding `n` MUST be statically typed `int` inside the body, or
    // else `n + 1` / `n * 2` would either fail type-check or fall back to a
    // generic add. This test passing means the narrowed type flows
    // correctly into the body's expressions.
    let output = baml_test!(
        r#"
        function double_plus_one(v: int | string) -> int {
            if let n: int = v {
                return n * 2 + 1;
            }
            return -1;
        }

        function main() -> int {
            double_plus_one(20)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(41)));
}

#[tokio::test]
async fn let_else_binding_supports_string_methods() {
    let output = baml_test!(
        r#"
        function with_prefix(v: int | string) -> string {
            let s: string = v else {
                return "fallback";
            };
            // String methods only work because s is narrowed to `string`.
            return s + "!";
        }

        function main() -> string {
            with_prefix("hello")
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello!".to_string()))
    );
}

// ── if-let returning class instance ───────────────────────────────────────

#[tokio::test]
async fn if_let_returns_narrowed_class_instance() {
    let output = baml_test!(
        r#"
        class Wrapper {
            inner int
        }

        function unwrap_or(v: Wrapper | int) -> int {
            if let w: Wrapper = v {
                return w.inner;
            }
            if let n: int = v {
                return n;
            }
            return 0;
        }

        function main() -> int {
            unwrap_or(Wrapper { inner: 999 })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(999)));
}
