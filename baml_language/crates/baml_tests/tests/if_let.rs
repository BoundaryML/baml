//! Runtime tests for `if let` expressions.
//!
//! `if let <pat> = <expr> { then } [else { else }]` desugars at AST lowering
//! to a `match` expression with two arms. These tests run the compiled
//! bytecode and assert the *value* produced, not just the bytecode shape.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ── Success path: pattern matches ─────────────────────────────────────────

#[tokio::test]
async fn if_let_type_narrow_matches_returns_then() {
    let output = baml_test!(
        r#"
        function classify(v: int | string) -> int {
            if let n: int = v {
                return n + 1;
            }
            return -1;
        }

        function main() -> int {
            classify(7)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(8)));
}

#[tokio::test]
async fn if_let_type_narrow_mismatches_falls_through() {
    let output = baml_test!(
        r#"
        function classify(v: int | string) -> int {
            if let n: int = v {
                return n + 1;
            }
            return -1;
        }

        function main() -> int {
            classify("hi")
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn if_let_with_else_branch_runs_on_mismatch() {
    let output = baml_test!(
        r#"
        function classify(v: int | string) -> int {
            if let n: int = v {
                return n;
            } else {
                return 99;
            }
        }

        function main() -> int {
            classify("not_an_int")
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

#[tokio::test]
async fn if_let_with_else_branch_runs_on_match() {
    let output = baml_test!(
        r#"
        function classify(v: int | string) -> int {
            if let n: int = v {
                return n * 10;
            } else {
                return 99;
            }
        }

        function main() -> int {
            classify(4)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(40)));
}

// ── if-let as an expression with a value ──────────────────────────────────

#[tokio::test]
async fn if_let_as_value_match_branch() {
    let output = baml_test!(
        r#"
        function pick(v: int | string) -> int {
            let r = if let n: int = v {
                n * 2
            } else {
                0
            };
            return r;
        }

        function main() -> int {
            pick(7)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(14)));
}

#[tokio::test]
async fn if_let_as_value_else_branch() {
    let output = baml_test!(
        r#"
        function pick(v: int | string) -> int {
            let r = if let n: int = v {
                n * 2
            } else {
                999
            };
            return r;
        }

        function main() -> int {
            pick("nope")
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(999)));
}

// ── else if let chains ────────────────────────────────────────────────────

#[tokio::test]
async fn else_if_let_chain_picks_first_match() {
    let output = baml_test!(
        r#"
        function classify(v: int | string | bool) -> int {
            if let n: int = v {
                return 1;
            } else if let s: string = v {
                return 2;
            } else if let b: bool = v {
                return 3;
            } else {
                return -1;
            }
        }

        function main() -> int {
            classify("hello")
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn else_if_let_chain_falls_to_last() {
    let output = baml_test!(
        r#"
        function classify(v: int | string | bool) -> int {
            if let n: int = v {
                return 1;
            } else if let s: string = v {
                return 2;
            } else if let b: bool = v {
                return 3;
            } else {
                return -1;
            }
        }

        function main() -> int {
            classify(true)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// ── shadowing: outer binding is restored after the if-let body ────────────

#[tokio::test]
async fn if_let_shadows_outer_then_restores() {
    let output = baml_test!(
        r#"
        function compute(v: int | string) -> int {
            let n = 100;
            let inside = if let n: int = v {
                n + 1
            } else {
                0
            };
            // `n` here MUST be the outer 100, not the if-let's binding.
            return inside + n;
        }

        function main() -> int {
            compute(5)
        }
    "#
    );
    // inside = 5 + 1 = 6; outer n = 100; total = 106.
    assert_eq!(output.result, Ok(BexExternalValue::Int(106)));
}

#[tokio::test]
async fn if_let_else_branch_sees_outer_binding() {
    let output = baml_test!(
        r#"
        function compute(v: int | string) -> int {
            let n = 42;
            return if let n: int = v {
                n
            } else {
                // `n` here must be the OUTER 42.
                n
            };
        }

        function main() -> int {
            compute("oops")
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// ── nested if-let ─────────────────────────────────────────────────────────

#[tokio::test]
async fn nested_if_let_both_match() {
    let output = baml_test!(
        r#"
        function combine(a: int | string, b: int | string) -> int {
            if let x: int = a {
                if let y: int = b {
                    return x + y;
                } else {
                    return x;
                }
            }
            return -1;
        }

        function main() -> int {
            combine(3, 4)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

#[tokio::test]
async fn nested_if_let_inner_misses() {
    let output = baml_test!(
        r#"
        function combine(a: int | string, b: int | string) -> int {
            if let x: int = a {
                if let y: int = b {
                    return x + y;
                } else {
                    return x;
                }
            }
            return -1;
        }

        function main() -> int {
            combine(3, "no")
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn nested_if_let_outer_misses() {
    let output = baml_test!(
        r#"
        function combine(a: int | string, b: int | string) -> int {
            if let x: int = a {
                return x;
            }
            return -1;
        }

        function main() -> int {
            combine("no", 0)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

// ── lambda capturing the if-let binding ───────────────────────────────────

#[tokio::test]
async fn if_let_lambda_captures_binding() {
    let output = baml_test!(
        r#"
        function compute(v: int | string) -> int {
            if let x: int = v {
                let f = () -> int { x * 3 };
                return f();
            }
            return -1;
        }

        function main() -> int {
            compute(6)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(18)));
}

// ── string return values ──────────────────────────────────────────────────

#[tokio::test]
async fn if_let_string_branch_value() {
    let output = baml_test!(
        r#"
        function describe(v: int | string) -> string {
            if let s: string = v {
                return s;
            }
            return "no-string";
        }

        function main() -> string {
            describe("hello")
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello".to_string()))
    );
}

#[tokio::test]
async fn if_let_string_fallback_value() {
    let output = baml_test!(
        r#"
        function describe(v: int | string) -> string {
            if let s: string = v {
                return s;
            }
            return "no-string";
        }

        function main() -> string {
            describe(42)
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("no-string".to_string()))
    );
}
