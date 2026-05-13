//! Runtime tests for `while let` loops.
//!
//! `while let <pat> = <expr> { body }` desugars to a `while (true)` loop
//! containing a `match` whose fail arm `break`s. These tests run the
//! compiled bytecode and assert the *value* produced.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ── basic termination when the pattern stops matching ────────────────────

#[tokio::test]
async fn while_let_terminates_when_type_changes() {
    let output = baml_test!(
        r#"
        function count_loops() -> int {
            let v: int | string = 0;
            let count = 0;
            while let n: int = v {
                count = count + 1;
                if n >= 3 {
                    v = "stop";
                } else {
                    v = n + 1;
                }
            };
            return count;
        }

        function main() -> int {
            count_loops()
        }
    "#
    );
    // n = 0, 1, 2, 3 → 4 iterations, then v="stop" causes loop to break.
    assert_eq!(output.result, Ok(BexExternalValue::Int(4)));
}

#[tokio::test]
async fn while_let_immediate_break_on_first_mismatch() {
    let output = baml_test!(
        r#"
        function run() -> int {
            let v: int | string = "no";
            let count = 0;
            while let n: int = v {
                count = count + 1;
            };
            return count;
        }

        function main() -> int {
            run()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn while_let_accumulates_binding_values() {
    let output = baml_test!(
        r#"
        function sum_to_5() -> int {
            let v: int | string = 1;
            let acc = 0;
            while let n: int = v {
                acc = acc + n;
                if n >= 5 {
                    v = "done";
                } else {
                    v = n + 1;
                }
            };
            return acc;
        }

        function main() -> int {
            sum_to_5()
        }
    "#
    );
    // 1+2+3+4+5 = 15
    assert_eq!(output.result, Ok(BexExternalValue::Int(15)));
}

// ── break / continue inside the body ─────────────────────────────────────

#[tokio::test]
async fn while_let_explicit_break() {
    let output = baml_test!(
        r#"
        function run() -> int {
            let v: int | string = 1;
            let count = 0;
            while let n: int = v {
                count = count + 1;
                if n >= 3 {
                    break;
                }
                v = n + 1;
            };
            return count;
        }

        function main() -> int {
            run()
        }
    "#
    );
    // count goes 1, 2, 3 — third iteration hits break.
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn while_let_continue_skips_body_tail() {
    let output = baml_test!(
        r#"
        function run() -> int {
            let v: int | string = 0;
            let acc = 0;
            while let n: int = v {
                v = n + 1;
                if n == 2 {
                    continue;
                }
                acc = acc + n;
                if n >= 4 {
                    v = "done";
                }
            };
            return acc;
        }

        function main() -> int {
            run()
        }
    "#
    );
    // 0+1+3+4 = 8 (we skip n==2)
    assert_eq!(output.result, Ok(BexExternalValue::Int(8)));
}

// ── nested while-let ─────────────────────────────────────────────────────

#[tokio::test]
async fn while_let_nested_inner_runs_fully_each_outer() {
    let output = baml_test!(
        r#"
        function nested() -> int {
            let outer: int | string = 1;
            let total = 0;
            while let o: int = outer {
                let inner: int | string = 1;
                while let i: int = inner {
                    total = total + i;
                    if i >= 3 {
                        inner = "stop";
                    } else {
                        inner = i + 1;
                    }
                };
                if o >= 2 {
                    outer = "stop";
                } else {
                    outer = o + 1;
                }
            };
            return total;
        }

        function main() -> int {
            nested()
        }
    "#
    );
    // Outer runs twice (o=1, o=2). Each outer iter, inner sums 1+2+3=6. Total 12.
    assert_eq!(output.result, Ok(BexExternalValue::Int(12)));
}

// ── lambda capturing the loop binding ────────────────────────────────────

#[tokio::test]
async fn while_let_lambda_uses_current_binding_value() {
    let output = baml_test!(
        r#"
        function run() -> int {
            let v: int | string = 1;
            let acc = 0;
            while let n: int = v {
                let f = () -> int { n * 10 };
                acc = acc + f();
                if n >= 2 {
                    v = "stop";
                } else {
                    v = n + 1;
                }
            };
            return acc;
        }

        function main() -> int {
            run()
        }
    "#
    );
    // f() returns 10 then 20 → 10 + 20 = 30
    assert_eq!(output.result, Ok(BexExternalValue::Int(30)));
}

// ── shadow / restore ─────────────────────────────────────────────────────

#[tokio::test]
async fn while_let_outer_binding_restored_after_loop() {
    let output = baml_test!(
        r#"
        function run() -> int {
            let n = 1000;
            let v: int | string = 1;
            let acc = 0;
            while let n: int = v {
                acc = acc + n;
                v = "stop";
            };
            // `n` here MUST be the OUTER 1000.
            return acc + n;
        }

        function main() -> int {
            run()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1001)));
}

// ── scrutinee re-evaluated each iteration (function-call side-effect) ────

#[tokio::test]
async fn while_let_scrutinee_re_evaluated_each_iteration() {
    let output = baml_test!(
        r#"
        function run() -> int {
            let v: int | string = 0;
            let iters = 0;
            while let n: int = v {
                iters = iters + 1;
                // Mutating v here changes what the next iteration's
                // `while let n: int = v` sees. If the scrutinee were
                // evaluated once, the loop would never terminate.
                if n >= 2 {
                    v = "exit";
                } else {
                    v = n + 1;
                }
            };
            return iters;
        }

        function main() -> int {
            run()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}
