//! Runtime tests for `let ... else` statements.
//!
//! `let <pat> = <expr> else { <div> };` desugars to a `match` where the
//! success arm binds the pattern's name and the fail arm runs the diverging
//! `<div>` block.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ── match path: pattern matches, binding is visible after ─────────────────

#[tokio::test]
async fn let_else_match_path_binds_and_continues() {
    let output = baml_test!(
        r#"
        function pull(v: int | string) -> int {
            let n: int = v else {
                return -1;
            };
            return n + 100;
        }

        function main() -> int {
            pull(7)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(107)));
}

// ── mismatch path: else runs, function returns from else ─────────────────

#[tokio::test]
async fn let_else_mismatch_runs_else_return() {
    let output = baml_test!(
        r#"
        function pull(v: int | string) -> int {
            let n: int = v else {
                return -1;
            };
            return n + 100;
        }

        function main() -> int {
            pull("nope")
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

// ── else block can throw ─────────────────────────────────────────────────

#[tokio::test]
async fn let_else_mismatch_runs_else_throw_caught() {
    let output = baml_test!(
        r#"
        function pull(v: int | string) -> int {
            let n: int = v else {
                throw "not an int";
            };
            return n;
        }

        function main() -> int {
            pull("nope") catch (e) {
                _ => -42
            }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-42)));
}

// ── multiple sequenced let-elses ──────────────────────────────────────────

#[tokio::test]
async fn let_else_chained_bindings() {
    let output = baml_test!(
        r#"
        function both_match(a: int | string, b: int | string) -> int {
            let x: int = a else {
                return -1;
            };
            let y: int = b else {
                return -2;
            };
            return x + y;
        }

        function main() -> int {
            both_match(3, 4)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

#[tokio::test]
async fn let_else_chained_first_fails() {
    let output = baml_test!(
        r#"
        function pull_two(a: int | string, b: int | string) -> int {
            let x: int = a else {
                return -1;
            };
            let y: int = b else {
                return -2;
            };
            return x + y;
        }

        function main() -> int {
            pull_two("no", 4)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn let_else_chained_second_fails() {
    let output = baml_test!(
        r#"
        function pull_two(a: int | string, b: int | string) -> int {
            let x: int = a else {
                return -1;
            };
            let y: int = b else {
                return -2;
            };
            return x + y;
        }

        function main() -> int {
            pull_two(3, "no")
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-2)));
}

// ── divergent break inside a loop ────────────────────────────────────────

#[tokio::test]
async fn let_else_with_break_exits_loop() {
    let output = baml_test!(
        r#"
        function run(values: (int | string)[]) -> int {
            let acc = 0;
            let i = 0;
            while (i < 10) {
                if i >= 4 {
                    break;
                };
                let raw: int | string = values[i];
                let n: int = raw else {
                    break;
                };
                acc = acc + n;
                i = i + 1;
            };
            return acc;
        }

        function main() -> int {
            run([1, 2, "bad", 4])
        }
    "#
    );
    // 1 + 2 = 3, then "bad" hits the else and breaks.
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// ── divergent continue inside a loop ─────────────────────────────────────

#[tokio::test]
async fn let_else_with_continue_skips_element() {
    let output = baml_test!(
        r#"
        function sum_ints(values: (int | string)[]) -> int {
            let acc = 0;
            let i = 0;
            while (i < 4) {
                let raw: int | string = values[i];
                i = i + 1;
                let n: int = raw else {
                    continue;
                };
                acc = acc + n;
            };
            return acc;
        }

        function main() -> int {
            sum_ints([1, "skip", 2, "skip2"])
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// ── shadowing: outer name shadowed, only inner name visible after ────────

#[tokio::test]
async fn let_else_shadow_uses_new_binding_after() {
    let output = baml_test!(
        r#"
        function compute(raw: int | string) -> int {
            let n = 1000;
            let n: int = raw else {
                return -1;
            };
            // `n` here is the let-else binding (the int), not the outer 1000.
            return n;
        }

        function main() -> int {
            compute(5)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn let_else_else_branch_sees_outer_binding() {
    let output = baml_test!(
        r#"
        function compute(raw: int | string) -> int {
            let n = 999;
            let n: int = raw else {
                // `n` here is the OUTER 999 — the let-else binding is not
                // yet in scope in the else block.
                return n;
            };
            return n;
        }

        function main() -> int {
            compute("oops")
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(999)));
}

// ── wildcard let-else: no binding, just a type assertion ─────────────────

#[tokio::test]
async fn let_else_wildcard_passthrough() {
    let output = baml_test!(
        r#"
        function check(raw: int | string) -> int {
            let _: int = raw else {
                return -1;
            };
            return 1;
        }

        function main() -> int {
            check(42)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn let_else_wildcard_fail() {
    let output = baml_test!(
        r#"
        function check(raw: int | string) -> int {
            let _: int = raw else {
                return -1;
            };
            return 1;
        }

        function main() -> int {
            check("oops")
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

// ── lambda captures the let-else binding ─────────────────────────────────

#[tokio::test]
async fn let_else_binding_captured_by_lambda() {
    let output = baml_test!(
        r#"
        function compute(raw: int | string) -> int {
            let n: int = raw else {
                return -1;
            };
            let f = () -> int { n * 7 };
            return f();
        }

        function main() -> int {
            compute(6)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// ── string bindings work too ─────────────────────────────────────────────

#[tokio::test]
async fn let_else_string_binding_match() {
    let output = baml_test!(
        r#"
        function pull(raw: int | string) -> string {
            let s: string = raw else {
                return "fallback";
            };
            return s;
        }

        function main() -> string {
            pull("hello")
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello".to_string()))
    );
}

#[tokio::test]
async fn let_else_string_binding_fallback() {
    let output = baml_test!(
        r#"
        function pull(raw: int | string) -> string {
            let s: string = raw else {
                return "fallback";
            };
            return s;
        }

        function main() -> string {
            pull(42)
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("fallback".to_string()))
    );
}
